# Log Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Реализовать 7.95: redaction динамических tracing-полей и sampling повторяющихся worker health warning’ов.

**Architecture:** Server использует общий helper над `evohime_memory::redact_secrets`; worker metrics хранит последнее сообщение health failure и ограничивает только идентичные повторения по интервалу. Уникальные ошибки и все counters сохраняются.

**Tech Stack:** Rust, tracing, tracing-subscriber, existing `evohime-memory` redaction, Tokio mutex/Instant.

## Global Constraints

- Не раскрывать API keys, bearer tokens, passwords, cookies или private keys в tracing.
- Не редактировать unrelated logs и не скрывать уникальные error/warn сообщения.
- После изменения файлов создать git-коммит; push только по прямому запросу.

### Task 1: Redaction helper

**Files:**
- Create: `crates/server/src/log_safety.rs`
- Modify: `crates/server/src/main.rs`
- Test: `crates/server/src/log_safety.rs`

- [ ] Написать failing tests для bearer/API-key/password redaction и benign text.
- [ ] Реализовать `pub fn redact_for_log(input: &str) -> String` через `evohime_memory::redact_secrets(input).text`.
- [ ] Запустить server log-safety tests и убедиться, что секреты не встречаются в результате.

### Task 2: Apply redaction at logging boundaries

**Files:**
- Modify: `crates/server/src/api_error.rs`
- Modify: `crates/server/src/worker_observability.rs`
- Modify: `crates/server/src/worker_api.rs`
- Modify: `crates/server/src/otel.rs`

- [ ] Добавить redaction к internal error details, worker error/retry reasons и OTLP endpoint log field.
- [ ] Не менять HTTP error body и counters.
- [ ] Добавить regression assertion для redacted internal log message helper.

### Task 3: Bounded repeated health sampling

**Files:**
- Modify: `crates/server/src/worker_observability.rs`
- Test: `crates/server/src/worker_observability.rs`

- [ ] Написать failing test для одинаковой ошибки в пределах interval и для новой ошибки.
- [ ] Добавить `EVOHIME_LOG_SAMPLE_SECS`, default 30, с clamp к безопасному диапазону.
- [ ] Сохранять counters/last health всегда; `tracing::warn!` выполнять только при первом/новом/просроченном сообщении.

### Task 4: Verification, docs, commit

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`
- Modify: `AGENTS.md`

- [ ] Обновить 7.95 и следующий пункт roadmap.
- [ ] Запустить `cargo test --workspace --all-features --all-targets`, Clippy, frontend typecheck/build и `git diff --check`.
- [ ] Создать коммит `feat(security): harden tracing redaction and sampling`.
