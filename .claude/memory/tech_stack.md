---
name: tech_stack
description: "Технологический стек EvoHime (Rust, React, PostgreSQL, Docker)"
metadata: 
  node_type: memory
  type: reference
  originSessionId: adc991a0-0ba6-4d74-ace4-d6cf01d7403e
  modified: 2026-07-24T16:13:37.239Z
---

## Backend (Rust)

- **Фреймворк:** Axum 0.7 (async HTTP + WebSocket)
- **БД:** PostgreSQL 16 с sqlx
- **Runtime:** Tokio multi-threaded async
- **Сериализация:** serde/serde_json
- **Криптография:** aes-gcm (AES-256-GCM для API-keys), sha2
- **Observability:** OpenTelemetry + OTLP, tracing/tracing-subscriber
- **Rust edition:** 2021

## Frontend (TypeScript/React)

- **Framework:** React 18.3.1
- **Build tool:** Vite 6.3.5
- **TypeScript:** 5.8.3
- **Markdown:** react-markdown + remark-gfm
- **Редактор:** Monaco Editor 0.55.1
- **Terminal:** xterm.js 6.0.0
- **Package manager:** npm

## DevOps & Deployment

- **Local development:** PostgreSQL 16 portable + Windows launcher (tray icon, start-dev.ps1)
- **Cross-platform:** Docker Compose + VS Code Dev Containers
- **CI/CD:** GitHub Actions
  - typecheck (frontend)
  - production build
  - PostgreSQL integration tests
  - gitleaks (security scanning)
- **Recommended deploy:** Native Windows launcher

## LLM Provider

- **LiteRouter:** OpenAI-compatible API
- **Env vars:** `LITEROUTER_API_KEY`, `LITEROUTER_MODEL=deepseek:free`
- **Routing:** Model selection по задаче (конфиг в web-панели)

## Python Worker

- Bounded HTTP job queue (health, submit, poll)
- Handlers: text.summarize, text.chunk, text.similarity, text.entities, text.diff, text.classify, text.language, text.redact
- Schema: `workers/schemas/worker-tasks.schema.json`

## Workspace Organization (Rust)

Monorepo с 12 Rust crates + React frontend:
- `protocol` — shared event schema (JSON Schema + enums)
- `storage` — PostgreSQL CRUD
- `memory` — redact/dedupe/conflict/retrieve/extract/feedback
- `tool-runtime` — sandboxed tools
- `agent-runtime` — ReAct loop
- `task-engine` — task lifecycle
- `model-gateway` — LiteRouter adapter
- `evals` — eval harness
- `server` — HTTP + WebSocket entrypoint
- И ещё несколько других

## Database

PostgreSQL 16 с миграциями в `migrations/`:
- `sessions` — сессии пользователя
- `tasks` — таски с чекпоинтами
- `events` — history stream
- `memory_items` — структурированная память (6.16+)
- `permission_approval_audit` — дurable approvals (7.23)
- `metrics_snapshots` — persisted metrics (7.24)
- `app_settings` — конфиг хранилище
- `worker_jobs` — bounded job queue с claim lease (7.26)

## Key Characteristics

1. **Модульность:** Чистое разделение между Rust crates и React frontend
2. **Single source of truth:** JSON Schema для типизации
3. **Real-time:** WebSocket + typed ServerEvent/ClientCommand
4. **Security:** Sandboxed tools, API-key encryption, CSP headers, plugin integrity checks
5. **Performance:** Lazy-loaded panels, code splitting, keyset pagination
