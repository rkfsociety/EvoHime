# Backup/export: sessions + memory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить переносимый JSON backup через `evohime-export`, содержащий все сессии, связанные task/message/event данные и structured memory items.

**Architecture:** Общий модуль `evohime-storage::backup` собирает DTO из `PgPool` и существующих storage-моделей. Бинарник в том же storage-пакете отвечает только за аргументы, подключение к БД, сериализацию и безопасную запись результата.

**Tech Stack:** Rust 2021, Tokio, SQLx/PostgreSQL, serde/serde_json, существующий `evohime-storage` pool.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Не изменять БД из CLI и не экспортировать legacy memory как отдельный источник.
- Формат первой версии — UTF-8 JSON с `format = "evohime-backup"` и `version = 1`.
- После каждой законченной задачи создавать git-коммит; push не выполнять без прямого запроса.
- Не включать посторонние пользовательские изменения в коммит.

---

### Task 1: Backup DTOs and database collector

**Files:**
- Create: `crates/storage/src/backup.rs`
- Modify: `crates/storage/src/lib.rs`
- Test: `crates/storage/src/backup.rs` unit tests

**Interfaces:**
- Consumes: `PgPool`, `list_tasks`, `list_task_steps`, `list_session_messages`, `list_session_events`, `list_all_memory_items` and the existing row models.
- Produces: `pub struct BackupDump`, `pub struct BackupSession`, `pub struct BackupTask`, `pub struct BackupMessage`, `pub struct BackupEvent`, `pub async fn collect_backup(pool: &PgPool) -> Result<BackupDump, StorageError>`.

- [ ] **Step 1: Write failing serialization tests**

Add tests that construct `BackupDump::empty()` and assert:

```rust
assert_eq!(dump.format, "evohime-backup");
assert_eq!(dump.version, 1);
assert!(dump.sessions.is_empty());
assert!(dump.memory_items.is_empty());
let json = serde_json::to_value(dump).unwrap();
assert_eq!(json["format"], "evohime-backup");
assert!(json["sessions"].is_array());
```

Run `cargo test -p evohime-storage backup::tests`; it must fail because the module and DTOs do not exist.

- [ ] **Step 2: Implement DTOs and empty constructor**

Create serializable DTOs with explicit fields:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDump {
    pub format: String,
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub sessions: Vec<BackupSession>,
    pub memory_items: Vec<MemoryItemRow>,
}
```

`BackupSession` contains `id`, `created_at`, `archived`, `title`, `workspace_path`, `messages`, `tasks`, and `events`. `BackupTask` contains the complete `TaskRow` fields plus `steps: Vec<TaskStepRow>`. `BackupMessage` contains `role`, `content`, and `created_at`; `BackupEvent` contains `sequence`, `created_at`, and `event_json`.

Implement `BackupDump::empty()` with the fixed format/version and `Utc::now()`. Add `pub mod backup;` and re-export `collect_backup` and `BackupDump` from `lib.rs`.

- [ ] **Step 3: Implement collection using existing storage operations**

Add one backup-only session query that returns every session, including `archived_at IS NOT NULL` as a boolean. For each session, call the existing message/task/event functions; for each task call `list_task_steps`. Call `list_all_memory_items(pool, 50_000)` once and map no legacy tables. Return deterministic newest-session ordering and preserve event/message/task ordering supplied by storage.

- [ ] **Step 4: Run storage tests**

Run `cargo test -p evohime-storage backup::tests` and `cargo clippy -p evohime-storage --all-targets -- -D warnings`; both must pass.

- [ ] **Step 5: Commit**

```text
git add crates/storage/src/backup.rs crates/storage/src/lib.rs
git commit -m "feat(storage): collect sessions and memory backups"
```

### Task 2: `evohime-export` CLI and atomic output

**Files:**
- Create: `crates/storage/src/bin/evohime-export.rs`
- Modify: `crates/storage/Cargo.toml` only if a runtime dependency is required
- Test: `crates/storage/src/bin/evohime-export.rs` unit tests for argument parsing and destination validation

**Interfaces:**
- Consumes: `collect_backup`, `DATABASE_URL`, `PoolConfig::from_env`, and `--output <path>`.
- Produces: exit code 0 with a valid backup file, or a non-zero exit with a concise error.

- [ ] **Step 1: Write failing CLI tests**

Test that `parse_args(["evohime-export", "--output", "backup.json"])` returns the requested path, while missing `--output` and an extra positional argument return errors. Test destination validation rejects a path whose parent directory does not exist.

Run `cargo test -p evohime-storage --bin evohime-export`; it must fail because the binary does not exist.

- [ ] **Step 2: Implement argument parsing and output validation**

Use `std::env::args_os` without adding a CLI dependency. Accept exactly `--output <path>` and `-o <path>`, reject all other flags, require a non-empty path, and require an existing parent directory. Keep parser functions pure for unit tests.

- [ ] **Step 3: Implement export execution**

Connect with `connect_pool(&database_url, &PoolConfig::from_env())`, call `collect_backup`, serialize with `serde_json::to_vec_pretty`, create a unique sibling temporary file using `create_new`, write and flush all bytes, then rename it to the destination. On any error, remove the temporary file and return a non-zero result. Print only a short success line with the output path and counts; never print message or memory contents.

- [ ] **Step 4: Run CLI tests and smoke check**

Run `cargo test -p evohime-storage --bin evohime-export`. If `DATABASE_URL` is available, run:

```text
cargo run -p evohime-storage --bin evohime-export -- --output .evohime/backup-smoke.json
```

Parse the file with PowerShell `Get-Content -Raw ... | ConvertFrom-Json`, then remove only this generated smoke file.

- [ ] **Step 5: Commit**

```text
git add crates/storage/src/bin/evohime-export.rs crates/storage/Cargo.toml
git commit -m "feat(cli): add evohime export command"
```

### Task 3: Documentation and final verification

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`
- Modify: `AGENTS.md`
- Modify: `README.md` if the CLI command section is present there

- [ ] **Step 1: Document 7.97 completion**

Mark roadmap item `7.97` complete, set the next item to `7.98`, and document the exact command and JSON scope in current-state/project guidance.

- [ ] **Step 2: Run repository verification**

Run:

```text
cargo test --workspace --all-features --all-targets
cargo clippy --workspace --all-features --all-targets -- -D warnings
cd frontend/web && npm run typecheck && npm run build
git diff --check
```

Treat pre-existing unrelated `cargo fmt --check` differences as existing only if the changed files themselves are formatted and the command output identifies unrelated paths.

- [ ] **Step 3: Commit documentation**

```text
git add AGENTS.md docs/roadmap.md docs/current-state.md README.md
git commit -m "docs: mark backup export complete"
```
