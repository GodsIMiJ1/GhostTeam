use serde::{Deserialize, Serialize};

use super::{ApiIdResponse, ApiResponse, GhostTeamClient, GhostTeamError};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Agent {
    pub id: String,
    pub role: String,
    pub backend: String,
    pub joined_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JoinAgentRequest {
    pub id: String,
    pub role: String,
    pub backend: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LeaveAgentRequest {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LeaveAgentResponse {
    pub ok: bool,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum JoinAgentResponse {
    Detailed(ApiResponse<Agent>),
    Acknowledged(ApiIdResponse<String>),
}

pub type AgentJoinResponse = JoinAgentResponse;

pub async fn list_agents(client: &GhostTeamClient) -> Result<Vec<Agent>, GhostTeamError> {
    let response: ApiResponse<Vec<Agent>> = client.get_json("/agents").await?;
    Ok(response.data)
}

pub async fn join_agent(
    client: &GhostTeamClient,
    request: &JoinAgentRequest,
) -> Result<JoinAgentResponse, GhostTeamError> {
    client.post_json("/agents/join", request).await
}

pub async fn leave_agent(
    client: &GhostTeamClient,
    request: &LeaveAgentRequest,
) -> Result<LeaveAgentResponse, GhostTeamError> {
    client.post_json("/agents/leave", request).await
}

pub async fn get_agent(
    client: &GhostTeamClient,
    id: &str,
) -> Result<Option<Agent>, GhostTeamError> {
    let response = client
        .request(reqwest::Method::GET, &format!("/agents/{id}"))
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;

    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !status.is_success() {
        return Err(GhostTeamError::Api { status, body });
    }

    let response: ApiResponse<Agent> = serde_json::from_str(&body)?;
    Ok(Some(response.data))
}
