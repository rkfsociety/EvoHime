# Project Index Context Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Реализовать рабочий `project-index` для поиска релевантного контекста по workspace и подключить его в `agent-runtime`.

**Architecture:** `project-index` сканирует workspace на лету, собирает релевантные текстовые сниппеты и возвращает их с ранжированием. `agent-runtime` использует индекс перед вызовом модели, чтобы дополнять prompt найденным контекстом. Документация отражает новый stage 6 slice и текущий статус.

**Tech Stack:** Rust 2021, walkdir, serde, chrono, Tokio, existing `agent-runtime` and `protocol`.

## Global Constraints

- Browser-first, no desktop or mobile clients.
- Frontend stays free of business logic.
- Keep paths inside `WORKSPACE_ROOT`.
- Minimize diff scope.
- Update roadmap and current-state docs alongside code.

### Task 1: Project index crate

**Files:**
- Modify: `crates/project-index/src/lib.rs`
- Modify: `crates/project-index/Cargo.toml`
- Test: `crates/project-index/src/lib.rs`

- [x] Add a workspace-root-aware index that can search text files and rank snippets.
- [x] Add unit tests for ranking, limit handling, and path filtering.

### Task 2: Agent runtime integration

**Files:**
- Modify: `crates/agent-runtime/src/agent_loop.rs`
- Modify: `crates/agent-runtime/Cargo.toml`
- Modify: `crates/agent-runtime/src/lib.rs`
- Test: `crates/agent-runtime/tests/agent_loop.rs`

- [x] Inject project-index context into the model prompt without breaking the current planning flow.
- [x] Add tests that verify the model prompt includes relevant workspace context.

### Task 3: Docs and verification

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`
- Modify: `crates/project-index/README.md`
- Modify: `crates/agent-runtime/README.md`

- [x] Update stage 6 roadmap text and current-state status.
- [x] Run crate tests and workspace checks.
