# EvoHime — Current State

Last updated: 2026-07-15

## Stage: 3 complete, stage 4 foundations in place

## Crates

| Crate | Status | Notes |
| --- | --- | --- |
| `server` | Active | HTTP + WebSocket, `/api/models/config` |
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
| WS | `/ws/:session_id` | Real-time streaming events |

The current API is intentionally small. File browsing/editing and Git views still need dedicated end-to-end HTTP/WebSocket flows.

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
| Files, Editor, Git | Placeholder |

## Tests

- `crates/model-gateway` — mock stream, LiteRouter SSE (wiremock)
- `crates/agent-runtime` — agent loop with mock gateway
- `crates/protocol` — event serialization
- `crates/tool-runtime` — filesystem, shell, and Git tool coverage
- `crates/permissions` — policy engine and approval flow

## Next recommended step

**Stage 4 / Milestone 3**: finish file browsing, Monaco editing, and Git UI in the browser. Then harden task recovery and orchestration as a separate vertical slice.
