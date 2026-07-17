# EvoHime — Current State

Last updated: 2026-07-17

## Stage: 6 in progress

Stages 1–5 complete. Structured memory `6.16`–`6.25`: schema through experience, Memory panel, feedback loop, and hybrid embeddings.

## Crates

| Crate | Status | Notes |
| --- | --- | --- |
| `server` | Active | HTTP + WebSocket, workspace file/Git/MCP/tools, GitHub PR detail/create, worker jobs, memory extract/gate |
| `protocol` | Active | ServerEvent, ClientCommand enums + JSON Schema (incl. `memory.*`) |
| `memory` | Active | Redact/dedupe/conflict + retrieve + extract/gate + experience + feedback + **hybrid embeddings** (`6.18`–`6.25`) |
| `storage` | Active | Sessions, tasks, events, messages, legacy notes, **memory_items**, settings, worker jobs |
| `tool-runtime` | Active | Sandboxed filesystem, shell, Git, browser, MCP call |
| `agent-runtime` | Active | Plan → batches → bounded replan; checkpoints; structured + legacy memory context |
| `model-gateway` | Active | Route-based gateway, LiteRouter + OpenAI-compatible + mock |
| `task-engine` | Active | Lifecycle, dependency batching, checkpoints, cancel/resume/retry |
| `permissions` | Active | ask/allow/deny + session/path overrides + temp allow + approval audit |
| `project-index` | Active | Chunk search, path/symbol weights, binary/noise filter (P2) |
| Python worker | Active | Health/stall reliability; handlers incl. `text.summarize`, `text.chunk` |

## API endpoints

| Method | Path | Description |
| --- | --- | --- |
| GET | `/health` | Health check |
| GET/PUT | `/api/models/config` | Model routes from web panel |
| GET | `/api/auth/github` | GitHub auth via local `gh` |
| GET | `/api/permissions` | Permission policy snapshot |
| GET | `/api/permissions/audit` | Approval audit log |
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
| POST | `/api/worker/jobs` | Submit worker job |
| GET | `/api/worker/jobs/:id` | Job state/result |
| POST | `/api/worker/jobs/:id/retry` | Retry job |
| GET | `/api/memory` | List memory items (filters: scope, status, q) + privacy policy |
| GET/PATCH/DELETE | `/api/memory/:id` | Get / update (content/status/pin, redacted) / delete |
| WS | `/ws/:session_id` | Real-time events |

## Agent flow

```text
user.message
  → save user message (session_messages)
  → load prior chat history
  → retrieve structured memory_items (budget + untrusted tag) + legacy notes
  → plan steps → dependency batches → tools
  → bounded replan (≤3) → respond
  → checkpoints / approval pause / resume
  → extract candidates (LLM JSON + heuristic + experience patterns)
  → admit → decision gate (auto-promote | memory.ask | drop)
  → legacy notes + structured memory_items (incl. experience/playbooks)
  → task.completed | task.failed
```

## Database tables

- `sessions`, `tasks`, `task_steps`, `task_checkpoints`
- `session_events`, `session_messages`
- `session_memory` / `global_memory` — legacy free-text (still used by agent loop)
- `memory_items` — structured scopes/items (`6.16`–`6.21`)
- `app_settings`, `worker_jobs`

## Frontend panels

| Panel | Status |
| --- | --- |
| Chat | ✅ |
| Settings | ✅ models, permissions, MCP, tools, archive |
| Tasks | ✅ deep: steps, deps, pause reason, retries, recovery, approvals |
| Actions | ✅ deep: timeline + retry/approval/recovery metrics |
| Terminal / Files / Editor / Git | ✅ |
| Plugins / Sites | ✅ Plugins: installed list + remote catalog/install |
| Pull Requests | ✅ list + detail (diff/comments/checks) + create |
| Memory | ✅ tabs + overrides (`6.22`/`6.24`); feedback (`6.23`); hybrid lexical+embedding retrieve (`6.25`) |

Frontend layout: `app.tsx` shell + `panels/` + typed `api/` + `hooks/useServerEventHandler` (`6.13` ✅). Brand: SVG mark + portrait mascot (`AgentBrand` / `AgentMark` / `AgentAvatar`, favicon).

## Tests

- `crates/agent-runtime` — pipeline integration: tool events → completion; approval pause → resume
- `crates/task-engine` — lifecycle integration: pause/resume/complete, fail/retry, recover_after_restart
- `crates/memory` — redact/normalize/dedupe/conflict + extract/gate + experience/playbooks + admit integration
- `crates/storage` — memory_items CRUD + overview/update/delete + legacy import
- `crates/model-gateway`, `protocol`, `tool-runtime`, `permissions`, `server`

## Next recommended step

1. Optional neural embeddings / OpenTelemetry
2. More specialized ML handlers as needed
3. Experience/playbooks + Memory UI polish (post-6.20)