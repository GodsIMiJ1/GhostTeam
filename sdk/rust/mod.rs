pub mod agents;
pub mod ghostos;
pub mod messages;
pub mod tasks;

use reqwest::{Client, Method, Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone)]
pub struct GhostTeamClient {
    base_url: String,
    api_key: Option<String>,
    http: Client,
}

#[derive(Debug, Error)]
pub enum GhostTeamError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("api error {status}: {body}")]
    Api { status: StatusCode, body: String },
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: T,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiIdResponse<T> {
    pub ok: bool,
    pub id: T,
    pub note: Option<String>,
    pub warning: Option<String>,
}

impl GhostTeamClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, GhostTeamError> {
        Ok(Self {
            base_url: normalize_base_url(base_url.into()),
            api_key: None,
            http: Client::new(),
        })
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn set_api_key(&mut self, api_key: impl Into<String>) {
        self.api_key = Some(api_key.into());
    }

    pub fn clear_api_key(&mut self) {
        self.api_key = None;
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    pub(crate) fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let mut request = self.http.request(method, self.endpoint(path));
        if let Some(api_key) = &self.api_key {
            request = request.header("X-GhostTeam-Key", api_key);
        }
        request
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, GhostTeamError> {
        let response = self.request(Method::GET, path).send().await?;
        self.decode_json(response).await
    }

    pub(crate) async fn post_json<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, GhostTeamError> {
        let response = self.request(Method::POST, path).json(body).send().await?;
        self.decode_json(response).await
    }

    async fn decode_json<T: DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T, GhostTeamError> {
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(GhostTeamError::Api { status, body });
        }

        Ok(serde_json::from_str(&body)?)
    }
}

fn normalize_base_url(base_url: String) -> String {
    base_url.trim_end_matches('/').to_string()
}

pub use agents::{
    Agent,
    AgentJoinResponse,
    JoinAgentRequest,
    JoinAgentResponse,
    LeaveAgentRequest,
    LeaveAgentResponse,
};
pub use ghostos::{GhostOsInferRequest, GhostOsInferResponse};
pub use messages::{
    MarkReadRequest,
    Message,
    MessageReadResponse,
    MessageSendResponse,
    SendMessageRequest,
};
pub use tasks::{
    AckTaskRequest,
    CompleteTaskRequest,
    CreateTaskRequest,
    Task,
    TaskCreateResponse,
    TaskDetails,
    TaskHistory,
    TaskStatusResponse,
};
