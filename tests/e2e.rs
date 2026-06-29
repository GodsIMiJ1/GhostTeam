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
#[path = "../src/tasks.rs"]
mod tasks;

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

#[derive(Clone)]
struct MockBackend {
    reply: String,
    prompts: Arc<Mutex<Vec<String>>>,
}

impl MockBackend {
    fn new(reply: &str) -> Self {
        Self {
            reply: reply.to_string(),
            prompts: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl model::ModelBackend for MockBackend {
    fn generate(&self, prompt: &str) -> anyhow::Result<String> {
        self.prompts
            .lock()
            .expect("prompt log poisoned")
            .push(prompt.to_string());
        Ok(self.reply.clone())
    }
}

fn unique_workspace() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    env::temp_dir().join(format!("ghostteam-e2e-{stamp}"))
}

fn prepare_workspace() -> (PathBuf, WorkspaceEnv) {
    let root = unique_workspace();
    fs::create_dir_all(root.join("roles")).expect("failed to create roles directory");
    fs::create_dir_all(root.join("teams")).expect("failed to create teams directory");
    let env_guard = WorkspaceEnv::set(&root);
    (root, env_guard)
}

fn write_role_file(root: &Path, role: &str, body: &str) {
    let path = root.join("roles").join(format!("{role}.md"));
    fs::write(path, body).expect("failed to write role file");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn end_to_end_collaboration_flow_works_offline() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (root, _env) = prepare_workspace();

        write_role_file(&root, "manager", "# manager\nManage the team.");
        write_role_file(&root, "worker", "# worker\nExecute assigned work.");
        write_role_file(&root, "inspector", "# inspector\nReview outcomes.");

        db::init_workspace().expect("failed to initialize workspace");

        let manager_id = agent::join_agent("manager", "manager", "ghostos")
            .expect("failed to join manager");
        let worker_id = agent::join_agent("worker", "worker", "ghostos")
            .expect("failed to join worker");
        let inspector_id = agent::join_agent("inspector", "inspector", "ghostos")
            .expect("failed to join inspector");

        assert_eq!(manager_id, "manager");
        assert_eq!(worker_id, "worker");
        assert_eq!(inspector_id, "inspector");

        agent::send_message(&manager_id, &worker_id, "Please reply with status.")
            .expect("failed to send manager message");

        let backend = MockBackend::new("worker reply: acknowledged");
        let processed = agent::process_inbox_once(&worker_id, "worker", &backend)
            .expect("failed to process worker inbox");
        assert_eq!(processed, 1);

        let prompts = backend.prompts.lock().expect("prompt log poisoned");
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("Please reply with status."));
        assert!(prompts[0].contains("worker"));

        let manager_messages = agent::receive_messages(&manager_id, false)
            .expect("failed to receive manager messages");
        assert_eq!(manager_messages.len(), 1);
        assert_eq!(manager_messages[0].sender, worker_id);
        assert_eq!(manager_messages[0].recipient, manager_id);
        assert!(manager_messages[0].body.contains("worker reply: acknowledged"));

        let task_id = tasks::create_task(&manager_id, &worker_id, "Write the status report")
            .expect("failed to create task");
        assert!(task_id > 0);

        tasks::ack_task(task_id, &worker_id).expect("failed to ack task");
        tasks::complete_task(task_id, &worker_id, "status report ready")
            .expect("failed to complete task");

        let task_rows = tasks::list_tasks().expect("failed to list tasks");
        assert_eq!(task_rows.len(), 1);
        assert_eq!(task_rows[0].status, "completed");
        assert_eq!(task_rows[0].result.as_deref(), Some("status report ready"));

        let connection = db::open().expect("failed to open db");
        let history_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_history WHERE task_id = ?1",
                params![task_id],
                |row| row.get(0),
            )
            .expect("failed to count task history");
        assert_eq!(history_count, 3);

        let inspector_view = tasks::list_tasks().expect("inspector failed to review tasks");
        assert_eq!(inspector_view[0].id, task_id);
        assert_eq!(inspector_view[0].creator, manager_id);
        assert_eq!(inspector_view[0].assignee.as_deref(), Some(worker_id.as_str()));
    }
}
