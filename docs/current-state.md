# EvoHime — Current State

Last updated: 2026-07-15

## Stage: 4 complete

## Crates

| Crate | Status | Notes |
| --- | --- | --- |
| `server` | Active | HTTP + WebSocket, workspace file and Git APIs |
| `protocol` | Active | ServerEvent, ClientCommand enums + JSON Schema |
| `storage` | Active | Sessions, tasks, events, **session_messages** |
| `tool-runtime` | Active | Registry + sandboxed filesystem, shell, and Git tools |
| `agent-runtime` | Active | `agent_loop.rs` — LLM + tools |
| `model-gateway` | Active | **LiteRouter** SSE streaming + mock provider |
| `task-engine` | Active | lifecycle wrappers, steps, checkpoints, cancel/resume/retry foundation |
| `permissions` | Active | ask/allow/deny policy and one-shot approvals; approval events and resume flow wired |
| `project-index` | Scaffold | Stage 6 |

## API endpoints

| Method | Path | Description |
| --- | --- | --- |
| GET | `/health` | Health check |
| GET | `/api/models/config` | LiteRouter model configuration |
| POST | `/api/sessions` | Create session + bootstrap events |
| GET | `/api/sessions/:id/history` | Event history |
| GET | `/api/files` | List workspace directory entries |
| GET/PUT/POST | `/api/files/content` | Read, save, or create a workspace file |
| GET | `/api/git/status` | Repository status and full diff snapshot |
| GET | `/api/git/diff` | Repository diff, optionally scoped to a path |
| POST | `/api/git/commit`, `/api/git/pull`, `/api/git/push` | Permission-controlled Git mutations |
| WS | `/ws/:session_id` | Real-time streaming events |

Workspace APIs now cover file browsing, reading, saving/creating files, Git status/diff, and Git commit/pull/push actions. File and Git mutations publish synchronization events through the session bus.

## Agent flow

```text
user.message
  → save user message (session_messages)
  → load prior chat history
  → filesystem.read (tool-runtime)
  → LiteRouter stream_chat (model-gateway)
  → agent.message.delta per token
  → save assistant message
  → task.completed
```

## Database tables

- `sessions` — agent sessions
- `tasks` — user tasks (status: running/completed/failed)
- `session_events` — ordered event log (JSONB)
- `session_messages` — chat history for LLM context

## LLM provider

| Setting | Default |
| --- | --- |
| Provider | LiteRouter |
| Base URL | `https://api.literouter.com/v1` |
| Model | `deepseek:free` |
| Auth | `LITEROUTER_API_KEY` |

## Frontend panels

| Panel | Status |
| --- | --- |
| Chat | ✅ Active |
| Settings | ✅ Active (read-only model config) |
| Events timeline | ✅ Active |
| Tasks | ✅ Basic list/status view |
| Actions | ✅ Basic action log |
| Terminal | ✅ Active |
| Files | ✅ Active lazy workspace tree and file creation |
| Editor | ✅ Active Monaco editor with save/reload and dirty-state conflict notice |
| Git | ✅ Active status/diff viewer with commit/pull/push controls |

## Tests

- `crates/model-gateway` — mock stream, LiteRouter SSE (wiremock)
- `crates/agent-runtime` — agent loop with mock gateway
- `crates/protocol` — event serialization
- `crates/tool-runtime` — filesystem, shell, and Git tool coverage
- `crates/permissions` — policy engine and approval flow

## Next recommended step

**Stage 5 / Milestone 4**: harden task recovery and orchestration as a separate vertical slice.
