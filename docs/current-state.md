# EvoHime — Current State

Last updated: 2026-07-17

## Stage: 7 planned (Stage 6 foundations done)

Stages 1–5 complete. Stage 6 structured memory `6.16`–`6.25` and optional polish landed. **Next:** Stage 7 hardening + product surface — see roadmap.

## Crates

| Crate | Status | Notes |
| --- | --- | --- |
| `server` | Active | HTTP + WebSocket, workspace, GitHub, workers, memory, pipeline metrics + optional OTLP |
| `protocol` | Active | ServerEvent, ClientCommand enums + JSON Schema (incl. `memory.*`) |
| `memory` | Active | Redact/dedupe/conflict + retrieve + extract/gate + experience + feedback + hybrid embeddings (hash default, optional remote neural) |
| `storage` | Active | Sessions, tasks, events, messages, legacy notes, **memory_items**, settings, worker jobs |
| `tool-runtime` | Active | Sandboxed filesystem, shell, Git, browser, MCP call |
| `agent-runtime` | Active | Plan → batches → bounded replan; checkpoints; structured memory context |
| `model-gateway` | Active | Route-based gateway, LiteRouter + OpenAI-compatible + mock |
| `task-engine` | Active | Lifecycle, dependency batching, checkpoints, cancel/resume/retry |
| `permissions` | Active | ask/allow/deny + session/path overrides + temp allow + durable approval audit (PG) |
| `project-index` | Active | Chunk search, path/symbol weights, binary/noise filter (P2) |
| Python worker | Active | Health/stall reliability; handlers: stats/keywords/summarize/chunk/similarity/entities/diff |

## API endpoints

Errors return `{ "error", "code", "retryable" }` (plus `tool` / `approval_id` for approvals).

| Method | Path | Description |
| --- | --- | --- |
| GET | `/health` | Health check (public) |
| GET | `/api/auth/status` | Local auth mode (`token_configured`, public) |
| GET/PUT | `/api/models/config` | Model routes from web panel |
| GET | `/api/auth/github` | GitHub auth via local `gh` |
| GET | `/api/permissions` | Permission policy snapshot |
| GET | `/api/permissions/audit` | Durable approval audit (PG; `?limit=`) |
| GET | `/api/permissions/scopes` | Session overrides + path grants |
| PUT | `/api/permissions/:permission` | Update permission mode |
| GET | `/api/tools` | Tool catalog |
| GET | `/api/plugins` | Installed plugins under `.evohime/plugins` |
| GET | `/api/plugins/catalog` | Merged remote OSS marketplaces with semantic groups (`groups[]`, `category`/`group` on plugins); override via `EVOHIME_PLUGIN_CATALOG_URL` or `app_settings.plugin_catalog.sources` |
| POST | `/api/plugins/install` | Install catalog plugin via `git clone` into `.evohime/plugins` |
| GET/PUT | `/api/mcp/servers` | MCP server list |
| POST | `/api/sessions` | Create session + bootstrap |
| GET | `/api/sessions/:id/history` | Event history |
| GET | `/api/files` | List workspace entries |
| GET/PUT/POST | `/api/files/content` | Read / save / create file |
| GET | `/api/git/status` | Git status snapshot |
| GET | `/api/git/diff` | Diff (optional path) |
| POST | `/api/git/commit`, `/pull`, `/push` | Git mutations |
| GET | `/api/github/pull-requests` | PR list |
| GET | `/api/github/pull-requests/:number` | PR detail (diff, comments, reviews, checks) |
| POST | `/api/github/pull-requests` | Create PR via `gh` |
| GET | `/api/worker/status` | Worker metrics + DB status counts |
| GET | `/api/worker/jobs` | Recent durable worker jobs (`limit`, default 50) |
| POST | `/api/worker/jobs` | Submit worker job |
| GET | `/api/worker/jobs/:id` | Job state/result |
| POST | `/api/worker/jobs/:id/retry` | Retry job |
| GET | `/api/metrics` | Pipeline+worker snapshot + persist status |
| GET | `/api/metrics/history` | Persisted metrics snapshots from PG |
| GET | `/metrics` | Prometheus text exposition |
| GET | `/api/memory` | List memory items (filters: scope, status, q) + privacy policy |
| GET/PATCH/DELETE | `/api/memory/:id` | Get / update (content/status/pin, redacted) / delete |
| WS | `/ws/:session_id` | Real-time events |

## Agent flow

```text
user.message
  → save user message (session_messages)
  → load prior chat history
  → retrieve structured memory_items (budget + untrusted tag)
  → plan steps → dependency batches → tools
  → bounded replan (≤3) → respond
  → checkpoints / approval pause / resume
  → extract candidates (LLM JSON + heuristic + experience patterns)
  → admit → decision gate (auto-promote | memory.ask | drop)
  → legacy notes migrated at startup; structured memory_items only at runtime
  → task.completed | task.failed
```

## Database tables

- `sessions`, `tasks`, `task_steps`, `task_checkpoints`
- `session_events`, `session_messages`
- `session_memory` / `global_memory` — legacy free-text (migrate-only; imported at startup into `memory_items`)
- `memory_items` — structured scopes/items (`6.16`–`6.21`; runtime source of truth)
- `app_settings`, `worker_jobs` (incl. `claim_token` lease, `7.26`)
- `permission_approval_audit` — durable approval decisions (`7.23`)
- `metrics_snapshots` — periodic pipeline/worker snapshots (`7.24`)

## Frontend panels

| Panel | Status |
| --- | --- |
| Chat | ✅ |
| Settings | ✅ models, permissions, MCP, tools, worker, metrics, archive |
| Tasks | ✅ deep: steps, deps, pause reason, retries, recovery, approvals |
| Actions | ✅ deep: timeline + retry/approval/recovery metrics |
| Terminal / Files / Editor / Git | ✅ |
| Plugins / Sites | ✅ Plugins: installed list + remote catalog/install |
| Pull Requests | ✅ list + detail (diff/comments/checks) + create |
| Memory | ✅ tabs + playbook view + kind filters + JSON export; feedback; hybrid retrieve |

Frontend layout: `app.tsx` shell + `panels/` + typed `api/` + `hooks/useServerEventHandler` (`6.13` ✅). Brand: SVG mark + portrait mascot (`AgentBrand` / `AgentMark` / `AgentAvatar`, favicon).

## Tests

- `crates/agent-runtime` — pipeline integration: tool events → completion; approval pause → resume
- `crates/task-engine` — lifecycle integration: pause/resume/complete, fail/retry, recover_after_restart
- `crates/memory` — redact/normalize/dedupe/conflict + extract/gate + experience/playbooks + admit integration
- `crates/storage` — memory_items CRUD + overview/update/delete + legacy import
- `crates/model-gateway`, `protocol`, `tool-runtime`, `permissions`, `server`

## Next recommended step

1. **Stage 7** — Wave A/B ✅; Wave C: `7.28`–`7.33`, `7.52` ✅; next planner telemetry `7.34`
2. Рекомендуемая следующая волна: planner step telemetry (`7.34`), plan approve (`7.35`)
3. Product honesty: Sites/Scheduled либо реализовать (`7.62`+), либо убрать вводящий в заблуждение UI