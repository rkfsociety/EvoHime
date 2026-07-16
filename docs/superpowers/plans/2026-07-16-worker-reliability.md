# Worker Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Process health watchdog, per-job stall detection, and typed worker payload schemas.

**Architecture:** Pull health/job heartbeats from Python worker; Rust owns durable retry via existing backoff/`max_attempts`.

**Tech Stack:** Python stdlib worker, Rust Axum server, PostgreSQL `worker_jobs`, reqwest.

## Global Constraints

- Worker stays process-local (no Postgres).
- Auto-retry on process death / stall while attempts remain.
- Phases: process health → per-job stall → schemas.
- No new UI panels.

## File map

| File | Role |
| --- | --- |
| `workers/python/worker.py` | health fields, job heartbeat, payload validation |
| `workers/python/test_worker.py` | Python tests |
| `workers/python/README.md` | Contract docs |
| `crates/server/src/worker.rs` | health client, stall helpers, payload validation |
| `crates/server/src/app.rs` | health/stall interval config |
| `crates/server/src/main.rs` | health loop, stall in poll, validate on create |
| `docs/roadmap.md` / `docs/current-state.md` | status notes |

---

### Task 1: Phase 1 — process health

- [x] Add failing Python test: `/health` includes `started_at` and `pid`
- [x] Implement health enrichment in `worker.py`
- [x] Add Rust `WorkerHealth` + `WorkerClient::health` + unit test
- [x] Add config env vars + `worker_health_loop` with started_at/stale recover
- [x] Run Python + targeted Rust tests

### Task 2: Phase 2 — per-job stall

- [x] Failing Python test: running job snapshot has fresh `heartbeat_at`
- [x] Heartbeat updater in JobService while running
- [x] Rust: parse `heartbeat_at`, `heartbeat_is_stale`, use in `run_worker_job`
- [x] Config `WORKER_JOB_STALL_SECS`
- [x] Tests green

### Task 3: Phase 3 — schemas

- [x] Failing tests: bad `text.stats` payload rejected
- [x] `validate_task_payload` in Python + Rust (mirrored rules)
- [x] Wire into submit paths
- [x] Update README + roadmap notes
- [x] Full verification: `python -m unittest` + `cargo test -p evohime-server`
