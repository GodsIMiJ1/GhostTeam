use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use criterion::{Criterion, black_box, criterion_group, criterion_main};

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

static BENCH_LOCK: Mutex<()> = Mutex::new(());

struct WorkspaceEnv {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
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
        Self { reply: reply.to_string(), prompts: Arc::new(Mutex::new(Vec::new())) }
    }
}

impl model::ModelBackend for MockBackend {
    fn generate(&self, prompt: &str) -> anyhow::Result<String> {
        self.prompts.lock().expect("prompt log poisoned").push(prompt.to_string());
        Ok(self.reply.clone())
    }
}

fn unique_workspace(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    env::temp_dir().join(format!("ghostteam-bench-{label}-{stamp}"))
}

fn prepare_workspace() -> (PathBuf, WorkspaceEnv) {
    let root = unique_workspace("agent");
    fs::create_dir_all(root.join("roles")).expect("failed to create roles dir");
    fs::create_dir_all(root.join("teams")).expect("failed to create teams dir");
    fs::write(root.join("roles").join("worker.md"), "# worker\nWorker role")
        .expect("failed to write role");
    (root.clone(), WorkspaceEnv::set(&root))
}

fn bench_message_polling(c: &mut Criterion) {
    let _guard = BENCH_LOCK.lock().expect("benchmark lock poisoned");
    let (_root, _env) = prepare_workspace();
    db::init_workspace().expect("failed to init workspace");
    agent::join_agent("worker", "worker", "ghostos").expect("failed to join worker");

    c.bench_function("message polling", |b| {
        b.iter(|| {
            let connection = db::open().expect("failed to open db");
            connection
                .execute(
                    "INSERT INTO messages (sender, recipient, body, created_at, read) VALUES (?1, ?2, ?3, datetime('now'), 0)",
                    rusqlite::params!["manager", "worker", "benchmark message"],
                )
                .expect("failed to seed message");
            let messages = agent::receive_messages(black_box("worker"), black_box(false))
                .expect("receive_messages failed");
            black_box(messages.len())
        })
    });
}

fn bench_task_processing(c: &mut Criterion) {
    let _guard = BENCH_LOCK.lock().expect("benchmark lock poisoned");
    let (_root, _env) = prepare_workspace();
    db::init_workspace().expect("failed to init workspace");

    c.bench_function("task processing", |b| {
        b.iter(|| {
            let task_id = tasks::create_task("manager", "worker", "benchmark task")
                .expect("failed to create task");
            tasks::ack_task(task_id, "worker").expect("failed to ack task");
            tasks::complete_task(task_id, "worker", "done").expect("failed to complete task");
            black_box(task_id)
        })
    });
}

fn bench_model_backend_calls(c: &mut Criterion) {
    let backend = MockBackend::new("mock reply");
    c.bench_function("mock model backend calls", |b| {
        b.iter(|| {
            let reply =
                backend.generate(black_box("benchmark prompt")).expect("mock backend failed");
            black_box(reply)
        })
    });
}

criterion_group!(benches, bench_message_polling, bench_task_processing, bench_model_backend_calls);
criterion_main!(benches);
