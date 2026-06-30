use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::http::Request as WsRequest};

#[path = "../src/agent.rs"]
mod agent;
#[path = "../src/api/mod.rs"]
mod api;
#[path = "../src/db.rs"]
mod db;
#[path = "../src/model/mod.rs"]
mod model;
#[path = "../src/roles.rs"]
mod roles;
#[path = "../src/tasks.rs"]
mod tasks;

static TEST_LOCK: Mutex<()> = Mutex::new(());
const API_KEY: &str = "abc123";

#[derive(Clone)]
struct WorkspaceEnv {
    key: &'static str,
    previous: Option<OsString>,
}

impl WorkspaceEnv {
    fn set(path: &Path) -> Self {
        let key = "GHOSTTEAM_WORKSPACE_DIR";
        let previous = env::var_os(key);
        unsafe {
            env::set_var(key, path);
        }
        Self { key, previous }
    }
}

impl Drop for WorkspaceEnv {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(previous) => env::set_var(self.key, previous),
                None => env::remove_var(self.key),
            }
        }
    }
}

#[derive(Clone)]
struct MockGhostOsState {
    received: Arc<Mutex<Option<Value>>>,
    output: String,
}

fn unique_workspace(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    env::temp_dir().join(format!("ghostteam-api-{label}-{stamp}"))
}

fn prepare_workspace(label: &str) -> (PathBuf, WorkspaceEnv) {
    let root = unique_workspace(label);
    fs::create_dir_all(root.join(".ghostteam/logs")).expect("failed to create logs directory");
    let api_keys = root.join(".ghostteam/api_keys.yaml");
    fs::write(
        &api_keys,
        "keys:\n  - abc123\n  - xyz789\n",
    )
    .expect("failed to write api keys");
    let config = root.join(".ghostteam/config.yaml");
    fs::write(
        &config,
        "ghostos_endpoint: \"http://127.0.0.1:0/infer\"\nghostos_model: \"ghost-1\"\n",
    )
    .expect("failed to write config");
    let env_guard = WorkspaceEnv::set(&root);
    (root, env_guard)
}

async fn spawn_router_server(router: Router) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test server");
    let addr = listener.local_addr().expect("failed to read local addr");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    (addr, handle)
}

async fn wait_for_api(base_url: &str, client: &Client) {
    for _ in 0..50 {
        match client
            .get(format!("{base_url}/agents"))
            .header("X-GhostTeam-Key", API_KEY)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => return,
            _ => sleep(Duration::from_millis(50)).await,
        }
    }
    panic!("API server did not become ready");
}

async fn spawn_api_server() -> (String, tokio::task::JoinHandle<()>) {
    let router = api::server::build_router();
    let (addr, handle) = spawn_router_server(router).await;
    (format!("http://{addr}"), handle)
}

async fn spawn_mock_ghostos() -> (String, Arc<Mutex<Option<Value>>>, tokio::task::JoinHandle<()>) {
    let received = Arc::new(Mutex::new(None));
    let output = "mock ghostos output".to_string();
    let state = MockGhostOsState {
        received: Arc::clone(&received),
        output: output.clone(),
    };

    async fn infer_handler(
        State(state): State<MockGhostOsState>,
        Json(payload): Json<Value>,
    ) -> Json<Value> {
        *state.received.lock().expect("ghostos state poisoned") = Some(payload);
        Json(json!({ "output": state.output }))
    }

    let router = Router::new()
        .route("/infer", post(infer_handler))
        .with_state(state);
    let (addr, handle) = spawn_router_server(router).await;
    (format!("http://{addr}/infer"), received, handle)
}

fn auth_client() -> Client {
    Client::builder()
        .build()
        .expect("failed to build reqwest client")
}

struct ServerHandle(tokio::task::JoinHandle<()>);

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn append_log_line(root: &Path, agent: &str, line: &str) {
    let log_path = root.join(".ghostteam/logs").join(format!("{agent}.log"));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("failed to open log file");
    writeln!(file, "{line}").expect("failed to append log line");
    file.flush().expect("failed to flush log file");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn agents_join_and_list_work_through_the_api() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (root, _env) = prepare_workspace("agents");
        db::init_workspace().expect("failed to initialize workspace");

        let (base_url, handle) = spawn_api_server().await;
        let _server = ServerHandle(handle);
        let client = auth_client();
        wait_for_api(&base_url, &client).await;

        let join_response: Value = client
            .post(format!("{base_url}/agents/join"))
            .header("X-GhostTeam-Key", API_KEY)
            .json(&json!({
                "id": "manager",
                "role": "manager",
                "backend": "ghostos"
            }))
            .send()
            .await
            .expect("join request failed")
            .json()
            .await
            .expect("failed to decode join response");

        assert!(join_response["ok"].as_bool().unwrap_or(false));
        assert_eq!(join_response["data"]["id"], "manager");
        assert_eq!(join_response["data"]["role"], "manager");

        let list_response: Value = client
            .get(format!("{base_url}/agents"))
            .header("X-GhostTeam-Key", API_KEY)
            .send()
            .await
            .expect("list request failed")
            .json()
            .await
            .expect("failed to decode agent list");

        assert!(list_response["ok"].as_bool().unwrap_or(false));
        assert_eq!(list_response["data"].as_array().map(|items| items.len()), Some(1));
        assert_eq!(list_response["data"][0]["id"], "manager");

        drop(root);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn messages_send_and_unread_lookup_work() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("messages");
        db::init_workspace().expect("failed to initialize workspace");

        let (base_url, handle) = spawn_api_server().await;
        let _server = ServerHandle(handle);
        let client = auth_client();
        wait_for_api(&base_url, &client).await;

        let send_response: Value = client
            .post(format!("{base_url}/messages/send"))
            .header("X-GhostTeam-Key", API_KEY)
            .json(&json!({
                "from": "manager",
                "to": "worker",
                "body": "hello worker"
            }))
            .send()
            .await
            .expect("send request failed")
            .json()
            .await
            .expect("failed to decode send response");

        assert!(send_response["ok"].as_bool().unwrap_or(false));
        assert_eq!(send_response["from"], "manager");
        assert_eq!(send_response["to"], "worker");

        let unread_response: Value = client
            .get(format!("{base_url}/messages/worker"))
            .header("X-GhostTeam-Key", API_KEY)
            .send()
            .await
            .expect("unread request failed")
            .json()
            .await
            .expect("failed to decode unread response");

        assert!(unread_response["ok"].as_bool().unwrap_or(false));
        assert_eq!(unread_response["data"].as_array().map(|items| items.len()), Some(1));
        assert_eq!(unread_response["data"][0]["body"], "hello worker");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tasks_create_work_through_the_api() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("tasks");
        db::init_workspace().expect("failed to initialize workspace");

        let (base_url, handle) = spawn_api_server().await;
        let _server = ServerHandle(handle);
        let client = auth_client();
        wait_for_api(&base_url, &client).await;

        let create_response: Value = client
            .post(format!("{base_url}/tasks/create"))
            .header("X-GhostTeam-Key", API_KEY)
            .json(&json!({
                "from": "manager",
                "to": "worker",
                "description": "write the report"
            }))
            .send()
            .await
            .expect("create request failed")
            .json()
            .await
            .expect("failed to decode create response");

        assert!(create_response["ok"].as_bool().unwrap_or(false));
        assert!(create_response["data"]["task"]["id"].as_i64().unwrap_or(0) > 0);
        assert_eq!(create_response["data"]["task"]["description"], "write the report");

        let list_response: Value = client
            .get(format!("{base_url}/tasks"))
            .header("X-GhostTeam-Key", API_KEY)
            .send()
            .await
            .expect("task list request failed")
            .json()
            .await
            .expect("failed to decode task list");

        assert!(list_response["ok"].as_bool().unwrap_or(false));
        assert_eq!(list_response["data"].as_array().map(|items| items.len()), Some(1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ghostos_infer_passthrough_works() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("ghostos");
        let (ghostos_endpoint, received, ghostos_handle) = spawn_mock_ghostos().await;
        let _ghostos_server = ServerHandle(ghostos_handle);

        let config_path = _root.join(".ghostteam/config.yaml");
        fs::write(
            &config_path,
            format!(
                "ghostos_endpoint: \"{ghostos_endpoint}\"\nghostos_model: \"ghost-1\"\n"
            ),
        )
        .expect("failed to write ghostos config");

        db::init_workspace().expect("failed to initialize workspace");

        let (base_url, handle) = spawn_api_server().await;
        let _server = ServerHandle(handle);
        let client = auth_client();
        wait_for_api(&base_url, &client).await;

        let infer_response: Value = client
            .post(format!("{base_url}/ghostos/infer"))
            .header("X-GhostTeam-Key", API_KEY)
            .json(&json!({
                "prompt": "hello ghostos"
            }))
            .send()
            .await
            .expect("infer request failed")
            .json()
            .await
            .expect("failed to decode infer response");

        assert_eq!(infer_response["output"], "mock ghostos output");

        let received_payload = received.lock().expect("received payload poisoned").clone();
        assert!(received_payload.is_some());
        let payload = received_payload.expect("missing ghostos payload");
        assert_eq!(payload["model"], "ghost-1");
        assert_eq!(payload["prompt"], "hello ghostos");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn websocket_log_stream_emits_new_lines() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (root, _env) = prepare_workspace("logs");
        db::init_workspace().expect("failed to initialize workspace");

        let (base_url, handle) = spawn_api_server().await;
        let _server = ServerHandle(handle);
        let client = auth_client();
        wait_for_api(&base_url, &client).await;

        let agent = "manager";
        let log_path = root.join(".ghostteam/logs").join(format!("{agent}.log"));
        fs::write(&log_path, "").expect("failed to create log file");

        let ws_request = WsRequest::builder()
            .uri(format!(
                "ws://{}{}",
                base_url.trim_start_matches("http://"),
                format!("/logs/{agent}/stream")
            ))
            .header("X-GhostTeam-Key", API_KEY)
            .body(())
            .expect("failed to build websocket request");

        let (mut ws_stream, _) = connect_async(ws_request)
            .await
            .expect("websocket connect failed");

        append_log_line(&root, agent, "first streamed line");

        let next_message = timeout(Duration::from_secs(5), ws_stream.next())
            .await
            .expect("timed out waiting for websocket message");

        let message = next_message
            .expect("websocket stream closed unexpectedly")
            .expect("websocket message error");

        assert_eq!(message.to_text().expect("message was not text"), "first streamed line");
    }
}
