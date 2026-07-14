# EvoHime

Web-first AI-agent monorepo.

## Что уже есть

- Rust backend scaffold with HTTP + WebSocket
- React + TypeScript frontend scaffold
- PostgreSQL schema and migrations
- Docker Compose setup
- Vertical slice for:
  - user message
  - task creation
  - streaming response
  - `filesystem.read`
  - PostgreSQL history

## Локальный запуск

Frontend:

```bash
cd frontend/web
npm install
npm run dev
```

Backend requires Rust toolchain and PostgreSQL. The server expects:

- `DATABASE_URL`
- `BIND_ADDR`

See `.env.example`.

