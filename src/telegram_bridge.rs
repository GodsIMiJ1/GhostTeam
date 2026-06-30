use std::{env, time::Duration};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use shell_words::split as split_shell_words;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::time::sleep;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderName, header::HeaderValue},
    },
};

const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8080";
const DEFAULT_TELEGRAM_API_BASE_URL: &str = "https://api.telegram.org";
const TELEGRAM_LONG_POLL_SECONDS: u64 = 25;
const TELEGRAM_RETRY_DELAY: Duration = Duration::from_secs(3);
const TELEGRAM_MESSAGE_CHUNK_LIMIT: usize = 3500;
const TELEGRAM_LOG_TIMEOUT: Duration = Duration::from_secs(2);
const TELEGRAM_LOG_MAX_LINES: usize = 12;

pub async fn run() -> Result<()> {
    let config = BridgeConfig::from_env()?;
    init_logging(&config.log_level);

    let bridge = TelegramBridge::new(config)?;
    bridge.run().await
}

fn init_logging(level: &str) {
    let env = env_logger::Env::default().default_filter_or(level);
    let _ = env_logger::Builder::from_env(env).format_timestamp_secs().try_init();
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    telegram_bot_token: String,
    telegram_api_base_url: String,
    api_base_url: String,
    api_key: Option<String>,
    log_level: String,
}

impl BridgeConfig {
    pub fn from_env() -> Result<Self> {
        let telegram_bot_token = env::var("TELEGRAM_BOT_TOKEN")
            .context("TELEGRAM_BOT_TOKEN is required for the Telegram bridge")?;
        let telegram_api_base_url = env::var("TELEGRAM_API_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_TELEGRAM_API_BASE_URL.to_string());
        let api_base_url =
            env::var("GHOSTTEAM_API_URL").unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_string());
        let api_key = env::var("GHOSTTEAM_API_KEY").ok().and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        Ok(Self {
            telegram_bot_token,
            telegram_api_base_url: telegram_api_base_url.trim_end_matches('/').to_string(),
            api_base_url: api_base_url.trim_end_matches('/').to_string(),
            api_key,
            log_level,
        })
    }

    pub fn for_test(
        telegram_bot_token: impl Into<String>,
        telegram_api_base_url: impl Into<String>,
        api_base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            telegram_bot_token: telegram_bot_token.into(),
            telegram_api_base_url: telegram_api_base_url.into(),
            api_base_url: api_base_url.into(),
            api_key,
            log_level: "debug".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TelegramBridge {
    config: BridgeConfig,
    http: Client,
}

impl TelegramBridge {
    pub fn new(config: BridgeConfig) -> Result<Self> {
        let http = Client::builder()
            .user_agent("ghostteam-telegram/phase1")
            .build()
            .context("failed to build Telegram bridge HTTP client")?;

        Ok(Self { config, http })
    }

    pub async fn run(self) -> Result<()> {
        let me = self.get_me().await?;
        if let Some(username) = me.username {
            log::info!("Telegram bridge connected as @{}", username);
        } else {
            log::info!("Telegram bridge connected");
        }

        let mut offset = 0_i64;
        loop {
            match self.run_once(offset).await {
                Ok(next_offset) => {
                    offset = next_offset;
                }
                Err(error) => {
                    log::warn!("Telegram polling failed: {error}");
                    sleep(TELEGRAM_RETRY_DELAY).await;
                }
            }
        }
    }

    pub async fn run_once(&self, offset: i64) -> Result<i64> {
        let updates = self.poll_updates(offset).await?;
        let mut next_offset = offset;

        for update in updates {
            next_offset = update.update_id + 1;
            if let Err(error) = self.handle_update(update).await {
                log::warn!("failed to handle Telegram update: {error}");
            }
        }

        Ok(next_offset)
    }

    async fn handle_update(&self, update: TelegramUpdate) -> Result<()> {
        let message = match update.message {
            Some(message) => message,
            None => return Ok(()),
        };

        let text = match message.text {
            Some(text) => text.trim().to_string(),
            None => return Ok(()),
        };

        let response = self.handle_command_text(&text).await?;
        self.send_reply(message.chat.id, &response).await?;

        Ok(())
    }

    async fn handle_command_text(&self, text: &str) -> Result<String> {
        let command = match parse_command(text) {
            Ok(command) => command,
            Err(error) => return Ok(command_error_message(&error)),
        };

        match command {
            BridgeCommand::Status(agent) => match agent {
                Some(agent) => self.agent_status_message(&agent).await,
                None => Ok(self.status_message().await),
            },
            BridgeCommand::Agents => Ok(self.agents_message().await),
            BridgeCommand::Tasks => Ok(self.tasks_message().await),
            BridgeCommand::Help => Ok(help_message()),
            BridgeCommand::Assign { agent, description } => {
                self.assign_message(&agent, &description).await
            }
            BridgeCommand::Logs { agent } => self.logs_message(&agent).await,
            BridgeCommand::Handoff { agent, description } => {
                self.handoff_message(&agent, &description).await
            }
        }
    }

    async fn status_message(&self) -> String {
        let agents_result = self.fetch_agents().await;
        let tasks_result = self.fetch_tasks().await;
        let timestamp = utc_timestamp();

        let mut lines = vec![
            "GhostTeam status".to_string(),
            format!("API base URL: {}", self.config.api_base_url),
            format!("Timestamp: {timestamp}"),
            describe_probe("Agents", agents_result.as_ref().map(|items| items.len())),
            describe_probe("Tasks", tasks_result.as_ref().map(|items| items.len())),
            "Messages: summary is not exposed by the current API shape".to_string(),
        ];

        if let Ok(agents) = agents_result {
            let sample = agents.iter().take(3).map(|agent| agent.id.as_str()).collect::<Vec<_>>();
            if !sample.is_empty() {
                lines.push(format!("Active agents: {}", sample.join(", ")));
            }
        }

        lines.join("\n")
    }

    async fn agent_status_message(&self, agent: &str) -> Result<String> {
        let agent_record = match self.fetch_agent(agent).await {
            Ok(agent_record) => agent_record,
            Err(error) => {
                return Ok(format!(
                    "GhostTeam agent status for {agent}\nUnable to load agent details from {}.\n{}",
                    self.config.api_base_url,
                    friendly_api_error(&error)
                ));
            }
        };

        let unread_messages = self.fetch_messages(agent).await.unwrap_or_default();
        let tasks = self.fetch_tasks().await.unwrap_or_default();
        let assigned_tasks =
            tasks.iter().filter(|task| task.assignee.as_deref() == Some(agent)).collect::<Vec<_>>();
        let log_excerpt = self.fetch_log_excerpt(agent).await.unwrap_or_default();

        let mut lines = vec![
            format!("GhostTeam agent status for {agent}"),
            format!("Role: {}", agent_record.role),
            format!("Backend: {}", agent_record.backend),
        ];

        if let Some(joined_at) = agent_record.joined_at {
            lines.push(format!("Joined at: {joined_at}"));
        }

        lines.push(format!("Unread messages: {}", unread_messages.len()));
        lines.push(format!("Assigned tasks: {}", assigned_tasks.len()));

        if !unread_messages.is_empty() {
            lines.push("Recent messages:".to_string());
            for message in unread_messages.iter().take(3) {
                lines.push(format!("- {}: {}", message.sender, message.body));
            }
        }

        if !log_excerpt.is_empty() {
            lines.push("Recent logs:".to_string());
            for line in log_excerpt.iter().take(3) {
                lines.push(format!("- {line}"));
            }
        }

        Ok(lines.join("\n"))
    }

    async fn agents_message(&self) -> String {
        match self.fetch_agents().await {
            Ok(agents) => format_agents(&agents),
            Err(error) => format!(
                "GhostTeam agents\nUnable to load registered agents from {}.\n{}",
                self.config.api_base_url,
                friendly_api_error(&error)
            ),
        }
    }

    async fn tasks_message(&self) -> String {
        match self.fetch_tasks().await {
            Ok(tasks) => format_tasks(&tasks),
            Err(error) => format!(
                "GhostTeam tasks\nUnable to load tasks from {}.\n{}",
                self.config.api_base_url,
                friendly_api_error(&error)
            ),
        }
    }

    async fn assign_message(&self, agent: &str, description: &str) -> Result<String> {
        let task_id = self
            .create_task("telegram", agent, description)
            .await
            .with_context(|| format!("failed to assign task to {agent}"))?;

        Ok(match task_id {
            Some(id) => format!(
                "Task assigned\nTarget: {agent}\nTask id: #{id}\nDescription: {description}"
            ),
            None => format!("Task assigned\nTarget: {agent}\nDescription: {description}"),
        })
    }

    async fn handoff_message(&self, agent: &str, description: &str) -> Result<String> {
        let handoff_description = format!("Handoff via Telegram: {description}");
        let task_id = self
            .create_task("telegram", agent, &handoff_description)
            .await
            .with_context(|| format!("failed to create handoff for {agent}"))?;

        Ok(match task_id {
            Some(id) => format!(
                "Task handoff queued\nTarget: {agent}\nTask id: #{id}\nDescription: {description}"
            ),
            None => format!("Task handoff queued\nTarget: {agent}\nDescription: {description}"),
        })
    }

    async fn logs_message(&self, agent: &str) -> Result<String> {
        match self.fetch_log_excerpt(agent).await {
            Ok(lines) => Ok(format_logs(agent, &lines)),
            Err(error) => Ok(format!(
                "GhostTeam logs for {agent}\nUnable to load log stream.\n{}",
                friendly_api_error(&error)
            )),
        }
    }

    async fn get_me(&self) -> Result<TelegramUser> {
        self.telegram_get::<TelegramResponse<TelegramUser>>("getMe", None)
            .await?
            .into_result()
            .context("Telegram rejected the bot token")
    }

    async fn poll_updates(&self, offset: i64) -> Result<Vec<TelegramUpdate>> {
        let query = vec![
            ("timeout", TELEGRAM_LONG_POLL_SECONDS.to_string()),
            ("offset", offset.to_string()),
        ];
        let response = self
            .telegram_get::<TelegramResponse<Vec<TelegramUpdate>>>("getUpdates", Some(&query))
            .await?
            .into_result()
            .context("Telegram returned a malformed getUpdates response")?;

        Ok(response)
    }

    async fn send_reply(&self, chat_id: i64, body: &str) -> Result<()> {
        for chunk in chunk_message(body, TELEGRAM_MESSAGE_CHUNK_LIMIT) {
            let payload = TelegramSendMessageRequest { chat_id, text: &chunk };
            let response = self
                .telegram_post::<TelegramResponse<TelegramMessage>, _>("sendMessage", &payload)
                .await?
                .into_result()
                .context("Telegram rejected sendMessage")?;
            if response.message_id == 0 {
                log::debug!("Telegram returned a sendMessage response without a message id");
            }
        }

        Ok(())
    }

    async fn fetch_agents(&self) -> Result<Vec<AgentRecord>, ApiError> {
        let envelope = self.api_get::<ApiEnvelope<Vec<AgentRecord>>>("/agents").await?;
        envelope.into_data("agents")
    }

    async fn fetch_tasks(&self) -> Result<Vec<TaskRecord>, ApiError> {
        let envelope = self.api_get::<ApiEnvelope<Vec<TaskRecord>>>("/tasks").await?;
        envelope.into_data("tasks")
    }

    async fn fetch_agent(&self, agent: &str) -> Result<AgentRecord, ApiError> {
        let envelope =
            self.api_get::<ApiEnvelope<AgentRecord>>(&format!("/agents/{agent}")).await?;
        envelope.into_data("agent")
    }

    async fn fetch_messages(&self, agent: &str) -> Result<Vec<MessageRecord>, ApiError> {
        let envelope =
            self.api_get::<ApiEnvelope<Vec<MessageRecord>>>(&format!("/messages/{agent}")).await?;
        envelope.into_data("messages")
    }

    async fn create_task(
        &self,
        from: &str,
        to: &str,
        description: &str,
    ) -> Result<Option<i64>, ApiError> {
        let payload = CreateTaskRequest {
            from: from.to_string(),
            to: to.to_string(),
            description: description.to_string(),
        };
        let response = self.api_post::<serde_json::Value, _>("/tasks/create", &payload).await?;
        Ok(extract_id(&response))
    }

    async fn fetch_log_excerpt(&self, agent: &str) -> Result<Vec<String>, ApiError> {
        let url = self.api_ws_url(&format!("/logs/{agent}/stream"));
        let mut request = url.into_client_request().map_err(|error| {
            ApiError::Malformed(format!("failed to build websocket request: {error}"))
        })?;
        if let Some(api_key) = &self.config.api_key {
            let header_name = HeaderName::from_static("x-ghostteam-key");
            let header_value = HeaderValue::from_str(api_key).map_err(|error| {
                ApiError::Malformed(format!("failed to encode websocket header: {error}"))
            })?;
            request.headers_mut().insert(header_name, header_value);
        }

        let (mut socket, _) =
            connect_async(request).await.map_err(|error| ApiError::Transport(error.to_string()))?;

        let mut lines = Vec::new();
        let timer = sleep(TELEGRAM_LOG_TIMEOUT);
        tokio::pin!(timer);

        loop {
            if lines.len() >= TELEGRAM_LOG_MAX_LINES {
                break;
            }

            tokio::select! {
                _ = &mut timer => {
                    break;
                }
                message = socket.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            lines.push(text.to_string());
                        }
                        Some(Ok(Message::Binary(_))) => {}
                        Some(Ok(Message::Ping(payload))) => {
                            if let Err(error) = socket.send(Message::Pong(payload)).await {
                                return Err(ApiError::Transport(error.to_string()));
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {}
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(error)) => {
                            return Err(ApiError::Transport(error.to_string()));
                        }
                        Some(Ok(Message::Frame(_))) => {}
                    }
                }
            }
        }

        Ok(lines)
    }

    async fn api_get<T>(&self, path: &str) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
    {
        let url = format!("{}{}", self.config.api_base_url, path);
        let mut request = self.http.get(&url);
        if let Some(api_key) = &self.config.api_key {
            request = request.header("X-GhostTeam-Key", api_key);
        }

        let response =
            request.send().await.map_err(|error| ApiError::Transport(error.to_string()))?;

        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err(ApiError::Unauthorized);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::Http { status, body: body.trim().to_string() });
        }

        response.json::<T>().await.map_err(|error| ApiError::Malformed(error.to_string()))
    }

    async fn api_post<T, P>(&self, path: &str, payload: &P) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let url = format!("{}{}", self.config.api_base_url, path);
        let mut request = self.http.post(&url).json(payload);
        if let Some(api_key) = &self.config.api_key {
            request = request.header("X-GhostTeam-Key", api_key);
        }

        let response =
            request.send().await.map_err(|error| ApiError::Transport(error.to_string()))?;

        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err(ApiError::Unauthorized);
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::Http { status, body: body.trim().to_string() });
        }

        response.json::<T>().await.map_err(|error| ApiError::Malformed(error.to_string()))
    }

    async fn telegram_get<T>(
        &self,
        method: &str,
        query: Option<&[(&str, String)]>,
    ) -> Result<T, TelegramError>
    where
        T: DeserializeOwned,
    {
        let url = format!(
            "{}/bot{}/{}",
            self.config.telegram_api_base_url, self.config.telegram_bot_token, method
        );
        let mut request = self.http.get(&url);
        if let Some(query) = query {
            request = request.query(query);
        }

        let response =
            request.send().await.map_err(|error| TelegramError::Transport(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TelegramError::Http { status, body: body.trim().to_string() });
        }

        response.json::<T>().await.map_err(|error| TelegramError::Malformed(error.to_string()))
    }

    async fn telegram_post<T, P>(&self, method: &str, payload: &P) -> Result<T, TelegramError>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let url = format!(
            "{}/bot{}/{}",
            self.config.telegram_api_base_url, self.config.telegram_bot_token, method
        );

        let response = self
            .http
            .post(&url)
            .json(payload)
            .send()
            .await
            .map_err(|error| TelegramError::Transport(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(TelegramError::Http { status, body: body.trim().to_string() });
        }

        response.json::<T>().await.map_err(|error| TelegramError::Malformed(error.to_string()))
    }

    fn api_ws_url(&self, path: &str) -> String {
        let base = self.config.api_base_url.trim_end_matches('/');
        let ws_base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            base.to_string()
        };
        format!("{ws_base}{path}")
    }
}

#[derive(Debug, Deserialize)]
struct TelegramResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

impl<T> TelegramResponse<T> {
    fn into_result(self) -> Result<T, TelegramError> {
        if self.ok {
            self.result
                .ok_or_else(|| TelegramError::Malformed("Telegram response missing result".into()))
        } else {
            Err(TelegramError::Api(
                self.description.unwrap_or_else(|| "Telegram request failed".to_string()),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
struct TelegramUser {
    username: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    chat: TelegramChat,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
}

#[derive(Debug, Serialize)]
struct TelegramSendMessageRequest<'a> {
    chat_id: i64,
    text: &'a str,
}

#[derive(Debug, Serialize)]
struct CreateTaskRequest {
    from: String,
    to: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T> ApiEnvelope<T> {
    fn into_data(self, label: &str) -> Result<T, ApiError> {
        if self.ok {
            self.data.ok_or_else(|| {
                ApiError::Malformed(format!("{label} response was missing a data field"))
            })
        } else {
            Err(ApiError::Malformed(
                self.error.unwrap_or_else(|| format!("{label} request failed")),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
struct AgentRecord {
    id: String,
    role: String,
    backend: String,
    joined_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskRecord {
    id: i64,
    creator: String,
    assignee: Option<String>,
    description: String,
    status: String,
    result: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MessageRecord {
    id: i64,
    sender: String,
    recipient: String,
    body: String,
    created_at: Option<String>,
    read: i64,
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("GhostTeam API is unreachable: {0}")]
    Transport(String),
    #[error("GhostTeam API rejected the configured key")]
    Unauthorized,
    #[error("GhostTeam API returned HTTP {status}: {body}")]
    Http { status: StatusCode, body: String },
    #[error("GhostTeam API returned malformed JSON: {0}")]
    Malformed(String),
}

#[derive(Debug, Error)]
enum TelegramError {
    #[error("Telegram request failed: {0}")]
    Transport(String),
    #[error("Telegram returned HTTP {status}: {body}")]
    Http { status: StatusCode, body: String },
    #[error("Telegram response was malformed: {0}")]
    Malformed(String),
    #[error("Telegram API error: {0}")]
    Api(String),
}

#[derive(Debug)]
enum BridgeCommand {
    Status(Option<String>),
    Agents,
    Tasks,
    Help,
    Assign { agent: String, description: String },
    Logs { agent: String },
    Handoff { agent: String, description: String },
}

fn parse_command(text: &str) -> Result<BridgeCommand, String> {
    let parts =
        split_shell_words(text).map_err(|error| format!("Could not parse command: {error}"))?;
    let Some(command) = parts.first().map(|token| normalize_command(token)) else {
        return Err(help_message());
    };

    match command.as_str() {
        "/status" => match parts.len() {
            1 => Ok(BridgeCommand::Status(None)),
            2 => Ok(BridgeCommand::Status(Some(parts[1].clone()))),
            _ => Err("Usage: /status [agent]".to_string()),
        },
        "/agents" => Ok(BridgeCommand::Agents),
        "/tasks" => Ok(BridgeCommand::Tasks),
        "/help" => Ok(BridgeCommand::Help),
        "/assign" => {
            if parts.len() < 3 {
                Err("Usage: /assign <agent> <description>".to_string())
            } else {
                Ok(BridgeCommand::Assign {
                    agent: parts[1].clone(),
                    description: parts[2..].join(" "),
                })
            }
        }
        "/logs" => {
            if parts.len() < 2 {
                Err("Usage: /logs <agent>".to_string())
            } else {
                Ok(BridgeCommand::Logs { agent: parts[1].clone() })
            }
        }
        "/handoff" => {
            if parts.len() < 3 {
                Err("Usage: /handoff <agent> <description>".to_string())
            } else {
                Ok(BridgeCommand::Handoff {
                    agent: parts[1].clone(),
                    description: parts[2..].join(" "),
                })
            }
        }
        other => Err(format!("Unknown command: {other}\n\n{}", help_message())),
    }
}

fn normalize_command(token: &str) -> String {
    token.split_whitespace().next().unwrap_or(token).split('@').next().unwrap_or(token).to_string()
}

fn help_message() -> String {
    [
        "GhostTeam Telegram bridge is online.",
        "Commands:",
        "- /help",
        "- /status",
        "- /status <agent>",
        "- /agents",
        "- /tasks",
        "- /assign <agent> <description>",
        "- /logs <agent>",
        "- /handoff <agent> <description>",
    ]
    .join("\n")
}

fn command_error_message(error: &str) -> String {
    format!("{}\n\n{error}", help_message())
}

fn chunk_message(text: &str, limit: usize) -> Vec<String> {
    if text.len() <= limit {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        let ch_len = ch.len_utf8();
        if !current.is_empty() && current.len() + ch_len > limit {
            chunks.push(current);
            current = String::new();
        }
        current.push(ch);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn utc_timestamp() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_else(|_| "unknown".to_string())
}

fn describe_probe(label: &str, result: Result<usize, &ApiError>) -> String {
    match result {
        Ok(count) => format!("{label}: reachable ({count} items)"),
        Err(ApiError::Unauthorized) => format!("{label}: reachable (authentication rejected)"),
        Err(ApiError::Http { status, .. }) => format!("{label}: reachable (HTTP {status})"),
        Err(ApiError::Malformed(message)) => {
            format!("{label}: reachable (malformed response: {message})")
        }
        Err(ApiError::Transport(message)) => format!("{label}: unreachable ({message})"),
    }
}

fn friendly_api_error(error: &ApiError) -> String {
    match error {
        ApiError::Unauthorized => "The API key was rejected. Check GHOSTTEAM_API_KEY.".to_string(),
        ApiError::Http { status, body } => {
            if body.is_empty() {
                format!("The API returned HTTP {status}.")
            } else {
                format!("The API returned HTTP {status}: {body}")
            }
        }
        ApiError::Malformed(message) => {
            format!("The API responded, but the payload was malformed: {message}")
        }
        ApiError::Transport(message) => format!("The API is unreachable: {message}"),
    }
}

fn format_agents(agents: &[AgentRecord]) -> String {
    if agents.is_empty() {
        return "GhostTeam agents\nNo agents are currently registered.".to_string();
    }

    let mut lines = vec![format!("GhostTeam agents ({})", agents.len())];
    for agent in agents {
        let mut line = format!("- {} | role={} | backend={}", agent.id, agent.role, agent.backend);
        if let Some(joined_at) = &agent.joined_at {
            line.push_str(&format!(" | joined_at={joined_at}"));
        }
        lines.push(line);
    }
    lines.push("Last heartbeat/status is not exposed by the current API.".to_string());
    lines.join("\n")
}

fn format_tasks(tasks: &[TaskRecord]) -> String {
    if tasks.is_empty() {
        return "GhostTeam tasks\nNo tasks are currently registered.".to_string();
    }

    let mut lines = vec![format!("GhostTeam tasks ({})", tasks.len())];
    for task in tasks.iter().take(10) {
        let mut line = format!(
            "- #{} | creator={} | assignee={} | status={}",
            task.id,
            task.creator,
            task.assignee.clone().unwrap_or_else(|| "unassigned".to_string()),
            task.status
        );
        if !task.description.is_empty() {
            line.push_str(&format!(" | {}", task.description));
        }
        if let Some(result) = &task.result {
            line.push_str(&format!(" | result={result}"));
        }
        if let Some(created_at) = &task.created_at {
            line.push_str(&format!(" | created_at={created_at}"));
        }
        if let Some(updated_at) = &task.updated_at {
            line.push_str(&format!(" | updated_at={updated_at}"));
        }
        lines.push(line);
    }

    if tasks.len() > 10 {
        lines.push(format!("...and {} more", tasks.len() - 10));
    }

    lines.join("\n")
}

fn format_logs(agent: &str, lines: &[String]) -> String {
    if lines.is_empty() {
        return format!("GhostTeam logs for {agent}\nNo recent log lines were received.");
    }

    let mut output = vec![format!("GhostTeam logs for {agent}")];
    output.extend(lines.iter().cloned());
    output.join("\n")
}

fn extract_id(value: &serde_json::Value) -> Option<i64> {
    value
        .get("id")
        .and_then(|v| v.as_i64())
        .or_else(|| value.get("data").and_then(|data| data.get("id")).and_then(|v| v.as_i64()))
        .or_else(|| {
            value
                .get("data")
                .and_then(|data| data.get("task"))
                .and_then(|task| task.get("id"))
                .and_then(|v| v.as_i64())
        })
}
