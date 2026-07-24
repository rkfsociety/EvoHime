# Eval harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Первая волна `7.101`: крейт `evohime-evals` — golden tasks (JSON) против реального `run_agent_loop` + `ToolRegistry` с mock-моделью; интеграционный тест в CI и CLI-отчёт.

**Architecture:** `crates/evals/src/lib.rs` — формат задач (serde), конвертация script → `ChatResult`-последовательность для `MockProvider::with_tool_call_sequence`, `run_golden_task` (temp workspace, seed-файлы, agent loop, проверка ожиданий) → `EvalReport`. `golden/*.json` — задачи. `tests/golden.rs` — раннер. `src/bin/evohime-eval.rs` — локальный отчёт.

**Tech Stack:** Rust, tokio, serde_json, tempfile, evohime-agent-runtime / model-gateway / tool-runtime.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Harness без сети/БД/реального LLM; задачи изолированы по temp workspace.
- CI покрывается существующими `--workspace` jobs; workspace members обновить в корневом Cargo.toml.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: Крейт + harness

**Files:**
- Create: `crates/evals/Cargo.toml`, `crates/evals/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Unit tests first** — парсинг golden JSON, script → ChatResult (tool step с auto-id, reply step), `check_expectations` позитив/негатив (missing substring, missing file).
- [ ] **Step 2: Implement** — `GoldenTask`, `Expectations`, `EvalReport { name, passed, failures }`, `run_golden_task`; AgentConfig с `memory_pool: None`, temp demo file.
- [ ] **Step 3: Checks** — `cargo test -p evohime-evals`, Clippy.
- [ ] **Step 4: Commit** — `feat(evals): add golden task harness`.

### Task 2: Golden tasks + раннер + CLI

**Files:**
- Create: `crates/evals/golden/reply-only.json`, `read-file.json`, `write-file.json`
- Create: `crates/evals/tests/golden.rs`, `crates/evals/src/bin/evohime-eval.rs`

- [ ] **Step 1: Три задачи** — reply-only; read (seed-файл → ответ из содержимого); write (создание файла + проверка содержимого).
- [ ] **Step 2: Раннер-тест** — каталог golden обязателен и непуст; провал печатает failures по задаче.
- [ ] **Step 3: CLI** — таблица имя/статус/failures, exit 1 при провале.
- [ ] **Step 4: Checks** — `cargo test --workspace`, Clippy workspace, `cargo run -p evohime-evals --bin evohime-eval` локальный smoke.
- [ ] **Step 5: Commit** — `feat(evals): add golden tasks and runner`.

### Task 3: Документация

**Files:**
- Modify: `AGENTS.md`, `docs/roadmap.md`, `docs/current-state.md`

- [ ] **Step 1: Docs** — `7.101` 🟡 wave 1 ✅ (остаток — LLM-as-judge, live-provider сравнение); команда eval в AGENTS.md.
- [ ] **Step 2: Commit** — `docs: mark eval harness wave complete`.
