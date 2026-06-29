#!/usr/bin/env bash
set -euo pipefail

SESSION="ghostteam"

if tmux has-session -t "${SESSION}" 2>/dev/null; then
  echo "tmux session '${SESSION}' already exists"
  exit 0
fi

tmux new-session -d -s "${SESSION}" -n "manager" "ghostteam join manager --role manager --backend ollama"
tmux new-window -t "${SESSION}" -n "worker-1" "ghostteam join worker --role worker --backend ollama"
tmux new-window -t "${SESSION}" -n "worker-2" "ghostteam join worker --role worker --backend ollama"
tmux new-window -t "${SESSION}" -n "inspector" "ghostteam join inspector --role inspector --backend ollama"

tmux select-window -t "${SESSION}:0"
tmux attach-session -t "${SESSION}"
