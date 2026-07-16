# EvoHime — Current State

Last updated: 2026-07-16

## Stage: 6 in progress

## Crates

| Crate | Status | Notes |
| --- | --- | --- |
| `server` | Active | HTTP + WebSocket, workspace file, Git, MCP servers, and tool catalog APIs |
| `protocol` | Active | ServerEvent, ClientCommand enums + JSON Schema |
| `storage` | Active | Sessions, tasks, events, **session_messages**, **session_memory**, **app_settings** |
| `tool-runtime` | Active | Registry + sandboxed filesystem, shell, Git, browser, and MCP call tools |
| `agent-runtime` | Active | `agent_loop.rs` — LLM + tool planning parser + project/memory context |
| `model-gateway` | Active | Route-based gateway, **LiteRouter** + OpenAI-compatible endpoints, and mock provider |
| `task-engine` | Active | lifecycle wrappers, dependency batching, checkpoints, cancel/resume/retry foundation |
| `permissions` | Active | ask/allow/deny policy and one-shot approvals; approval events and resume flow wired |
| `project-index` | Active | Workspace text search for agent context |
| Python worker | Active | Bounded HTTP job queue with health, submit/poll lifecycle, and structured task failures |

## API endpoints

| Method | Path | Description |
| --- | --- | --- |
| GET | `/health` | Health check |
| GET/PUT | `/api/models/config` | View and apply route-based model configuration from the web panel |
| GET | `/api/auth/github` | GitHub auth snapshot via local `gh` CLI |
| GET | `/api/permissions` | Current permission policy snapshot |
| PUT | `/api/permissions/:permission` | Update a permission mode |
| GET | `/api/tools` | Tool catalog for Settings |
| GET/PUT | `/api/mcp/servers` | MCP server management list |
| POST | `/api/sessions` | Create session + bootstrap events |
| GET | `/api/sessions/:id/history` | Event history |
| GET | `/api/files` | List workspace directory entries |
| GET/PUT/POST | `/api/files/content` | Read, save, or create a workspace file |
| GET | `/api/git/status` | Repository status and full diff snapshot |
| GET | `/api/git/diff` | Repository diff, optionally scoped to a path |
| POST | `/api/git/commit`, `/api/git/pull`, `/api/git/push` | Permission-controlled Git mutations |
| GET | `/api/github/pull-requests` | GitHub pull request dashboard data |
| POST | `/api/worker/jobs` | Submit a durable job to the isolated Python worker |
| GET | `/api/worker/jobs/:id` | Read worker job state/result |
| POST | `/api/worker/jobs/:id/retry` | Re-submit a worker job and increment attempts |
| WS | `/ws/:session_id` | Real-time streaming events |

Workspace APIs now cover file browsing, reading, saving/creating files, Git status/diff, Git commit/pull/push actions, tool catalog inspection, GitHub auth and pull request views, and MCP server management. File and Git mutations publish synchronization events through the session bus. Permissions are editable through the API, and approval-required tool calls round-trip through the browser.

## Agent flow

```text
user.message
  → save user message (session_messages)
  → load prior chat history
  → filesystem.read (tool-runtime)
  → persist task plan + task_steps
  → emit task.step.changed updates
  → model route stream_chat (model-gateway)
  → agent.message.delta per token
  → save assistant message
  → store session memory summary
  → task.completed
```

## Database tables

- `sessions` — agent sessions
- `tasks` — user tasks (status: running/completed/failed/paused/cancelled/retrying)
- `tasks.model_route` — selected model route for the task
- `task_steps` — structured plan steps + step status history
- `task_checkpoints` — resume state and workspace context
- `session_events` — ordered event log (JSONB)
- `session_messages` — chat history for LLM context
- `session_memory` — persistent short notes for future agent runs
- `app_settings` — persisted web-panel configuration, including model routes
- `worker_jobs` — durable Python worker job state, attempts, results, and errors

## LLM provider

| Setting | Default |
| --- | --- |
| Default route | default |
| Provider | LiteRouter |
| Base URL | `https://api.literouter.com/v1` |
| Model | `deepseek:free` |
| Auth | `LITEROUTER_API_KEY` |

## Frontend panels

| Panel | Status |
| --- | --- |
| Chat | ✅ Active |
| Settings | ✅ Active (model routes + permission policies) |
| Events timeline | ✅ Active |
| Tasks | ✅ Task list, statuses, plan steps, cancel/resume/retry |
| Actions | ✅ Action log + task orchestration events |
| Terminal | ✅ Active shell output view |
| Files | ✅ Active lazy workspace tree and file creation |
| Editor | ✅ Active Monaco editor with save/reload and dirty-state conflict notice |
| Git | ✅ Active status/diff viewer with commit/pull/push controls |
| Plugins | ✅ Active catalog-style panel |
| Pull Requests | ✅ Active GitHub dashboard panel |
| Sites | ✅ Active empty-state landing panel |

## Launcher notes

- `start-dev.ps1` is the local tray launcher for server + web
- Native launcher mode uses portable PostgreSQL 16 under `%LOCALAPPDATA%\EvoHime` and does not require administrator rights
- Run `scripts/setup-local.tests.ps1` to verify PostgreSQL, migrations, and the local SQL connection
- Tray menu now includes a server restart action
- The server tray state should reflect the local `gh` auth snapshot when the backend is up

## Tests

- `crates/model-gateway` — route-based mock stream, LiteRouter SSE (wiremock)
- `crates/agent-runtime` — agent loop with mock gateway, plan parser fallback
- `crates/protocol` — event serialization
- `crates/tool-runtime` — filesystem, shell, Git, browser, and MCP tool coverage
- `crates/task-engine` — cancel/resume/retry state machine, dependency batching
- `crates/server` — task plan persistence, step status propagation, tool catalog, and MCP server management
- `crates/permissions` — policy engine and approval flow

## Next recommended step

**Stage 6 / Milestone 5**: project index, MCP, memory, multi-model routes, browser tools, Python worker integration, background polling, retention, recovery, and bounded retries are in place. Next is adding specialized ML handlers and observability while keeping worker execution isolated.
