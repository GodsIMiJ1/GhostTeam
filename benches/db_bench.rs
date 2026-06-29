use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use criterion::{black_box, criterion_group, criterion_main, Criterion};

#[path = "../src/db.rs"]
mod db;

static BENCH_LOCK: Mutex<()> = Mutex::new(());

struct WorkspaceEnv {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl WorkspaceEnv {
    fn set(path: &std::path::Path) -> Self {
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

fn unique_workspace() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    env::temp_dir().join(format!("ghostteam-bench-db-{stamp}"))
}

fn prepare_workspace() -> (PathBuf, WorkspaceEnv) {
    let root = unique_workspace();
    fs::create_dir_all(&root).expect("failed to create workspace");
    (root.clone(), WorkspaceEnv::set(&root))
}

fn bench_db_reads_writes(c: &mut Criterion) {
    let _guard = BENCH_LOCK.lock().expect("benchmark lock poisoned");
    let (_root, _env) = prepare_workspace();
    db::init_workspace().expect("failed to init workspace");

    let connection = db::open().expect("failed to open db");

    c.bench_function("db write agent", |b| {
        b.iter(|| {
            connection
                .execute(
                    "INSERT INTO agents (id, role, backend, joined_at) VALUES (?1, ?2, ?3, datetime('now'))",
                    rusqlite::params!["bench-agent", "worker", "ghostos"],
                )
                .expect("failed to insert agent");
            connection
                .execute("DELETE FROM agents WHERE id = ?1", rusqlite::params!["bench-agent"])
                .expect("failed to delete agent");
        })
    });

    connection
        .execute(
            "INSERT INTO agents (id, role, backend, joined_at) VALUES (?1, ?2, ?3, datetime('now'))",
            rusqlite::params!["bench-agent", "worker", "ghostos"],
        )
        .expect("failed to seed agent");

    c.bench_function("db read agent", |b| {
        b.iter(|| {
            let role: String = connection
                .query_row(
                    "SELECT role FROM agents WHERE id = ?1",
                    rusqlite::params!["bench-agent"],
                    |row| row.get(0),
                )
                .expect("failed to read agent");
            black_box(role)
        })
    });
}

criterion_group!(benches, bench_db_reads_writes);
criterion_main!(benches);
