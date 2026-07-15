# EvoHime — Agent Guide

Web-first AI-agent platform. **Browser only** — no Electron, desktop, or mobile clients.

## Stack

| Layer | Tech |
| --- | --- |
| Frontend | React + TypeScript + Vite (`frontend/web/`) |
| Backend | Rust / Axum (`crates/server/`) |
| Real-time | WebSocket |
| API | HTTP/REST |
| Database | PostgreSQL (`migrations/`, `crates/storage/`) |
| ML workers | Python (`workers/python/`) — stage 6 |
| Deploy | Docker Compose |

## Architecture

```text
Browser
   ├── HTTP API
   └── WebSocket
          ▼
EvoHime Server (crates/server)
├── agent-runtime/     — agent orchestration
├── task-engine/       — task lifecycle
├── model-gateway/     — LLM providers (stage 2)
├── tool-runtime/      — tool registry + execution
├── permissions/       — permission types
├── project-index/     — semantic search (stage 6)
├── protocol/          — shared event schema
└── storage/           — PostgreSQL access
```

## Current state (Stage 1 — done)

Vertical slice works end-to-end:

```text
User message
  → POST /api/sessions
  → WS /ws/:session_id
  → task-engine creates task
  → agent-runtime runs demo flow
  → tool-runtime executes filesystem.read
  → events persisted in PostgreSQL
  → browser shows chat + event timeline
```

### Implemented

- Monorepo with all crate scaffolds
- HTTP: `/health`, `POST /api/sessions`, `GET /api/sessions/:id/history`
- WebSocket: typed event protocol
- Tool: `filesystem.read` (with path traversal protection)
- Frontend: workspace shell (chat active, other panels are placeholders)
- Protocol codegen: JSON Schema → TypeScript
- Tests: `crates/protocol`, `crates/tool-runtime`
- Docker Compose: db + server + web

### Not yet implemented

- Real LLM integration (stage 2)
- Shell, terminal, permission approvals (stage 3)
- Monaco editor, file tree, Git (stage 4)
- Task planning, parallel tools, cancel/resume (stage 5)
- Project index, MCP, multi-model, Python workers (stage 6)

## WebSocket events

```text
session.created
task.started
agent.message.delta
agent.plan.updated
tool.started
tool.output
tool.completed
task.completed
task.failed
```

Planned later: `file.changed`, `git.diff.changed`, `approval.required`

## Protocol workflow

1. Edit schema: `crates/protocol/schema/evohime.protocol.schema.json`
2. Update Rust enums: `crates/protocol/src/lib.rs`
3. Regenerate TS: `npm run generate:protocol` → `frontend/web/src/protocol.generated.ts`
4. Re-export from: `frontend/web/src/protocol.ts`

**Never edit `protocol.generated.ts` by hand.**

## Tools (planned full list)

Current: `filesystem.read`

Future: `filesystem.write`, `filesystem.patch`, `filesystem.search`, `shell.execute`, `git.status`, `git.diff`, `git.commit`, `git.pull`, `git.push`, `browser.open`, `browser.extract`, `mcp.call`

Each tool must have: unique name, description, JSON Schema input, required permissions, timeout, cancellation, structured result, execution log.

## Coding rules

1. **No business logic in frontend** — UI renders server events only
2. **Strict typing** — Rust + TypeScript, shared schema
3. **Modular crates** — one concern per crate
4. **Minimize diff scope** — don't touch unrelated code
5. **Tests** for core logic and tools
6. **Structured logs** via `tracing`
7. **DB migrations** in `migrations/`
8. **Error handling** — server must not crash on bad input
9. **Resource limits** — timeouts on tools
10. **Security** — sandbox filesystem and shell operations

## Environment

```env
DATABASE_URL=postgres://evohime:evohime@localhost:5432/evohime
BIND_ADDR=0.0.0.0:3000
WORKSPACE_ROOT=.
DEMO_FILE_PATH=docs/sample-context.md
```

## Commands

```bash
# Full stack
docker compose up --build

# Backend
cargo run -p evohime-server
cargo test

# Frontend
cd frontend/web && npm install && npm run dev

# Protocol types
npm run generate:protocol
```

## Roadmap

See [docs/development-plan.md](docs/development-plan.md) and [docs/roadmap.md](docs/roadmap.md).

| Stage | Status |
| --- | --- |
| 1 Foundation | ✅ Done |
| 2 Chat with model (LiteRouter) | 🔜 Next |
| 3 Tools + shell | Planned |
| 4 Editor + Git | Planned |
| 5 Task orchestration | Planned |
| 6 Advanced (MCP, index) | Planned |

## LLM Provider — LiteRouter (stage 2)

First provider: **LiteRouter** — OpenAI-compatible API.

```env
MODEL_PROVIDER=literouter
LITEROUTER_API_KEY=lr_...
LITEROUTER_BASE_URL=https://api.literouter.com/v1
LITEROUTER_MODEL=deepseek:free
```

- Docs: [docs/providers/literouter.md](docs/providers/literouter.md)
- Code: `crates/model-gateway/src/providers/literouter.rs`

## Key files

| File | Purpose |
| --- | --- |
| `crates/server/src/main.rs` | HTTP + WS handlers |
| `crates/agent-runtime/src/vertical_slice.rs` | Demo agent flow |
| `crates/tool-runtime/src/tools/filesystem.rs` | filesystem.read |
| `frontend/web/src/app.tsx` | Workspace UI |
| `migrations/0001_init.sql` | DB schema |
| `crates/model-gateway/src/providers/literouter.rs` | LiteRouter provider (stage 2) |
| `docker-compose.yml` | Local deployment |
