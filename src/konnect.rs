use anyhow::{Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::env;
use std::process;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4077";
const BASE_URL_ENV: &str = "GHOSTTEAM_KASPERKONNECT_URL";

#[derive(Debug, Clone)]
pub struct KasperKonnectClient {
    base_url: String,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageMetadata {
    pub trace_id: Option<String>,
    pub priority: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEnvelope {
    pub id: String,
    pub node_id: String,
    pub source_env: String,
    pub target_env: String,
    pub channel: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub payload: Value,
    pub metadata: MessageMetadata,
    pub status: String,
    pub acknowledged_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskHandoff {
    pub task_id: String,
    pub source_env: String,
    pub target_env: String,
    pub title: String,
    pub description: String,
    pub message_id: Option<String>,
    pub status: String,
    pub payload: Value,
    pub created_at: String,
    pub acknowledged_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterEnvironmentRequest {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub version: Option<String>,
    pub pid: Option<i64>,
    pub endpoint: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub bind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub version: Option<String>,
    pub pid: Option<i64>,
    pub endpoint: Option<String>,
    pub capabilities: Vec<String>,
    pub status: String,
    pub registered_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub reachable: bool,
    pub base_url: String,
    pub health: Option<HealthResponse>,
    pub environments: Vec<Environment>,
    pub registered: Vec<Environment>,
}

impl KasperKonnectClient {
    pub fn from_env() -> Option<Self> {
        if let Ok(base_url) = env::var(BASE_URL_ENV) {
            let base_url = base_url.trim();
            if !base_url.is_empty() {
                return Some(Self::new(base_url));
            }
        }

        if probe_default_runtime() {
            Some(Self::new(DEFAULT_BASE_URL))
        } else {
            None
        }
    }

    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to build KasperKonnect HTTP client"),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn register_environment(
        &self,
        id: &str,
        role: &str,
        backend: &str,
    ) -> Result<()> {
        let request = RegisterEnvironmentRequest {
            id: id.to_string(),
            display_name: format!("GhostTeam {id}"),
            kind: role.to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            pid: Some(process::id() as i64),
            endpoint: Some(format!("ghostteam://{id}")),
            capabilities: capabilities_for(role, backend),
        };

        let _: Value = self.request_json(
            Method::POST,
            "/environments/register",
            Some(serde_json::to_value(request)?),
        )?;
        Ok(())
    }

    pub fn heartbeat(&self, id: &str) -> Result<()> {
        let _: Value = self.request_json(
            Method::POST,
            "/environments/heartbeat",
            Some(json!({ "id": id })),
        )?;
        Ok(())
    }

    pub fn send_message(
        &self,
        local_id: i64,
        from: &str,
        to: &str,
        body: &str,
    ) -> Result<String> {
        let payload = json!({
            "body": body,
            "sourceAgent": from,
            "targetAgent": to,
        });
        let request_id = remote_message_id(local_id);
        let request = json!({
            "id": request_id,
            "nodeId": "ghostteam-runtime",
            "sourceEnv": from,
            "targetEnv": to,
            "channel": "message.routing",
            "type": "direct.message",
            "payload": payload,
            "metadata": message_metadata(),
        });

        let response: MessageEnvelope = self.request_json(Method::POST, "/messages", Some(request))?;
        Ok(response.id)
    }

    pub fn poll_messages(&self, target_env: &str) -> Result<Vec<MessageEnvelope>> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ResponseBody {
            messages: Vec<MessageEnvelope>,
        }

        let response: ResponseBody = self.request_json(
            Method::GET,
            &format!("/messages?targetEnv={target_env}"),
            None,
        )?;
        Ok(response.messages)
    }

    pub fn acknowledge_message(&self, message_id: &str, env_id: &str) -> Result<()> {
        let _: Value = self.request_json(
            Method::POST,
            &format!("/messages/{message_id}/ack"),
            Some(json!({ "envId": env_id })),
        )?;
        Ok(())
    }

    pub fn create_task_handoff(
        &self,
        local_id: i64,
        source_env: &str,
        target_env: &str,
        description: &str,
    ) -> Result<String> {
        let request_id = remote_task_id(local_id);
        let request = json!({
            "id": request_id,
            "sourceEnv": source_env,
            "targetEnv": target_env,
            "title": first_line(description),
            "description": description,
            "payload": {
                "ghostteamLocalTaskId": local_id,
            }
        });

        let response: TaskHandoff = self.request_json(Method::POST, "/tasks/handoff", Some(request))?;
        Ok(response.task_id)
    }

    pub fn acknowledge_task_handoff(&self, local_id: i64, env_id: &str) -> Result<()> {
        let _: Value = self.request_json(
            Method::POST,
            &format!("/tasks/handoff/{}/ack", remote_task_id(local_id)),
            Some(json!({ "envId": env_id })),
        )?;
        Ok(())
    }

    pub fn complete_task_handoff(&self, local_id: i64, env_id: &str) -> Result<()> {
        let _: Value = self.request_json(
            Method::POST,
            &format!("/tasks/handoff/{}/complete", remote_task_id(local_id)),
            Some(json!({ "envId": env_id })),
        )?;
        Ok(())
    }

    pub fn health(&self) -> Result<HealthResponse> {
        self.request_json(Method::GET, "/health", None)
    }

    pub fn list_environments(&self) -> Result<Vec<Environment>> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ResponseBody {
            environments: Vec<Environment>,
        }

        let response: ResponseBody = self.request_json(Method::GET, "/environments", None)?;
        Ok(response.environments)
    }

    pub fn runtime_status(&self, registered_ids: &[String]) -> RuntimeStatus {
        match self.health() {
            Ok(health) => {
                let environments = self.list_environments().unwrap_or_default();
                let registered = environments
                    .iter()
                    .filter(|environment| registered_ids.iter().any(|id| id == &environment.id))
                    .cloned()
                    .collect();

                RuntimeStatus {
                    reachable: true,
                    base_url: self.base_url.clone(),
                    health: Some(health),
                    environments,
                    registered,
                }
            }
            Err(error) => {
                log::debug!("KasperKonnect status probe failed at {}: {error}", self.base_url);
                RuntimeStatus {
                    reachable: false,
                    base_url: self.base_url.clone(),
                    health: None,
                    environments: Vec::new(),
                    registered: Vec::new(),
                }
            }
        }
    }

    fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut request = self.client.request(method, &url);
        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request
            .send()
            .with_context(|| format!("failed to send KasperKonnect request to {url}"))?;
        let response = response.error_for_status().with_context(|| {
            format!("KasperKonnect returned an error status from {url}")
        })?;
        decode_json(response, &url)
    }
}

fn decode_json<T: DeserializeOwned>(response: Response, url: &str) -> Result<T> {
    let text = response
        .text()
        .with_context(|| format!("failed to read KasperKonnect response from {url}"))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to decode KasperKonnect response from {url}: {text}"))
}

fn message_metadata() -> MessageMetadata {
    MessageMetadata {
        trace_id: None,
        priority: "normal".to_string(),
        created_at: now_string(),
    }
}

fn capabilities_for(role: &str, backend: &str) -> Vec<String> {
    let mut caps = vec!["workspace.task".to_string(), "runtime.context".to_string()];
    caps.push(format!("agent.role.{role}"));
    caps.push(format!("agent.backend.{backend}"));
    caps
}

fn first_line(input: &str) -> String {
    input
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or("GhostTeam task")
        .to_string()
}

fn now_string() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now.as_secs().to_string()
}

pub fn client() -> Option<KasperKonnectClient> {
    static CLIENT: OnceLock<Option<KasperKonnectClient>> = OnceLock::new();
    CLIENT.get_or_init(KasperKonnectClient::from_env).clone()
}

pub fn runtime_status(registered_ids: &[String]) -> Option<RuntimeStatus> {
    client().map(|client| client.runtime_status(registered_ids))
}

pub fn remote_message_id(local_id: i64) -> String {
    format!("ghostteam-msg-{local_id}")
}

pub fn remote_task_id(local_id: i64) -> String {
    format!("ghostteam-task-{local_id}")
}

fn probe_default_runtime() -> bool {
    let client = match Client::builder().timeout(Duration::from_millis(400)).build() {
        Ok(client) => client,
        Err(error) => {
            log::debug!("failed to build KasperKonnect probe client: {error}");
            return false;
        }
    };

    let url = format!("{}/health", DEFAULT_BASE_URL.trim_end_matches('/'));
    match client.get(&url).send() {
        Ok(response) if response.status().is_success() => true,
        Ok(response) => {
            log::debug!(
                "KasperKonnect probe at {url} returned status {}",
                response.status()
            );
            false
        }
        Err(error) => {
            log::debug!("KasperKonnect probe failed at {url}: {error}");
            false
        }
    }
}
