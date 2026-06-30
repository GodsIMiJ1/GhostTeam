use std::{env, time::Duration};

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::time::sleep;

const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8080";
const TELEGRAM_LONG_POLL_SECONDS: u64 = 25;
const TELEGRAM_RETRY_DELAY: Duration = Duration::from_secs(3);
const TELEGRAM_MESSAGE_CHUNK_LIMIT: usize = 3500;

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
struct BridgeConfig {
    telegram_bot_token: String,
    api_base_url: String,
    api_key: Option<String>,
    log_level: String,
}

impl BridgeConfig {
    fn from_env() -> Result<Self> {
        let telegram_bot_token = env::var("TELEGRAM_BOT_TOKEN")
            .context("TELEGRAM_BOT_TOKEN is required for the Telegram bridge")?;
        let api_base_url =
            env::var("GHOSTTEAM_API_URL").unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_string());
        let api_key = env::var("GHOSTTEAM_API_KEY").ok().and_then(|value| {
            let trimmed = value.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });
        let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        Ok(Self {
            telegram_bot_token,
            api_base_url: api_base_url.trim_end_matches('/').to_string(),
            api_key,
            log_level,
        })
    }
}

#[derive(Debug)]
struct TelegramBridge {
    config: BridgeConfig,
    http: Client,
}

impl TelegramBridge {
    fn new(config: BridgeConfig) -> Result<Self> {
        let http = Client::builder()
            .user_agent("ghostteam-telegram/phase1")
            .build()
            .context("failed to build Telegram bridge HTTP client")?;

        Ok(Self { config, http })
    }

    async fn run(self) -> Result<()> {
        let me = self.get_me().await?;
        if let Some(username) = me.username {
            log::info!("Telegram bridge connected as @{}", username);
        } else {
            log::info!("Telegram bridge connected");
        }

        let mut offset = 0_i64;
        loop {
            match self.poll_updates(offset).await {
                Ok(updates) => {
                    for update in updates {
                        offset = update.update_id + 1;
                        if let Err(error) = self.handle_update(update).await {
                            log::warn!("failed to handle Telegram update: {error}");
                        }
                    }
                }
                Err(error) => {
                    log::warn!("Telegram polling failed: {error}");
                    sleep(TELEGRAM_RETRY_DELAY).await;
                }
            }
        }
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

        let command = normalize_command(&text);
        let response = match command.as_str() {
            "/status" => self.status_message().await,
            "/agents" => self.agents_message().await,
            _ => Some(
                "GhostTeam Telegram bridge is online.\nCommands:\n- /status\n- /agents".to_string(),
            ),
        };

        if let Some(body) = response {
            self.send_reply(message.chat.id, &body).await?;
        }

        Ok(())
    }

    async fn status_message(&self) -> Option<String> {
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

        Some(lines.join("\n"))
    }

    async fn agents_message(&self) -> Option<String> {
        match self.fetch_agents().await {
            Ok(agents) => Some(format_agents(&agents)),
            Err(error) => Some(format!(
                "GhostTeam agents\nUnable to load registered agents from {}.\n{}",
                self.config.api_base_url,
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

    async fn telegram_get<T>(
        &self,
        method: &str,
        query: Option<&[(&str, String)]>,
    ) -> Result<T, TelegramError>
    where
        T: DeserializeOwned,
    {
        let url =
            format!("https://api.telegram.org/bot{}/{}", self.config.telegram_bot_token, method);
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
        let url =
            format!("https://api.telegram.org/bot{}/{}", self.config.telegram_bot_token, method);

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
    #[allow(dead_code)]
    id: i64,
    #[allow(dead_code)]
    creator: String,
    #[allow(dead_code)]
    assignee: Option<String>,
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    status: String,
    #[allow(dead_code)]
    result: Option<String>,
    #[allow(dead_code)]
    created_at: Option<String>,
    #[allow(dead_code)]
    updated_at: Option<String>,
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

fn normalize_command(text: &str) -> String {
    let command = text.split_whitespace().next().unwrap_or(text);
    command.split('@').next().unwrap_or(command).to_string()
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
        ApiError::Transport(message) => {
            format!("The API is unreachable: {message}")
        }
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
