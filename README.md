# EvoHime

Web-first AI-agent monorepo. Browser-only: no Electron, desktop, or mobile clients.

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
Browser workspace
  → session/task lifecycle
  → streamed LiteRouter response
  → filesystem/shell/git tools
  → approval flow
  → chat, settings, events, tasks, actions, terminal, files, editor, git panels
  → history stored in PostgreSQL
```

Stage 5 is complete. LiteRouter is the active model provider. The tool runtime includes sandboxed filesystem read/write/patch/search, shell execution, and Git operations. Task lifecycle, task steps, checkpoints, cancel/resume/retry commands, approval flow, and the Tasks/Actions UI are available in the current browser workflow. Stage 6 remains planned for project index, MCP, memory, additional providers, Python workers, and browser automation tools.

## Local development without Docker

The recommended Windows path runs Rust, Vite, and a portable PostgreSQL 16 process directly on the host. Administrator rights and Docker are not required.

First setup:

```powershell
.\scripts\setup-local.ps1 -InstallPostgres -ApplyMigrations
```

Start the tray launcher:

```powershell
.\start-dev.ps1
```

The launcher starts PostgreSQL, the Rust backend, and the Vite frontend. The backend can read the host GitHub CLI login:

```powershell
gh auth status
Invoke-RestMethod http://localhost:3000/api/auth/github
```

The convenience files `start-dev.bat` and `start-dev.vbs` start the same local launcher.

## Manual local development

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

## Alternative Docker Compose deployment

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
