use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use std::thread;
use std::time::Duration;

use crate::db::{self, AgentRow, MessageRow};
use crate::konnect;
use crate::model::{self, BackendKind};
use crate::roles;

pub fn join_agent(id: &str, role: &str, backend: &str) -> Result<String> {
    log::info!("joining agent id={id} role={role} backend={backend}");
    let connection = db::open()?;
    let final_id = allocate_agent_id(&connection, id)?;
    if final_id != id {
        log::debug!("auto-suffixed agent id from {id} to {final_id}");
    }
    connection.execute(
        "INSERT INTO agents (id, role, backend, joined_at) VALUES (?1, ?2, ?3, datetime('now'))",
        params![final_id, role, backend],
    ).map_err(|error| {
        log::error!("failed to insert agent {final_id}: {error}");
        error
    })?;
    if let Some(client) = konnect::client() {
        if let Err(error) = client.register_environment(&final_id, role, backend) {
            log::warn!("failed to register agent {final_id} with KasperKonnect: {error}");
        }
    }
    log::info!("agent joined id={final_id} role={role} backend={backend}");
    Ok(final_id)
}

pub fn leave_agent(id: &str) -> Result<()> {
    log::info!("leaving agent id={id}");
    let connection = db::open()?;
    connection
        .execute("DELETE FROM agents WHERE id = ?1", params![id])
        .map_err(|error| {
            log::error!("failed to delete agent {id}: {error}");
            error
        })?;
    if konnect::client().is_some() {
        log::debug!("agent {id} left local workspace; KasperKonnect remains stateful via heartbeat");
    }
    log::info!("agent removed id={id}");
    Ok(())
}

pub fn list_agents() -> Result<Vec<AgentRow>> {
    let connection = db::open()?;
    let mut statement = connection.prepare(
        "SELECT id, role, backend, joined_at
         FROM agents
         ORDER BY joined_at ASC, id ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(AgentRow {
            id: row.get(0)?,
            role: row.get(1)?,
            backend: row.get(2)?,
            joined_at: row.get(3)?,
        })
    })?;

    let mut agents = Vec::new();
    for row in rows {
        agents.push(row?);
    }
    Ok(agents)
}

pub fn send_message(from: &str, to: &str, message: &str) -> Result<()> {
    log::debug!("sending message from={from} to={to} bytes={}", message.len());
    let connection = db::open()?;
    connection.execute(
        "INSERT INTO messages (sender, recipient, body, created_at, read)
         VALUES (?1, ?2, ?3, datetime('now'), 0)",
        params![from, to, message],
    ).map_err(|error| {
        log::error!("failed to insert message from {from} to {to}: {error}");
        error
    })?;
    let local_id = connection.last_insert_rowid();
    if let Some(client) = konnect::client() {
        match client.send_message(local_id, from, to, message) {
            Ok(remote_id) => {
                if let Err(error) = db::record_id_mapping(
                    "message",
                    local_id,
                    &remote_id,
                    Some(client.base_url()),
                ) {
                    log::warn!(
                        "failed to persist message mapping local_id={local_id} remote_id={remote_id}: {error}"
                    );
                }
            }
            Err(error) => {
                log::warn!("failed to mirror message {local_id} to KasperKonnect: {error}");
            }
        }
    }
    Ok(())
}

pub fn receive_messages(id: &str, wait: bool) -> Result<Vec<MessageRow>> {
    loop {
        let messages = load_inbox_messages(id)?;
        log::debug!("message polling id={id} unread={}", messages.len());

        if !messages.is_empty() {
            log_received_messages(&messages);
            return Ok(messages);
        }

        if !wait {
            return Ok(messages);
        }

        thread::sleep(Duration::from_millis(500));
    }
}

pub fn run_loop(id: &str, role: &str, backend: &str) -> Result<()> {
    log::info!("starting agent loop id={id} role={role} backend={backend}");
    let backend_kind = BackendKind::parse(backend)?;
    let backend = model::backend_for(backend_kind);

    loop {
        log::debug!("polling inbox id={id} role={role}");
        process_inbox_once(id, role, backend.as_ref())?;
        if let Some(client) = konnect::client() {
            if let Err(error) = client.heartbeat(id) {
                log::debug!("failed to heartbeat KasperKonnect for {id}: {error}");
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

pub fn process_inbox_once(id: &str, role: &str, backend: &dyn model::ModelBackend) -> Result<usize> {
    let role_prompt = roles::load_role(role)?;
    let messages = load_inbox_messages(id)?;
    log::debug!("inbox poll id={id} unread_messages={}", messages.len());
    let connection = db::open()?;

    for message in &messages {
        let prompt = build_prompt(&role_prompt, id, message);
        log::debug!(
            "backend call id={id} backend_prompt_bytes={} message_id={}",
            prompt.len(),
            message.id
        );
        let reply = backend.generate(&prompt)?;
        log::debug!(
            "backend response id={id} message_id={} reply_bytes={}",
            message.id,
            reply.len()
        );
        connection.execute(
            "INSERT INTO messages (sender, recipient, body, created_at, read)
             VALUES (?1, ?2, ?3, datetime('now'), 0)",
            params![id, message.sender, reply],
        ).map_err(|error| {
            log::error!(
                "failed to insert generated reply for message {} from {}: {error}",
                message.id,
                message.sender
            );
            error
        })?;
        let reply_id = connection.last_insert_rowid();
        if let Some(client) = konnect::client() {
            match client.send_message(reply_id, id, &message.sender, &reply) {
                Ok(remote_id) => {
                    if let Err(error) = db::record_id_mapping(
                        "message",
                        reply_id,
                        &remote_id,
                        Some(client.base_url()),
                    ) {
                        log::warn!(
                            "failed to persist reply mapping local_id={reply_id} remote_id={remote_id}: {error}"
                        );
                    }
                }
                Err(error) => {
                    log::warn!(
                        "failed to mirror reply {reply_id} from {id} to {}: {error}",
                        message.sender
                    );
                }
            }
        }
    }

    if !messages.is_empty() {
        mark_messages_read(&connection, &messages)?;
    }

    Ok(messages.len())
}

fn allocate_agent_id(connection: &rusqlite::Connection, requested: &str) -> Result<String> {
    let mut candidate = requested.to_string();
    let mut suffix = 2;

    while agent_exists(connection, &candidate)? {
        candidate = format!("{requested}-{suffix}");
        suffix += 1;
    }

    Ok(candidate)
}

fn agent_exists(connection: &rusqlite::Connection, id: &str) -> Result<bool> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM agents WHERE id = ?1 LIMIT 1",
            params![id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists)
}

fn unread_messages(connection: &rusqlite::Connection, recipient: &str) -> Result<Vec<MessageRow>> {
    let mut statement = connection.prepare(
        "SELECT id, sender, recipient, body, created_at, read
         FROM messages
         WHERE recipient = ?1 AND read = 0
         ORDER BY id ASC",
    )?;
    let rows = statement.query_map(params![recipient], |row| {
        Ok(MessageRow {
            id: row.get(0)?,
            sender: row.get(1)?,
            recipient: row.get(2)?,
            body: row.get(3)?,
            created_at: row.get(4)?,
            read: row.get(5)?,
        })
    })?;

    let mut messages = Vec::new();
    for row in rows {
        messages.push(row?);
    }
    Ok(messages)
}

fn load_inbox_messages(id: &str) -> Result<Vec<MessageRow>> {
    let connection = db::open()?;
    let mut messages = unread_messages(&connection, id)?;

    if messages.is_empty() {
        sync_remote_messages(id)?;
        let refreshed = db::open()?;
        messages = unread_messages(&refreshed, id)?;
    }

    Ok(messages)
}

fn sync_remote_messages(id: &str) -> Result<()> {
    let Some(client) = konnect::client() else {
        return Ok(());
    };

    let messages = match client.poll_messages(id) {
        Ok(messages) => messages,
        Err(error) => {
            log::debug!("kasperkonnect poll failed for {id}: {error}");
            return Ok(());
        }
    };

    if messages.is_empty() {
        return Ok(());
    }

    let connection = db::open()?;
    for message in messages {
        if let Some(mapping) = db::lookup_mapping_by_remote("message", &message.id)? {
            log::debug!(
                "skipping already imported KasperKonnect message {} mapped to local {} for {id}",
                mapping.remote_id,
                mapping.local_id
            );
            if let Err(error) = client.acknowledge_message(&message.id, id) {
                log::warn!(
                    "failed to acknowledge already-mapped KasperKonnect message {} for {id}: {error}",
                    message.id
                );
            }
            continue;
        }

        connection.execute(
            "INSERT INTO messages (sender, recipient, body, created_at, read)
             VALUES (?1, ?2, ?3, datetime('now'), 0)",
            params![message.source_env, id, extract_message_body(&message)],
        ).map_err(|error| {
            log::error!(
                "failed to import KasperKonnect message {} for {id}: {error}",
                message.id
            );
            error
        })?;
        let local_id = connection.last_insert_rowid();
        if let Err(error) = db::record_id_mapping(
            "message",
            local_id,
            &message.id,
            Some(client.base_url()),
        ) {
            log::warn!(
                "failed to persist imported message mapping local_id={local_id} remote_id={}: {error}",
                message.id
            );
        }
        if let Err(error) = client.acknowledge_message(&message.id, id) {
            log::warn!("failed to acknowledge KasperKonnect message {} for {id}: {error}", message.id);
        }
    }

    Ok(())
}

fn mark_messages_read(connection: &rusqlite::Connection, messages: &[MessageRow]) -> Result<()> {
    for message in messages {
        connection.execute(
            "UPDATE messages SET read = 1 WHERE id = ?1",
            params![message.id],
        ).map_err(|error| {
            log::error!("failed to mark message {} read: {error}", message.id);
            error
        })?;
    }
    Ok(())
}

fn log_received_messages(messages: &[MessageRow]) {
    for message in messages {
        log::debug!(
            "message read id={} from={} to={} bytes={}",
            message.id,
            message.sender,
            message.recipient,
            message.body.len()
        );
    }
}

fn extract_message_body(message: &konnect::MessageEnvelope) -> String {
    message
        .payload
        .get("body")
        .and_then(|value| value.as_str())
        .or_else(|| message.payload.get("prompt").and_then(|value| value.as_str()))
        .or_else(|| message.payload.get("message").and_then(|value| value.as_str()))
        .or_else(|| message.payload.get("text").and_then(|value| value.as_str()))
        .unwrap_or_default()
        .to_string()
}

fn build_prompt(role_prompt: &str, agent_id: &str, message: &MessageRow) -> String {
    format!(
        "{}\n\nAgent: {}\nFrom: {}\nMessage: {}",
        role_prompt.trim(),
        agent_id,
        message.sender,
        message.body
    )
}

pub fn run_agent() -> Result<()> {
    Ok(())
}
