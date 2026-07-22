# Multi-operator authz scopes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить локальную multi-operator identity и изоляцию sessions/tasks/memory с owner-only управлением операторами, сохранив совместимость с `EVOHIME_API_TOKEN`.

**Architecture:** PostgreSQL хранит операторов и только хеши токенов. Auth middleware разрешает bearer-токен, создаёт `OperatorIdentity` и передаёт её через request extensions; storage-функции принимают обязательный `operator_id` для пользовательских данных. Legacy single-token режим отображается как bootstrap owner.

**Tech Stack:** Rust/Axum, PostgreSQL/SQLx migrations, Tokio, `sha2` + `subtle`, existing HTTP/WS middleware, React TypeScript only for displaying operator status later.

## Global Constraints

- Не создавать новую ветку; работать в текущей `main`.
- Не хранить plaintext bearer-токены в PostgreSQL, логах, ошибках или response после одноразовой выдачи.
- Нет fallback к выдаче чужих данных при отсутствии или невалидности `OperatorIdentity`.
- Существующий `EVOHIME_API_TOKEN` продолжает работать как owner-compatible режим.
- Push не выполнять без прямого запроса пользователя; после каждой законченной задачи создавать коммит.

---

### Task 1: Operator registry, hashing and migration

**Files:**
- Create: `migrations/0025_operators.sql`
- Create: `crates/storage/src/operators.rs`
- Modify: `crates/storage/src/lib.rs`
- Modify: `crates/storage/Cargo.toml`
- Test: `crates/storage/src/operators.rs` unit tests

**Interfaces:**
- Produces `OperatorRole`, `OperatorRow`, `create_operator`, `list_operators`, `find_operator_by_token_hash`, `rotate_operator_token`, `revoke_operator`, `active_owner_count`, `hash_operator_token`.
- `OperatorRow` never contains plaintext token; creation/rotation returns `(OperatorRow, String)` only to the caller.

- [ ] **Step 1: Write failing hashing and role tests**

Add tests for deterministic hash shape, different hashes for different tokens, `owner/member` parsing, and `is_last_active_owner` protection. Run `cargo test -p evohime-storage operators::tests`; it must fail before the module exists.

- [ ] **Step 2: Add migration**

Create `operators` with UUID id, unique name, role check, unique token hash, active flag, timestamps and `last_seen_at`. Insert fixed bootstrap owner UUID `00000000-0000-0000-0000-000000000001` named `local-owner` with `token_hash NULL`, then add nullable `operator_id` to `sessions`, `memory_items`, `scheduled_tasks`, `sites` and `permission_approval_audit`, backfill all rows to the bootstrap owner, set `NOT NULL`, add foreign keys and indexes. Existing rows remain accessible only through that owner.

- [ ] **Step 3: Implement storage operations**

Use SHA-256 plus constant-time byte comparison (`subtle::ConstantTimeEq`). Generate tokens with UUID v4 plus a second UUID, return plaintext only from create/rotate, and reject duplicate names, inactive owners and revoking the last active owner. Add `operator_id` to `SessionRow`/`SessionSummaryRow` and `MemoryItemRow` where required by scoped queries.

- [ ] **Step 4: Run tests and migration smoke**

Run `cargo test -p evohime-storage operators::tests`, `cargo clippy -p evohime-storage --all-targets -- -D warnings`, then apply `migrations/0025_operators.sql` against the local PostgreSQL and verify `operators` and every backfilled table exist.

- [ ] **Step 5: Commit**

```text
git add migrations/0025_operators.sql crates/storage/src/operators.rs crates/storage/src/lib.rs crates/storage/Cargo.toml
git commit -m "feat(auth): add operator registry storage"
```

### Task 2: Auth identity and operator management API

**Files:**
- Modify: `crates/server/src/auth.rs`
- Modify: `crates/server/src/app.rs`
- Create: `crates/server/src/operators_api.rs`
- Modify: `crates/server/src/routes.rs`
- Modify: `crates/server/Cargo.toml`
- Test: `crates/server/src/auth.rs` and `crates/server/src/operators_api.rs`

**Interfaces:**
- Produces `OperatorIdentity { id, name, role, source }`, `OperatorRole`, `operator_identity` request extension, and owner-only routes `/api/operators`, `/api/operators/:id/rotate`, `/api/operators/:id/revoke`.
- Consumes `AuthConfig`, `PgPool`, `find_operator_by_token_hash` and existing request id middleware.

- [ ] **Step 1: Write failing auth tests**

Test that a valid registry token maps to the correct operator, revoked tokens return `401`, member access to operator administration returns `403`, legacy env token maps to bootstrap owner, and identity never serializes a token. Run `cargo test -p evohime-server auth::tests operators_api::tests`; expected failures reference missing identity behavior.

- [ ] **Step 2: Implement identity resolution**

Extend `AuthConfig` with legacy token behavior only; resolve a registry token asynchronously from the pool after extracting the header/query token. For an unset token on loopback, resolve the bootstrap owner. Store `OperatorIdentity` in request extensions and pass the same identity into `ws_handler`/`handle_socket` before upgrade. Keep health/auth status public, but include only safe identity metadata.

- [ ] **Step 3: Implement owner API**

Add JSON request/response DTOs. `POST /api/operators` accepts name and role, returns operator metadata plus plaintext token exactly once. Rotate and revoke use path id, require owner identity, and return `403` for members. Never accept `operator_id` from body/query for scope selection.

- [ ] **Step 4: Run HTTP/WS auth tests**

Run targeted server tests and add route tests proving owner/member status codes and revoked-token behavior. Run `cargo clippy -p evohime-server --all-targets -- -D warnings`.

- [ ] **Step 5: Commit**

```text
git add crates/server/src/auth.rs crates/server/src/app.rs crates/server/src/operators_api.rs crates/server/src/routes.rs crates/server/Cargo.toml
git commit -m "feat(auth): resolve operator identity and management API"
```

### Task 3: Scope sessions, tasks and memory

**Files:**
- Modify: `crates/storage/src/lib.rs`
- Modify: `crates/storage/src/memory.rs`
- Modify: `crates/server/src/sessions_api.rs`
- Modify: `crates/server/src/memory_api.rs`
- Modify: `crates/server/src/task/*.rs` only at identity-to-storage boundaries
- Modify: `crates/server/src/ws.rs`
- Modify: `crates/server/src/backup.rs` and `crates/storage/src/bin/evohime-export.rs`
- Test: storage integration tests and server auth/scope tests

**Interfaces:**
- Every session/task/history/memory read and mutation takes `operator_id` or derives it from the session row.
- Backup export requires an operator scope and never exports another operator’s data.

- [ ] **Step 1: Write failing isolation tests**

Create two operators and two sessions/memory items in the storage integration setup. Assert operator A cannot list/load/delete/archive operator B’s rows, a memory lookup by id from the wrong operator returns `None`, and backup for A contains no B data. Run the targeted integration tests and confirm they fail before filters are added.

- [ ] **Step 2: Add scoped storage queries**

Add `operator_id = $scope` predicates to session list/load/mutate/history, task list/load, and all memory list/get/update/delete/conflict/feedback functions. Use SQL `RETURNING`/`fetch_optional` so wrong-scope mutations become ordinary not-found responses rather than data leaks.

- [ ] **Step 3: Thread identity through HTTP and WS handlers**

Extract `OperatorIdentity` from request extensions in sessions, memory, task and WS handlers. New sessions and memory admissions use `identity.id`; session-based task processing verifies the session owner before starting. WS backlog and commands use the same owner check before subscribing to the bus.

- [ ] **Step 4: Scope backup and run tests**

Change `collect_backup(pool, operator_id)` to select only scoped sessions/memory, update CLI to accept the current owner token or explicit operator identity only through authenticated server context, and add tests for no cross-operator export. Run storage/server targeted tests.

- [ ] **Step 5: Commit**

```text
git add crates/storage crates/server migrations
git commit -m "feat(authz): isolate sessions tasks and memory by operator"
```

### Task 4: Documentation and full verification

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `docs/openapi.json` only if the existing OpenAPI generator includes operator routes

- [ ] **Step 1: Document setup and token lifecycle**

Document the bootstrap owner compatibility, one-time token output, rotate/revoke commands/API, role behavior and the fact that scopes are deny-by-default.

- [ ] **Step 2: Run final verification**

Run:

```text
cargo test --workspace --all-features --all-targets
cargo clippy --workspace --all-features --all-targets -- -D warnings
npm run typecheck
npm run build
git diff --check
```

Apply all migrations to the configured local PostgreSQL and run HTTP/WS smoke checks with owner, member and revoked tokens. If any required tool is missing, install/configure it before rerunning the check.

- [ ] **Step 3: Commit documentation**

```text
git add AGENTS.md README.md docs/current-state.md docs/roadmap.md docs/openapi.json
git commit -m "docs: mark multi-operator authz complete"
```
