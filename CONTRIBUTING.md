# Contributing to GhostTeam

Thanks for helping improve GhostTeam from GodsIMiJ AI Solutions Inc. This document keeps the workflow consistent so changes stay easy to review and release.

## Coding Style

- Run `cargo fmt` before submitting changes.
- Keep functions small and focused.
- Prefer explicit error context with `anyhow::Context` when a failure can be confusing.
- Use `log::debug!`, `log::info!`, and `log::error!` for observable behavior.
- Keep SQL and file I/O paths clear and local-first.
- Prefer safe, testable helpers over inline logic in command handlers.

## Commit Message Format

Use Conventional Commits:

- `feat: add GhostOS config command`
- `fix: handle retry backoff in backend calls`
- `test: add end-to-end collaboration coverage`
- `docs: update installation instructions`

Keep the subject line imperative and concise. Include a short body when the change needs context.

## Branching Model

- Branch from `main` for feature or fix work.
- Keep branches focused on one logical change.
- Rebase or merge `main` before opening a release PR if needed.
- Use tags like `v0.1.0` for releases.

## Testing Requirements

Before opening a PR, run:

- `cargo fmt --check`
- `cargo clippy-strict`
- `cargo test`
- `cargo test --test e2e`

If you change benchmarks or release packaging, also run:

- `cargo bench`
- `cargo build --release`

When touching workspace initialization or config loading, add or update coverage in the integration tests.

---

Copyright 2026 GodsIMiJ AI Solutions Inc. All rights reserved.
