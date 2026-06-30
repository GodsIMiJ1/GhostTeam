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
pub struct CreateTaskRequest {
    pub from: String,
    pub to: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct AckTaskRequest {
    pub id: i64,
    pub worker: String,
}

#[derive(Debug, Deserialize)]
pub struct CompleteTaskRequest {
    pub id: i64,
    pub worker: String,
    pub result: String,
}

#[derive(Debug, Deserialize)]
pub struct RequeueTaskRequest {
    pub id: i64,
}

#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: i64,
    pub creator: String,
    pub assignee: Option<String>,
    pub description: String,
    pub status: String,
    pub result: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskHistoryResponse {
    pub id: i64,
    pub task_id: i64,
    pub event: String,
    pub actor: String,
    pub at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaskDetailsResponse {
    pub task: TaskResponse,
    pub history: Vec<TaskHistoryResponse>,
}

#[derive(Debug, Serialize)]
pub struct ApiMessage<T> {
    pub ok: bool,
    pub data: T,
}

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_tasks))
        .route("/create", post(create_task))
        .route("/ack", post(ack_task))
        .route("/complete", post(complete_task))
        .route("/requeue", post(requeue_task))
        .route("/:id", get(get_task))
}

pub async fn list_tasks() -> impl IntoResponse {
    match load_all_tasks() {
        Ok(tasks) => (
            StatusCode::OK,
            Json(ApiMessage {
                ok: true,
                data: tasks,
            }),
        )
            .into_response(),
        Err(error) => {
            log::error!("failed to list tasks: {error}");
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

pub async fn create_task(Json(request): Json<CreateTaskRequest>) -> impl IntoResponse {
    match tasks::create_task(&request.from, &request.to, &request.description) {
        Ok(id) => match get_task_detail(id) {
            Ok(Some(task)) => (
                StatusCode::CREATED,
                Json(ApiMessage { ok: true, data: task }),
            )
                .into_response(),
            Ok(None) => (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "ok": true,
                    "id": id
                })),
            )
                .into_response(),
            Err(error) => {
                log::error!("created task {id} but failed to fetch detail: {error}");
                (
                    StatusCode::CREATED,
                    Json(serde_json::json!({
                        "ok": true,
                        "id": id,
                        "warning": error.to_string()
                    })),
                )
                    .into_response()
            }
        },
        Err(error) => {
            log::error!("failed to create task from {} to {}: {error}", request.from, request.to);
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

pub async fn ack_task(Json(request): Json<AckTaskRequest>) -> impl IntoResponse {
    match tasks::ack_task(request.id, &request.worker) {
        Ok(()) => task_status_response(request.id, StatusCode::OK),
        Err(error) => task_error("ack", request.id, error),
    }
}

pub async fn complete_task(Json(request): Json<CompleteTaskRequest>) -> impl IntoResponse {
    match tasks::complete_task(request.id, &request.worker, &request.result) {
        Ok(()) => task_status_response(request.id, StatusCode::OK),
        Err(error) => task_error("complete", request.id, error),
    }
}

pub async fn requeue_task(Json(request): Json<RequeueTaskRequest>) -> impl IntoResponse {
    match tasks::requeue_task(request.id) {
        Ok(()) => task_status_response(request.id, StatusCode::OK),
        Err(error) => task_error("requeue", request.id, error),
    }
}

pub async fn get_task(Path(id): Path<i64>) -> impl IntoResponse {
    match get_task_detail(id) {
        Ok(Some(task)) => (
            StatusCode::OK,
            Json(ApiMessage {
                ok: true,
                data: task,
            }),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("task not found: {id}")
            })),
        )
            .into_response(),
        Err(error) => {
            log::error!("failed to fetch task {id}: {error}");
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

fn task_status_response(id: i64, status: StatusCode) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({
            "ok": true,
            "id": id
        })),
    )
        .into_response()
}

fn task_error(action: &str, id: i64, error: anyhow::Error) -> axum::response::Response {
    log::error!("failed to {action} task {id}: {error}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "ok": false,
            "error": error.to_string()
        })),
    )
        .into_response()
}

fn load_all_tasks() -> anyhow::Result<Vec<TaskResponse>> {
    let connection = db::open()?;
    let mut statement = connection.prepare(
        "SELECT id, creator, assignee, description, status, result, created_at, updated_at
         FROM tasks
         ORDER BY id DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(TaskResponse {
            id: row.get(0)?,
            creator: row.get(1)?,
            assignee: row.get(2)?,
            description: row.get(3)?,
            status: row.get(4)?,
            result: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }
    Ok(tasks)
}

fn get_task_detail(id: i64) -> anyhow::Result<Option<TaskDetailsResponse>> {
    let connection = db::open()?;
    let task = connection
        .prepare(
            "SELECT id, creator, assignee, description, status, result, created_at, updated_at
             FROM tasks
             WHERE id = ?1
             LIMIT 1",
        )?
        .query_row(params![id], |row| {
            Ok(TaskResponse {
                id: row.get(0)?,
                creator: row.get(1)?,
                assignee: row.get(2)?,
                description: row.get(3)?,
                status: row.get(4)?,
                result: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .optional()?;

    let Some(task) = task else {
        return Ok(None);
    };

    let history = load_task_history(&connection, id)?;
    Ok(Some(TaskDetailsResponse { task, history }))
}

fn load_task_history(
    connection: &rusqlite::Connection,
    id: i64,
) -> anyhow::Result<Vec<TaskHistoryResponse>> {
    let mut statement = connection.prepare(
        "SELECT id, task_id, event, actor, at
         FROM task_history
         WHERE task_id = ?1
         ORDER BY id ASC",
    )?;
    let rows = statement.query_map(params![id], |row| {
        Ok(TaskHistoryResponse {
            id: row.get(0)?,
            task_id: row.get(1)?,
            event: row.get(2)?,
            actor: row.get(3)?,
            at: row.get(4)?,
        })
    })?;

    let mut history = Vec::new();
    for row in rows {
        history.push(row?);
    }
    Ok(history)
}

pub fn load_tasks() -> anyhow::Result<()> {
    Ok(())
}
