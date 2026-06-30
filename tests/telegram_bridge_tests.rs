use std::{collections::VecDeque, sync::Arc};

use axum::{
    Json, Router,
    body::Body,
    extract::ws::{Message, WebSocketUpgrade},
    http::{HeaderMap, Request, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use ghostteam::telegram_bridge::{BridgeConfig, TelegramBridge};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{net::TcpListener, sync::Mutex};

#[derive(Default)]
struct TelegramState {
    updates: VecDeque<TelegramUpdate>,
    sent_messages: Vec<TelegramSendMessageRequest>,
}

#[derive(Default)]
struct ApiState {
    agents: serde_json::Value,
    agent_detail: serde_json::Value,
    tasks: serde_json::Value,
    messages: serde_json::Value,
    task_creates: Vec<TaskCreateRequest>,
    log_lines: Vec<String>,
    api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramMessage>,
}

#[derive(Debug, Clone, Serialize)]
struct TelegramMessage {
    message_id: i64,
    chat: TelegramChat,
    text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TelegramChat {
    id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct TelegramSendMessageRequest {
    chat_id: i64,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskCreateRequest {
    from: String,
    to: String,
    description: String,
}

#[tokio::test]
async fn status_command_reports_api_summary() {
    let telegram = spawn_telegram_mock(TelegramState {
        updates: VecDeque::from(vec![update(1, "/status")]),
        sent_messages: Vec::new(),
    })
    .await;
    let api = spawn_api_mock(ApiState {
        agents: json!({
            "ok": true,
            "data": [
                {"id": "manager", "role": "manager", "backend": "ollama", "joined_at": "2026-06-29T00:00:00Z"},
                {"id": "worker-1", "role": "worker", "backend": "ghostos", "joined_at": "2026-06-29T00:01:00Z"}
            ]
        }),
        agent_detail: json!({
            "ok": true,
            "data": {
                "id": "manager",
                "role": "manager",
                "backend": "ollama",
                "joined_at": "2026-06-29T00:00:00Z"
            }
        }),
        tasks: json!({
            "ok": true,
            "data": [
                {
                    "id": 7,
                    "creator": "manager",
                    "assignee": "worker-1",
                    "description": "Draft the status report",
                    "status": "created",
                    "result": null,
                    "created_at": "2026-06-29T00:02:00Z",
                    "updated_at": "2026-06-29T00:02:00Z"
                }
            ]
        }),
        messages: json!({"ok": true, "data": []}),
        task_creates: Vec::new(),
        log_lines: vec!["ready".to_string()],
        api_key: Some("test-key".to_string()),
    })
    .await;

    let bridge = test_bridge(&telegram.base_url, &api.base_url, Some("test-key".to_string()));
    bridge.run_once(0).await.expect("status command should succeed");

    let sent = telegram.state.lock().await.sent_messages.clone();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].text.contains("GhostTeam status"));
    assert!(sent[0].text.contains("API base URL:"));
    assert!(sent[0].text.contains("Agents: reachable"));
    assert!(sent[0].text.contains("Tasks: reachable"));
}

#[tokio::test]
async fn tasks_command_lists_registered_tasks() {
    let telegram = spawn_telegram_mock(TelegramState {
        updates: VecDeque::from(vec![update(1, "/tasks")]),
        sent_messages: Vec::new(),
    })
    .await;
    let api = spawn_api_mock(ApiState {
        agents: json!({"ok": true, "data": []}),
        agent_detail: json!({"ok": true, "data": {"id": "manager", "role": "manager", "backend": "ollama", "joined_at": null}}),
        tasks: json!({
            "ok": true,
            "data": [
                {
                    "id": 7,
                    "creator": "manager",
                    "assignee": "worker-1",
                    "description": "Draft the status report",
                    "status": "created",
                    "result": null,
                    "created_at": "2026-06-29T00:02:00Z",
                    "updated_at": "2026-06-29T00:02:00Z"
                },
                {
                    "id": 8,
                    "creator": "manager",
                    "assignee": "worker-2",
                    "description": "Review the handoff",
                    "status": "acked",
                    "result": null,
                    "created_at": "2026-06-29T00:03:00Z",
                    "updated_at": "2026-06-29T00:04:00Z"
                }
            ]
        }),
        messages: json!({"ok": true, "data": []}),
        task_creates: Vec::new(),
        log_lines: Vec::new(),
        api_key: Some("test-key".to_string()),
    })
    .await;

    let bridge = test_bridge(&telegram.base_url, &api.base_url, Some("test-key".to_string()));
    bridge.run_once(0).await.expect("tasks command should succeed");

    let sent = telegram.state.lock().await.sent_messages.clone();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].text.contains("GhostTeam tasks (2)"));
    assert!(sent[0].text.contains("#7"));
    assert!(sent[0].text.contains("Draft the status report"));
    assert!(sent[0].text.contains("#8"));
}

#[tokio::test]
async fn assign_and_handoff_create_tasks() {
    let telegram = spawn_telegram_mock(TelegramState {
        updates: VecDeque::from(vec![
            update(1, "/assign kodii \"fix dashboard bug\""),
            update(2, "/handoff axiom \"review this plan\""),
        ]),
        sent_messages: Vec::new(),
    })
    .await;
    let api = spawn_api_mock(ApiState {
        agents: json!({"ok": true, "data": []}),
        agent_detail: json!({"ok": true, "data": {"id": "manager", "role": "manager", "backend": "ollama", "joined_at": null}}),
        tasks: json!({"ok": true, "data": []}),
        messages: json!({"ok": true, "data": []}),
        task_creates: Vec::new(),
        log_lines: Vec::new(),
        api_key: Some("test-key".to_string()),
    })
    .await;

    let bridge = test_bridge(&telegram.base_url, &api.base_url, Some("test-key".to_string()));
    bridge.run_once(0).await.expect("first command should succeed");
    bridge.run_once(2).await.expect("second command should succeed");

    let creates = api.state.lock().await.task_creates.clone();
    assert_eq!(creates.len(), 2);
    assert_eq!(creates[0].from, "telegram");
    assert_eq!(creates[0].to, "kodii");
    assert_eq!(creates[0].description, "fix dashboard bug");
    assert_eq!(creates[1].to, "axiom");
    assert!(creates[1].description.contains("Handoff via Telegram"));
    assert!(creates[1].description.contains("review this plan"));

    let sent = telegram.state.lock().await.sent_messages.clone();
    assert_eq!(sent.len(), 2);
    assert!(sent[0].text.contains("Task assigned"));
    assert!(sent[0].text.contains("kodii"));
    assert!(sent[1].text.contains("Task handoff queued"));
    assert!(sent[1].text.contains("axiom"));
}

#[tokio::test]
async fn logs_command_streams_recent_lines() {
    let telegram = spawn_telegram_mock(TelegramState {
        updates: VecDeque::from(vec![update(1, "/logs omari")]),
        sent_messages: Vec::new(),
    })
    .await;
    let api = spawn_api_mock(ApiState {
        agents: json!({"ok": true, "data": []}),
        agent_detail: json!({"ok": true, "data": {"id": "manager", "role": "manager", "backend": "ollama", "joined_at": null}}),
        tasks: json!({"ok": true, "data": []}),
        messages: json!({"ok": true, "data": []}),
        task_creates: Vec::new(),
        log_lines: vec!["omari: boot complete".to_string(), "omari: awaiting work".to_string()],
        api_key: Some("test-key".to_string()),
    })
    .await;

    let bridge = test_bridge(&telegram.base_url, &api.base_url, Some("test-key".to_string()));
    bridge.run_once(0).await.expect("logs command should succeed");

    let sent = telegram.state.lock().await.sent_messages.clone();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].text.contains("GhostTeam logs for omari"));
    assert!(sent[0].text.contains("boot complete"));
    assert!(sent[0].text.contains("awaiting work"));
}

#[tokio::test]
async fn help_command_lists_supported_commands() {
    let telegram = spawn_telegram_mock(TelegramState {
        updates: VecDeque::from(vec![update(1, "/help")]),
        sent_messages: Vec::new(),
    })
    .await;
    let api = spawn_api_mock(ApiState {
        agents: json!({"ok": true, "data": []}),
        agent_detail: json!({"ok": true, "data": {"id": "manager", "role": "manager", "backend": "ollama", "joined_at": null}}),
        tasks: json!({"ok": true, "data": []}),
        messages: json!({"ok": true, "data": []}),
        task_creates: Vec::new(),
        log_lines: Vec::new(),
        api_key: Some("test-key".to_string()),
    })
    .await;

    let bridge = test_bridge(&telegram.base_url, &api.base_url, Some("test-key".to_string()));
    bridge.run_once(0).await.expect("help command should succeed");

    let sent = telegram.state.lock().await.sent_messages.clone();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].text.contains("/help"));
    assert!(sent[0].text.contains("/status <agent>"));
    assert!(sent[0].text.contains("/handoff <agent>"));
}

#[tokio::test]
async fn agent_status_command_reports_agent_details() {
    let telegram = spawn_telegram_mock(TelegramState {
        updates: VecDeque::from(vec![update(1, "/status omari")]),
        sent_messages: Vec::new(),
    })
    .await;
    let api = spawn_api_mock(ApiState {
        agents: json!({
            "ok": true,
            "data": [
                {"id": "omari", "role": "overseer", "backend": "ollama", "joined_at": "2026-06-29T00:05:00Z"}
            ]
        }),
        agent_detail: json!({
            "ok": true,
            "data": {
                "id": "omari",
                "role": "overseer",
                "backend": "ollama",
                "joined_at": "2026-06-29T00:05:00Z"
            }
        }),
        tasks: json!({
            "ok": true,
            "data": [
                {
                    "id": 41,
                    "creator": "manager",
                    "assignee": "omari",
                    "description": "Fix dashboard bug",
                    "status": "acked",
                    "result": null,
                    "created_at": "2026-06-29T00:07:00Z",
                    "updated_at": "2026-06-29T00:08:00Z"
                }
            ]
        }),
        messages: json!({
            "ok": true,
            "data": [
                {
                    "id": 18,
                    "sender": "manager",
                    "recipient": "omari",
                    "body": "Please check the dashboard bug",
                    "created_at": "2026-06-29T00:09:00Z",
                    "read": 0
                }
            ]
        }),
        task_creates: Vec::new(),
        log_lines: vec!["omari: boot complete".to_string()],
        api_key: Some("test-key".to_string()),
    })
    .await;

    let bridge = test_bridge(&telegram.base_url, &api.base_url, Some("test-key".to_string()));
    bridge.run_once(0).await.expect("agent status command should succeed");

    let sent = telegram.state.lock().await.sent_messages.clone();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].text.contains("GhostTeam agent status for omari"));
    assert!(sent[0].text.contains("Role: overseer"));
    assert!(sent[0].text.contains("Unread messages: 1"));
    assert!(sent[0].text.contains("Assigned tasks: 1"));
    assert!(sent[0].text.contains("Please check the dashboard bug"));
}

fn test_bridge(
    telegram_base_url: &str,
    api_base_url: &str,
    api_key: Option<String>,
) -> TelegramBridge {
    let config = BridgeConfig::for_test(
        "test-token",
        telegram_base_url.to_string(),
        api_base_url.to_string(),
        api_key,
    );
    TelegramBridge::new(config).expect("bridge should construct")
}

async fn spawn_telegram_mock(state: TelegramState) -> TelegramMockHandle {
    let state = Arc::new(Mutex::new(state));
    let app = Router::new().fallback({
        let state = state.clone();
        move |request| telegram_dispatch(state.clone(), request)
    });
    spawn_server(app, state).await
}

async fn spawn_api_mock(state: ApiState) -> ApiMockHandle {
    let state = Arc::new(Mutex::new(state));
    let app = Router::new()
        .route(
            "/agents",
            get({
                let state = state.clone();
                move |headers| api_agents(state.clone(), headers)
            }),
        )
        .route(
            "/agents/{id}",
            get({
                let state = state.clone();
                move |headers| api_agent(state.clone(), headers)
            }),
        )
        .route(
            "/tasks",
            get({
                let state = state.clone();
                move |headers| api_tasks(state.clone(), headers)
            }),
        )
        .route(
            "/messages/{agent}",
            get({
                let state = state.clone();
                move |headers| api_messages(state.clone(), headers)
            }),
        )
        .route(
            "/tasks/create",
            post({
                let state = state.clone();
                move |headers, payload| api_create_task(state.clone(), headers, payload)
            }),
        )
        .route(
            "/logs/{agent}/stream",
            get({
                let state = state.clone();
                move |headers, ws| api_logs_stream(state.clone(), headers, ws)
            }),
        );
    spawn_server(app, state).await
}

async fn spawn_server<T>(app: Router, state: Arc<Mutex<T>>) -> MockServerHandle<T> {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock server");
    let addr = listener.local_addr().expect("mock server addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("mock server should run");
    });

    MockServerHandle { base_url: format!("http://{addr}"), state }
}

async fn telegram_dispatch(
    state: Arc<Mutex<TelegramState>>,
    request: Request<Body>,
) -> impl IntoResponse {
    let path = request.uri().path().to_string();
    match (request.method().as_str(), path.as_str()) {
        ("GET", path) if path.ends_with("/getMe") => Json(json!({
            "ok": true,
            "result": {
                "username": "ghostteam-bot"
            }
        }))
        .into_response(),
        ("GET", path) if path.ends_with("/getUpdates") => {
            let mut guard = state.lock().await;
            let updates: Vec<_> = guard.updates.drain(..).collect();
            Json(json!({
                "ok": true,
                "result": updates
            }))
            .into_response()
        }
        ("POST", path) if path.ends_with("/sendMessage") => {
            let body_bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
                .await
                .expect("read sendMessage body");
            let payload: TelegramSendMessageRequest =
                serde_json::from_slice(&body_bytes).expect("parse sendMessage body");
            let mut guard = state.lock().await;
            guard.sent_messages.push(payload.clone());
            Json(json!({
                "ok": true,
                "result": {
                    "message_id": guard.sent_messages.len() as i64,
                    "chat": {
                        "id": payload.chat_id
                    },
                    "text": payload.text
                }
            }))
            .into_response()
        }
        _ => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn api_agents(state: Arc<Mutex<ApiState>>, headers: HeaderMap) -> impl IntoResponse {
    assert_api_key(&state, &headers).await;
    Json(state.lock().await.agents.clone()).into_response()
}

async fn api_tasks(state: Arc<Mutex<ApiState>>, headers: HeaderMap) -> impl IntoResponse {
    assert_api_key(&state, &headers).await;
    Json(state.lock().await.tasks.clone()).into_response()
}

async fn api_agent(state: Arc<Mutex<ApiState>>, headers: HeaderMap) -> impl IntoResponse {
    assert_api_key(&state, &headers).await;
    Json(state.lock().await.agent_detail.clone()).into_response()
}

async fn api_messages(state: Arc<Mutex<ApiState>>, headers: HeaderMap) -> impl IntoResponse {
    assert_api_key(&state, &headers).await;
    Json(state.lock().await.messages.clone()).into_response()
}

async fn api_create_task(
    state: Arc<Mutex<ApiState>>,
    headers: HeaderMap,
    Json(request): Json<TaskCreateRequest>,
) -> impl IntoResponse {
    assert_api_key(&state, &headers).await;
    let mut guard = state.lock().await;
    guard.task_creates.push(request);
    let id = (guard.task_creates.len() as i64) + 40;
    Json(json!({
        "ok": true,
        "id": id,
        "data": {
            "id": id
        }
    }))
    .into_response()
}

async fn api_logs_stream(
    state: Arc<Mutex<ApiState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    assert_api_key(&state, &headers).await;
    let lines = state.lock().await.log_lines.clone();
    ws.on_upgrade(move |mut socket| async move {
        for line in lines {
            if socket.send(Message::Text(line.into())).await.is_err() {
                break;
            }
        }
        let _ = socket.send(Message::Close(None)).await;
    })
}

async fn assert_api_key(state: &Arc<Mutex<ApiState>>, headers: &HeaderMap) {
    let expected = state.lock().await.api_key.clone();
    if let Some(expected) = expected {
        let observed = headers
            .get("X-GhostTeam-Key")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(observed, expected, "bridge must forward X-GhostTeam-Key");
    }
}

fn update(update_id: i64, text: &str) -> TelegramUpdate {
    TelegramUpdate {
        update_id,
        message: Some(TelegramMessage {
            message_id: update_id,
            chat: TelegramChat { id: 42 },
            text: Some(text.to_string()),
        }),
    }
}

struct MockServerHandle<T> {
    base_url: String,
    state: Arc<Mutex<T>>,
}

type TelegramMockHandle = MockServerHandle<TelegramState>;
type ApiMockHandle = MockServerHandle<ApiState>;
