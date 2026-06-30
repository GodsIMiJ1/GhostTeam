# GhostTeam Hardening Checklist (v0.2.0 Gate)

Owners are roles, not individuals. Dependencies are explicit. This is ready to drop into `docs/release-gate.md`.

## MUST-FIX NOW (Release Blockers)

| Priority | Done | Item | Owner | Dependencies |
|---|---|---|---|---|
| Must-fix now | [ ] | Verify real GhostOS HTTP contract end-to-end | Backend/API owner | Live GhostOS runtime, bridge container, API tests |
| Must-fix now | [ ] | Clean repo hygiene (remove stray caches, enforce ignore rules) | Repo owner | `.gitignore`, workspace cleanup |
| Must-fix now | [ ] | Validate Docker deployment path (full compose run with real GhostOS) | DevOps/Platform owner | Dockerfile, compose stack, GhostOS bridge |
| Must-fix now | [ ] | Cache API keys at startup (no per-request YAML reads) | Backend/API owner | `src/api/auth.rs`, config loader |
| Must-fix now | [ ] | Run full release test gate | QA/Release owner | `cargo test`, `cargo clippy`, API tests, Docker smoke test |
| Must-fix now | [ ] | Align README deployment docs with verified runtime behavior | Docs owner | Verified GhostOS contract, compose validation |

## SHOULD-FIX NEXT (Pre-Release Enhancements)

| Priority | Done | Item | Owner | Dependencies |
|---|---|---|---|---|
| Should-fix next | [ ] | Package Rust SDK (crate metadata, workspace layout) | SDK owner | Stable API |
| Should-fix next | [ ] | Package TypeScript SDK (package.json, build pipeline) | SDK owner | Stable API |
| Should-fix next | [ ] | Sync OpenAPI with code (or generate automatically) | Backend/API owner | Final route shapes |
| Should-fix next | [ ] | Add `.env.example` (API port, GhostOS endpoint, model, auth key path) | Docs/DevEx owner | Final env var list |
| Should-fix next | [ ] | Add health + readiness endpoints | Backend/API owner | API server wiring, GhostOS bridge |
| Should-fix next | [ ] | Tighten release automation (GitHub workflow, artifacts, checksums) | Release owner | Verified build pipeline |
| Should-fix next | [ ] | Confirm benchmarks reflect shipped code | Performance owner | Stable agent/task/model paths |

## NICE-TO-HAVE LATER (Post-v0.2.0)

| Priority | Done | Item | Owner | Dependencies |
|---|---|---|---|---|
| Nice-to-have later | [ ] | Build small web dashboard | Product/UI owner | Stable API, auth |
| Nice-to-have later | [ ] | Add metrics + tracing | Observability owner | Logging format, middleware |
| Nice-to-have later | [ ] | Expand auth (rotation, scopes) | Security owner | Current key model |
| Nice-to-have later | [ ] | Add Kubernetes/Helm deployment | DevOps/Platform owner | Stable Docker image, readiness endpoints |
| Nice-to-have later | [ ] | Improve task orchestration (reassignment, scheduling, delivery ACKs) | Backend owner | Task lifecycle rules |
| Nice-to-have later | [ ] | Replace GhostOS bridge with native adapter | Backend/Integration owner | Verified GhostOS API |
| Nice-to-have later | [ ] | Packaging polish (signed releases, checksums, SDK publishing) | Release owner | Stable release pipeline |

## Recommended Execution Order (v0.2.0 → v0.3.0)

### v0.2.0 (Hardening Release)
1. Verify GhostOS contract
2. Fix repo hygiene
3. Validate Docker deployment
4. Cache API keys
5. Run full test gate
6. Sync README + OpenAPI
7. Package SDKs
8. Add health/readiness endpoints
9. Tighten release automation

### v0.3.0 (Enhancement Release)
1. Dashboard
2. Metrics + tracing
3. Auth expansion
4. Kubernetes/Helm
5. Task orchestration improvements
6. Native GhostOS adapter
7. Packaging polish
