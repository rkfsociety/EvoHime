# Backup restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Вторая волна `7.99`: идемпотентный импорт `BackupDump` в PostgreSQL (`restore_backup`) и CLI `evohime-import`, чтобы backup переносился на другую машину.

**Architecture:** `crates/storage/src/restore.rs` вставляет сессии/messages/tasks/steps/events и memory items в одной транзакции; конфликт по `id` — skip (сессия пропускается целиком). Ссылки memory (`source_session_id`, `source_task_id`, `supersedes`) восстанавливаются через `(SELECT id FROM … WHERE id = $n)`-подзапросы, что автоматически NULL-ит отсутствующие цели; `supersedes` — вторым проходом. CLI зеркален `evohime-export`.

**Tech Stack:** Rust, SQLx transaction, существующие DTO `BackupDump`, `find_operator_by_name` в operators storage.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Импорт в одной транзакции; никаких частичных восстановлений.
- `operator_id` всех строк — целевой оператор, не значения из дампа.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: restore engine в storage

**Files:**
- Create: `crates/storage/src/restore.rs`
- Modify: `crates/storage/src/lib.rs`, `crates/storage/src/operators.rs` (`find_operator_by_name`)
- Test: unit + integration тесты в `restore.rs`

**Interfaces:**
- `RestoreReport { sessions_inserted, sessions_skipped, messages_inserted, tasks_inserted, steps_inserted, events_inserted, memory_inserted, memory_skipped }`
- `restore_backup(pool, operator_id, &BackupDump) -> Result<RestoreReport, StorageError>`
- `validate_backup_header(&BackupDump) -> Result<(), StorageError>`

- [ ] **Step 1: Unit tests first** — заголовок формата/версии, пустой дамп → нулевой отчёт.
- [ ] **Step 2: Implement** — транзакция; INSERT сессий `ON CONFLICT (id) DO NOTHING` с RETURNING для определения skip; messages/tasks/steps/events только для новых сессий; memory items двумя проходами.
- [ ] **Step 3: Integration tests** — round-trip export→restore под нового оператора; повторный restore (все skipped); NULL-инг битых ссылок.
- [ ] **Step 4: Checks** — `cargo test -p evohime-storage restore`, Clippy.
- [ ] **Step 5: Commit** — `feat(storage): restore sessions and memory from backup`.

### Task 2: CLI evohime-import

**Files:**
- Create: `crates/storage/src/bin/evohime-import.rs`
- Test: unit-тесты parse_args, smoke через integration

- [ ] **Step 1: CLI** — `--input`, `--operator-name` (default `local-owner`); чтение файла, `serde_json` parse, `validate_backup_header`, `restore_backup`, печать отчёта.
- [ ] **Step 2: Checks** — `cargo test -p evohime-storage`, Clippy workspace, `cargo run -p evohime-storage --bin evohime-import -- --input` smoke на файле из export.
- [ ] **Step 3: Commit** — `feat(cli): add evohime import command`.

### Task 3: Документация

**Files:**
- Modify: `docs/roadmap.md`, `docs/current-state.md`, `AGENTS.md`

- [ ] **Step 1: Docs** — `7.99` notes дополнить wave 2 (restore/import); команды в AGENTS.md.
- [ ] **Step 2: Commit** — `docs: mark backup restore wave complete`.
