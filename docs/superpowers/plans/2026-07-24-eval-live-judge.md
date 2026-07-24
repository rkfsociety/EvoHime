# Eval live + judge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Вторая волна `7.101`: live-режим golden tasks против `ModelGateway::try_from_env()` и LLM-as-judge по `rubric`; CI остаётся mock-детерминированным.

**Architecture:** `run_golden_task` рефакторится в `run_golden_task_with_gateway(task, &gateway, judge)`; mock-обёртка сохраняет прежнее поведение. Judge: `judge_prompt`, `parse_judge_verdict` (первый JSON-объект из текста), `judge_final_answer` (stream_chat → собранный текст → вердикт). CLI получает `--live`/`--judge`.

**Tech Stack:** Rust, futures-util (сбор stream), существующие ModelGateway/MockProvider.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- CI-тест — только mock; live только по флагу CLI.
- Ошибки провайдера не печатают ключи; непарсибельный вердикт = провал.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: Обобщение harness + judge

**Files:**
- Modify: `crates/evals/src/lib.rs`, `crates/evals/Cargo.toml` (futures-util)

- [ ] **Step 1: Unit tests first** — `parse_judge_verdict`: чистый JSON, JSON в markdown-обёртке, мусор → err, `pass` обязателен; judge-промпт содержит rubric/ответ.
- [ ] **Step 2: Refactor** — `run_golden_task_with_gateway`; `rubric: Option<String>`; `EvalReport.judge: Option<JudgeVerdict>`; judge=false в mock-пути.
- [ ] **Step 3: Integration** — mock-gateway через generic-путь: expect проходит; judge-сценарий: вторая задача с rubric + mock-вердикт `{"pass":false,...}` → задача провалена.
- [ ] **Step 4: Checks** — `cargo test -p evohime-evals`, Clippy.
- [ ] **Step 5: Commit** — `feat(evals): generalize harness with llm judge`.

### Task 2: CLI + документация

**Files:**
- Modify: `crates/evals/src/bin/evohime-eval.rs`
- Modify: `AGENTS.md`, `docs/roadmap.md`, `docs/current-state.md`

- [ ] **Step 1: CLI** — `--live` (try_from_env, ошибка без конфигурации), `--judge` (только с --live), печать score/reason.
- [ ] **Step 2: Checks** — workspace test, Clippy, mock-смок CLI.
- [ ] **Step 3: Docs** — `7.101` ✅ (остаток side-by-side провайдеры → вне пункта), команды.
- [ ] **Step 4: Commit** — `feat(evals): add live eval mode` и `docs: mark eval live judge complete`.
