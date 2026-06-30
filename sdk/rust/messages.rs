use serde::{Deserialize, Serialize};

use super::{GhostTeamClient, GhostTeamError};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub id: i64,
    pub sender: String,
    pub recipient: String,
    pub body: String,
    pub created_at: Option<String>,
    pub read: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendMessageRequest {
    pub from: String,
    pub to: String,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarkReadRequest {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageSendResponse {
    pub ok: bool,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageReadResponse {
    pub ok: bool,
    pub id: i64,
}

pub async fn get_unread_messages(
    client: &GhostTeamClient,
    agent: &str,
) -> Result<Vec<Message>, GhostTeamError> {
    let response: super::ApiResponse<Vec<Message>> = client
        .get_json(&format!("/messages/{agent}"))
        .await?;
    Ok(response.data)
}

pub async fn send_message(
    client: &GhostTeamClient,
    request: &SendMessageRequest,
) -> Result<MessageSendResponse, GhostTeamError> {
    client.post_json("/messages/send", request).await
}

pub async fn mark_read(
    client: &GhostTeamClient,
    request: &MarkReadRequest,
) -> Result<MessageReadResponse, GhostTeamError> {
    client.post_json("/messages/mark-read", request).await
}
