use anyhow::Result;
use rusqlite::params;
use std::thread;
use std::time::Duration;

use crate::db::{self, MessageRow, TaskRow};

pub fn send_message(from: &str, to: &str, body: &str) -> Result<()> {
    log::debug!("task-layer send_message from={from} to={to} bytes={}", body.len());
    let connection = db::open()?;
    connection.execute(
        "INSERT INTO messages (sender, recipient, body, created_at, read)
         VALUES (?1, ?2, ?3, datetime('now'), 0)",
        params![from, to, body],
    ).map_err(|error| {
        log::error!("failed to insert message from {from} to {to}: {error}");
        error
    })?;
    Ok(())
}

pub fn receive_messages(id: &str, wait: bool) -> Result<Vec<MessageRow>> {
    loop {
        let connection = db::open()?;
        let messages = unread_messages(&connection, id)?;
        log::debug!("task-layer message poll id={id} unread={}", messages.len());

        if !messages.is_empty() {
            for message in &messages {
                log::debug!(
                    "task-layer message read id={} from={} to={} bytes={}",
                    message.id,
                    message.sender,
                    message.recipient,
                    message.body.len()
                );
            }
            mark_messages_read(&connection, &messages)?;
            return Ok(messages);
        }

        if !wait {
            return Ok(messages);
        }

        thread::sleep(Duration::from_millis(500));
    }
}

pub fn create_task(from: &str, to: &str, description: &str) -> Result<i64> {
    log::info!("creating task creator={from} assignee={to}");
    let connection = db::open()?;
    connection.execute(
        "INSERT INTO tasks (creator, assignee, description, status, result, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'created', NULL, datetime('now'), datetime('now'))",
        params![from, to, description],
    ).map_err(|error| {
        log::error!("failed to create task creator={from} assignee={to}: {error}");
        error
    })?;
    let task_id = connection.last_insert_rowid();
    insert_history(&connection, task_id, "created", from)?;
    log::debug!("task created id={task_id} creator={from} assignee={to}");
    Ok(task_id)
}

pub fn ack_task(id: i64, worker: &str) -> Result<()> {
    log::info!("acking task id={id} worker={worker}");
    let connection = db::open()?;
    connection.execute(
        "UPDATE tasks
         SET status = 'acked', assignee = ?2, updated_at = datetime('now')
         WHERE id = ?1",
        params![id, worker],
    ).map_err(|error| {
        log::error!("failed to ack task id={id} worker={worker}: {error}");
        error
    })?;
    insert_history(&connection, id, "acked", worker)?;
    log::debug!("task transitioned id={id} status=acked worker={worker}");
    Ok(())
}

pub fn complete_task(id: i64, worker: &str, result: &str) -> Result<()> {
    log::info!("completing task id={id} worker={worker}");
    let connection = db::open()?;
    connection.execute(
        "UPDATE tasks
         SET status = 'completed', result = ?2, assignee = ?3, updated_at = datetime('now')
         WHERE id = ?1",
        params![id, result, worker],
    ).map_err(|error| {
        log::error!("failed to complete task id={id} worker={worker}: {error}");
        error
    })?;
    insert_history(&connection, id, "completed", worker)?;
    log::debug!("task transitioned id={id} status=completed worker={worker}");
    Ok(())
}

pub fn requeue_task(id: i64) -> Result<()> {
    log::info!("requeueing task id={id}");
    let connection = db::open()?;
    connection.execute(
        "UPDATE tasks
         SET status = 'requeued', updated_at = datetime('now')
         WHERE id = ?1",
        params![id],
    ).map_err(|error| {
        log::error!("failed to requeue task id={id}: {error}");
        error
    })?;
    insert_history(&connection, id, "requeued", "system")?;
    log::debug!("task transitioned id={id} status=requeued");
    Ok(())
}

pub fn list_tasks() -> Result<Vec<TaskRow>> {
    let connection = db::open()?;
    let mut statement = connection.prepare(
        "SELECT id, creator, assignee, description, status, result, created_at, updated_at
         FROM tasks
         ORDER BY id DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(TaskRow {
            id: row.get(0)?,
            creator: row.get(1)?,
            assignee: row.get(2)?,
            description: row.get(3)?,
            status: row.get(4)?,
            result: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }

    log::debug!("listing tasks count={}", tasks.len());
    for task in &tasks {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            task.id,
            task.creator,
            task.assignee.clone().unwrap_or_default(),
            task.description,
            task.status,
            task.result.clone().unwrap_or_default(),
            task.created_at.clone().unwrap_or_default(),
            task.updated_at.clone().unwrap_or_default()
        );
    }

    Ok(tasks)
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

fn mark_messages_read(connection: &rusqlite::Connection, messages: &[MessageRow]) -> Result<()> {
    for message in messages {
        connection.execute(
            "UPDATE messages SET read = 1 WHERE id = ?1",
            params![message.id],
        )?;
    }
    Ok(())
}

fn insert_history(
    connection: &rusqlite::Connection,
    task_id: i64,
    event: &str,
    actor: &str,
) -> Result<()> {
    connection.execute(
        "INSERT INTO task_history (task_id, event, actor, at)
         VALUES (?1, ?2, ?3, datetime('now'))",
        params![task_id, event, actor],
    ).map_err(|error| {
        log::error!("failed to insert task history task_id={task_id} event={event} actor={actor}: {error}");
        error
    })?;
    log::debug!("task history recorded task_id={task_id} event={event} actor={actor}");
    Ok(())
}

pub fn load_tasks() -> Result<()> {
    Ok(())
}
