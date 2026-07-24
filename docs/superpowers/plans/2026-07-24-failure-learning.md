# Failure learning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Первая волна `7.103`: ограниченная полоса извлечения памяти из проваленных задач — максимум 2 кандидата `failure_pattern`/`verification_rule` scope `experience`, confidence cap 0.6 (только Ask-гейт, без auto-promote).

**Architecture:** `crates/memory/src/extract.rs` — `extract_failure_candidates(llm_raw)`; `crates/server/src/task/memory.rs` — `FAILURE_EXTRACT_PROMPT` + ветка `!task_ok` в `persist_structured_memory` через существующий admit/gate/event-конвейер.

**Tech Stack:** Rust, существующий memory-пайплайн.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Из провала только Experience × {FailurePattern, VerificationRule}; cap 0.6/0.7; лимит 2; без эвристического fallback.
- Успешный путь не меняется.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: extract_failure_candidates

**Files:**
- Modify: `crates/memory/src/extract.rs`, `crates/memory/src/lib.rs`

- [ ] **Step 1: Unit tests first** — fact/preference/playbook из провала отброшены; failure_pattern/verification_rule оставлены с capped confidence; лимит 2; мусор/пусто → пусто; transient-инфра отфильтрована; гейт даёт Ask на capped кандидате.
- [ ] **Step 2: Implement + export**.
- [ ] **Step 3: Checks** — `cargo test -p evohime-memory extract`, Clippy.
- [ ] **Step 4: Commit** — `feat(memory): extract failure lessons`.

### Task 2: Server wiring + документация

**Files:**
- Modify: `crates/server/src/task/memory.rs`
- Modify: `AGENTS.md`, `docs/roadmap.md`, `docs/current-state.md`

- [ ] **Step 1: FAILURE_EXTRACT_PROMPT** — анализ провала: симптом, вероятная причина, verification_rule на будущее; JSON-only формат как в основном промпте.
- [ ] **Step 2: Ветка `!task_ok`** — LLM extract → `extract_failure_candidates` → существующий цикл admit/gate/events.
- [ ] **Step 3: Checks** — `cargo test --workspace`, Clippy, frontend build.
- [ ] **Step 4: Docs** — `7.103` 🟡 wave 1 ✅ (остаток — эскалация повторов, retrieval-приоритизация уроков).
- [ ] **Step 5: Commit** — `feat(memory): learn from failed tasks` и `docs: mark failure learning wave complete`.
