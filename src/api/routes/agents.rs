use axum::{
    extract::{Json, Path},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{agent, db};

#[derive(Debug, Deserialize)]
pub struct JoinAgentRequest {
    pub id: String,
    pub role: String,
    pub backend: String,
}

#[derive(Debug, Deserialize)]
pub struct LeaveAgentRequest {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub id: String,
    pub role: String,
    pub backend: String,
    pub joined_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiMessage<T> {
    pub ok: bool,
    pub data: T,
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_agents))
        .route("/join", post(join_agent))
        .route("/leave", post(leave_agent))
        .route("/:id", get(get_agent))
}

pub async fn list_agents() -> impl IntoResponse {
    match agent::list_agents() {
        Ok(agents) => {
            let payload = agents
                .into_iter()
                .map(AgentResponse::from)
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(ApiMessage { ok: true, data: payload })).into_response()
        }
        Err(error) => {
            log::error!("failed to list agents: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": error.to_string()
                })),
            )
                .into_response()
        }
    }
}

pub async fn join_agent(Json(request): Json<JoinAgentRequest>) -> impl IntoResponse {
    match agent::join_agent(&request.id, &request.role, &request.backend) {
        Ok(final_id) => match get_agent_row(&final_id) {
            Ok(Some(agent)) => (
                StatusCode::CREATED,
                Json(ApiMessage {
                    ok: true,
                    data: agent,
                }),
            )
                .into_response(),
            Ok(None) => (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "ok": true,
                    "id": final_id,
                    "note": "agent joined"
                })),
            )
                .into_response(),
            Err(error) => {
                log::error!("joined agent {final_id} but failed to read back record: {error}");
                (
                    StatusCode::CREATED,
                    Json(serde_json::json!({
                        "ok": true,
                        "id": final_id,
                        "warning": error.to_string()
                    })),
                )
                    .into_response()
            }
        },
        Err(error) => {
            log::error!("failed to join agent {}: {error}", request.id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": error.to_string()
                })),
            )
                .into_response()
        }
    }
}

pub async fn leave_agent(Json(request): Json<LeaveAgentRequest>) -> impl IntoResponse {
    match agent::leave_agent(&request.id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "id": request.id
            })),
        )
            .into_response(),
        Err(error) => {
            log::error!("failed to remove agent {}: {error}", request.id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": error.to_string()
                })),
            )
                .into_response()
        }
    }
}

pub async fn get_agent(Path(id): Path<String>) -> impl IntoResponse {
    match get_agent_row(&id) {
        Ok(Some(agent)) => (StatusCode::OK, Json(ApiMessage { ok: true, data: agent })).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("agent not found: {id}")
            })),
        )
            .into_response(),
        Err(error) => {
            log::error!("failed to fetch agent {id}: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": error.to_string()
                })),
            )
                .into_response()
        }
    }
}

impl From<db::AgentRow> for AgentResponse {
    fn from(value: db::AgentRow) -> Self {
        Self {
            id: value.id,
            role: value.role,
            backend: value.backend,
            joined_at: value.joined_at,
        }
    }
}

fn get_agent_row(id: &str) -> anyhow::Result<Option<AgentResponse>> {
    let connection = db::open()?;
    let mut statement = connection.prepare(
        "SELECT id, role, backend, joined_at
         FROM agents
         WHERE id = ?1
         LIMIT 1",
    )?;

    let row = statement
        .query_row(params![id], |row| {
            Ok(AgentResponse {
                id: row.get(0)?,
                role: row.get(1)?,
                backend: row.get(2)?,
                joined_at: row.get(3)?,
            })
        })
        .optional()?;

    Ok(row)
}
