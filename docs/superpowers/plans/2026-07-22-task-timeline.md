# Task Timeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Реализовать roadmap 7.94: correlation id copy и server-provided latency bars в Tasks/Actions.

**Architecture:** Расширить protocol optional telemetry-полями, передать task duration из server observability в terminal events, сохранить данные в frontend view-model и отрисовать timeline/latency без клиентского бизнес-расчёта.

**Tech Stack:** Rust/Axum, shared JSON schema, generated TypeScript protocol, React/TypeScript/CSS, Cargo tests, Vite build.

## Global Constraints

- Не редактировать `frontend/web/src/protocol.generated.ts` вручную.
- Не добавлять бизнес-логику в frontend; UI отображает server events.
- Сохранять backward compatibility optional protocol fields.
- После изменения файлов репозитория создать git-коммит; push выполнять только по прямому запросу.

### Task 1: Protocol telemetry fields

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json`
- Modify: `crates/protocol/src/lib.rs`
- Regenerate: `frontend/web/src/protocol.generated.ts`
- Test: `crates/protocol/src/lib.rs`

- [ ] Добавить optional `correlation_id` и `duration_ms` к `ActionLoggedEvent`, optional `duration_ms` к `TaskCompletedEvent` и `TaskFailedEvent`.
- [ ] Добавить Rust-поля `Option<Uuid>`/`Option<u64>` с `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- [ ] Добавить тест round-trip optional fields и запустить `npm run generate:protocol`.

### Task 2: Server duration propagation

**Files:**
- Modify: `crates/server/src/observability.rs`
- Modify: `crates/server/src/task/pipeline.rs`
- Modify: `crates/server/src/ws.rs`
- Test: `crates/server/src/observability.rs`

- [ ] Добавить read-only метод task duration, возвращающий elapsed milliseconds для открытой задачи.
- [ ] В terminal success/failure events передать duration и correlation id равный task id.
- [ ] В action logging helper передавать correlation id; duration оставлять `None`, если событие не представляет завершённый timed span.
- [ ] Тестировать duration без sleep-зависимости через существующие observability counters/импульсы.

### Task 3: Frontend timeline view-model

**Files:**
- Modify: `frontend/web/src/types.ts`
- Modify: `frontend/web/src/hooks/applyServerEvent.ts`
- Modify: `frontend/web/src/panels/TasksPanel.tsx`
- Modify: `frontend/web/src/panels/ActionsPanel.tsx`
- Modify: `frontend/web/src/lib/format.ts`
- Test: existing frontend test setup or pure formatter tests

- [ ] Write failing tests for latency labels, bounded bar width, and correlation copy state.
- [ ] Store optional telemetry fields in `TaskView`/`ActionView` and render server-provided values.
- [ ] Add copy button with `navigator.clipboard.writeText`, fallback error state, and no fake id generation.
- [ ] Render max-relative latency bars with accessible labels and neutral state for missing values.

### Task 4: Styling, docs, verification, commit

**Files:**
- Modify: `frontend/web/src/styles/panels.css`
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`
- Modify: `docs/development-plan.md`
- Modify: `AGENTS.md`

- [ ] Add compact timeline/latency styles consistent with existing panels.
- [ ] Mark 7.94 and request-context phase as complete; set next roadmap item.
- [ ] Run protocol generation drift, Rust tests/Clippy, frontend typecheck/build, and `git diff --check`.
- [ ] Commit only scoped changes with message `feat(ui): add task timeline latency telemetry`.
