# Assistant Reply и подтверждение плана Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать Stage 7.35: пользователь видит план агента, редактирует его и подтверждает выполнение до запуска инструментов.

**Architecture:** Сервер сохраняет ожидающий план в существующем PostgreSQL checkpoint и переводит задачу в `paused`; WebSocket-команды approve/reject изменяют checkpoint только после валидации. Frontend редактирует локальную копию плана и отправляет полный утверждённый план, после чего получает обычные task/step events.

**Tech Stack:** Rust/Axum/WebSocket, `evohime-protocol`, PostgreSQL через `evohime-storage`, React/TypeScript/Vite.

## Global Constraints

- Не редактировать `frontend/web/src/protocol.generated.ts` вручную; менять schema/Rust и запускать codegen.
- Не добавлять бизнес-логику в frontend: UI только редактирует и отправляет серверное решение.
- Сохранять workspace/session isolation и существующую FSM задач.
- Каждая production-функция получает тест до реализации по TDD.

---

### Task 1: Protocol and plan validation

**Files:**
- Modify: `crates/protocol/schema/evohime.protocol.schema.json`
- Modify: `crates/protocol/src/lib.rs`
- Modify: `crates/server/src/task/steps.rs`
- Test: existing unit tests in the same Rust modules

- [ ] Write failing tests for `task.plan.approve`/`task.plan.reject` serialization and plan validation (duplicate ids, unknown dependency, cycle, valid edited plan).
- [ ] Run `cargo test -p evohime-protocol -p evohime-server plan` and verify the new tests fail because the command/validator does not exist.
- [ ] Add schema and Rust command variants with `task_id` and `plan` payload, plus a focused `validate_plan` helper.
- [ ] Run the focused tests and regenerate TypeScript protocol types with `npm run generate:protocol`.
- [ ] Commit protocol and validator changes.

### Task 2: Server pause and approve/reject flow

**Files:**
- Modify: `crates/server/src/ws.rs`
- Modify: `crates/server/src/task/pipeline.rs`
- Modify: `crates/server/src/task/steps.rs`
- Modify: `crates/task-engine/src/lib.rs` only if an existing transition helper cannot express the pause/resume path
- Test: server/task unit tests and task-engine lifecycle tests

- [ ] Write failing tests for persisting `plan_approval_required`, rejecting edits without changing checkpoint, and approving a valid edited plan.
- [ ] Run the focused server/task tests and verify failure.
- [ ] On first `AgentPlanUpdated`, persist the plan, pause the task, emit status, and stop the agent before tool execution.
- [ ] Handle approve/reject only for the current session and expected paused state; replace checkpoint/task steps for approve, resume execution, and cancel on reject.
- [ ] Run focused Rust tests and then `cargo test --workspace`.
- [ ] Commit the server flow.

### Task 3: Frontend plan editor

**Files:**
- Modify: `frontend/web/src/protocol.ts` only for exports if needed
- Modify: `frontend/web/src/hooks/applyServerEvent.ts`
- Modify: `frontend/web/src/app.tsx` or an extracted plan panel component following existing panel conventions
- Test: frontend tests colocated with the plan editor/event handler

- [ ] Write failing tests for rendering an approval editor from `plan_approval_required`, editing a description/dependency, and sending approve/reject commands.
- [ ] Run the focused frontend test/build command and verify failure.
- [ ] Add the editor with validation feedback, approve/reject buttons, and WS payloads using generated types.
- [ ] Ensure reconnect/history restores the pending plan from durable events without auto-submitting it.
- [ ] Run `npm run build` from `F:/github/EvoHime/frontend/web`.
- [ ] Commit the frontend flow.

### Task 4: End-to-end verification and docs

**Files:**
- Modify: `docs/current-state.md`
- Modify: `docs/roadmap.md`

- [ ] Run `cargo test --workspace` and `npm run build` from `frontend/web` with fresh output.
- [ ] Run `git diff --check` and inspect `git status --short --branch`.
- [ ] Mark `7.35` complete and update current-state to identify the next item as `7.36`.
- [ ] Commit documentation and final verification changes.
