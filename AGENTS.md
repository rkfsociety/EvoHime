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
| Deploy | Native Windows launcher + Dev Container/Compose |

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
├── permissions/       — scoped permissions + approval audit (P2)
├── memory/            — redact / dedupe / conflict / admit (6.18)
├── project-index/     — workspace text search
├── protocol/          — shared event schema
└── storage/           — PostgreSQL access (incl. memory_items)
```

## Current state (Stages 1–6 ✅; Stage 7 in progress)

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
- HTTP: health, sessions, files, git, models, permissions, tools, MCP, GitHub PRs, worker jobs, OpenAPI, feature flags
- WebSocket: typed event protocol + approvals
- Tools: filesystem, shell, Git, browser, MCP call (sandboxed)
- Agent loop: native ReAct tool call → observation → next action; bounded iterations with checkpoints and pause/resume
- Frontend shell split (`6.13`): `types` / `api` / `lib` / `hooks` / `panels`
- Panels: Chat, Settings (Worker + Metrics), Tasks (deep), Actions (deep), Terminal, Files, Editor, Git, Plugins, Pull Requests (detail/diff/checks/create), Sites, Scheduled, Memory
- Structured memory: `memory_items` + admit/retrieve/extract/experience/feedback + hybrid embeddings (`6.16`–`6.25`, optional remote neural); legacy notes are migrate-only
- Workers: health/stall reliability + `text.summarize` / `text.chunk` / `text.similarity` / `text.entities`
- Native launcher + GitHub auth via local `gh`
- CI: Rust format/Clippy/docs, protocol and OpenAPI drift, frontend typecheck/build, Python worker tests, PostgreSQL integration tests

### Incomplete / next

- **Stage 7** Hardening + Product — Waves A–D ✅; Wave E `7.84`–`7.98` ✅; `7.99`–`7.116` ✅ — **Stage 7 полностью завершён**. Cloud sync включает owner-only `/api/sync/status|push|pull`, историю `sync_runs` с direction, `EVOHIME_SYNC_URL`/`EVOHIME_SYNC_TOKEN`, идемпотентный restore (`restore_backup` + CLI `evohime-import`) и авто-push (`EVOHIME_SYNC_AUTO_MINUTES`); **First implementation wave Phase 1–5** ✅. Phase 3 даёт backend feature enforcement для Sites/Scheduled/OTLP (403 Forbidden при отключённом feature), сгенерированный OpenAPI-контракт из 98 операций и CI drift check (`openapi-drift`); Phase 4 — graceful shutdown, bounded session bus cleanup, integration harness и E2E database persistence; Phase 5 — API-key encryption (AES-256-GCM), gitleaks, secure headers, plugin trust/risk/integrity и frontend code splitting. Также завершены voice input / TTS (`7.105`), apply-once diff review для `filesystem.patch` с typed `approval.required.review` и лимитом 128 KiB (`7.106`), worktree-aware multi-checkout agent — параллельные задачи изолируются в detached-HEAD git worktree и мёржатся обратно (`7.107`), extended reasoning (`7.111`), semantic project search (`7.112`), plugin audit trail (`7.113`), OTLP metrics trends (`7.114`), frontend performance (`7.115`) и session recovery (`7.116`). Дальнейшая работа — кандидаты Stage 8 (см. `docs/roadmap.md` § Stage 8).
- **Stage 8.1** ✅ Tree-of-Thoughts bounded planner
- **Stage 8.2** ✅ Self-reflection loop: `ReflectionStage` runs in the ReAct loop after every tool observation — success_score + failure patterns pulled from experience memory (`6.21`), verdict persisted to `reflection_events`, `agent.reflection` emitted over WS, hint appended to the observation the model reads next; `retry_tool` re-opens the duplicate-call guard within the retry budget, 3 failures in a row switch to `revise_plan`. UI: `ReflectionTimeline.tsx`. Off switch: `EVOHIME_REFLECTION_ENABLED=0`. Not part of this item: blocking ask-gate (`8.4`), automatic re-planning through `8.1`. See `docs/features/reflection.md`.
- Sites, Scheduled, OTLP и Cloud sync имеют gates через `EVOHIME_FEATURE_*` и `/api/features`
- `7.92` уже покрыт существующим Prometheus `/metrics` из `7.24`; `7.93`–`7.99` ✅; `7.100` ✅ — `browser.session.*` tools с persistent CDP-вкладкой на задачу (`EVOHIME_BROWSER_CDP_URL`); `7.101` ✅ — eval harness `evohime-evals`: golden tasks против реального agent loop; CI — mock, `--live --judge` — реальный провайдер + LLM-вердикты; `7.102` ✅ — trust scores в каталоге плагинов, UI-бейджи, risk-scan гейт установки и `.evohime/plugins.lock.json` + `/api/plugins/integrity`; `7.103` wave 1 ✅ — обучение на провалах: `extract_failure_candidates` (≤2 урока `failure_pattern`/`verification_rule`, confidence cap 0.6 — только Ask, без auto-promote), `FAILURE_EXTRACT_PROMPT` в post-task extract; wave 2 ✅ — эскалация повторов: `FeedbackSignal::Repeated` поднимает confidence (жёсткий кап 0.6, auto-promote по-прежнему невозможен) и importance (без верхнего капа) существующей experience-записи при повторном admit-дубликате `failure_pattern`/`verification_rule` в статусе `Candidate`; retrieval даёт этим двум kind'ам дополнительный бонус ранжирования над `success_pattern`/`playbook`; `7.104` ✅ — mobile-responsive shell: сайдбар и трейс задачи схлопываются в off-canvas дравер через CSS media query `≤768px` (гамбургер в topBar, общий backdrop, закрытие по Escape/клику вне/выбору пункта сайдбара), touch-таргеты (`sendButton`, пункты сайдбара, `traceToggle`/`traceClose`) увеличены до ≥44px на мобильном; десктопная раскладка (grid 280px+main+320px) не тронута

## WebSocket events

```text
session.created
task.started
agent.message.delta
agent.plan.updated
tool.started
tool.output
tool.output.delta
tool.completed
task.completed
task.failed
```

Also: `file.changed`, `git.diff.changed`, task status/step events, action log events, `approval.required` / grant / deny (wired), `agent.plan`, `agent.thinking`, `agent.reflection`.

## Protocol workflow

1. Edit schema: `crates/protocol/schema/evohime.protocol.schema.json`
2. Update Rust enums: `crates/protocol/src/lib.rs`
3. Regenerate TS: `npm run generate:protocol` → `frontend/web/src/protocol.generated.ts`
4. Re-export from: `frontend/web/src/protocol.ts`

**Never edit `protocol.generated.ts` by hand.**

## Tools

Implemented: `filesystem.read`, `filesystem.write`, `filesystem.patch`, `filesystem.search`, `shell.execute`, `git.status`, `git.diff`, `git.commit`, `git.pull`, `git.push`, `browser.open`, `browser.extract`, `browser.session.navigate|read|click|type|screenshot|close` (persistent CDP tab per task; needs `EVOHIME_BROWSER_CDP_URL`), `mcp.call`, `memory.search`

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
12. **Рабочая ветка** — разработка в этом репозитории всегда выполняется прямо в текущей `main`; отдельные ветки и worktree не создавать.
13. **Keep CI current** — when changing Rust workspace members, dependencies, lint rules, or test expectations, update `.github/workflows/rust.yml` in the same change and keep the workflow aligned with the codebase.
14. **Fix missing tools first** — if a required tool or command is not available in `PATH`, install or configure it before claiming a backend/frontend check passed.

15. **Clean build artifacts** - after a build or verification, remove the workspace `target/` directory and any temporary Rust target/toolchain installed for that check when they are no longer needed for the next step; do not delete artifacts still required by an active process or subsequent verification.

## Environment

Если обязательный инструмент разработки отсутствует в `PATH`, агент должен сначала установить или настроить его, а затем продолжить работу. Нельзя считать проверку выполненной, пока нужный инструмент не был реально запущен. Для Rust это включает установку/настройку toolchain, если `cargo` или `rustc` отсутствуют.

```env
DATABASE_URL=postgres://evohime:evohime@localhost:5432/evohime
BIND_ADDR=127.0.0.1:3000
# To listen on all interfaces: BIND_ADDR=0.0.0.0:3000 and set EVOHIME_API_TOKEN
# Optional local API auth (Stage 7.1). When set, HTTP/WS require Bearer token.
# When unset, only loopback clients are allowed (non-loopback → 401).
# EVOHIME_API_TOKEN=dev-local-token
# Optional: comma-separated CORS origins (default: localhost Vite + :3000)
# EVOHIME_CORS_ORIGINS=http://127.0.0.1:5173,http://localhost:5173
# Experimental feature flags (default enabled; set to 0/false to disable)
# EVOHIME_FEATURE_SITES=1
# EVOHIME_FEATURE_SCHEDULED=1
# EVOHIME_FEATURE_OTLP=1
# EVOHIME_CORS_PERMISSIVE=1
WORKSPACE_ROOT=.
DEMO_FILE_PATH=docs/sample-context.md
```

## Commands

```bash
# Native Windows local stack WITH tray icons (обязательный способ «запустить»)
.\start-dev.ps1

# Cross-platform development stack (Docker required)
docker compose -f .devcontainer/docker-compose.yml up -d

# One-shot setup only
.\scripts\setup-local.ps1 -InstallPostgres -ApplyMigrations

# Backend / frontend in isolation (only when debugging a single process)
.\start-dev.ps1 -Server
.\start-dev.ps1 -Web
.\start-dev.ps1 -Worker

# Backend
cargo run -p evohime-server
cargo test

# Backup/export sessions and structured memory
cargo run -p evohime-storage --bin evohime-export -- --output .evohime/backup.json

# Restore a backup (idempotent; default operator local-owner)
cargo run -p evohime-storage --bin evohime-import -- --input .evohime/backup.json --operator-name local-owner

# Golden-task eval report (regression harness, no network/LLM)
cargo run -p evohime-evals --bin evohime-eval
# Against the real provider, with LLM-as-judge for rubric tasks
cargo run -p evohime-evals --bin evohime-eval -- --live --judge

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
| 6 Advanced | ✅ Foundations complete |
| 7 Hardening + Product | ✅ Done; `7.1`–`7.116` complete |
| 8.1 Tree-of-Thoughts Bounded Planner | ✅ Done; Multi-path reasoning: K candidate plans, unified scoring (similarity + tool success + complexity + feedback), deterministic pruning to top-N, fallback on error, history with 30-day TTL, frontend AgentPlanView, E2E test |

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
| `crates/server/src/main.rs` | Server entrypoint (bootstrap + bind) |
| `crates/server/src/routes.rs` | HTTP router assembly |
| `crates/server/src/task/` | Task pipeline / steps / memory |
| `crates/agent-runtime/src/agent_loop/` | Agent orchestration (ReAct / execute / context / protocol parsing) |
| `crates/memory/` | Memory admit service (redact/dedupe/conflict) |
| `crates/storage/src/memory.rs` | `memory_items` CRUD |
| `crates/tool-runtime/src/tools/` | filesystem, shell, Git, browser, MCP |
| `frontend/web/src/app.tsx` | Workspace shell |
| `frontend/web/src/panels/` | Extracted panels |
| `migrations/0013_memory_items.sql` | Structured memory schema |
| `crates/model-gateway/src/providers/literouter.rs` | LiteRouter provider |
| `start-dev.ps1` | Local development launcher |
