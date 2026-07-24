# Cloud sync push Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Первая вертикаль `7.99`: owner может отправить свой `BackupDump` на настроенный remote endpoint (`PUT` JSON + checksum), история попыток хранится в `sync_runs`, статус доступен через API без раскрытия секретов.

**Architecture:** `sync_runs` в PostgreSQL хранит историю push для каждого оператора. Конфигурация remote (`EVOHIME_SYNC_URL`, `EVOHIME_SYNC_TOKEN`) читается из env; server-модуль `sync` собирает dump через `evohime_storage::collect_backup`, шлёт его `reqwest`-клиентом без redirect и с таймаутом, и фиксирует результат. Feature flag `EVOHIME_FEATURE_CLOUD_SYNC` включает endpoints и отражается в `/api/features`.

**Tech Stack:** Rust/Axum, SQLx/PostgreSQL migration, reqwest (rustls, no-redirect), sha2, существующие `OperatorIdentity`/`require_owner`, OpenAPI generation script, frontend только тип `FeatureFlags`.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- `EVOHIME_SYNC_TOKEN` и полный URL с credentials не попадают в логи, ошибки, ответы API и таблицу runs.
- HTTP-клиент: только `http`/`https`, redirect запрещён, таймаут обязателен.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: sync_runs storage + migration

**Files:**
- Create: `migrations/0026_sync_runs.sql`
- Create: `crates/storage/src/sync.rs`
- Modify: `crates/storage/src/lib.rs`
- Test: `crates/storage/src/sync.rs` unit + integration tests

**Interfaces:**
- `SyncRunRow { id, operator_id, started_at, finished_at, status, bytes_total, checksum, error }`
- `start_sync_run(pool, operator_id) -> SyncRunRow`
- `finish_sync_run(pool, run_id, status, bytes_total, checksum, error) -> Option<SyncRunRow>`
- `list_sync_runs(pool, operator_id, limit) -> Vec<SyncRunRow>`
- `find_active_sync_run(pool, operator_id, stale_after) -> Option<SyncRunRow>` — `running` свежее порога.

- [ ] **Step 1: Migration** — таблица `sync_runs` с CHECK по status (`running`/`success`/`failed`), FK на `operators`, индекс `(operator_id, started_at DESC)`.
- [ ] **Step 2: Storage functions** — по паттерну `scheduled.rs`; unit-тест на допустимые статусы, integration-тесты (test_db) на lifecycle и изоляцию операторов.
- [ ] **Step 3: Checks** — `cargo test -p evohime-storage sync`, `cargo clippy -p evohime-storage --all-targets -- -D warnings`.
- [ ] **Step 4: Commit** — `feat(storage): add sync run history`.

### Task 2: sync config, push engine и API

**Files:**
- Create: `crates/server/src/sync_api.rs`
- Modify: `crates/server/src/routes.rs`, `crates/server/src/features.rs`, `crates/server/Cargo.toml` (sha2)
- Test: unit-тесты в `sync_api.rs`

**Interfaces:**
- `SyncConfig::from_env()` → `Option<SyncConfig { url, token }>`; валидация схемы и отсутствия userinfo.
- `GET /api/sync/status` (owner-only): `{ feature_enabled, configured, remote_host, runs }`.
- `POST /api/sync/push` (owner-only): 409 при активном run, 503 при отсутствии конфигурации; собирает dump, SHA-256, `PUT` с `X-EvoHime-Backup-Checksum`, финализирует run.

- [ ] **Step 1: Unit tests first** — валидация URL (`ftp://` → err, userinfo → err, https → ok), редакция host, checksum hex, усечение ошибок.
- [ ] **Step 2: Implement module** — reqwest client `redirect(Policy::none())`, timeout 30s; ошибки remote усечены до 512 символов; токен только в заголовке запроса.
- [ ] **Step 3: Feature flag** — `cloud_sync` в `FeatureFlags` (`EVOHIME_FEATURE_CLOUD_SYNC`, default true); гейт в handlers.
- [ ] **Step 4: Routes + OpenAPI** — маршруты в `routes.rs`, `node scripts/generate-openapi.mjs`, коммит `docs/openapi.json` + `frontend/web/src/api/generated.ts`.
- [ ] **Step 5: Checks** — `cargo test -p evohime-server`, Clippy workspace.
- [ ] **Step 6: Commit** — `feat(sync): add cloud sync push API`.

### Task 3: Frontend flag + документация

**Files:**
- Modify: `frontend/web/src/api/features.ts`
- Modify: `docs/roadmap.md`, `docs/development-plan.md`, `docs/current-state.md`, `AGENTS.md`, `.env.example`

- [ ] **Step 1: FeatureFlags type** — добавить `cloud_sync: boolean`; `npm run typecheck`/build.
- [ ] **Step 2: Docs** — `7.99` пометить первой вертикалью (push) с notes; env-пример дополнить `EVOHIME_SYNC_URL`/`EVOHIME_SYNC_TOKEN`/`EVOHIME_FEATURE_CLOUD_SYNC`.
- [ ] **Step 3: Commit** — `docs: mark cloud sync push wave complete`.
