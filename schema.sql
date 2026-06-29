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
