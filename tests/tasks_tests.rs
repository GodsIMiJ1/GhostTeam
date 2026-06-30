use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "../src/db.rs"]
mod db;
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
    fn create_task_inserts_created_row_and_history() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("task-create");
        db::init_workspace().expect("workspace initialization failed");

        let task_id = tasks::create_task("manager", "worker", "draft the report")
            .expect("failed to create task");
        assert!(task_id > 0);

        let connection = db::open().expect("failed to open db");
        let status: String = connection
            .query_row("SELECT status FROM tasks WHERE id = ?1", params![task_id], |row| row.get(0))
            .expect("failed to fetch task status");
        let history_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_history WHERE task_id = ?1",
                params![task_id],
                |row| row.get(0),
            )
            .expect("failed to count history rows");

        assert_eq!(status, "created");
        assert_eq!(history_count, 1);
    }

    #[test]
    fn ack_task_updates_status_and_records_history() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("task-ack");
        db::init_workspace().expect("workspace initialization failed");

        let task_id = tasks::create_task("manager", "worker", "draft the report")
            .expect("failed to create task");
        tasks::ack_task(task_id, "worker").expect("failed to ack task");

        let connection = db::open().expect("failed to open db");
        let status: String = connection
            .query_row("SELECT status FROM tasks WHERE id = ?1", params![task_id], |row| row.get(0))
            .expect("failed to fetch task status");
        let history_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_history WHERE task_id = ?1",
                params![task_id],
                |row| row.get(0),
            )
            .expect("failed to count history rows");

        assert_eq!(status, "acked");
        assert_eq!(history_count, 2);
    }

    #[test]
    fn complete_task_updates_result_and_records_history() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("task-complete");
        db::init_workspace().expect("workspace initialization failed");

        let task_id = tasks::create_task("manager", "worker", "draft the report")
            .expect("failed to create task");
        tasks::ack_task(task_id, "worker").expect("failed to ack task");
        tasks::complete_task(task_id, "worker", "done").expect("failed to complete task");

        let connection = db::open().expect("failed to open db");
        let status: String = connection
            .query_row("SELECT status FROM tasks WHERE id = ?1", params![task_id], |row| row.get(0))
            .expect("failed to fetch task status");
        let result: String = connection
            .query_row("SELECT result FROM tasks WHERE id = ?1", params![task_id], |row| row.get(0))
            .expect("failed to fetch task result");
        let history_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_history WHERE task_id = ?1",
                params![task_id],
                |row| row.get(0),
            )
            .expect("failed to count history rows");

        assert_eq!(status, "completed");
        assert_eq!(result, "done");
        assert_eq!(history_count, 3);
    }

    #[test]
    fn requeue_task_sets_status_and_records_history() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("task-requeue");
        db::init_workspace().expect("workspace initialization failed");

        let task_id = tasks::create_task("manager", "worker", "draft the report")
            .expect("failed to create task");
        tasks::requeue_task(task_id).expect("failed to requeue task");

        let connection = db::open().expect("failed to open db");
        let status: String = connection
            .query_row("SELECT status FROM tasks WHERE id = ?1", params![task_id], |row| row.get(0))
            .expect("failed to fetch task status");
        let history_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_history WHERE task_id = ?1",
                params![task_id],
                |row| row.get(0),
            )
            .expect("failed to count history rows");

        assert_eq!(status, "requeued");
        assert_eq!(history_count, 2);
    }

    #[test]
    fn list_tasks_returns_saved_rows() {
        let _guard = TEST_LOCK.lock().expect("test lock poisoned");
        let (_root, _env) = prepare_workspace("task-list");
        db::init_workspace().expect("workspace initialization failed");

        let task_id = tasks::create_task("manager", "worker", "draft the report")
            .expect("failed to create task");
        tasks::ack_task(task_id, "worker").expect("failed to ack task");

        let rows = tasks::list_tasks().expect("failed to list tasks");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, task_id);
        assert_eq!(rows[0].status, "acked");
        assert_eq!(rows[0].creator, "manager");
    }
}
