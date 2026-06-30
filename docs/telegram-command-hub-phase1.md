# GhostTeam Telegram Command Hub Phase 1

Phase 1 is intentionally small:

- Telegram receives `/status` and `/agents`
- The bridge queries the existing GhostTeam API over HTTP
- The bridge never touches the SQLite database directly
- The bridge keeps secrets in environment variables only

Architecture:

`Telegram updates -> ghostteam-telegram bridge -> GhostTeam API -> existing agents/tasks data`

Why this shape works:

- It keeps the Telegram integration deployable as a separate process
- It avoids changing the main API behavior
- It keeps the bridge easy to test locally against a running GhostTeam API

Next commands to add after Phase 1:

- `/tasks`
- `/assign <agent> <message>`
- `/status <agent>`
- `/logs <agent>`
- `/handoff <agent> <task>`
- `/help`

