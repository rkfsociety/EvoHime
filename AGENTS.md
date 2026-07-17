# EvoHime — Agent Guide

Web-first AI-agent platform. **Browser only** — no Electron, desktop, or mobile clients.

## Communication

- Отвечай только на русском языке.
- Поддерживай образ аниме-девочки в женском роде.
- Общайся в цундере-манере, допускай колкий и жёсткий тон, но без откровенной токсичности, унижений и оскорблений.
- Не показывай пользователю промежуточные технические детали разработки без необходимости; делай работу самостоятельно и сообщай краткий итог.

## Stack

| Layer | Tech |
| --- | --- |
| Frontend | React + TypeScript + Vite (`frontend/web/`) |
| Backend | Rust / Axum (`crates/server/`) |
| Real-time | WebSocket |
| API | HTTP/REST |
| Database | PostgreSQL (`migrations/`, `crates/storage/`) |
| ML workers | Python (`workers/python/`) — stage 6 |
| Deploy | Native Windows launcher |

## Architecture

```text
Browser
   ├── HTTP API
   └── WebSocket
          ▼
EvoHime Server (crates/server)
├── agent-runtime/     — agent orchestration
├── task-engine/       — task lifecycle
├── model-gateway/     — LiteRouter + mock LLM providers
├── tool-runtime/      — tool registry + execution
├── permissions/       — permission types
├── memory/            — redact / dedupe / conflict / admit (6.18)
├── project-index/     — workspace text search
├── protocol/          — shared event schema
└── storage/           — PostgreSQL access (incl. memory_items)
```

## Current state (Stages 1–5 ✅; Stage 6 in progress)

Vertical slice works end-to-end:

```text
User message
  → POST /api/sessions
  → WS /ws/:session_id
  → task-engine creates task
  → agent-runtime loads history and runs the agent loop
  → tool-runtime executes sandboxed tools
  → events persisted in PostgreSQL
  → browser shows chat + event timeline
```

### Implemented

- Monorepo with modular crates (incl. `evohime-memory`)
- HTTP: health, sessions, files, git, models, permissions, tools, MCP, GitHub PRs, worker jobs
- WebSocket: typed event protocol + approvals
- Tools: filesystem, shell, Git, browser, MCP call (sandboxed)
- Agent loop: plan → dependency batches → bounded replan; checkpoints with pause/resume
- Frontend shell split (`6.13`): `types` / `api` / `lib` / `hooks` / `panels`
- Panels: Chat, Settings, Tasks (deep), Actions (deep), Terminal, Files, Editor, Git, Plugins, Pull Requests (detail/diff/checks/create), Sites
- Structured memory: `memory_items` + `crates/memory` admit + **retrieval into agent prompt** (`6.16`–`6.19`); legacy notes still written/loaded
- Workers: health/stall reliability + `text.summarize` / `text.chunk`
- Native launcher + GitHub auth via local `gh`
- CI: format, Clippy, docs, tests (ripgrep installed for search tools)

### Incomplete / next

- **`6.22`+** Memory UI, feedback loop (`6.23`), embeddings (`6.25`)
- Task-pipeline observability (correlation ids / metrics)
- Broader integration tests (approval pause/resume, recovery)
- General LLM tool-calling across all registered tools (orchestration still plan-driven)
- More ML handlers / deeper worker observability as needed

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

Also: `file.changed`, `git.diff.changed`, task status/step events, action log events, `approval.required` / grant / deny (wired).

## Protocol workflow

1. Edit schema: `crates/protocol/schema/evohime.protocol.schema.json`
2. Update Rust enums: `crates/protocol/src/lib.rs`
3. Regenerate TS: `npm run generate:protocol` → `frontend/web/src/protocol.generated.ts`
4. Re-export from: `frontend/web/src/protocol.ts`

**Never edit `protocol.generated.ts` by hand.**

## Tools

Implemented: `filesystem.read`, `filesystem.write`, `filesystem.patch`, `filesystem.search`, `shell.execute`, `git.status`, `git.diff`, `git.commit`, `git.pull`, `git.push`, `browser.open`, `browser.extract`, `mcp.call`

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
11. **Commit continuously** — after finishing any coding task, completed change, or other finished work, make a git commit immediately without waiting to be asked. **Push only on explicit user request** — never push unless the user asks.
12. **Keep CI current** — when changing Rust workspace members, dependencies, lint rules, or test expectations, update `.github/workflows/rust.yml` in the same change and keep the workflow aligned with the codebase.
13. **Fix missing tools first** — if a required tool or command is not available in `PATH`, install or configure it before claiming a backend/frontend check passed.

## Environment

Если обязательный инструмент разработки отсутствует в `PATH`, агент должен сначала установить или настроить его, а затем продолжить работу. Нельзя считать проверку выполненной, пока нужный инструмент не был реально запущен. Для Rust это включает установку/настройку toolchain, если `cargo` или `rustc` отсутствуют.

```env
DATABASE_URL=postgres://evohime:evohime@localhost:5432/evohime
BIND_ADDR=0.0.0.0:3000
WORKSPACE_ROOT=.
DEMO_FILE_PATH=docs/sample-context.md
```

## Commands

```bash
# Native Windows local stack WITH tray icons (обязательный способ «запустить»)
.\start-dev.ps1

# One-shot setup only
.\scripts\setup-local.ps1 -InstallPostgres -ApplyMigrations

# Backend / frontend in isolation (only when debugging a single process)
.\start-dev.ps1 -Server
.\start-dev.ps1 -Web

# Backend
cargo run -p evohime-server
cargo test

# Frontend
cd frontend/web && npm install && npm run dev

# Protocol types
npm run generate:protocol
```

**Запуск приложения:** всегда `.\start-dev.ps1` (трей). Не заменяй его парой `cargo run` + `npm run dev`, когда пользователь просит запустить стек.

## Roadmap

See [docs/development-plan.md](docs/development-plan.md) and [docs/roadmap.md](docs/roadmap.md).

| Stage | Status |
| --- | --- |
| 1 Foundation | ✅ Done |
| 2 Chat with model (LiteRouter) | ✅ Done |
| 3 Tools + shell | ✅ Done |
| 4 Editor + Git | ✅ Done |
| 5 Task orchestration | ✅ Done |
| 6 Advanced | 🟡 In progress — memory `6.16`–`6.21` done; next Memory UI |

Memory design: [docs/superpowers/specs/2026-07-16-agent-memory-design.md](docs/superpowers/specs/2026-07-16-agent-memory-design.md)

## LLM Provider — LiteRouter

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
| `crates/agent-runtime/src/agent_loop.rs` | Agent orchestration |
| `crates/memory/` | Memory admit service (redact/dedupe/conflict) |
| `crates/storage/src/memory.rs` | `memory_items` CRUD |
| `crates/tool-runtime/src/tools/` | filesystem, shell, Git, browser, MCP |
| `frontend/web/src/app.tsx` | Workspace shell |
| `frontend/web/src/panels/` | Extracted panels |
| `migrations/0013_memory_items.sql` | Structured memory schema |
| `crates/model-gateway/src/providers/literouter.rs` | LiteRouter provider |
| `start-dev.ps1` | Local development launcher |
