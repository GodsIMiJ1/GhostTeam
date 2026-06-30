use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
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

#[derive(Debug, Clone)]
pub struct IdMappingRow {
    pub entity_kind: String,
    pub local_id: String,
    pub remote_id: String,
    pub remote_source: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IdMappingHistoryRow {
    pub entity_kind: String,
    pub local_id: String,
    pub remote_id: String,
    pub remote_source: Option<String>,
    pub action: String,
    pub recorded_at: Option<String>,
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
            log::error!("failed to create workspace directory at {}: {error}", path.display());
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

pub fn record_id_mapping(
    entity_kind: &str,
    local_id: impl ToString,
    remote_id: &str,
    remote_source: Option<&str>,
) -> Result<()> {
    let local_id = local_id.to_string();
    let connection = open()?;
    connection
        .execute(
            "INSERT INTO konnect_id_mappings (
                entity_kind, local_id, remote_id, remote_source, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))
            ON CONFLICT(entity_kind, local_id)
             DO UPDATE SET
                remote_id = excluded.remote_id,
                remote_source = excluded.remote_source,
                updated_at = datetime('now')",
            rusqlite::params![entity_kind, local_id.as_str(), remote_id, remote_source],
        )
        .map_err(|error| {
            log::error!(
                "failed to record id mapping kind={entity_kind} local_id={} remote_id={remote_id}: {error}",
                local_id
            );
            error
        })?;
    append_mapping_history(entity_kind, &local_id, remote_id, remote_source, "upsert")?;
    Ok(())
}

pub fn lookup_remote_id(entity_kind: &str, local_id: impl ToString) -> Result<Option<String>> {
    let connection = open()?;
    let local_id = local_id.to_string();
    let remote_id = connection
        .query_row(
            "SELECT remote_id FROM konnect_id_mappings WHERE entity_kind = ?1 AND local_id = ?2 LIMIT 1",
            rusqlite::params![entity_kind, local_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            log::error!(
                "failed to lookup remote id kind={entity_kind} local_id={local_id}: {error}"
            );
            error
        })?;
    Ok(remote_id)
}

pub fn lookup_local_id(entity_kind: &str, remote_id: &str) -> Result<Option<String>> {
    let connection = open()?;
    let local_id = connection
        .query_row(
            "SELECT local_id FROM konnect_id_mappings WHERE entity_kind = ?1 AND remote_id = ?2 LIMIT 1",
            rusqlite::params![entity_kind, remote_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            log::error!(
                "failed to lookup local id kind={entity_kind} remote_id={remote_id}: {error}"
            );
            error
        })?;
    Ok(local_id)
}

pub fn lookup_mapping_by_remote(
    entity_kind: &str,
    remote_id: &str,
) -> Result<Option<IdMappingRow>> {
    let connection = open()?;
    let mapping = connection
        .query_row(
            "SELECT entity_kind, local_id, remote_id, remote_source, created_at, updated_at
             FROM konnect_id_mappings
             WHERE entity_kind = ?1 AND remote_id = ?2
             LIMIT 1",
            rusqlite::params![entity_kind, remote_id],
            |row| {
                Ok(IdMappingRow {
                    entity_kind: row.get(0)?,
                    local_id: row.get(1)?,
                    remote_id: row.get(2)?,
                    remote_source: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            log::error!(
                "failed to lookup mapping kind={entity_kind} remote_id={remote_id}: {error}"
            );
            error
        })?;
    Ok(mapping)
}

pub fn list_id_mappings() -> Result<Vec<IdMappingRow>> {
    let connection = open()?;
    let mut statement = connection.prepare(
        "SELECT entity_kind, local_id, remote_id, remote_source, created_at, updated_at
         FROM konnect_id_mappings
         ORDER BY entity_kind ASC, local_id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(IdMappingRow {
            entity_kind: row.get(0)?,
            local_id: row.get(1)?,
            remote_id: row.get(2)?,
            remote_source: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    })?;

    let mut mappings = Vec::new();
    for row in rows {
        mappings.push(row?);
    }
    Ok(mappings)
}

pub fn list_id_mapping_history() -> Result<Vec<IdMappingHistoryRow>> {
    let connection = open()?;
    let mut statement = connection.prepare(
        "SELECT entity_kind, local_id, remote_id, remote_source, action, recorded_at
         FROM konnect_id_mapping_history
         ORDER BY recorded_at ASC, id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(IdMappingHistoryRow {
            entity_kind: row.get(0)?,
            local_id: row.get(1)?,
            remote_id: row.get(2)?,
            remote_source: row.get(3)?,
            action: row.get(4)?,
            recorded_at: row.get(5)?,
        })
    })?;

    let mut history = Vec::new();
    for row in rows {
        history.push(row?);
    }
    Ok(history)
}

fn append_mapping_history(
    entity_kind: &str,
    local_id: &str,
    remote_id: &str,
    remote_source: Option<&str>,
    action: &str,
) -> Result<()> {
    let connection = open()?;
    connection
        .execute(
            "INSERT INTO konnect_id_mapping_history (
                entity_kind, local_id, remote_id, remote_source, action, recorded_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            rusqlite::params![entity_kind, local_id, remote_id, remote_source, action],
        )
        .map_err(|error| {
            log::error!(
                "failed to append mapping history kind={entity_kind} local_id={local_id} remote_id={remote_id}: {error}"
            );
            error
        })?;
    Ok(())
}
