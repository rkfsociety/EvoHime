# Deep Health Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Реализовать 7.96: bounded deep health checks для PostgreSQL, Python worker и workspace disk.

**Architecture:** Сохранить текущий `/health` как liveness. Новый handler параллельно запускает независимые probes через `tokio::join!` и `timeout`, затем строит безопасный aggregate response с HTTP 200/503.

**Tech Stack:** Rust/Axum, Tokio timeout/join, SQLx PostgreSQL, existing WorkerClient, serde JSON.

## Global Constraints

- Не раскрывать database URLs, worker URLs или filesystem paths/errors в public response.
- Не блокировать handler дольше общего bounded timeout.
- Не менять контракт `{status:"ok"}` у `/health`.
- После изменения файлов создать git-коммит; push только по прямому запросу.

### Task 1: Health status model

**Files:**
- Modify: `crates/server/src/health.rs`
- Modify: `crates/server/src/main.rs`
- Test: `crates/server/src/health.rs`

- [ ] Написать failing tests для `ok`, `degraded`, `failed` aggregation.
- [ ] Добавить serializable component/aggregate types and safe status mapping.
- [ ] Keep `/health` unchanged and add pure aggregation tests.

### Task 2: Deep probes and route

**Files:**
- Modify: `crates/server/src/health.rs`
- Modify: `crates/server/src/routes.rs`
- Modify: `crates/server/src/auth.rs`

- [ ] Add parallel database, worker, and workspace probes with per-probe timeout.
- [ ] Return 503 only for database/disk critical failure; worker outage is degraded.
- [ ] Keep deep endpoint public and test auth public-path behavior.

### Task 3: Docs and verification

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`
- Modify: `AGENTS.md`

- [ ] Mark 7.96 complete and set next roadmap item.
- [ ] Run full Rust tests/Clippy, frontend typecheck/build, and `git diff --check`.
- [ ] Commit `feat(health): add bounded deep health checks`.
