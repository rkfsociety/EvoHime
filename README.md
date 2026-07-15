# EvoHime

Web-first AI-agent monorepo.

## Stack

- Frontend: React + TypeScript + Vite
- Backend: Rust (Axum)
- Database: PostgreSQL
- Real-time: WebSocket
- Deploy: Docker Compose

## Repository layout

```text
evohime/
├── frontend/web/          # React workspace UI
├── crates/
│   ├── server/            # HTTP + WebSocket entrypoint
│   ├── agent-runtime/     # Agent orchestration
│   ├── task-engine/       # Task lifecycle
│   ├── tool-runtime/      # Tool registry + execution
│   ├── permissions/       # Permission types
│   ├── protocol/          # Shared event schema
│   └── storage/           # PostgreSQL access
├── workers/python/        # Planned ML workers (stage 6)
├── migrations/            # SQL migrations
├── docker/                # Container images
└── docs/                  # Architecture, roadmap, and status
```

## Current state

```text
User message
  → task created
  → agent loop loads history
  → filesystem tool reads the workspace
  → streamed model response
  → events shown in browser
  → history stored in PostgreSQL
```

LiteRouter is the active model provider. The tool runtime currently includes sandboxed filesystem read/write/patch/search, shell execution, and Git operations. Task lifecycle, task steps, checkpoints, cancel/resume/retry commands, and the Tasks/Actions UI are present at foundation level. Approval delivery, end-to-end tool-calling orchestration, terminal streaming, file editor, and Git UI still need completion.

## Local development

Frontend:

```bash
cd frontend/web
npm install
npm run dev
```

Backend (Rust + PostgreSQL):

```bash
cp .env.example .env
cargo run -p evohime-server
```

Generate protocol types:

```bash
npm install
npm run generate:protocol
```

Docker Compose:

```bash
docker compose up --build
```

- Web: http://localhost:5173
- API: http://localhost:3000/health

## Environment

See `.env.example`:

- `DATABASE_URL`
- `BIND_ADDR`
- `WORKSPACE_ROOT`
- `DEMO_FILE_PATH`
- `LITEROUTER_API_KEY` — LiteRouter API key
- `LITEROUTER_MODEL` — default `deepseek:free`

## Documentation

- [docs/development-plan.md](docs/development-plan.md) — полный план разработки
- [docs/providers/literouter.md](docs/providers/literouter.md) — первый LLM-провайдер (LiteRouter)
- [docs/roadmap.md](docs/roadmap.md) — дорожная карта с milestones
- [docs/current-state.md](docs/current-state.md) — текущий статус
- [docs/architecture.md](docs/architecture.md) — архитектура
- [AGENTS.md](AGENTS.md) — гайд для AI-агентов

The implementation status is tracked in [docs/current-state.md](docs/current-state.md). The roadmap separates backend foundations from end-to-end browser milestones.
