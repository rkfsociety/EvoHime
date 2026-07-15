# Task orchestration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать управляемые задачи с планом, параллельными независимыми tools, cancel/resume/retry и recovery после restart сервера.

**Architecture:** Protocol описывает команды и события. Storage хранит задачи, шаги и checkpoint; task-engine владеет переходами состояний и восстановлением. Tool runtime получает cancellation token и запускает dependency-ready шаги конкурентно. Server связывает WebSocket-команды с engine, frontend отображает Tasks и Actions из серверных событий.

**Tech Stack:** Rust, Tokio, Axum, SQLx/PostgreSQL, serde JSON, React + TypeScript + Vite.

## Global Constraints

- Сервер остаётся источником истины; frontend только отображает события и вызывает команды.
- Изменения схемы БД выполняются отдельной SQL migration.
- Protocol workflow: JSON Schema → Rust → `npm run generate:protocol` → TypeScript.
- Отмена должна быть cooperative и не оставлять task в `running`.
- Независимые steps выполняются одновременно, зависимые ждут завершения prerequisites.

### Task 1: Protocol и persistence model

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json`
- Modify: `crates/protocol/src/lib.rs`
- Modify: `crates/storage/src/lib.rs`
- Create: `migrations/0003_task_orchestration.sql`
- Test: `crates/protocol/src/lib.rs`

- [x] Добавить `task.cancel`, `task.resume`, `task.retry`, события task status/step/action и типы checkpoint.
- [x] Добавить таблицы `task_steps`, `task_checkpoints` и запросы для списка задач, шагов и recovery.
- [x] Проверить сериализацию новых команд/событий и migration SQL.

### Task 2: Task engine state machine

**Files:**
- Modify: `crates/task-engine/src/lib.rs`
- Create: `crates/task-engine/tests/orchestration.rs`

- [x] Реализовать переходы `running → cancelling → cancelled`, `paused → running`, `failed → retrying → running`.
- [x] Реализовать storage checkpoint API и recovery `running → paused` при старте.
- [x] Написать unit-тесты cancel, resume, retry и recovery state machine.

### Task 3: Planner и parallel tool executor

**Files:**
- Modify: `crates/agent-runtime/src/agent_loop.rs`
- Modify: `crates/tool-runtime/src/registry.rs`
- Modify: `crates/tool-runtime/src/lib.rs`
- Create: `crates/tool-runtime/tests/orchestration.rs`

- [ ] Перевести план из model response в структурированные steps с dependencies, сохранив fallback для mock provider.
- [x] Добавить `CancellationToken`-совместимый execution API и `execute_parallel`.
- [x] Проверить параллельный запуск независимых tools и cooperative cancellation.

### Task 4: Server commands, recovery bootstrap и API

**Files:**
- Modify: `crates/server/src/app.rs`
- Modify: `crates/server/src/main.rs`

- [x] Обработать WebSocket-команды cancel/resume/retry и публиковать status/action events.
- [x] Инициализировать recovery при запуске сервера и подключить cancellation handles.
- [x] Добавить HTTP endpoint для списка задач.

### Task 5: Tasks/Actions UI

**Files:**
- Modify: `frontend/web/src/app.tsx`
- Modify: `frontend/web/src/protocol.ts`
- Modify: `frontend/web/src/styles.css`

- [x] Отображать список задач, статусы, шаги и кнопки cancel/resume/retry.
- [x] Отображать журнал действий из серверных событий.
- [x] Проверить TypeScript build и round-trip команд.

### Task 6: Verification

- [ ] Запустить `cargo fmt --check` (в окружении отсутствует Rust toolchain).
- [ ] Запустить `cargo test --workspace` (в окружении отсутствует Rust toolchain).
- [x] Запустить `npm run generate:protocol` и frontend typecheck.
- [ ] Сверить каждый deliverable 5.1–5.10: planner parser и automatic resume worker требуют отдельного runtime-среза.
