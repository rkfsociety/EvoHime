# EvoHime — Current State

Last updated: 2026-07-15

## Stage: 2 complete, stages 3-5 foundations in place

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
| `permissions` | Active | ask/allow/deny policy and one-shot approvals; server event delivery remains |
| `project-index` | Scaffold | Stage 6 |

## API endpoints

| Method | Path | Description |
| --- | --- | --- |
| GET | `/health` | Health check |
| GET | `/api/models/config` | LiteRouter model configuration |
| POST | `/api/sessions` | Create session + bootstrap events |
| GET | `/api/sessions/:id/history` | Event history |
| WS | `/ws/:session_id` | Real-time streaming events |

The current API is intentionally small. File browsing/editing, terminal streaming, Git views, and approval resolution still need dedicated end-to-end HTTP/WebSocket flows.

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
| Files, Editor, Terminal, Git | Placeholder |

## Tests

- `crates/model-gateway` — mock stream, LiteRouter SSE (wiremock)
- `crates/agent-runtime` — agent loop with mock gateway
- `crates/protocol` — event serialization
- `crates/tool-runtime` — filesystem, shell, and Git tool coverage

## Next recommended step

**Stage 3 / Milestone 2**: wire the existing tool registry into general LLM tool calls, emit `approval.required` from the server, resume paused tasks after approval, and connect terminal output to the browser. Then complete file/editor/Git UI and task recovery as separate vertical slices.
