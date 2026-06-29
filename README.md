# GhostTeam

GhostTeam is built by GodsIMiJ AI Solutions Inc. and architected by James D. Ingersoll.

GhostTeam is a local-first multi-agent coordination playground written in Rust. It keeps a small SQLite-backed workspace on disk, lets agents join under a role and backend, routes messages between agents, and tracks task handoffs through a simple command-line interface.

## Overview

GhostTeam is built around three core ideas:

- `agents` register themselves with a role and model backend
- `messages` provide agent-to-agent communication
- `tasks` track work items, acknowledgements, completions, and history

The workspace lives in `.ghostteam/`, which holds:

- `ghostteam.db` for SQLite state
- `roles/` for role prompt files
- `teams/` for team definitions

## Installation

### Prerequisites

- Rust toolchain
- `cargo`
- `tmux` for the launcher script
- Optional local model runtime:
  - Ollama
  - llama.cpp binary

### Build from source

From the repository root:

```bash
cd ghostteam
cargo build
```

### Release build

To build an optimized binary:

```bash
cargo build --release
```

### Install system-wide

You can install GhostTeam system-wide with either `make` or the helper script.

Using `make`:

```bash
make install
```

Using the install script:

```bash
./scripts/install.sh
```

Both flows build the release binary, copy it to `/usr/local/bin/ghostteam`, and create `~/.ghostteam` if it does not already exist.

### Uninstall

Use `make` to remove the binary:

```bash
make uninstall
```

Or run the uninstall script:

```bash
./scripts/uninstall.sh
```

The uninstall script removes `/usr/local/bin/ghostteam` and then asks whether you want to delete `~/.ghostteam`.

### Initialize the workspace

Before running agents or tasks, initialize the local workspace:

```bash
ghostteam init
```

This creates `.ghostteam/` and initializes the SQLite schema.

## Running Local Models

GhostTeam supports a small backend abstraction with three backend names:

- `ollama`
- `llamacpp` or `llama.cpp`
- `ghostos`

### Ollama

The Ollama backend sends requests to:

```text
http://localhost:11434/api/generate
```

The current request shape is:

```json
{
  "model": "llama3",
  "prompt": "...",
  "stream": false
}
```

Make sure Ollama is running locally before starting an agent with:

```bash
ghostteam join manager --role manager --backend ollama
```

### llama.cpp

The llama.cpp backend spawns a local binary and writes the prompt to stdin.

By default it looks for:

```text
llama-cli
```

You can override the binary with:

```bash
GHOSTTEAM_LLAMA_CPP_BIN=/path/to/llama-cli
```

Then start an agent with:

```bash
ghostteam join worker --role worker --backend llamacpp
```

### GhostOS

`ghostos` is currently a placeholder backend that returns a formatted stub response. It is useful for testing the rest of the workflow without a real model runtime.

## Commands

### `ghostteam init`

Initializes the local workspace and creates the SQLite schema.

```bash
ghostteam init
```

### `ghostteam join manager`

Starts a manager agent. If the requested ID already exists, GhostTeam auto-suffixes it:

- `manager`
- `manager-2`
- `manager-3`

Example:

```bash
ghostteam join manager --role manager --backend ollama
```

### `ghostteam join worker`

Starts a worker agent. Multiple workers can join with the same base ID and GhostTeam will assign suffixes automatically.

Example:

```bash
ghostteam join worker --role worker --backend ollama
```

### `ghostteam join inspector`

Starts an inspector agent that can watch the message flow and task state.

Example:

```bash
ghostteam join inspector --role inspector --backend ollama
```

## Task Workflow Example

Initialize the workspace first:

```bash
ghostteam init
```

Start a manager, worker, and inspector in separate terminals:

```bash
ghostteam join manager --role manager --backend ollama
ghostteam join worker --role worker --backend ollama
ghostteam join inspector --role inspector --backend ollama
```

Create a task:

```bash
ghostteam task-create manager worker "Summarize the latest team notes"
```

The worker can acknowledge the task:

```bash
ghostteam task-ack 1 worker
```

Then complete it with a result:

```bash
ghostteam task-complete 1 worker "Summary complete and ready for review"
```

If the task needs to go back into the queue:

```bash
ghostteam task-requeue 1
```

List all tasks at any time:

```bash
ghostteam task-list
```

## Multi-Agent Collaboration Example

Here is a simple collaboration flow:

1. The manager creates a task for a worker.
2. The worker receives the message, acknowledges the task, and works on it.
3. The worker sends a completion message or updates the task result.
4. The inspector reviews the task history and message trail.

Example command sequence:

```bash
ghostteam send manager worker "Please handle task 1"
ghostteam task-create manager worker "Draft the status report"
ghostteam receive worker
ghostteam task-ack 1 worker
ghostteam task-complete 1 worker "Status report drafted"
ghostteam receive inspector
ghostteam task-list
```

## tmux Launcher Usage

GhostTeam includes a tmux launcher that creates a four-window session:

- `manager`
- `worker-1`
- `worker-2`
- `inspector`

Run it from the `ghostteam/` directory:

```bash
./scripts/ghostteam-tmux.sh
```

The script creates a tmux session named `ghostteam` and launches each role in its own window using the Ollama backend.

If the session already exists, the script exits without creating a duplicate session.

## Layout

- `src/` contains the Rust binary crate
- `.ghostteam/` contains role and team configuration
- `scripts/` contains helper scripts

---

Copyright 2026 GodsIMiJ AI Solutions Inc. All rights reserved.
