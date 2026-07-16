# Python worker Rust bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Связать Rust server с Python worker и добавить durable worker jobs с retry.

**Architecture:** Rust server создаёт запись job, отправляет HTTP job worker’у, затем poll’ит результат и сохраняет его. Python остаётся stateless execution boundary с process-local queue.

**Tech Stack:** Rust/Axum/sqlx/reqwest, PostgreSQL, Python standard library.

## Global Constraints

- Worker не получает доступ к PostgreSQL.
- Frontend не содержит бизнес-логики worker jobs.
- Все новые публичные server paths имеют тестируемые JSON-контракты.
- После изменения миграций запускаются migration-aware Rust tests/full suite.

### Task 1: Python ML handler

**Files:** `workers/python/worker.py`, `workers/python/test_worker.py`, `workers/python/README.md`

- [ ] Написать failing test для `text.keywords`.
- [ ] Добавить handler с ограничением размера текста и детерминированным списком частотных слов.
- [ ] Запустить `python -m unittest discover -s workers/python -p "test_*.py"`.

### Task 2: Durable storage

**Files:** `migrations/0010_worker_jobs.sql`, `crates/storage/src/lib.rs`

- [ ] Добавить таблицу `worker_jobs`.
- [ ] Добавить `WorkerJobRow` и create/get/update/retry storage functions.
- [ ] Проверить компиляцию storage.

### Task 3: Rust worker client and API

**Files:** `crates/server/src/worker.rs`, `crates/server/src/main.rs`, `crates/server/Cargo.toml`

- [ ] Добавить сериализуемые request/response types и reqwest client.
- [ ] Добавить submit/poll/retry handlers и routes.
- [ ] Вернуть 503/502 на недоступность или некорректный worker response.

### Task 4: Verification and bookkeeping

- [ ] Запустить Python tests и `cargo fmt --check`.
- [ ] Запустить `cargo test`.
- [ ] Обновить current-state/roadmap и сделать один итоговый commit.
