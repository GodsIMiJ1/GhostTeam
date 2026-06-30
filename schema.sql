CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    role TEXT,
    backend TEXT,
    joined_at DATETIME
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    sender TEXT,
    recipient TEXT,
    body TEXT,
    created_at DATETIME,
    read INTEGER
);

CREATE TABLE IF NOT EXISTS tasks (
    id INTEGER PRIMARY KEY,
    creator TEXT,
    assignee TEXT,
    description TEXT,
    status TEXT,
    result TEXT,
    created_at DATETIME,
    updated_at DATETIME
);

CREATE TABLE IF NOT EXISTS task_history (
    id INTEGER PRIMARY KEY,
    task_id INTEGER,
    event TEXT,
    actor TEXT,
    at DATETIME
);

CREATE TABLE IF NOT EXISTS konnect_id_mappings (
    id INTEGER PRIMARY KEY,
    entity_kind TEXT NOT NULL,
    local_id TEXT NOT NULL,
    remote_id TEXT NOT NULL,
    remote_source TEXT,
    created_at DATETIME NOT NULL DEFAULT (datetime('now')),
    updated_at DATETIME NOT NULL DEFAULT (datetime('now')),
    UNIQUE(entity_kind, local_id),
    UNIQUE(entity_kind, remote_id)
);

CREATE TABLE IF NOT EXISTS konnect_id_mapping_history (
    id INTEGER PRIMARY KEY,
    entity_kind TEXT NOT NULL,
    local_id TEXT NOT NULL,
    remote_id TEXT NOT NULL,
    remote_source TEXT,
    action TEXT NOT NULL,
    recorded_at DATETIME NOT NULL DEFAULT (datetime('now'))
);
