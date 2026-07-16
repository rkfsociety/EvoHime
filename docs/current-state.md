# EvoHime — Current State

Last updated: 2026-07-16

## Stage: 6 in progress

Stages 1–5 complete. Stage 6 foundations + structured memory service (`6.16`–`6.18`) in place. Agent loop still consumes legacy free-text notes; structured retrieval not wired yet.

## Crates

| Crate | Status | Notes |
| --- | --- | --- |
| `server` | Active | HTTP + WebSocket, workspace file/Git/MCP/tools, GitHub PR detail/create, worker jobs |
| `protocol` | Active | ServerEvent, ClientCommand enums + JSON Schema |
| `memory` | Active | Redaction, normalize, dedupe, conflict + `admit_memory_item` (6.18) |
| `storage` | Active | Sessions, tasks, events, messages, legacy notes, **memory_items**, settings, worker jobs |
| `tool-runtime` | Active | Sandboxed filesystem, shell, Git, browser, MCP call |
| `agent-runtime` | Active | Plan → batches → bounded replan; checkpoints; legacy memory context |
| `model-gateway` | Active | Route-based gateway, LiteRouter + OpenAI-compatible + mock |
| `task-engine` | Active | Lifecycle, dependency batching, checkpoints, cancel/resume/retry |
| `permissions` | Active | ask/allow/deny + one-shot approvals; grant/deny resume wired |
| `project-index` | Active | Workspace text search for agent context |
| Python worker | Active | Health/stall reliability; handlers incl. `text.summarize`, `text.chunk` |

## API endpoints

| Method | Path | Description |
| --- | --- | --- |
| GET | `/health` | Health check |
| GET/PUT | `/api/models/config` | Model routes from web panel |
| GET | `/api/auth/github` | GitHub auth via local `gh` |
| GET | `/api/permissions` | Permission policy snapshot |
| PUT | `/api/permissions/:permission` | Update permission mode |
| GET | `/api/tools` | Tool catalog |
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
| WS | `/ws/:session_id` | Real-time events |

## Agent flow

```text
user.message
  → save user message (session_messages)
  → load prior chat history + legacy memory notes
  → plan steps → dependency batches → tools
  → bounded replan (≤3) → respond
  → checkpoints / approval pause / resume
  → task.completed | task.failed
```

Structured `memory_items` / `admit_memory_item` exist but are **not yet** injected by the agent loop (`6.19` next).

## Database tables

- `sessions`, `tasks`, `task_steps`, `task_checkpoints`
- `session_events`, `session_messages`
- `session_memory` / `global_memory` — legacy free-text (still used by agent loop)
- `memory_items` — structured scopes/items (`6.16`/`6.17`)
- `app_settings`, `worker_jobs`

## Frontend panels

| Panel | Status |
| --- | --- |
| Chat | ✅ |
| Settings | ✅ models, permissions, MCP, tools, archive |
| Tasks | ✅ deep: steps, deps, pause reason, retries, recovery, approvals |
| Actions | ✅ deep: timeline + retry/approval/recovery metrics |
| Terminal / Files / Editor / Git | ✅ |
| Plugins / Sites | ✅ |
| Pull Requests | ✅ list + detail (diff/comments/checks) + create |
| Memory | ❌ planned (`6.22`/`6.24`) |

Frontend layout: `app.tsx` shell + `panels/` + typed `api/` + `hooks/useServerEventHandler` (`6.13` ✅).

## Tests

- `crates/memory` — redact/normalize/dedupe/conflict + admit integration
- `crates/storage` — memory_items CRUD + legacy import
- `crates/model-gateway`, `agent-runtime`, `protocol`, `tool-runtime`, `task-engine`, `permissions`, `server`

## Next recommended step

1. **`6.19`** — lexical retrieval + token budget + untrusted tagging; wire into `agent-runtime` (replace/augment legacy notes)
2. Then **`6.20`** — extraction + ask-on-uncertainty decision gate
3. Parallel P1: task-pipeline observability; broader integration tests
