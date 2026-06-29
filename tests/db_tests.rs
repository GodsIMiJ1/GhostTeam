use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "../src/db.rs"]
mod db;

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
    fs::create_dir_all(&root).expect("failed to create temp workspace");
    (root.clone(), WorkspaceEnv::set(&root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn init_workspace_creates_directory_and_schema() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (root, _env) = prepare_workspace("init");

        db::init_workspace().expect("workspace initialization failed");

        assert!(root.exists());
        assert!(root.join("ghostteam.db").exists());

        let connection = db::open().expect("failed to open workspace db");
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("failed to prepare schema query");
        let tables = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("failed to query sqlite_master")
            .map(|row| row.expect("failed to read schema row"))
            .collect::<Vec<_>>();

        assert!(tables.contains(&"agents".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"tasks".to_string()));
        assert!(tables.contains(&"task_history".to_string()));
    }

    #[test]
    fn schema_accepts_agent_message_and_task_rows() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("rows");

        db::init_workspace().expect("workspace initialization failed");
        let connection = db::open().expect("failed to open workspace db");

        connection
            .execute(
                "INSERT INTO agents (id, role, backend, joined_at) VALUES (?1, ?2, ?3, datetime('now'))",
                params!["worker", "worker", "ollama"],
            )
            .expect("failed to insert agent");
        connection
            .execute(
                "INSERT INTO messages (sender, recipient, body, created_at, read) VALUES (?1, ?2, ?3, datetime('now'), 0)",
                params!["manager", "worker", "hello"],
            )
            .expect("failed to insert message");
        connection
            .execute(
                "INSERT INTO tasks (creator, assignee, description, status, result, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, datetime('now'), datetime('now'))",
                params!["manager", "worker", "document the plan", "created"],
            )
            .expect("failed to insert task");

        let agent_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM agents", [], |row| row.get(0))
            .expect("failed to count agents");
        let message_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .expect("failed to count messages");
        let task_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .expect("failed to count tasks");

        assert_eq!(agent_count, 1);
        assert_eq!(message_count, 1);
        assert_eq!(task_count, 1);
    }
}
