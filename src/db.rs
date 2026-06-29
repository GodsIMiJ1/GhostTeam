use anyhow::{Context, Result};
use rusqlite::Connection;
use std::env;
use std::fs;
use std::path::PathBuf;

const WORKSPACE_DIR: &str = ".ghostteam";
const DATABASE_FILE: &str = "ghostteam.db";
const WORKSPACE_DIR_ENV: &str = "GHOSTTEAM_WORKSPACE_DIR";

#[derive(Debug, Clone)]
pub struct AgentRow {
    pub id: String,
    pub role: String,
    pub backend: String,
    pub joined_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub id: i64,
    pub sender: String,
    pub recipient: String,
    pub body: String,
    pub created_at: Option<String>,
    pub read: i64,
}

#[derive(Debug, Clone)]
pub struct TaskRow {
    pub id: i64,
    pub creator: String,
    pub assignee: Option<String>,
    pub description: String,
    pub status: String,
    pub result: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

pub fn open() -> Result<Connection> {
    ensure_workspace_dir()?;
    let path = database_path();
    log::debug!("opening sqlite database at {}", path.display());
    let connection = Connection::open(&path)
        .map_err(|error| {
            log::error!("failed to open sqlite database at {}: {error}", path.display());
            error
        })
        .with_context(|| format!("failed to open SQLite database at {}", path.display()))?;
    Ok(connection)
}

pub fn init_workspace() -> Result<()> {
    log::info!("initializing ghostteam workspace");
    ensure_workspace_dir()?;
    let connection = open()?;
    let schema = include_str!("../schema.sql");
    connection
        .execute_batch(schema)
        .map_err(|error| {
            log::error!("failed to initialize ghostteam schema: {error}");
            error
        })
        .context("failed to initialize GhostTeam schema")?;
    log::info!("ghostteam workspace initialized");
    Ok(())
}

fn ensure_workspace_dir() -> Result<PathBuf> {
    let path = workspace_dir();
    log::debug!("ensuring workspace directory exists at {}", path.display());
    fs::create_dir_all(&path)
        .map_err(|error| {
            log::error!(
                "failed to create workspace directory at {}: {error}",
                path.display()
            );
            error
        })
        .with_context(|| format!("failed to create workspace directory at {}", path.display()))?;
    Ok(path)
}

fn workspace_dir() -> PathBuf {
    env::var_os(WORKSPACE_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(WORKSPACE_DIR))
}

fn database_path() -> PathBuf {
    workspace_dir().join(DATABASE_FILE)
}

pub fn now_expr() -> &'static str {
    "datetime('now')"
}
