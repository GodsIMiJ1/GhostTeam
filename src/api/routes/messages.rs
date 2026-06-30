use axum::{
    extract::{Json, Path},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{db, tasks};

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub from: String,
    pub to: String,
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct MarkReadRequest {
    pub id: i64,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: i64,
    pub sender: String,
    pub recipient: String,
    pub body: String,
    pub created_at: Option<String>,
    pub read: i64,
}

#[derive(Debug, Serialize)]
pub struct ApiMessage<T> {
    pub ok: bool,
    pub data: T,
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_messages))
        .route("/:agent", get(list_unread_messages))
        .route("/send", post(send_message))
        .route("/mark-read", post(mark_read))
}

pub async fn list_messages() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "note": "use /messages/{agent} to fetch unread messages"
        })),
    )
        .into_response()
}

pub async fn list_unread_messages(Path(agent): Path<String>) -> impl IntoResponse {
    match unread_messages(&agent) {
        Ok(messages) => (
            StatusCode::OK,
            Json(ApiMessage {
                ok: true,
                data: messages,
            }),
        )
            .into_response(),
        Err(error) => {
            log::error!("failed to list unread messages for {agent}: {error}");
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

pub async fn send_message(Json(request): Json<SendMessageRequest>) -> impl IntoResponse {
    match tasks::send_message(&request.from, &request.to, &request.body) {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "ok": true,
                "from": request.from,
                "to": request.to
            })),
        )
            .into_response(),
        Err(error) => {
            log::error!(
                "failed to send message from {} to {}: {error}",
                request.from,
                request.to
            );
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

pub async fn mark_read(Json(request): Json<MarkReadRequest>) -> impl IntoResponse {
    match mark_message_read(request.id) {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "id": request.id
            })),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("message not found: {}", request.id)
            })),
        )
            .into_response(),
        Err(error) => {
            log::error!("failed to mark message {} read: {error}", request.id);
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

fn unread_messages(agent: &str) -> anyhow::Result<Vec<MessageResponse>> {
    let connection = db::open()?;
    let mut statement = connection.prepare(
        "SELECT id, sender, recipient, body, created_at, read
         FROM messages
         WHERE recipient = ?1 AND read = 0
         ORDER BY id ASC",
    )?;
    let rows = statement.query_map(params![agent], |row| {
        Ok(MessageResponse {
            id: row.get(0)?,
            sender: row.get(1)?,
            recipient: row.get(2)?,
            body: row.get(3)?,
            created_at: row.get(4)?,
            read: row.get(5)?,
        })
    })?;

    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(messages)
}

fn mark_message_read(id: i64) -> anyhow::Result<bool> {
    let connection = db::open()?;
    let affected = connection.execute(
        "UPDATE messages SET read = 1 WHERE id = ?1",
        params![id],
    )?;
    Ok(affected > 0)
}

#[allow(dead_code)]
fn _get_message(id: i64) -> anyhow::Result<Option<MessageResponse>> {
    let connection = db::open()?;
    let mut statement = connection.prepare(
        "SELECT id, sender, recipient, body, created_at, read
         FROM messages
         WHERE id = ?1
         LIMIT 1",
    )?;
    let row = statement
        .query_row(params![id], |row| {
            Ok(MessageResponse {
                id: row.get(0)?,
                sender: row.get(1)?,
                recipient: row.get(2)?,
                body: row.get(3)?,
                created_at: row.get(4)?,
                read: row.get(5)?,
            })
        })
        .optional()?;
    Ok(row)
}
