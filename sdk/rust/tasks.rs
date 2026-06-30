use serde::{Deserialize, Serialize};

use super::{ApiIdResponse, ApiResponse, GhostTeamClient, GhostTeamError};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    pub id: i64,
    pub creator: String,
    pub assignee: Option<String>,
    pub description: String,
    pub status: String,
    pub result: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskHistory {
    pub id: i64,
    pub task_id: i64,
    pub event: String,
    pub actor: String,
    pub at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskDetails {
    pub task: Task,
    pub history: Vec<TaskHistory>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateTaskRequest {
    pub from: String,
    pub to: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AckTaskRequest {
    pub id: i64,
    pub worker: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CompleteTaskRequest {
    pub id: i64,
    pub worker: String,
    pub result: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequeueTaskRequest {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskStatusResponse {
    pub ok: bool,
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TaskCreateResponse {
    Detailed(ApiResponse<TaskDetails>),
    Acknowledged(ApiIdResponse<i64>),
}

pub async fn list_tasks(client: &GhostTeamClient) -> Result<Vec<Task>, GhostTeamError> {
    let response: ApiResponse<Vec<Task>> = client.get_json("/tasks").await?;
    Ok(response.data)
}

pub async fn create_task(
    client: &GhostTeamClient,
    request: &CreateTaskRequest,
) -> Result<TaskCreateResponse, GhostTeamError> {
    client.post_json("/tasks/create", request).await
}

pub async fn ack_task(
    client: &GhostTeamClient,
    request: &AckTaskRequest,
) -> Result<TaskStatusResponse, GhostTeamError> {
    client.post_json("/tasks/ack", request).await
}

pub async fn complete_task(
    client: &GhostTeamClient,
    request: &CompleteTaskRequest,
) -> Result<TaskStatusResponse, GhostTeamError> {
    client.post_json("/tasks/complete", request).await
}

pub async fn requeue_task(
    client: &GhostTeamClient,
    request: &RequeueTaskRequest,
) -> Result<TaskStatusResponse, GhostTeamError> {
    client.post_json("/tasks/requeue", request).await
}

pub async fn get_task(
    client: &GhostTeamClient,
    id: i64,
) -> Result<Option<TaskDetails>, GhostTeamError> {
    let response = client
        .request(reqwest::Method::GET, &format!("/tasks/{id}"))
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

    let response: ApiResponse<TaskDetails> = serde_json::from_str(&body)?;
    Ok(Some(response.data))
}
