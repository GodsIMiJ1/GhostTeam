use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "../src/agent.rs"]
mod agent;
#[path = "../src/db.rs"]
mod db;
#[path = "../src/model/mod.rs"]
mod model;
#[path = "../src/roles.rs"]
mod roles;

static TEST_LOCK: Mutex<()> = Mutex::new(());

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

fn unique_workspace(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    env::temp_dir().join(format!("ghostteam-{label}-{stamp}"))
}

fn prepare_workspace(label: &str) -> (PathBuf, WorkspaceEnv) {
    let root = unique_workspace(label);
    fs::create_dir_all(root.join("roles")).expect("failed to create roles directory");
    (root.clone(), WorkspaceEnv::set(&root))
}

fn write_role_files(root: &Path) {
    fs::write(root.join("roles").join("manager.md"), "# manager\nmanager role prompt")
        .expect("failed to write manager role");
    fs::write(root.join("roles").join("worker.md"), "# worker\nworker role prompt")
        .expect("failed to write worker role");
    fs::write(root.join("roles").join("inspector.md"), "# inspector\ninspector role prompt")
        .expect("failed to write inspector role");
}

#[derive(Clone, Default)]
struct MockBackend {
    prompts: Arc<Mutex<Vec<String>>>,
}

impl model::ModelBackend for MockBackend {
    fn generate(&self, prompt: &str) -> anyhow::Result<String> {
        self.prompts.lock().expect("prompt log poisoned").push(prompt.to_string());
        Ok(format!("mock-reply::{prompt}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn join_auto_suffixes_existing_ids() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (root, _env) = prepare_workspace("join");
        write_role_files(&root);
        db::init_workspace().expect("workspace initialization failed");

        let first = agent::join_agent("worker", "worker", "ollama").expect("first join failed");
        let second = agent::join_agent("worker", "worker", "ollama").expect("second join failed");
        let third = agent::join_agent("worker", "worker", "ollama").expect("third join failed");

        assert_eq!(first, "worker");
        assert_eq!(second, "worker-2");
        assert_eq!(third, "worker-3");

        let connection = db::open().expect("failed to open db");
        let mut statement = connection
            .prepare("SELECT id FROM agents ORDER BY id ASC")
            .expect("failed to prepare query");
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("failed to query agents")
            .map(|row| row.expect("failed to read agent row"))
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["worker", "worker-2", "worker-3"]);
    }

    #[test]
    fn leave_removes_agent() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (root, _env) = prepare_workspace("leave");
        write_role_files(&root);
        db::init_workspace().expect("workspace initialization failed");

        agent::join_agent("worker", "worker", "ollama").expect("join failed");
        agent::leave_agent("worker").expect("leave failed");

        let connection = db::open().expect("failed to open db");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM agents", [], |row| row.get(0))
            .expect("failed to count agents");
        assert_eq!(count, 0);
    }

    #[test]
    fn run_loop_polls_messages_and_generates_replies_once() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (root, _env) = prepare_workspace("loop");
        write_role_files(&root);
        db::init_workspace().expect("workspace initialization failed");

        let connection = db::open().expect("failed to open db");
        connection
            .execute(
                "INSERT INTO messages (sender, recipient, body, created_at, read) VALUES (?1, ?2, ?3, datetime('now'), 0)",
                params!["manager", "worker", "please review the plan"],
            )
            .expect("failed to seed message");

        let backend = MockBackend::default();
        let processed =
            agent::process_inbox_once("worker", "worker", &backend).expect("poll failed");
        assert_eq!(processed, 1);

        let prompts = backend.prompts.lock().expect("prompt log poisoned");
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("worker role prompt"));
        assert!(prompts[0].contains("please review the plan"));

        let original_read: i64 = connection
            .query_row(
                "SELECT read FROM messages WHERE sender = ?1 AND recipient = ?2",
                params!["manager", "worker"],
                |row| row.get(0),
            )
            .expect("failed to query original message");
        assert_eq!(original_read, 1);

        let reply_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE sender = ?1 AND recipient = ?2",
                params!["worker", "manager"],
                |row| row.get(0),
            )
            .expect("failed to count replies");
        assert_eq!(reply_count, 1);

        let reply_body: String = connection
            .query_row(
                "SELECT body FROM messages WHERE sender = ?1 AND recipient = ?2",
                params!["worker", "manager"],
                |row| row.get(0),
            )
            .expect("failed to fetch reply body");
        assert!(reply_body.starts_with("mock-reply::"));
    }
}
