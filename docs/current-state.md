# EvoHime — Current State

Last updated: 2026-07-15

## Stage: 2 (Milestone 1 complete)

## Crates

| Crate | Status | Notes |
| --- | --- | --- |
| `server` | Active | HTTP + WebSocket, `/api/models/config` |
| `protocol` | Active | ServerEvent, ClientCommand enums + JSON Schema |
| `storage` | Active | Sessions, tasks, events, **session_messages** |
| `tool-runtime` | Active | Registry + `filesystem.read` |
| `agent-runtime` | Active | `agent_loop.rs` — LLM + tools |
| `model-gateway` | Active | **LiteRouter** SSE streaming + mock provider |
| `task-engine` | Active | start/complete/fail task wrappers |
| `permissions` | Scaffold | Permission enum types |
| `project-index` | Scaffold | Stage 6 |

## API endpoints

| Method | Path | Description |
| --- | --- | --- |
| GET | `/health` | Health check |
| GET | `/api/models/config` | LiteRouter model configuration |
| POST | `/api/sessions` | Create session + bootstrap events |
| GET | `/api/sessions/:id/history` | Event history |
| WS | `/ws/:session_id` | Real-time streaming events |

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
| Files, Editor, Terminal, Git, Tasks, Actions | Placeholder |

## Tests

- `crates/model-gateway` — mock stream, LiteRouter SSE (wiremock)
- `crates/agent-runtime` — agent loop with mock gateway
- `crates/protocol` — event serialization
- `crates/tool-runtime` — filesystem.read

## Next recommended step

**Stage 3 / Milestone 2**: `filesystem.write`, `shell.execute`, xterm.js, permissions.
