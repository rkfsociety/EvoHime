---
name: development_workflow
description: "Как разрабатывать EvoHime, сборка, тестирование, запуск локально"
metadata: 
  node_type: memory
  type: reference
  originSessionId: adc991a0-0ba6-4d74-ace4-d6cf01d7403e
  modified: 2026-07-24T16:14:08.409Z
---

## Локальная Разработка

### Запуск (Рекомендуемый способ)

**Windows:**
```powershell
.\start-dev.ps1
```

Этот скрипт:
- Запускает PostgreSQL 16 (portable, если не запущена)
- Запускает backend (Rust сервер)
- Запускает frontend (Vite dev сервер)
- Открывает браузер на localhost

### Требования

- **Windows 11 Pro** (может работать и на других версиях)
- **PostgreSQL 16** (portable версия или система)
- **Rust toolchain** (последняя stable)
- **Node.js** + npm (для frontend)

### Dev Containers

Для cross-platform:
```bash
# VS Code Dev Containers
# Используй .devcontainer/devcontainer.json
```

Docker Compose:
```bash
docker-compose -f .devcontainer/docker-compose.yml up
```

## Сборка

### Backend (Rust)

```bash
cargo build
cargo build --release
```

Основной entrypoint: `crates/server/src/main.rs`

Binaries выходят в `target/debug/` или `target/release/`

### Frontend (React)

```bash
cd frontend/web
npm install
npm run dev      # dev server
npm run build    # production bundle
npm run typecheck # TypeScript check
```

### OpenAPI Schema

Auto-generated from Rust endpoints:

```bash
cargo run --bin evohime-generate-openapi
```

Выходит в `docs/openapi.json`, затем используется фронтендом.

## CI/CD (GitHub Actions)

`.github/workflows/rust.yml`:
- typecheck frontend (npm run typecheck)
- production build (npm run build)
- PostgreSQL integration tests (cargo test --features test-db)
- gitleaks security scan

## Тестирование

### Unit Tests

```bash
cargo test
```

### Integration Tests

```bash
cargo test --features test-db
```

Требует PostgreSQL (или docker).

### Golden Tests (Evals)

```bash
cargo run --bin evohime-eval
```

Тесты в `crates/evals/golden/*.json`.

## Кодовая база

### Форматирование

```bash
cargo fmt          # Rust
cd frontend/web && npm run format  # TypeScript
```

### Linting

```bash
cargo clippy
cd frontend/web && npm run lint
```

## Миграции БД

Находятся в `migrations/`:
- 0001–0013+: sessions, tasks, events
- 0013: memory_items (6.16+)
- 0024+: scheduled, workers, sync, operators, etc.

Миграции автоматически применяются при запуске сервера (если используется встроенный migration runner).

## Переменные окружения

**Backend (.env.example → .env):**
```
BIND_ADDR=127.0.0.1:3000
DATABASE_URL=postgres://...
LITEROUTER_API_KEY=...
LITEROUTER_MODEL=deepseek:free
```

**Frontend:**
- Конфигурируется через `frontend/web/src/api/client.ts`
- Обычно указывает на `http://localhost:3000`

## Development Tips

1. **Hot reload:** Frontend автоматически перезагружается (Vite)
2. **Backend changes:** Нужно перекомпилировать (`cargo build`)
3. **DB schema changes:** Добавь новую миграцию в `migrations/`
4. **Protocol changes:** JSON Schema в `crates/protocol/schema/` → re-export в frontend
5. **New tools:** Добавь в `crates/tool-runtime/src/tools/`
6. **New panels:** Добавь в `frontend/web/src/panels/`

## Deployment

1. **Build:** `npm run build` (frontend) + `cargo build --release` (backend)
2. **Database:** PostgreSQL 16 (с применёнными миграциями)
3. **Secrets:** LITEROUTER_API_KEY, DATABASE_URL
4. **Run:** `./target/release/evohime-server` или docker image
5. **Port:** По умолчанию 3000 (BIND_ADDR)

## Documentation

- `docs/current-state.md` — стадия, endpoints, таблицы
- `docs/development-plan.md` — детальный plan
- `docs/roadmap.md` — milestones
- `docs/superpowers/plans/` — specs фич
- `AGENTS.md` — описание Agent типов в проекте
