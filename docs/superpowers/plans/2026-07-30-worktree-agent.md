# Worktree-Aware Multi-Checkout Agent (`7.107`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a second task starts while another task is already running against the same server, isolate it in a detached-HEAD git worktree instead of the shared `workspace_root`, and automatically fold its changes back into the primary checkout when it finishes.

**Architecture:** A new `task_worktrees` Postgres table tracks one row per isolated task (`base_commit_sha`, `worktree_path`, `primary_workspace_root`). `crates/server/src/ws.rs` decides isolation atomically alongside its existing `task_cancellations` bookkeeping (which must survive an approval pause, a `TaskCancel`, a `TaskRetry`'s stale-worktree teardown, and a server restart for the isolation decision to stay correct across all of a task's lifecycle transitions — Task 5, Task 7 Step 3.5) and provisions the worktree via `git worktree add --detach` into the OS temp directory (never inside the tracked repo tree), idempotently and with rollback if persisting the row fails. `crates/server/src/task/pipeline.rs` points the agent's `workspace_root` at the worktree when a row exists, then on success squash-merges the worktree's diff back onto the primary checkout under a per-`primary_workspace_root` lock (`workspace_merge_locks`, not one global lock), using `git add -A` + `git diff --cached` + `git apply --3way --index` + `git commit`, resetting the primary checkout back to a clean `HEAD` on conflict. Startup cleanup keys off each row's owning task's *live* status, not a restart-scoped snapshot. All git subprocess calls follow the existing `tokio::process::Command` pattern already used in `crates/tool-runtime/src/tools/git.rs` and `crates/server/src/github_api.rs`.

**Tech Stack:** Rust (axum, sqlx/Postgres, tokio), `git` CLI via `tokio::process::Command`.

## Global Constraints

- No feature branches are ever created — worktrees are detached HEAD only. (Design §Non-goals)
- No new approval/review UI — per-write approvals already happened inside the worktree via the existing permissions engine. (Design §Non-goals)
- `tokio::sync::Mutex` only for the new lock registry (`workspace_merge_locks`) and any reuse of `task_cancellations` — never `std::sync::Mutex`, since tokio's mutex never poisons on panic. (Design §Trigger)
- Every DB schema change goes through `migrations/`, next free number is `0035`. (`AGENTS.md` rule 7; confirmed via `ls migrations/`)
- After finishing this plan: `cargo test --workspace --all-features --all-targets`, `cargo clippy --workspace --all-features --all-targets -- -D warnings`, and clean up `target/` when no longer needed for a later step. (`AGENTS.md` rules 5, 15)
- Commit after every task; never push unless the user explicitly asks. (`AGENTS.md` rule 11)

---

## Known Limitations (accepted, not fixed by this plan)

Recorded here so a future reader doesn't mistake these for oversights — each was considered and deliberately left as-is, with the reasoning:

- **`restore_paths`'s rollback for an added file (`tokio::fs::remove_file`) doesn't remove now-empty parent directories** (e.g. `dir/sub/` survives after `dir/sub/file.txt` is removed). Left as-is because git itself never tracks empty directories — no future `git` operation on `primary_root` is affected by a leftover empty directory, so this is filesystem-only cosmetic debris, not a correctness issue for anything this feature does.
- **A worktree's patch and a live unisolated task's uncommitted edits colliding on the *exact same file*.** Every merge-back operation (`git add -A`, `diff --cached`, `apply --3way --index`, the scoped `commit`, and `restore_paths`'s scoped `checkout HEAD --`) is deliberately restricted to the paths *this merge's own patch* touches — see Task 3's tests, especially `merge_back_never_touches_unrelated_uncommitted_primary_changes`. That protects everything outside the patch's own file set unconditionally. It does not protect against the narrower case of a live unisolated task independently, concurrently editing one of the *same* files the isolated task's patch also touches — restoring to `HEAD` on a failed apply would discard that overlapping edit too. Solving this in general requires per-file locking across the whole task-execution pipeline (not just the merge step), which is out of scope for this feature; in practice two tasks racing to edit the identical file at the identical moment is already an inherently conflict-prone scenario with or without worktree isolation.
- **`git worktree add --detach` never carries over untracked files** (`.env`, local config, build caches) from `primary_workspace_root` into the isolated worktree, since a worktree is populated purely from a git commit. An agent whose task depends on an untracked file existing may fail inside the worktree in a way it wouldn't have unisolated. Copying untracked files automatically is not done here — it would risk carrying stale build artifacts into every isolated run, and copying arbitrary untracked files (potentially including secrets in a gitignored `.env`) into a scratch directory under the OS temp dir is its own exposure to weigh, not a default to reach for silently.
- **`worktree_op_timeout()`/`merge_timeout()` cache their env-var value in a `OnceLock` for the process's lifetime**, so a test that wants to exercise a *different* timeout within the same `cargo test` binary can't override it after the first call. No test in this plan needs to; noted so a future test author doesn't lose time discovering it.
- **`workspace_merge_locks` never evicts an entry** once created for a given `primary_workspace_root` — the map only grows for the process's lifetime. Bounded by the number of distinct project/repo roots a server instance ever serves tasks against, which for realistic (single-tenant or small-team) deployments is small and stable, not by task volume. Worth revisiting only if a deployment's number of distinct workspaces itself becomes large.
- **`remove_worktree` unconditionally runs `git worktree prune`** rather than only during startup/periodic cleanup. This is intentional, not an oversight: Task 2's fix for the metadata-leak bug (point 5 in review) depends on `prune` always running, including on the failure path inside a single `remove_worktree` call — making it conditional would reintroduce that leak. `git worktree prune` is a metadata-only scan of `.git/worktrees/`, not a full-repository operation, and is cheap at the scale this table operates at (one row per concurrently isolated task).
- **A `.git`-marker check before deleting a worktree directory isn't cryptographic proof of ownership.** Both call sites that delete based on this check (`add_worktree`'s self-heal path, `cleanup_orphaned_worktree_directories`) only ever act on paths of the shape `temp_root/evohime-worktrees/<uuid>` — a namespace this feature owns exclusively — so the realistic collision this would need to guard against (an unrelated git repository landing at the exact same random UUID path under our own subdirectory) doesn't have a plausible trigger.
- **`base_commit_sha` could theoretically stop existing** if `primary_workspace_root` undergoes an aggressive `git gc`, a history rewrite, or a force-push that drops it. `git worktree add --detach <sha>` would then fail with a normal git error, which `add_worktree`/`provision_worktree` already propagate as `WorktreeError` — surfacing as a clean task failure with the underlying git message, not a crash or silent corruption. No special-cased handling is added because the existing error path already degrades safely.
- **`git add -A` inside the merge-back stages whatever the agent left in the worktree, gitignore gaps and all.** This is not a risk this feature introduces — the agent's existing non-isolated flow already stages/commits via the same `git add -A` convention today, so a `.gitignore` gap affects both paths identically. Tightening `.gitignore` coverage is a separate, standing concern independent of worktree isolation.
- **`worktree_op_timeout()`'s 30-second default may be too tight for `git worktree add` on a very large repository.** Rather than guess at a larger blanket default that would then be needlessly long for the common case, this is a tuning knob: `EVOHIME_WORKTREE_OP_TIMEOUT_SECS` already exists for exactly this. Operators running this against a large repository should set it explicitly; this plan doesn't attempt to auto-detect repo size to pick a default.
- **Server-restart cleanup and the retry-teardown in Task 5, Step 5 are both best-effort** (`remove_worktree` failures are logged and leave the row for the next pass, never surfaced as a hard error) — consistent with every other cleanup path in this design.
- **Multiple isolated tasks completing in sequence each land their own commit** on `primary_workspace_root` rather than being squashed together into one. This is intentional, not an omission: one commit per task keeps the primary checkout's history legible about which task authored which change, which matters more here than a shorter commit list. Squashing across tasks is not discussed further because it's a strictly worse default for traceability, not an alternative left unweighed.
- **The task-start rate limiter now counts every `paused` task toward its concurrency cap, indefinitely, not just actively-running ones.** `ws.rs`'s `UserMessage` handler feeds `state.task_cancellations.lock().await.len()` into `state.rate_limiter.allow_task_start(concurrent)` (`crates/server/src/rate_limit.rs`'s `check_concurrent_tasks`, default cap `EVOHIME_MAX_CONCURRENT_TASKS=16`). Task 5's fix deliberately keeps a paused task's `task_cancellations` entry alive for as long as it has unfinished business (Task 5, Step 3) — and this counter reads the same map, so a handful of tasks sitting on `approval.required` now count against the same budget that used to only reflect tasks actually executing. Not fixed here: a paused task genuinely does still represent open, unresolved work on the server, so counting it toward a *concurrency* limit isn't obviously wrong — and decoupling the limiter from `task_cancellations` would mean adding a dedicated DB query on every task-start message (a hot path), for a soft protective limit whose default (16) leaves a wide margin before this matters in practice. Revisit only if the default cap turns out to be too tight for real usage patterns with several approval-paused tasks outstanding at once.
- **`TaskRetry`'s own atomic `is_concurrent` check can observe its own not-yet-released stale entry.** Step 5's `TaskRetry` rewrite reads `!guard.is_empty()` (Task 5, Step 5) before inserting a fresh token; if this exact task's own prior entry somehow failed to be released (e.g. `release_task_cancellation_if_terminal`'s DB check hit a transient error — logged, not retried), `is_concurrent` would read `true` from the task's own leftover entry, not a genuinely different concurrent task. The failure direction is safe (spurious extra isolation, never spurious *lack* of isolation), so this is accepted rather than fixed with, e.g., an `is_concurrent = guard.keys().any(|k| *k != task_id)` refinement — that refinement would itself need to be justified against every other call site's identical pattern (`UserMessage`, `TaskPlanApprove`, `TaskResume` all have the same theoretical shape), which is a larger change than this one path warrants on its own.
- **A merge conflict on `finalize_worktree` discards the agent's final message and emits a `TaskCompleted` immediately followed by a `TaskFailed`.** The agent loop finishes its own steps and emits `TaskCompleted` (and marks its steps `completed`) before `finalize_worktree` ever runs (Task 6, Step 2's placement — right after the success-path `match agent_result` block, immediately before `insert_message`). If the merge then conflicts, the client sees a completed-then-failed sequence, and the assistant's own summary of the work is never persisted (`insert_message` never runs, since `finalize_worktree`'s error propagates first). This is a direct consequence of the plan's own mandated placement (merge-back must run before `process_user_message` returns, so `ws.rs`'s cleanup naturally waits — see Task 6, Step 2's own reasoning), not an implementation oversight — reordering it to insert the message first would mean persisting a message describing work that, from primary's perspective, never actually landed. Accepted as a UX rough edge, not fixed here.
- **The merge-back lock key (`primary_workspace_root`) isn't normalized identically across every task-start path.** For a task started via `ws.rs`'s `UserMessage` handler, `primary_workspace_root` is the canonicalized, `public_fs_path`-normalized string (`crates/server/src/task/helpers.rs::public_fs_path`, avoiding Windows' `\\?\`-prefixed `canonicalize()` output). For a task with no `task.workspace_path` at all (reachable via `state.workspace_root` fallback in `pipeline.rs`, e.g. a scheduler-started task), it's `state.workspace_root` — not necessarily run through the same normalization. Two tasks that are actually the same physical repository could in principle hash to two different `merge_lock_for` entries and merge concurrently without serializing against each other. This is a pre-existing path-representation inconsistency in the codebase (not introduced by this feature), and normalizing it project-wide is out of scope for the merge-lock registry specifically — flagged here rather than silently accepted, since it's the one place this feature's own correctness depends on two representations of "the same repo" actually being equal.
- **`worktree_row` is read once near the top of `pipeline.rs`'s task-execution function and reused for the merge-back call much later, without re-fetching.** In principle a stale snapshot could point `finalize_worktree` at a row/directory that's since been removed. In practice this isn't reachable given the rest of this design's own invariants: `cleanup_stale_worktrees` (Task 7) only ever touches rows for *terminal* tasks, and `TaskRetry`'s teardown (Task 5, Step 5) only runs for tasks already `status == "failed"` — while this function is running, the task is `running` (non-terminal) the entire time, so nothing else in this feature can remove its row out from under it mid-execution. Re-fetching immediately before the merge would add a DB round-trip to guard against a window that doesn't actually exist under this design's own state machine.

---

## File Structure

- **Create** `migrations/0035_task_worktrees.sql` — new table.
- **Create** `crates/storage/src/task_worktrees.rs` — DAO for the table (mirrors `crates/storage/src/plugin_audit.rs`).
- **Modify** `crates/storage/src/lib.rs` — register `pub mod task_worktrees;`.
- **Create** `crates/server/src/task/worktree.rs` — all git-subprocess mechanics (pure functions taking explicit paths) plus the `AppState`-aware orchestration functions (`provision_worktree`, `finalize_worktree`, `cleanup_stale_worktrees`).
- **Modify** `crates/server/src/task/mod.rs` — register the new module.
- **Modify** `crates/server/src/app.rs` — add `workspace_merge_locks` field and `merge_lock_for` helper method to `AppState`.
- **Modify** `crates/server/src/startup.rs` — construct the new field; add the startup cleanup pass after `recover_after_restart`; re-seed `task_cancellations` for tasks that were already non-terminal before this restart.
- **Modify** `crates/server/src/task/helpers.rs` — add `release_task_cancellation_if_terminal` and the `TaskCancellationGuard` panic-safety guard.
- **Modify** `crates/server/src/ws.rs` — atomic trigger decision, worktree provisioning, fail-fast on concurrent provisioning failure; keep `task_cancellations` alive through approval pauses, `TaskCancel`, `TaskPlanReject`, and `TaskRetry`.
- **Modify** `crates/server/src/task/pipeline.rs` — resolve worktree override for `AgentConfig.workspace_root`, call `finalize_worktree` on success.

---

### Task 1: `task_worktrees` table and storage DAO

**Files:**
- Create: `migrations/0035_task_worktrees.sql`
- Create: `crates/storage/src/task_worktrees.rs`
- Modify: `crates/storage/src/lib.rs` (add `pub mod task_worktrees;` alongside the other `pub mod` lines at the top of the file)

**Interfaces:**
- Produces: `evohime_storage::task_worktrees::{TaskWorktreeRow, NewTaskWorktree, insert_task_worktree, get_task_worktree, delete_task_worktree, list_task_worktrees}`.
  - `pub struct TaskWorktreeRow { pub task_id: Uuid, pub base_commit_sha: String, pub worktree_path: String, pub primary_workspace_root: String, pub created_at: DateTime<Utc> }`
  - `pub struct NewTaskWorktree { pub task_id: Uuid, pub base_commit_sha: String, pub worktree_path: String, pub primary_workspace_root: String }`
  - `pub async fn insert_task_worktree(pool: &PgPool, entry: &NewTaskWorktree) -> Result<TaskWorktreeRow, StorageError>`
  - `pub async fn get_task_worktree(pool: &PgPool, task_id: Uuid) -> Result<Option<TaskWorktreeRow>, StorageError>`
  - `pub async fn delete_task_worktree(pool: &PgPool, task_id: Uuid) -> Result<(), StorageError>`
  - `pub async fn list_task_worktrees(pool: &PgPool) -> Result<Vec<TaskWorktreeRow>, StorageError>`
  - `pub struct TaskWorktreeWithStatus { pub row: TaskWorktreeRow, pub task_status: String }`
  - `pub async fn list_task_worktrees_with_status(pool: &PgPool) -> Result<Vec<TaskWorktreeWithStatus>, StorageError>` — joins against `tasks.status` so startup cleanup can decide per row without a second round-trip per task.

- [ ] **Step 1: Write the migration**

```sql
-- 7.107: Worktree-aware multi-checkout agent.
-- Tracks isolated git worktrees allocated to concurrently-running tasks so
-- merge-back and server-restart cleanup can find them without scanning disk.

CREATE TABLE IF NOT EXISTS task_worktrees (
    task_id uuid PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    base_commit_sha text NOT NULL,
    worktree_path text NOT NULL UNIQUE,
    primary_workspace_root text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_task_worktrees_created_at ON task_worktrees (created_at);
```

Save as `migrations/0035_task_worktrees.sql`.

- [ ] **Step 2: Write the failing DAO test**

Create `crates/storage/src/task_worktrees.rs`:

```rust
//! Isolated git worktrees allocated to concurrently-running tasks (Stage 7.107).

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, FromRow)]
pub struct TaskWorktreeRow {
    pub task_id: Uuid,
    pub base_commit_sha: String,
    pub worktree_path: String,
    pub primary_workspace_root: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewTaskWorktree {
    pub task_id: Uuid,
    pub base_commit_sha: String,
    pub worktree_path: String,
    pub primary_workspace_root: String,
}

pub async fn insert_task_worktree(
    pool: &PgPool,
    entry: &NewTaskWorktree,
) -> Result<TaskWorktreeRow, StorageError> {
    Ok(sqlx::query_as::<_, TaskWorktreeRow>(
        r#"
        INSERT INTO task_worktrees (task_id, base_commit_sha, worktree_path, primary_workspace_root)
        VALUES ($1, $2, $3, $4)
        RETURNING task_id, base_commit_sha, worktree_path, primary_workspace_root, created_at
        "#,
    )
    .bind(entry.task_id)
    .bind(&entry.base_commit_sha)
    .bind(&entry.worktree_path)
    .bind(&entry.primary_workspace_root)
    .fetch_one(pool)
    .await?)
}

pub async fn get_task_worktree(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Option<TaskWorktreeRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskWorktreeRow>(
        r#"
        SELECT task_id, base_commit_sha, worktree_path, primary_workspace_root, created_at
        FROM task_worktrees
        WHERE task_id = $1
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn delete_task_worktree(pool: &PgPool, task_id: Uuid) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM task_worktrees WHERE task_id = $1")
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_task_worktrees(pool: &PgPool) -> Result<Vec<TaskWorktreeRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskWorktreeRow>(
        r#"
        SELECT task_id, base_commit_sha, worktree_path, primary_workspace_root, created_at
        FROM task_worktrees
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?)
}

#[derive(Debug, Clone, FromRow)]
struct TaskWorktreeWithStatusRow {
    task_id: Uuid,
    base_commit_sha: String,
    worktree_path: String,
    primary_workspace_root: String,
    created_at: DateTime<Utc>,
    task_status: String,
}

#[derive(Debug, Clone)]
pub struct TaskWorktreeWithStatus {
    pub row: TaskWorktreeRow,
    pub task_status: String,
}

/// Every `task_worktrees` row alongside its owning task's *current* status.
/// Startup cleanup uses `task_status` (not the transient, restart-scoped set
/// `recover_after_restart` returns) to decide whether a row is still needed —
/// see the design doc's Cleanup section for why that distinction matters
/// (an approval-paused task's worktree must never be swept just because it
/// wasn't mid-crash at the moment of *this* restart).
pub async fn list_task_worktrees_with_status(
    pool: &PgPool,
) -> Result<Vec<TaskWorktreeWithStatus>, StorageError> {
    let rows = sqlx::query_as::<_, TaskWorktreeWithStatusRow>(
        r#"
        SELECT tw.task_id, tw.base_commit_sha, tw.worktree_path, tw.primary_workspace_root,
               tw.created_at, t.status AS task_status
        FROM task_worktrees tw
        JOIN tasks t ON t.id = tw.task_id
        ORDER BY tw.created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TaskWorktreeWithStatus {
            row: TaskWorktreeRow {
                task_id: row.task_id,
                base_commit_sha: row.base_commit_sha,
                worktree_path: row.worktree_path,
                primary_workspace_root: row.primary_workspace_root,
                created_at: row.created_at,
            },
            task_status: row.task_status,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn connect_pool() -> Option<PgPool> {
        crate::connect_integration_pool().await
    }

    async fn seed_task(pool: &PgPool) -> Uuid {
        let session_id: Uuid =
            sqlx::query_scalar("INSERT INTO sessions DEFAULT VALUES RETURNING id")
                .fetch_one(pool)
                .await
                .expect("insert session");
        sqlx::query_scalar(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await
        .expect("insert task")
    }

    #[tokio::test]
    async fn inserts_gets_lists_and_deletes_a_row() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping task_worktrees integration test: database unavailable");
            return;
        };

        let task_id = seed_task(&pool).await;
        let inserted = insert_task_worktree(
            &pool,
            &NewTaskWorktree {
                task_id,
                base_commit_sha: "deadbeef".to_string(),
                worktree_path: "/tmp/evohime-worktrees/example".to_string(),
                primary_workspace_root: "/tmp/example-repo".to_string(),
            },
        )
        .await
        .expect("insert");
        assert_eq!(inserted.task_id, task_id);

        let fetched = get_task_worktree(&pool, task_id)
            .await
            .expect("get")
            .expect("row present");
        assert_eq!(fetched.base_commit_sha, "deadbeef");

        let listed = list_task_worktrees(&pool).await.expect("list");
        assert!(listed.iter().any(|row| row.task_id == task_id));

        delete_task_worktree(&pool, task_id).await.expect("delete");
        assert!(get_task_worktree(&pool, task_id)
            .await
            .expect("get after delete")
            .is_none());
    }

    #[tokio::test]
    async fn list_with_status_reports_the_owning_tasks_current_status() {
        let Some(pool) = connect_pool().await else {
            eprintln!("skipping task_worktrees integration test: database unavailable");
            return;
        };

        let task_id = seed_task(&pool).await; // seed_task inserts with status 'running'
        insert_task_worktree(
            &pool,
            &NewTaskWorktree {
                task_id,
                base_commit_sha: "deadbeef".to_string(),
                worktree_path: "/tmp/evohime-worktrees/example".to_string(),
                primary_workspace_root: "/tmp/example-repo".to_string(),
            },
        )
        .await
        .expect("insert");

        let with_status = list_task_worktrees_with_status(&pool)
            .await
            .expect("list with status");
        let found = with_status
            .iter()
            .find(|entry| entry.row.task_id == task_id)
            .expect("row present");
        assert_eq!(found.task_status, "running");

        sqlx::query("UPDATE tasks SET status = 'paused' WHERE id = $1")
            .bind(task_id)
            .execute(&pool)
            .await
            .expect("update status");

        let with_status = list_task_worktrees_with_status(&pool)
            .await
            .expect("list with status after update");
        let found = with_status
            .iter()
            .find(|entry| entry.row.task_id == task_id)
            .expect("row present after update");
        assert_eq!(found.task_status, "paused");

        delete_task_worktree(&pool, task_id).await.expect("cleanup");
    }
}
```

- [ ] **Step 3: Register the module**

In `crates/storage/src/lib.rs`, add alongside the existing `pub mod` block (after `pub mod sync;`):

```rust
pub mod task_worktrees;
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p evohime-storage task_worktrees -- --nocapture`
Expected: either `PASS` (if `DATABASE_URL`/integration Postgres is reachable) or the test prints `skipping task_worktrees integration test: database unavailable` and passes trivially — both are acceptable per this crate's existing integration-test convention (see `plugin_audit.rs`).

- [ ] **Step 5: Commit**

```bash
git add migrations/0035_task_worktrees.sql crates/storage/src/task_worktrees.rs crates/storage/src/lib.rs
git commit -m "feat(storage): add task_worktrees table and DAO (7.107)"
```

---

### Task 2: Pure git-subprocess worktree helpers

**Files:**
- Create: `crates/server/src/task/worktree.rs`

**Interfaces:**
- Consumes: nothing outside `std`/`tokio::process`.
- Produces (module-private-to-crate, used by Task 5/6/7):
  - `pub(crate) enum WorktreeError { NotAGitRepo(String), Conflict(String), Io(String) }` with a `Display` impl.
  - `pub(crate) fn worktree_op_timeout() -> Duration` (default 30s, overridable via `EVOHIME_WORKTREE_OP_TIMEOUT_SECS`) — quick, metadata-only git operations. Same configuration mechanism as `merge_timeout()` (env var + cached `OnceLock`), rather than a hardcoded constant for one and an env-configurable function for the other — one consistent approach for both timeout categories.
  - `pub(crate) async fn rev_parse_head(repo: &Path) -> Result<String, WorktreeError>`
  - `pub(crate) async fn add_worktree(repo: &Path, worktree_path: &Path, base_sha: &str) -> Result<(), WorktreeError>`
  - `pub(crate) async fn remove_worktree(repo: &Path, worktree_path: &Path) -> Result<(), WorktreeError>` (tolerates a missing `worktree_path`)

Every `git` subprocess call in this module runs under `tokio::time::timeout`, matching the per-operation `Duration` constants already used in `crates/tool-runtime/src/tools/git.rs` (`STATUS_TIMEOUT`, `COMMIT_TIMEOUT`, etc.) — a hung `git` process must not block a task, or a merge lock, forever.

**Files:**
- Test: inline `#[cfg(test)] mod tests` in the same file, using `tempfile::tempdir()` + `git init`, mirroring `crates/tool-runtime/src/tools/git.rs`'s test setup.

- [ ] **Step 1: Write the failing tests**

Create `crates/server/src/task/worktree.rs` with just the test module first:

```rust
//! Git-worktree isolation for concurrently-running tasks (Stage 7.107).
//!
//! All functions here shell out to the `git` CLI via `tokio::process::Command`,
//! matching the pattern already used in `evohime_tool_runtime`'s git tools and
//! `crate::github_api`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug)]
pub(crate) enum WorktreeError {
    NotAGitRepo(String),
    Conflict(String),
    Io(String),
}

impl fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorktreeError::NotAGitRepo(message) => write!(f, "not a git repository: {message}"),
            WorktreeError::Conflict(message) => write!(f, "merge conflict: {message}"),
            WorktreeError::Io(message) => write!(f, "{message}"),
        }
    }
}

/// Quick, metadata-only git operations (rev-parse, worktree add/remove/prune).
/// Overridable via `EVOHIME_WORKTREE_OP_TIMEOUT_SECS` (default 30s). Same
/// mechanism as `merge_timeout()` below (env var + cached `OnceLock`) —
/// one consistent configuration approach for both timeout categories,
/// rather than a hardcoded constant for this one and an env-configurable
/// function for the other.
pub(crate) fn worktree_op_timeout() -> Duration {
    static VALUE: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| {
        std::env::var("EVOHIME_WORKTREE_OP_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|secs: &u64| *secs > 0)
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(30))
    })
}

async fn run_git(repo: &Path, args: &[&str], timeout: Duration) -> Result<String, WorktreeError> {
    let run = async {
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to run git: {error}")))
    };
    let output = tokio::time::timeout(timeout, run)
        .await
        .map_err(|_| {
            WorktreeError::Io(format!(
                "git -C {} {} timed out after {timeout:?}",
                repo.display(),
                args.join(" ")
            ))
        })??;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let message = if stderr.is_empty() { stdout } else { stderr };
        return Err(WorktreeError::Io(format!(
            "git -C {} {} failed: {message}",
            repo.display(),
            args.join(" ")
        )));
    }
    Ok(stdout)
}

pub(crate) async fn rev_parse_head(repo: &Path) -> Result<String, WorktreeError> {
    run_git(repo, &["rev-parse", "HEAD"], worktree_op_timeout())
        .await
        .map_err(|error| WorktreeError::NotAGitRepo(error.to_string()))
}

pub(crate) async fn add_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_sha: &str,
) -> Result<(), WorktreeError> {
    // Defense in depth, two layers:
    // 1. A cheap lexical check on `worktree_path` itself — no I/O, works
    //    even though `worktree_path` doesn't exist yet (nothing to
    //    canonicalize). Catches the direct case: whatever constructed
    //    `worktree_path` handed this function a path that's textually
    //    inside `repo`.
    if repo == worktree_path || worktree_path.starts_with(repo) {
        return Err(WorktreeError::Io(format!(
            "refusing to create worktree {} nested inside primary root {}",
            worktree_path.display(),
            repo.display()
        )));
    }
    // 2. A canonical check on the OS temp *root* (`worktree_path` is always
    //    `temp_root/evohime-worktrees/<task_id>`, and temp_root always
    //    exists, unlike `worktree_path` itself) against `repo`, both
    //    canonicalized — catches a misconfigured TMPDIR/TEMP that's a
    //    symlink or Windows junction pointing inside `repo`, which check 1's
    //    lexical comparison alone could miss. `repo` is already canonical
    //    by the time it reaches here (it comes from `resolve_workspace_path`,
    //    which canonicalizes), but canonicalizing it again is idempotent.
    let temp_root = std::env::temp_dir();
    let canonical_temp_root = temp_root
        .canonicalize()
        .map_err(|error| WorktreeError::Io(format!("failed to canonicalize {}: {error}", temp_root.display())))?;
    let canonical_repo = repo
        .canonicalize()
        .map_err(|error| WorktreeError::Io(format!("failed to canonicalize {}: {error}", repo.display())))?;
    if canonical_temp_root == canonical_repo || canonical_temp_root.starts_with(&canonical_repo) {
        return Err(WorktreeError::Io(format!(
            "refusing to create worktree {} — the OS temp directory {} is nested inside primary root {}",
            worktree_path.display(),
            canonical_temp_root.display(),
            canonical_repo.display()
        )));
    }

    if worktree_path.exists() {
        // `git worktree add` refuses to target an existing directory. Since
        // `worktree_path` is derived from a fresh task_id, this only
        // happens if a prior attempt for this exact task_id partially
        // failed — e.g. `git worktree add` succeeded but the subsequent
        // `task_worktrees` insert failed *and* that failure's own rollback
        // (`remove_worktree`) also failed. Not reachable through this
        // design's normal call pattern today, but cheap to make
        // provisioning self-healing against it. Prefer `remove_worktree`
        // (git-aware — cleans up `.git/worktrees/<id>/` metadata registered
        // for it, avoiding a `fatal: ... is a missing but locked working
        // tree` / `prune` error on the *next* `git worktree add`) over a
        // raw directory delete. Only if that fails (e.g. this directory was
        // never actually registered as a worktree of `repo` at all) fall
        // back to a raw removal, and only after confirming it looks like a
        // worktree checkout (`.git` marker file present) rather than
        // blindly deleting an unrelated directory that happened to land at
        // this path.
        if let Err(remove_error) = remove_worktree(repo, worktree_path).await {
            // `remove_worktree` already ran `git worktree prune` internally
            // before returning this error (see its own implementation
            // above), so `.git/worktrees/<id>/` metadata for this path is
            // already cleared even though the directory removal itself
            // failed — the raw `remove_dir_all` below only has to deal with
            // the filesystem, not leftover git-internal bookkeeping.
            if worktree_path.exists() {
                if !worktree_path.join(".git").exists() {
                    return Err(WorktreeError::Io(format!(
                        "worktree path {} already exists, is not a recognized git worktree, and git worktree remove failed ({remove_error}) — refusing to delete it blindly",
                        worktree_path.display()
                    )));
                }
                tokio::fs::remove_dir_all(&worktree_path).await.map_err(|error| {
                    WorktreeError::Io(format!(
                        "worktree path {} already exists and could not be cleared: {error}",
                        worktree_path.display()
                    ))
                })?;
            }
        }
    }
    if let Some(parent) = worktree_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to create {}: {error}", parent.display())))?;
    }
    let worktree_path_str = worktree_path.to_string_lossy().into_owned();
    run_git(
        repo,
        &["worktree", "add", "--detach", &worktree_path_str, base_sha],
        worktree_op_timeout(),
    )
    .await?;
    Ok(())
}

pub(crate) async fn remove_worktree(repo: &Path, worktree_path: &Path) -> Result<(), WorktreeError> {
    // `worktree remove`'s own failure is captured, not propagated
    // immediately with `?` — `prune` below must always run regardless, or
    // `.git/worktrees/<id>/` metadata for this path is left registered in
    // `repo`. A subsequent `git worktree add` at the same path would then
    // fail with "missing but locked working tree" even after the directory
    // itself is gone (e.g. via `add_worktree`'s raw `remove_dir_all`
    // fallback below, which has no other way to clear that metadata).
    let remove_result = if worktree_path.exists() {
        let worktree_path_str = worktree_path.to_string_lossy().into_owned();
        run_git(
            repo,
            &["worktree", "remove", "--force", &worktree_path_str],
            worktree_op_timeout(),
        )
        .await
        .map(|_| ())
    } else {
        Ok(())
    };
    run_git(repo, &["worktree", "prune"], worktree_op_timeout()).await?;
    remove_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn run(dir: &Path, args: &[&str]) {
        let status = StdCommand::new(args[0])
            .args(&args[1..])
            .current_dir(dir)
            .status()
            .expect("run command");
        assert!(status.success(), "{:?} failed", args);
    }

    fn init_repo(dir: &Path) {
        run(dir, &["git", "init"]);
        run(dir, &["git", "config", "user.email", "test@example.com"]);
        run(dir, &["git", "config", "user.name", "Test"]);
        std::fs::write(dir.join("README.md"), "hello\n").expect("write");
        run(dir, &["git", "add", "."]);
        run(dir, &["git", "commit", "-m", "init"]);
    }

    #[tokio::test]
    async fn rev_parse_head_returns_a_sha() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());

        let sha = rev_parse_head(repo.path()).await.expect("rev-parse");
        assert_eq!(sha.len(), 40);
    }

    #[tokio::test]
    async fn rev_parse_head_fails_on_non_git_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = rev_parse_head(dir.path()).await.unwrap_err();
        assert!(matches!(error, WorktreeError::NotAGitRepo(_)));
    }

    #[tokio::test]
    async fn add_and_remove_worktree_round_trips() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");

        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");
        assert!(worktree_path.join("README.md").exists());

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
        assert!(!worktree_path.exists());
    }

    #[tokio::test]
    async fn remove_worktree_tolerates_an_already_missing_directory() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let missing = repo.path().join("never-existed");

        remove_worktree(repo.path(), &missing)
            .await
            .expect("remove of missing worktree must not error");
    }

    #[tokio::test]
    async fn add_worktree_refuses_a_path_nested_inside_the_repo() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");
        let nested = repo.path().join("nested-worktree");

        let error = add_worktree(repo.path(), &nested, &base_sha)
            .await
            .unwrap_err();
        assert!(matches!(error, WorktreeError::Io(_)));
        assert!(!nested.exists());
    }

    #[tokio::test]
    async fn add_worktree_self_heals_a_leftover_directory_from_a_prior_attempt() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");

        // First attempt succeeds normally.
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("first add worktree");

        // A second call for the exact same path — simulating provisioning
        // being retried after some earlier failure left this directory
        // behind — must not fail with "already exists".
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree must self-heal a pre-existing directory");
        assert!(worktree_path.join("README.md").exists());

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail to compile first, then pass**

Run: `cargo test -p evohime-server task::worktree:: -- --nocapture`
Expected: compiles and all six tests `PASS` (git must be on `PATH`, as it already is elsewhere in this workspace's tests, e.g. `crates/tool-runtime/src/tools/git.rs`).

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/task/worktree.rs
git commit -m "feat(server): add pure git-worktree add/remove helpers (7.107)"
```

---

### Task 3: Merge-back helper (`merge_worktree_into_primary`)

**Files:**
- Modify: `crates/server/src/task/worktree.rs`

**Interfaces:**
- Consumes: `WorktreeError`, `run_git`, `rev_parse_head` (Task 2).
- Produces:
  - `pub(crate) fn merge_timeout() -> Duration` (default 5 minutes, overridable via `EVOHIME_WORKTREE_MERGE_TIMEOUT_SECS` — diff/apply/commit can be slower than metadata ops on a large patch, and a fixed constant would be too tight for a very large repository).
  - `pub(crate) async fn merge_worktree_into_primary(worktree_path: &Path, primary_root: &Path, base_sha: &str, task_id: Uuid) -> Result<(), WorktreeError>`

**Every operation below is scoped to exactly the paths this merge's own patch touches — never to `workspace_root`'s whole index/tree.** Whenever the currently-*first* task is running unisolated (the common case: `is_concurrent == false` at its own start), its tool calls write directly into `primary_root`, uncommitted, for the entire time any *other* task is running isolated. A blanket `git commit` (with no pathspec) could sweep that first task's unrelated staged changes into this merge's commit; a blanket `git reset --hard` on conflict could destroy its in-progress uncommitted work outright. Scoping every git invocation to this merge's own path list is what makes it safe to run next to a live unisolated task.

- [ ] **Step 1: Write the failing tests**

Append to `crates/server/src/task/worktree.rs` (add `use uuid::Uuid;` and `use tracing::{info, warn};` to the top-level `use` block, and add these functions above the `#[cfg(test)]` module):

```rust
/// Diff/apply/commit can be slower than metadata-only git operations on a
/// large patch.
/// Overridable via `EVOHIME_WORKTREE_MERGE_TIMEOUT_SECS` (default 5
/// minutes) — a fixed constant would be too tight for diff/apply/commit
/// against a very large repository or patch.
pub(crate) fn merge_timeout() -> Duration {
    static VALUE: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();
    // Read (and validate) the env var once per process rather than on every
    // call — this is on the hot path of every merge-back, and the value
    // can't meaningfully change mid-process anyway (nothing in this design
    // re-reads the environment after startup).
    *VALUE.get_or_init(|| {
        std::env::var("EVOHIME_WORKTREE_MERGE_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|secs: &u64| *secs > 0)
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(5 * 60))
    })
}

struct ChangedPaths {
    /// Paths that existed at `base_sha` (modified, deleted, or either side
    /// of a rename) — safe to restore with `git checkout HEAD --`.
    existing: Vec<String>,
    /// Paths newly created by the worktree, absent at `base_sha` — `git
    /// checkout HEAD --` can't restore something that never existed there;
    /// these are unstaged and deleted directly on rollback instead.
    added: Vec<String>,
}

impl ChangedPaths {
    fn all(&self) -> Vec<&str> {
        self.existing
            .iter()
            .chain(self.added.iter())
            .map(String::as_str)
            .collect()
    }
}

async fn changed_paths(worktree_path: &Path, base_sha: &str) -> Result<ChangedPaths, WorktreeError> {
    // `-z`: NUL-delimited, *unquoted* paths. Without it, git's default
    // `core.quotePath=true` C-style-quotes any path containing a non-ASCII
    // byte — not a hypothetical for a project whose own docs and commit
    // history are full of Cyrillic — wrapping it in literal quote
    // characters that a naive tab/newline split would leave embedded in
    // the path string, breaking every later `checkout`/`reset`/`commit`
    // pathspec built from this list ("file not found"). `-z` sidesteps
    // quoting entirely and also removes any ambiguity from a filename that
    // happens to contain a literal newline.
    let run = async {
        Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args(["diff", "--cached", "--name-status", "-z", base_sha])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to run git diff --name-status: {error}")))
    };
    let output = tokio::time::timeout(merge_timeout(), run)
        .await
        .map_err(|_| WorktreeError::Io("git diff --cached --name-status -z timed out".to_string()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::Io(format!(
            "git diff --cached --name-status -z failed: {stderr}"
        )));
    }

    // Each record is NUL-separated: "STATUS\0PATH\0" for add/modify/delete,
    // or "R<score>\0OLD\0NEW\0" for a rename — never ambiguous, since a NUL
    // byte is exactly what `-z` guarantees never appears inside a field.
    let mut fields = output
        .stdout
        .split(|&byte| byte == 0)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .filter(|field| !field.is_empty())
        .collect::<std::collections::VecDeque<_>>();

    let mut existing = Vec::new();
    let mut added = Vec::new();
    while let Some(status) = fields.pop_front() {
        match status.chars().next() {
            Some('A') => added.extend(fields.pop_front()),
            // `R` (rename) and `C` (copy) both emit three NUL-separated
            // fields (`STATUS\0OLD\0NEW\0`) instead of two. This invocation
            // never passes `-C`/`--find-copies`, so `C` shouldn't appear from
            // *our own* command line — but `diff.renames` can be set to
            // `copies` in a user's or CI's global/repo git config, which
            // makes plain `git diff` emit `C` without any local flag asking
            // for it. Treating `C` as `_` (one field only) desyncs every
            // record after it: the untouched NEW_PATH field is then
            // misread as the next record's STATUS. Same handling as `R`.
            Some('R') | Some('C') => {
                // The old name existed at base_sha (restorable via `git
                // checkout HEAD --`); the new name did not (must be treated
                // like an added path — unstaged and deleted directly on
                // rollback). Lumping both into `existing`, as an earlier
                // version of this function did, breaks conflict recovery:
                // `git checkout HEAD -- <old> <new>` errors out entirely on
                // the unmatched new-name pathspec, aborting the restore for
                // *every* path in that one invocation, not just the rename.
                existing.extend(fields.pop_front());
                added.extend(fields.pop_front());
            }
            _ => existing.extend(fields.pop_front()),
        }
    }
    Ok(ChangedPaths { existing, added })
}

async fn diff_cached_patch(worktree_path: &Path, base_sha: &str) -> Result<Vec<u8>, WorktreeError> {
    // `--binary` is required or a changed binary file's actual bytes are
    // omitted from the diff and can never be reconstructed by `git apply` —
    // without it, a binary file change would silently vanish from the merge.
    let run = async {
        Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args(["diff", "--cached", "--binary", base_sha])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to run git diff: {error}")))
    };
    let output = tokio::time::timeout(merge_timeout(), run)
        .await
        .map_err(|_| WorktreeError::Io("git diff --cached --binary timed out".to_string()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::Io(format!(
            "git diff --cached --binary failed: {stderr}"
        )));
    }
    Ok(output.stdout)
}

async fn apply_patch(primary_root: &Path, patch: &[u8]) -> Result<(), WorktreeError> {
    let spawn = async {
        Command::new("git")
            .arg("-C")
            .arg(primary_root)
            .args(["apply", "--3way", "--index"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| WorktreeError::Io(format!("failed to spawn git apply: {error}")))
    };
    let mut child = spawn.await?;
    // `stdin` must be taken out of `child` *before* `wait_with_output()` is
    // called (which takes ownership of `child`), and the write must run
    // concurrently with draining stdout/stderr below — a large patch that
    // produces enough stdout/stderr (e.g. many per-file conflict messages
    // during a large 3-way merge) to fill the OS pipe buffer before the
    // parent starts reading would otherwise deadlock: the child blocks
    // writing to its full stdout/stderr pipe while the parent is still
    // blocked writing the remaining stdin.
    let mut stdin = child.stdin.take().expect("stdin piped");

    let io = async {
        use tokio::io::AsyncWriteExt;
        let write_fut = async {
            let result = stdin.write_all(patch).await;
            // Close stdin as soon as the write completes (rather than
            // waiting for the whole `io` future to finish) so `git apply`
            // sees EOF and can proceed even while stdout/stderr are still
            // being drained below.
            drop(stdin);
            result
        };
        tokio::join!(write_fut, child.wait_with_output())
    };

    let (write_result, output_result) = tokio::time::timeout(merge_timeout(), io)
        .await
        .map_err(|_| WorktreeError::Io("git apply timed out".to_string()))?;

    write_result.map_err(|error| WorktreeError::Io(format!("failed to write patch to git apply: {error}")))?;
    let output = output_result
        .map_err(|error| WorktreeError::Io(format!("failed to wait on git apply: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::Io(format!(
            "git apply --3way --index failed: {stderr}"
        )));
    }
    Ok(())
}

/// Restores exactly `changed`'s paths to their `HEAD` state — never a
/// blanket `git reset --hard`, which would also discard any unrelated
/// uncommitted work belonging to a concurrently-running unisolated task
/// elsewhere in `repo`. Best-effort: logs and continues on partial failure
/// rather than propagating, since this itself only runs while already
/// recovering from a failed apply/commit — the original error is what the
/// caller returns.
/// Returns `false` if any part of the restore itself failed — the caller
/// folds that into its own error message rather than leaving "rollback may
/// be incomplete" as something only visible in a log line elsewhere. A
/// partial-restore failure still returns the *original* conflict/commit
/// error to the task (this function's own failures are secondary), just
/// with that extra fact appended so it isn't silently invisible outside
/// the logs.
async fn restore_paths(repo: &Path, changed: &ChangedPaths) -> bool {
    let mut clean = true;
    if !changed.existing.is_empty() {
        let mut args: Vec<&str> = vec!["checkout", "HEAD", "--"];
        args.extend(changed.existing.iter().map(String::as_str));
        if let Err(error) = run_git(repo, &args, merge_timeout()).await {
            warn!(%error, "failed to restore existing paths to HEAD after a failed merge-back");
            clean = false;
        }
    }
    if !changed.added.is_empty() {
        let mut args: Vec<&str> = vec!["reset", "--"];
        args.extend(changed.added.iter().map(String::as_str));
        if let Err(error) = run_git(repo, &args, worktree_op_timeout()).await {
            warn!(%error, "failed to unstage added paths after a failed merge-back");
            clean = false;
        }
        for path in &changed.added {
            // Intentionally ignored: this path may never have actually been
            // created on primary_root at all (the failed apply could have
            // hit its conflict on an earlier file in a multi-file patch,
            // before ever touching this one) — "already absent" is the
            // expected common case here, not an error worth surfacing.
            let _ = tokio::fs::remove_file(repo.join(path)).await;
        }
    }
    clean
}

pub(crate) async fn merge_worktree_into_primary(
    worktree_path: &Path,
    primary_root: &Path,
    base_sha: &str,
    task_id: Uuid,
) -> Result<(), WorktreeError> {
    // Diagnostic only: `git apply --3way` already handles a moved primary
    // HEAD correctly, but logging the drift up front means a later conflict
    // can be told apart from "another task's merge landed in between" vs.
    // "same base, genuine textual collision" without reconstructing it after
    // the fact.
    if let Ok(current_head) = rev_parse_head(primary_root).await {
        info!(
            task_id = %task_id,
            base_commit_sha = base_sha,
            primary_head = %current_head,
            "starting worktree merge-back"
        );
    }

    // Stage everything first: `git diff` never shows untracked files
    // regardless of which commit it's compared against.
    run_git(worktree_path, &["add", "-A"], merge_timeout()).await?;

    let changed = changed_paths(worktree_path, base_sha).await?;
    let all_paths = changed.all();
    if all_paths.is_empty() {
        // Nothing changed relative to base — nothing to merge or commit.
        return Ok(());
    }

    let patch = diff_cached_patch(worktree_path, base_sha).await?;

    if let Err(apply_error) = apply_patch(primary_root, &patch).await {
        let restored_cleanly = restore_paths(primary_root, &changed).await;
        return Err(WorktreeError::Conflict(if restored_cleanly {
            apply_error.to_string()
        } else {
            format!("{apply_error} (rollback incomplete — see logs, primary_root may still be dirty)")
        }));
    }

    // Only commit if THIS merge's own paths actually have staged changes —
    // a 3-way merge can legitimately produce no diff if primary already
    // matched. Scoped to all_paths, not the whole index: a concurrently
    // running unisolated task could have unrelated staged changes too.
    let mut quiet_args: Vec<&str> = vec!["diff", "--cached", "--quiet", "--"];
    quiet_args.extend(all_paths.iter().copied());
    // Wrapped in the same `tokio::time::timeout` idiom as every other git
    // call in this file (`run_git`, `changed_paths`, `diff_cached_patch`) —
    // a bare untimed `.status().await` here would let a hung `git diff
    // --quiet` on a huge index block the merge indefinitely.
    let run = async {
        Command::new("git")
            .arg("-C")
            .arg(primary_root)
            .args(&quiet_args)
            .status()
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to run git diff --cached --quiet: {error}")))
    };
    let staged = tokio::time::timeout(merge_timeout(), run)
        .await
        .map_err(|_| WorktreeError::Io("git diff --cached --quiet timed out".to_string()))??;
    if staged.success() {
        // Exit code 0 means no staged differences in this merge's own paths.
        return Ok(());
    }

    let message = format!("agent: task {task_id} (worktree merge)");
    let mut commit_args: Vec<&str> = vec!["commit", "-m", message.as_str(), "--"];
    commit_args.extend(all_paths.iter().copied());
    // Per git's own semantics, a `git commit` given pathspecs commits only
    // changes to those paths regardless of what else is staged/dirty
    // elsewhere in the index — this is what keeps a concurrently-running
    // unisolated task's unrelated state out of this commit.
    if let Err(commit_error) = run_git(primary_root, &commit_args, merge_timeout()).await {
        let restored_cleanly = restore_paths(primary_root, &changed).await;
        return Err(WorktreeError::Io(format!(
            "commit failed after a clean apply: {commit_error}{}",
            if restored_cleanly { "" } else { " (rollback incomplete — see logs, primary_root may still be dirty)" }
        )));
    }
    Ok(())
}
```

Add this test to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn merge_back_lands_a_commit_for_an_untracked_file() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");
        let head_before = base_sha.clone();

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Agent creates a new file but never commits inside the worktree.
        std::fs::write(worktree_path.join("new-file.txt"), "generated\n").expect("write");

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        let head_after = rev_parse_head(repo.path()).await.expect("rev-parse after");
        assert_ne!(head_before, head_after, "HEAD must advance after merge-back");
        assert!(repo.path().join("new-file.txt").exists());

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_reports_a_conflict_without_advancing_head() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Worktree edits a line...
        std::fs::write(worktree_path.join("README.md"), "hello\nfrom worktree\n").expect("write");

        // ...while the primary checkout independently edits and commits the same line.
        std::fs::write(repo.path().join("README.md"), "hello\nfrom primary\n").expect("write");
        run(repo.path(), &["git", "add", "."]);
        run(repo.path(), &["git", "commit", "-m", "primary edit"]);
        let head_before_merge = rev_parse_head(repo.path()).await.expect("rev-parse");

        let result =
            merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4()).await;
        assert!(matches!(result, Err(WorktreeError::Conflict(_))));

        let head_after_merge = rev_parse_head(repo.path()).await.expect("rev-parse after");
        assert_eq!(
            head_before_merge, head_after_merge,
            "HEAD must not advance when merge-back conflicts"
        );

        // A failed 3-way apply must not leave primary_root with unmerged
        // index entries or a dirty working tree for the next task to inherit.
        let ls_files_unmerged = StdCommand::new("git")
            .args(["ls-files", "-u"])
            .current_dir(repo.path())
            .output()
            .expect("git ls-files -u");
        assert!(
            ls_files_unmerged.stdout.is_empty(),
            "primary_root must have no unmerged entries after a failed merge-back"
        );
        let status_output = StdCommand::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo.path())
            .output()
            .expect("git status --porcelain");
        assert!(
            status_output.stdout.is_empty(),
            "primary_root must be clean after a failed merge-back"
        );

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_never_touches_unrelated_uncommitted_primary_changes() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Worktree adds a new, unrelated file.
        std::fs::write(worktree_path.join("from-worktree.txt"), "isolated\n").expect("write");

        // Simulates a live unisolated task with in-progress, unrelated,
        // uncommitted edits directly in the primary checkout — one staged
        // (file A) and one merely written but never staged (file B). Both
        // shapes of "unrelated dirty state" must survive untouched.
        std::fs::write(repo.path().join("live-task-staged.txt"), "wip staged\n").expect("write");
        run(repo.path(), &["git", "add", "live-task-staged.txt"]);
        std::fs::write(repo.path().join("live-task-unstaged.txt"), "wip unstaged\n").expect("write");

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        assert!(repo.path().join("from-worktree.txt").exists());
        // The concurrently "live" task's staged and unstaged files must both
        // survive exactly as they were — neither committed into this
        // merge's commit nor reset away.
        assert!(repo.path().join("live-task-staged.txt").exists());
        assert!(repo.path().join("live-task-unstaged.txt").exists());
        let status = StdCommand::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(repo.path())
            .output()
            .expect("git diff --cached --name-only");
        let staged = String::from_utf8_lossy(&status.stdout);
        assert!(
            staged.contains("live-task-staged.txt"),
            "unrelated staged file must remain staged, not swept into this merge's commit"
        );
        let status_porcelain = StdCommand::new("git")
            .args(["status", "--porcelain", "live-task-unstaged.txt"])
            .current_dir(repo.path())
            .output()
            .expect("git status --porcelain live-task-unstaged.txt");
        assert!(
            String::from_utf8_lossy(&status_porcelain.stdout).contains("??"),
            "unrelated unstaged file must remain untracked/unstaged, not swept into this merge's commit"
        );

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_lands_binary_file_changes() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        let binary_content: &[u8] = &[0u8, 159, 146, 150, 0, 1, 2, 255, 254, 253];
        std::fs::write(worktree_path.join("image.bin"), binary_content).expect("write binary");

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        let landed = std::fs::read(repo.path().join("image.bin")).expect("read merged binary");
        assert_eq!(landed, binary_content, "binary content must survive merge-back byte-for-byte");

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_restores_cleanly_when_a_conflict_coincides_with_a_rename() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        std::fs::write(repo.path().join("other.txt"), "base\n").expect("write");
        run(repo.path(), &["git", "add", "."]);
        run(repo.path(), &["git", "commit", "-m", "add other.txt"]);
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Worktree renames README.md *and* edits an unrelated file.
        std::fs::rename(
            worktree_path.join("README.md"),
            worktree_path.join("RENAMED.md"),
        )
        .expect("rename");
        std::fs::write(worktree_path.join("other.txt"), "from worktree\n").expect("write");

        // Primary independently edits the same unrelated file, forcing a conflict.
        std::fs::write(repo.path().join("other.txt"), "from primary\n").expect("write");
        run(repo.path(), &["git", "add", "."]);
        run(repo.path(), &["git", "commit", "-m", "primary edit"]);
        let head_before_merge = rev_parse_head(repo.path()).await.expect("rev-parse");

        let result =
            merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4()).await;
        assert!(matches!(result, Err(WorktreeError::Conflict(_))));

        let head_after_merge = rev_parse_head(repo.path()).await.expect("rev-parse after");
        assert_eq!(head_before_merge, head_after_merge, "HEAD must not advance");

        // Regression check: lumping a rename's old+new name into the same
        // `existing` bucket makes `git checkout HEAD -- <old> <new>` error
        // out on the unmatched new-name pathspec, aborting the restore for
        // every path in that invocation — not just the rename — and leaving
        // the repo dirty.
        let status_output = StdCommand::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo.path())
            .output()
            .expect("git status --porcelain");
        assert!(
            status_output.stdout.is_empty(),
            "primary_root must be fully clean after a conflict that coincides with a rename"
        );

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_lands_a_file_with_a_non_ascii_name() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Without `-z`, git's default core.quotePath C-style-quotes this
        // filename, and a naive tab/newline split would leave the quote
        // characters embedded in the "path", breaking every later
        // checkout/reset/commit pathspec built from it.
        let filename = "отчёт.txt";
        std::fs::write(worktree_path.join(filename), "содержимое\n").expect("write");

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        assert!(
            repo.path().join(filename).exists(),
            "a non-ASCII filename must land at the exact same name on primary_root"
        );

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p evohime-server task::worktree:: -- --nocapture`
Expected: all twelve tests `PASS`.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/task/worktree.rs
git commit -m "feat(server): add worktree merge-back with 3-way apply + squash commit (7.107)"
```

---

### Task 4: `AppState.workspace_merge_locks` (per-path lock registry) and module registration

**Files:**
- Modify: `crates/server/src/app.rs`
- Modify: `crates/server/src/startup.rs`
- Modify: `crates/server/src/task/mod.rs`

**Interfaces:**
- Produces:
  - `AppState.workspace_merge_locks: Arc<tokio::sync::Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>`
  - `AppState::merge_lock_for(&self, primary_root: &Path) -> Arc<tokio::sync::Mutex<()>>`

A single global lock would make an unrelated task on workspace B wait on a slow merge for workspace A for no reason (Sites feature allows distinct `primary_workspace_root`s to be active concurrently). `merge_lock_for` briefly locks the outer map to get-or-insert a per-path `Mutex`, then returns it — the outer map lock is never held for the duration of an actual merge.

- [ ] **Step 1: Register the worktree module**

In `crates/server/src/task/mod.rs`, add `pub(crate) mod worktree;` after `pub mod steps;`:

```rust
//! Task orchestration: pipeline, steps, memory, helpers.
pub(crate) mod approval_review;
pub mod helpers;
pub mod memory;
pub mod pipeline;
pub mod steps;
pub(crate) mod worktree;

pub(crate) use helpers::*;
pub(crate) use memory::*;
pub(crate) use pipeline::*;
pub(crate) use steps::*;
```

- [ ] **Step 2: Add the field and helper method to `AppState`**

In `crates/server/src/app.rs`, add the field right after `task_cancellations` (around line 124; also add `use std::path::PathBuf;` to the top-level `use` block if not already present — it is, via `AppConfig.workspace_root: PathBuf`):

```rust
    pub task_cancellations: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    /// Per-`primary_workspace_root` locks serializing merge-back (Stage
    /// 7.107): applying an isolated worktree's diff onto its primary
    /// checkout and committing it there. Keyed by path rather than a single
    /// global lock so unrelated workspaces (Sites feature) never wait on
    /// each other. `tokio::sync::Mutex` like every other AppState lock — it
    /// never poisons on panic.
    pub workspace_merge_locks: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
```

In the `impl AppState` block (starts around line 139), add:

```rust
    /// Returns the merge-back lock for `primary_root`, creating one on
    /// first use. The outer map lock is held only long enough to
    /// get-or-insert the entry, never for the duration of a merge.
    pub(crate) async fn merge_lock_for(&self, primary_root: &std::path::Path) -> Arc<Mutex<()>> {
        let mut locks = self.workspace_merge_locks.lock().await;
        locks
            .entry(primary_root.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
```

- [ ] **Step 3: Construct it in `startup.rs`**

In `crates/server/src/startup.rs`, add the field to the `AppState { ... }` literal (around line 121, right after `task_cancellations: Arc::new(Mutex::new(HashMap::new())),`):

```rust
        task_cancellations: Arc::new(Mutex::new(HashMap::new())),
        workspace_merge_locks: Arc::new(Mutex::new(HashMap::new())),
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p evohime-server`
Expected: no errors (this task only adds a field/method and wires it up — nothing calls `merge_lock_for` yet, but it's `pub(crate)` so no dead-code warning fires for an unused-but-reachable method... actually it *will* warn as unused until Task 6 calls it. That's expected and resolved by the next task — do not silence it with `#[allow(dead_code)]`.)

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/app.rs crates/server/src/startup.rs crates/server/src/task/mod.rs
git commit -m "feat(server): add per-workspace merge lock registry to AppState (7.107)"
```

---

### Task 5: Trigger wiring in `ws.rs` — atomic decision, provisioning, fail-fast

**Files:**
- Modify: `crates/server/src/task/worktree.rs` (add the `AppState`-aware `provision_worktree`)
- Modify: `crates/server/src/ws.rs`

**Interfaces:**
- Consumes: `WorktreeError`, `rev_parse_head`, `add_worktree`, `remove_worktree` (Task 2); `evohime_storage::task_worktrees::{insert_task_worktree, NewTaskWorktree}`, `evohime_storage::load_task` (Task 1 / already existing); `AppState` (Task 4).
- Produces:
  - `pub(crate) async fn provision_worktree(state: &Arc<AppState>, task_id: Uuid, primary_root: &Path) -> Result<(), WorktreeError>`
  - `pub(crate) async fn release_task_cancellation_if_terminal(state: &Arc<AppState>, task_id: Uuid)` in `crate::task::helpers`

- [ ] **Step 1: Add `provision_worktree` to `worktree.rs`**

Add near the top of `crates/server/src/task/worktree.rs`, after the existing `use` block (add `use crate::app::AppState;` and `use std::sync::Arc;` to the imports):

```rust
pub(crate) async fn provision_worktree(
    state: &Arc<AppState>,
    task_id: Uuid,
    primary_root: &Path,
) -> Result<(), WorktreeError> {
    // Idempotent: Task 5, Step 5's `TaskRetry` teardown calls this again for
    // a task_id that already had a row once, after deleting the old one —
    // so a plain "row exists → already done" check isn't quite enough on
    // its own. A row can also outlive its own worktree directory: if a
    // caller's own removal succeeds but the matching `delete_task_worktree`
    // afterward fails (e.g. a transient DB blip right after a successful
    // `git worktree remove`), the row is left pointing at a directory that
    // no longer exists. Treating that phantom row as "already provisioned"
    // would make every later call here silently no-op forever, while
    // `pipeline.rs` keeps trying to run an agent against a workspace_root
    // that was never actually recreated. So: an existing row only means
    // "already done" if its directory is actually still there; otherwise
    // treat it as stale and reprovision fresh, same as a missing row.
    if let Some(existing) =
        evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id)
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to check for existing task_worktrees row: {error}")))?
    {
        if Path::new(&existing.worktree_path).exists() {
            return Ok(());
        }
        tracing::warn!(
            %task_id,
            worktree_path = %existing.worktree_path,
            "found a task_worktrees row with no matching directory; discarding it and reprovisioning"
        );
        if let Err(error) =
            evohime_storage::task_worktrees::delete_task_worktree(&state.pool, task_id).await
        {
            return Err(WorktreeError::Io(format!(
                "failed to delete phantom task_worktrees row before reprovisioning: {error}"
            )));
        }
    }

    let base_sha = rev_parse_head(primary_root).await?;
    let worktree_path = std::env::temp_dir()
        .join("evohime-worktrees")
        .join(task_id.to_string());

    add_worktree(primary_root, &worktree_path, &base_sha).await?;

    if let Err(error) = evohime_storage::task_worktrees::insert_task_worktree(
        &state.pool,
        &evohime_storage::task_worktrees::NewTaskWorktree {
            task_id,
            base_commit_sha: base_sha,
            worktree_path: worktree_path.to_string_lossy().into_owned(),
            primary_workspace_root: primary_root.to_string_lossy().into_owned(),
        },
    )
    .await
    {
        // Roll back: without a row, nothing (merge-back, startup cleanup)
        // will ever find this directory again — it would become a
        // permanent orphan otherwise. If this rollback attempt *also*
        // fails (e.g. a transient git/filesystem error right on top of the
        // DB error above), the directory is still not lost forever: it's
        // exactly what `cleanup_orphaned_worktree_directories` (Task 7)
        // exists to sweep — a `.git`-marked directory under
        // `evohime-worktrees/` with no matching `task_worktrees` row.
        let _ = remove_worktree(primary_root, &worktree_path).await;
        return Err(WorktreeError::Io(format!(
            "failed to persist task_worktrees row: {error}"
        )));
    }

    Ok(())
}
```

- [ ] **Step 2: Wire the trigger into `ws.rs`**

In `crates/server/src/ws.rs`, the relevant block currently reads (around lines 145–198):

```rust
                            let workspace_path = resolve_workspace_path(&state, workspace_path)?;
                            // Persist a stable public path so UI project matching works on Windows
                            // (canonicalize() otherwise yields `\\?\F:\...`).
                            let workspace_path =
                                crate::task::helpers::public_fs_path(&workspace_path);
                            let task = match start_task(
                                &state.pool,
                                session_id,
                                &content,
                                model_route.as_deref(),
                                model.as_deref(),
                                Some(&workspace_path),
                            )
                            .await
                            {
                                Ok(task) => task,
                                Err(error) => {
                                    error!("failed to create task: {error}");
                                    continue;
                                }
                            };

                            let task_id = task.id;
                            let token = CancellationToken::new();
                            state
                                .task_cancellations
                                .lock()
                                .await
                                .insert(task_id, token.clone());
                            let state_for_task = state.clone();
                            tokio::spawn(async move {
```

Replace it with:

```rust
                            let workspace_path_buf = resolve_workspace_path(&state, workspace_path)?;
                            // Persist a stable public path so UI project matching works on Windows
                            // (canonicalize() otherwise yields `\\?\F:\...`).
                            let workspace_path =
                                crate::task::helpers::public_fs_path(&workspace_path_buf);
                            let task = match start_task(
                                &state.pool,
                                session_id,
                                &content,
                                model_route.as_deref(),
                                model.as_deref(),
                                Some(&workspace_path),
                            )
                            .await
                            {
                                Ok(task) => task,
                                Err(error) => {
                                    error!("failed to create task: {error}");
                                    continue;
                                }
                            };

                            let task_id = task.id;
                            let token = CancellationToken::new();
                            // Single lock acquisition: the "is another task already
                            // running" check and this task's own registration must not
                            // be split into two separate lock/unlock pairs, or two tasks
                            // starting in the same instant could both observe an empty
                            // map and both skip isolation (7.107).
                            let is_concurrent = {
                                let mut guard = state.task_cancellations.lock().await;
                                let is_concurrent = !guard.is_empty();
                                guard.insert(task_id, token.clone());
                                is_concurrent
                            };
                            // `is_concurrent` is a snapshot at this exact instant, not
                            // re-checked after the lock above is released. If the other
                            // task finishes between here and `provision_worktree` below,
                            // this task still isolates unnecessarily — extra overhead,
                            // never a correctness issue (the reverse — starting unisolated
                            // when isolation was actually needed — is what must never
                            // happen, and this ordering guarantees that direction is safe).
                            if is_concurrent {
                                if let Err(error) = crate::task::worktree::provision_worktree(
                                    &state,
                                    task_id,
                                    &workspace_path_buf,
                                )
                                .await
                                {
                                    error!(%task_id, %error, "failed to allocate isolated worktree for concurrent task");
                                    state.task_cancellations.lock().await.remove(&task_id);
                                    let _ = fail_task(&state.pool, task_id).await;
                                    let _ = emit_event(
                                        &state,
                                        session_id,
                                        Some(task_id),
                                        ServerEvent::TaskFailed {
                                            task_id,
                                            error: format!(
                                                "failed to allocate isolated worktree: {error}"
                                            ),
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    continue;
                                }
                            }
                            let state_for_task = state.clone();
                            tokio::spawn(async move {
```

Note: `is_concurrent == false` (the common case) never calls `provision_worktree`, so a non-git `workspace_root` behaves exactly as it does today — this satisfies the "fallback only safe when not concurrent" rule from the design without any extra branching.

- [ ] **Step 3: Fix a live concurrency bug this feature depends on — `task_cancellations` must survive an approval pause**

`is_concurrent` (Step 2) is only correct if `task_cancellations` actually reflects every task that still has unfinished business in a workspace — including one that's `Paused` waiting on `approval.required`, not just one currently executing. That's a real broadening of what this pre-existing field means: it was a `CancellationToken` registry for actively-running tasks (its name and original purpose), and this fix repurposes it into a "task still has unfinished business" registry — a `Paused` task's entry stays not because anything might still cancel it right now, but because `is_concurrent` needs to know it exists. Renaming the field itself is out of scope here (it's pre-existing infrastructure used well beyond this feature, touching it would violate "minimize diff scope"); this paragraph is the canonical place documenting the expanded meaning for anyone reading `crates/server/src/app.rs`'s field later.

Today it doesn't: `crates/server/src/ws.rs` has four places that spawn a task run and unconditionally remove its `task_cancellations` entry once the run's `.await` resolves — the `UserMessage` handler this task is already editing (originally ~ws.rs:193-197, now shifted by Step 2's edit), and three resume paths (`ClientCommand::TaskPlanApprove`, `TaskResume`, `TaskRetry`) that all follow the identical insert-before-spawn / remove-after-await shape. `process_user_message`/`resume_task_run` return `Ok(())` both when a task truly completes *and* when it merely pauses for approval (`crates/server/src/task/pipeline.rs`'s `NeedsApproval` branch returns early). All four sites currently treat both cases identically and remove the entry either way — so a paused task's directory stops being protected by `is_concurrent` the moment it pauses, even though it's still going to resume into that same directory later. A second task starting in that window sees an empty map, runs unisolated, and can end up running *at the same time* as the first task once it's approved and resumes — the exact race this whole feature exists to prevent, on a mainline (not edge-case) flow.

Add a shared helper to `crates/server/src/task/helpers.rs` (alongside `resolve_workspace_path` etc.):

```rust
/// Removes `task_id` from `task_cancellations` only if the task has reached
/// a terminal status. A task that merely paused (e.g. for
/// `approval.required`) still has unfinished business in its workspace and
/// must keep signaling `is_concurrent` to any task starting while it's
/// paused — removing it early would let a second task start unisolated in
/// the same directory the paused task will resume into later (7.107).
pub(crate) async fn release_task_cancellation_if_terminal(state: &Arc<AppState>, task_id: Uuid) {
    let is_terminal = match evohime_storage::load_task(&state.pool, task_id).await {
        Ok(Some(task)) => matches!(task.status.as_str(), "completed" | "failed" | "cancelled"),
        Ok(None) => true, // task row is gone entirely — nothing left to track
        Err(error) => {
            tracing::warn!(%task_id, %error, "failed to check task status before releasing task_cancellations entry; leaving it in place");
            false
        }
    };
    if is_terminal {
        state.task_cancellations.lock().await.remove(&task_id);
    }
}

/// Guards against a panic inside `process_user_message`/`resume_task_run`
/// leaking a `task_cancellations` entry forever. Those calls run directly
/// inside a `tokio::spawn`'d block with no outer `.await` — a panic there
/// aborts that spawned task immediately; normal post-await cleanup code
/// (including `release_task_cancellation_if_terminal` above) never runs,
/// since Rust doesn't execute code *after* a panic within the same async
/// fn, only `Drop` impls of values already on the stack as it unwinds.
/// Construct one right after inserting into `task_cancellations`, and call
/// `.disarm()` immediately before invoking
/// `release_task_cancellation_if_terminal` on every normal (non-panicking)
/// exit path — that's what makes the forced removal fire *only* on a panic,
/// never on an ordinary pause. On an ordinary pause, disarming still lets
/// `release_task_cancellation_if_terminal`'s own terminal-status check
/// decide correctly whether to actually remove the entry; the guard itself
/// never makes that decision.
pub(crate) struct TaskCancellationGuard {
    state: Arc<AppState>,
    task_id: Uuid,
    armed: bool,
}

impl TaskCancellationGuard {
    pub(crate) fn new(state: Arc<AppState>, task_id: Uuid) -> Self {
        Self {
            state,
            task_id,
            armed: true,
        }
    }

    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TaskCancellationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // Dropping during a panic unwind can't run `.await` directly, so
        // spawn a fire-and-forget task to do the actual removal. Forced and
        // unconditional (unlike `release_task_cancellation_if_terminal`):
        // a panic means the task's real status is unknown/inconsistent, and
        // leaving every future task on this workspace isolated forever is
        // worse than the (already-abnormal) alternative.
        let state = self.state.clone();
        let task_id = self.task_id;
        tokio::spawn(async move {
            state.task_cancellations.lock().await.remove(&task_id);
        });
    }
}
```

Now, in three of the four spawn sites in `crates/server/src/ws.rs` — this task's own spawned block plus the two resume-path spawned blocks in the `TaskPlanApprove` and `TaskResume` handlers (find each with `grep -n "tokio::spawn" crates/server/src/ws.rs`) — make two changes. **Skip `TaskRetry` here** — Step 5 below replaces its entire arm from scratch (including its own copy of this exact guard wiring, plus the `is_concurrent`/`provision_worktree` logic Step 2 added to the `UserMessage` handler), so editing it here first would just be immediately overwritten:

1. Right after the block's existing `let state_for_task = state.clone();` (or equivalent clone) and before `tokio::spawn(async move { ... })`, add:

```rust
                            let mut cancellation_guard =
                                crate::task::helpers::TaskCancellationGuard::new(
                                    state_for_task.clone(),
                                    task_id,
                                );
```

Then move it into the spawned block by capturing it in the `async move` closure (it must be created outside `tokio::spawn` so a panic occurring anywhere inside the spawned future — not just after some particular line — is covered by the same guard instance; `async move` moves it in by value).

2. Replace the existing `.task_cancellations.lock().await.remove(&task_id);` call (or, per the rewiring below, the `release_task_cancellation_if_terminal` call this same step introduces) with:

```rust
                                cancellation_guard.disarm();
                                crate::task::helpers::release_task_cancellation_if_terminal(
                                    &state_for_task,
                                    task_id,
                                )
                                .await;
```

- [ ] **Step 4: Fix `TaskCancel` for a currently-paused task**

`ClientCommand::TaskCancel` (`ws.rs:200-218`) transitions the task to `Cancelled` by calling `evohime_task_engine::cancel_task` directly. For a task that's currently *paused* (no active spawned future — its `.await` already resolved when it first paused), nothing will ever re-run Step 3's cleanup check, so cancelling a paused task would transition it to `Cancelled` in the database while leaving its `task_cancellations` entry stuck forever — needlessly isolating every future task indefinitely.

For a task that's actively *running*, the situation is the opposite of what it first looks like: `cancel_task`'s `"running"` branch (`crates/task-engine/src/lib.rs:94-98`) transitions `running → cancelling → cancelled` as an immediate, synchronous DB update — it does **not** wait for the spawned `process_user_message` future to actually observe `token.cancel()` and return. That future may still be mid-way through a tool call for some time after `cancel_task` returns `Ok`, still genuinely writing to `primary_workspace_root`. Force-removing the `task_cancellations` entry the instant `cancel_task` succeeds — regardless of whether the task was `running` or `paused` beforehand — would open exactly the window this feature exists to close: a *new* task starting a moment later sees `is_concurrent = false` and runs unisolated in `primary_workspace_root`, concurrently with the still-unwinding cancelled task. The DB says `cancelled`; the filesystem doesn't know that yet. Only a *paused* task is safe to force-remove immediately, because by definition it has no live spawned future left to eventually clean up after itself — Step 4's fix must tell the two cases apart by checking the task's status *before* calling `cancel_task` (once cancelled, the DB no longer distinguishes which state it cancelled from).

```rust
                        ClientCommand::TaskCancel { task_id } => {
                            let was_paused = evohime_storage::load_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?
                                .map(|task| task.status == "paused")
                                .unwrap_or(false);
                            let cancellation =
                                state.task_cancellations.lock().await.get(&task_id).cloned();
                            if let Some(token) = cancellation {
                                token.cancel();
                            }
                            let cancel_result =
                                evohime_task_engine::cancel_task(&state.pool, task_id).await;
                            // Force-remove immediately only for a task that was
                            // `paused` (no live spawned future left to ever run
                            // Step 3's post-await cleanup on its own) — and only
                            // once the FSM transition actually landed it in
                            // `Cancelled`. A task that was `running` must NOT be
                            // force-removed here even on a successful cancel:
                            // `cancel_task`'s DB transition is immediate, but the
                            // spawned `process_user_message` future may still be
                            // actively writing to `primary_workspace_root` for
                            // some time after this call returns — removing the
                            // entry now, before that future actually stops, would
                            // let a new task start unisolated while the
                            // cancelled-but-not-yet-stopped one is still live in
                            // the same directory. For a `running` task, leave the
                            // entry in place and let that future's own eventual
                            // `release_task_cancellation_if_terminal` call (once
                            // `process_user_message` genuinely returns) remove it
                            // — that's the only moment the workspace is actually
                            // free again. See the `else` comment below for every
                            // other case.
                            if was_paused && cancel_result.is_ok() {
                                state.task_cancellations.lock().await.remove(&task_id);
                            }
                            // else: do nothing here. If the task was `running`,
                            // its own spawned future's eventual post-await
                            // `release_task_cancellation_if_terminal` call is the
                            // only correct place to release the entry — once
                            // that future actually stops writing to the
                            // workspace, not the instant `cancel_task`'s DB
                            // transition completes. If `cancel_task` itself
                            // failed (invalid FSM transition), there is nothing
                            // new to release here either: a task that was
                            // already terminal already released itself when it
                            // finished, and a task in some other live state
                            // still has a future that will release it in due
                            // course.
                            let _ = finalize_open_task_steps(&state, task_id, "cancelled").await;
```

This replaces the existing first five lines of the `TaskCancel` arm (find it via `grep -n "ClientCommand::TaskCancel" crates/server/src/ws.rs`) — everything from `emit_event(...)` onward in that arm is unchanged.

**Do not call `release_task_cancellation_if_terminal` from this handler at all**, including for the non-paused-success case — an earlier draft of this fix called it unconditionally in an `else` branch, reasoning it was harmless because it re-checks DB status before removing. That reasoning was wrong in a way that defeated the whole point of this step: by the time this line runs, `cancel_task` has *already* transitioned a `running` task all the way to `cancelled` (a terminal status) synchronously — so `release_task_cancellation_if_terminal` sees "terminal" and removes the entry immediately regardless of whether the task's actual spawned future has stopped running yet, which is exactly the premature-removal race this whole fix exists to prevent. There is no case where calling it here is both safe and necessary: a `paused` task is handled by the `if` branch above; a `running` task must wait for its own future's post-await cleanup; and any task that was already terminal before this command even ran already released itself when it finished. Do nothing else in this handler.

- [ ] **Step 4.5: Fix `TaskPlanReject` leaking a `task_cancellations` entry forever**

`ClientCommand::TaskPlanReject` (`ws.rs:453-483` before this step) requires the task to be `paused` on a plan-approval round-trip (same precondition shape as `TaskCancel`'s `was_paused`), then calls `evohime_task_engine::cancel_task` — and, before this fix, never touched `task_cancellations` at all. Before Step 3's fix, this was safe: pausing had already removed the old unconditional entry the instant the task paused. Now that a paused task's entry survives the pause by design (Step 3), rejecting its plan leaves no live spawned future behind that will ever call `release_task_cancellation_if_terminal` on its own — the entry (and the isolation it forces on every subsequent task) leaks permanently until the server restarts. This is the same class of gap Step 4 closed for `TaskCancel`'s paused case, just on a different command that also transitions a paused task straight to a terminal status.

In `crates/server/src/ws.rs`'s `ClientCommand::TaskPlanReject` handler, right after the existing `evohime_task_engine::cancel_task(&state.pool, task_id).await.map_err(...)?;` call succeeds (and before `finalize_open_task_steps`), add:

```rust
                            // The `pending` check above already confirmed this
                            // task was `paused` — same precondition as
                            // `TaskCancel`'s `was_paused` branch — so there is
                            // no live spawned future left that will ever run
                            // its own post-await cleanup. Force-remove now or
                            // this entry (and the isolation it forces on every
                            // subsequent task) leaks until server restart.
                            state.task_cancellations.lock().await.remove(&task_id);
```

Placed after the `?` on `cancel_task` (so a failed transition never triggers the removal) and before `finalize_open_task_steps`/`merge_checkpoint`/the `emit_event` calls (so a later failure in any of those can't leave the removal half-applied — the removal itself is infallible and synchronous, so ordering it early is simply cleaner, not required for correctness). `pending` at `ws.rs:464-467` already requires `task.status == "paused"` before this line is ever reachable, so this cannot accidentally force-remove an entry belonging to a task that's actually still `running`.

- [ ] **Step 5: Fix `TaskRetry` reusing a stale worktree after a failed merge**

`retry_task` (`crates/task-engine/src/lib.rs`) transitions the *same* `task_id`'s row `failed → retrying → running` — it does not create a new task. `finalize_worktree` (Task 6) deliberately leaves a task's `task_worktrees` row and worktree directory in place when `merge_worktree_into_primary` fails, for manual inspection. Combined, this means: a task that failed because its merge-back conflicted, when retried via `ClientCommand::TaskRetry`, resumes into `pipeline.rs`'s `get_task_worktree` lookup, which finds that same old row and points `workspace_root` right back at the same worktree — still holding whatever `git add -A`-staged state the failed merge attempt left inside it, and still carrying `base_commit_sha` from whenever it was *originally* provisioned, which may now be stale if the primary checkout has moved forward via other tasks' merges since. The retried run's own eventual merge-back would then diff against a base that no longer reflects primary's real history, risking a spurious conflict or, in the worst case, silently missing changes primary already incorporated through an unrelated merge in between.

A task that failed for a reason *other* than a merge conflict (e.g. the agent itself errored) never reached `finalize_worktree` at all — its worktree still has whatever the agent produced, uncommitted, and resuming into it on retry is exactly the desired "continue where it left off" behavior. The two cases are indistinguishable from `task_worktrees` alone (both just leave a row behind), so the safe general fix is: always reprovision fresh on retry rather than trying to tell the two cases apart. The in-progress agent work in the second case is not silently lost either way — nothing had merged it into primary yet, so discarding the isolated copy and starting the retry from primary's current `HEAD` is a clean restart, not a partial one.

Unlike `TaskPlanApprove`/`TaskResume` (which just re-spawn `resume_task_run` and rely on `pipeline.rs`'s existing `get_task_worktree` lookup — correct as-is, since a task that was never isolated to begin with is protected from colliding with a *new* concurrent task by Step 3's fix keeping its `task_cancellations` entry alive through the pause, not by anything re-checked here), `TaskRetry`'s current handler (`crates/server/src/ws.rs:492-557`, confirmed by direct inspection) never calls `provision_worktree` or recomputes `is_concurrent` at all — it unconditionally inserts a token and spawns `resume_task_run`. That's fine as long as the row from the task's original (possibly unisolated) start is left untouched. It stops being fine the moment this step's own teardown deletes that row: with no row, `pipeline.rs` falls back to `primary_workspace_root` unconditionally, so a retried task would always run unisolated after teardown — even if some *other* task is genuinely running concurrently against the same workspace at that exact moment, which is precisely the collision this whole feature exists to prevent. Teardown-without-recompute is not an option; the fix has to redo the atomic check.

The teardown itself must also be gated on the task actually *being* `failed` before this handler touches its worktree. `crates/server/src/ws.rs`'s current `TaskRetry` handler has no precondition check at all — it calls `retry_task` (which enforces the `failed → retrying` FSM transition internally and is silently ignored via `let _ = ...` on failure) and unconditionally emits `TaskStatusChanged`/`ActionLogged` regardless of whether that transition actually happened, unlike e.g. `TaskPlanApprove`'s explicit `pending` precondition check (`ws.rs:244-248`) before it does anything real. That existing looseness is tolerable today because nothing in the unconditional part has a destructive side effect. Adding worktree teardown changes that: if `TaskRetry` is sent for a task that *isn't* actually `failed` — a stale client message, a race with another status change — blindly tearing down `task_worktrees` for it would rip an in-use worktree out from under a task that's genuinely still `running`/`paused` and actively isolated. The fix must check the already-loaded `task`'s own status before doing anything destructive, not rely on `retry_task`'s internal FSM check running (silently ignored) after the fact.

In `crates/server/src/ws.rs`'s `ClientCommand::TaskRetry` handler, replace the existing body from `let task = match evohime_storage::load_task(...)` (`ws.rs:493`) through the `let state_for_task = state.clone();`/`tokio::spawn(async move { ... });` block (`ws.rs:533-556`) with:

```rust
                        ClientCommand::TaskRetry { task_id } => {
                            let task = match evohime_storage::load_task(&state.pool, task_id)
                                .await
                                .map_err(|error| ApiError::Internal(error.to_string()))?
                            {
                                Some(task) => task,
                                None => continue,
                            };
                            // Precondition gate for the worktree teardown below
                            // (new in 7.107): only a task that is actually
                            // `failed` right now may have its worktree torn
                            // down. Without this check, a stale/invalid
                            // `TaskRetry` for a task that's genuinely still
                            // `running` or `paused` would destroy a worktree
                            // still in active use — `retry_task`'s own FSM
                            // check happens too late (after teardown) and its
                            // failure is silently ignored (`let _ = ...`)
                            // exactly as it already is below, so it cannot be
                            // relied on to prevent this.
                            if task.status != "failed" {
                                continue;
                            }
                            // Discard any worktree left behind by a prior failed
                            // attempt (whether it failed mid-merge or mid-agent-run)
                            // rather than resuming into potentially stale/dirty
                            // state — see this step's design note above. Best-effort:
                            // a failure here just means the row/directory are left
                            // for the next startup cleanup pass, same as any other
                            // `remove_worktree` failure elsewhere in this feature.
                            if let Ok(Some(row)) =
                                evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id)
                                    .await
                            {
                                let worktree_path = std::path::PathBuf::from(&row.worktree_path);
                                let primary_root = std::path::PathBuf::from(&row.primary_workspace_root);
                                let lock = state.merge_lock_for(&primary_root).await;
                                let _guard = lock.lock().await;
                                if let Err(error) =
                                    crate::task::worktree::remove_worktree(&primary_root, &worktree_path)
                                        .await
                                {
                                    tracing::warn!(%task_id, %error, "failed to remove stale worktree before retry; leaving it for startup cleanup");
                                } else if let Err(error) = evohime_storage::task_worktrees::delete_task_worktree(
                                    &state.pool, task_id,
                                )
                                .await
                                {
                                    tracing::warn!(%task_id, %error, "failed to delete stale task_worktrees row before retry");
                                }
                            }
                            let _ = retry_task(&state.pool, task_id).await;
                            state.metrics.task_retry(session_id, task_id);
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::TaskStatusChanged {
                                    task_id,
                                    status: "running".to_string(),
                                },
                            )
                            .await?;
                            emit_event(
                                &state,
                                session_id,
                                Some(task_id),
                                ServerEvent::ActionLogged {
                                    task_id,
                                    action: "task.retry".to_string(),
                                    detail: "Failed task scheduled for retry".to_string(),
                                    created_at: chrono::Utc::now(),
                                    correlation_id: Some(task_id),
                                    duration_ms: None,
                                },
                            )
                            .await?;

                            let token = CancellationToken::new();
                            // Resolve the workspace path BEFORE the atomic
                            // insert below: this is fallible (e.g.
                            // `canonicalize()` failing on a since-deleted/
                            // renamed project directory), and a bare `?`
                            // failing here must happen before anything is
                            // inserted into `task_cancellations` — otherwise
                            // the freshly-inserted entry for `task_id` would
                            // leak forever with nothing left to ever release
                            // it (the retry never got far enough to spawn
                            // anything).
                            let primary_root = crate::task::helpers::resolve_workspace_path(
                                &state,
                                task.workspace_path.clone(),
                            )?;
                            // Same atomic insert-and-check as the `UserMessage`
                            // handler (Step 2) — teardown above may have just
                            // deleted this task's own row, so whether it needs a
                            // *fresh* one now depends on the current state of
                            // `task_cancellations` at this exact instant, not on
                            // whatever was true when the task originally started.
                            let is_concurrent = {
                                let mut guard = state.task_cancellations.lock().await;
                                let is_concurrent = !guard.is_empty();
                                guard.insert(task_id, token.clone());
                                is_concurrent
                            };
                            if is_concurrent {
                                if let Err(error) = crate::task::worktree::provision_worktree(
                                    &state, task_id, &primary_root,
                                )
                                .await
                                {
                                    error!(%task_id, %error, "failed to allocate isolated worktree for retried concurrent task");
                                    state.task_cancellations.lock().await.remove(&task_id);
                                    let _ = fail_task(&state.pool, task_id).await;
                                    let _ = emit_event(
                                        &state,
                                        session_id,
                                        Some(task_id),
                                        ServerEvent::TaskFailed {
                                            task_id,
                                            error: format!("failed to allocate isolated worktree: {error}"),
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    continue;
                                }
                            }
                            let mut cancellation_guard =
                                crate::task::helpers::TaskCancellationGuard::new(state.clone(), task_id);
                            let state_for_task = state.clone();
                            tokio::spawn(async move {
                                if let Err((task_id, error)) =
                                    resume_task_run(&state_for_task, task, token, false).await
                                {
                                    let _ = emit_event(
                                        &state_for_task,
                                        session_id,
                                        Some(task_id),
                                        ServerEvent::TaskFailed {
                                            task_id,
                                            error: error.to_string(),
                                            duration_ms: None,
                                        },
                                    )
                                    .await;
                                    let _ = fail_task(&state_for_task.pool, task_id).await;
                                }
                                cancellation_guard.disarm();
                                crate::task::helpers::release_task_cancellation_if_terminal(
                                    &state_for_task,
                                    task_id,
                                )
                                .await;
                            });
                        }
```

One detail worth calling out about this rewrite: unlike Step 2's `UserMessage` handler, this arm's `is_concurrent` check runs *after* `retry_task`/the status-changed events, not before — that's intentional here (retry's pre-existing side effects up through emitting `ActionLogged` must fire even if provisioning subsequently fails, so the client sees the retry was accepted before it's told the retry then failed to isolate), but it does mean the `is_concurrent` snapshot is taken slightly later relative to this arm's own side effects than Step 2's is. This is still safe for the same reason Step 2's own snapshot-then-provision gap is safe (documented inline there): the only direction that must never happen is starting unisolated when isolation was actually needed, and inserting into `task_cancellations` before provisioning (not after) still guarantees that. `resolve_workspace_path(state: &Arc<AppState>, requested_path: Option<String>) -> Result<PathBuf, ApiError>` (`crates/server/src/task/helpers.rs:42`, confirmed by direct inspection) takes an owned `Option<String>`, hence `task.workspace_path.clone()` above rather than `.as_deref()`.

Note the specific placement of `resolve_workspace_path(...)?` *before* the atomic `is_concurrent`/insert block, not after it. `resolve_workspace_path` is fallible in ordinary ways (e.g. `canonicalize()` failing on a since-deleted or renamed project directory), and its bare `?` propagates out of this whole message-handling function. If that call ran *after* the insert into `task_cancellations` — as an earlier draft of this fix did — a failure there would leak the just-inserted entry forever (nothing downstream would ever release it, since the retry never got far enough to spawn anything) on top of killing the client's websocket connection. Resolving the path first means a failure here happens before any state mutation, safe by construction, and consistent with how this same handler already treats `evohime_storage::load_task(...)?` at its very top.

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p evohime-server`
Expected: no errors. `fail_task` is already imported via `use evohime_task_engine::{fail_task, resume_task, retry_task, start_task};` at the top of `ws.rs`, so the unqualified call in Step 2 resolves correctly. `evohime_storage::load_task` is already used elsewhere in this file (`TaskPlanReject`), so no new import is needed for it in `helpers.rs` beyond what's already there (`evohime_storage` is used unqualified via its crate name throughout this module already).

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/task/worktree.rs crates/server/src/task/helpers.rs crates/server/src/ws.rs
git commit -m "feat(server): trigger worktree isolation atomically; keep task_cancellations alive through approval pauses and retries (7.107)"
```

> **Note (relevant to Task 7):** `AppState.task_cancellations` starts empty on every process restart, and `recover_after_restart` never touches or returns a task that was *already* `paused` before the restart (only ones that were crash-interrupted `running`/`cancelling`). That leaves a real gap — a restart-surviving `paused` task has no `task_cancellations` entry until it resumes, so a task starting in that window sees `is_concurrent = false` and runs unisolated, right up until the paused task resumes into the same directory. Task 7, Step 3.5 below closes this, since it's the task that already touches `startup.rs` for post-restart bookkeeping.

---

### Task 6: `pipeline.rs` — workspace_root override and merge-back on success

**Files:**
- Modify: `crates/server/src/task/worktree.rs` (add `finalize_worktree`)
- Modify: `crates/server/src/task/pipeline.rs`

**Interfaces:**
- Consumes: `merge_worktree_into_primary`, `remove_worktree`, `WorktreeError` (Task 2/3); `evohime_storage::task_worktrees::{get_task_worktree, delete_task_worktree, TaskWorktreeRow}` (Task 1); `AppState::merge_lock_for` (Task 4).
- Produces: `pub(crate) async fn finalize_worktree(state: &Arc<AppState>, task_id: Uuid, primary_root: &Path, row: &evohime_storage::task_worktrees::TaskWorktreeRow) -> Result<(), WorktreeError>`

- [ ] **Step 1: Add `finalize_worktree` to `worktree.rs`**

Add after `provision_worktree`:

```rust
pub(crate) async fn finalize_worktree(
    state: &Arc<AppState>,
    task_id: Uuid,
    primary_root: &Path,
    row: &evohime_storage::task_worktrees::TaskWorktreeRow,
) -> Result<(), WorktreeError> {
    let lock = state.merge_lock_for(primary_root).await;
    let _guard = lock.lock().await;

    let worktree_path = PathBuf::from(&row.worktree_path);
    // On Err (conflict), merge_worktree_into_primary has already restored
    // just its own changed paths on primary_root — the row and worktree
    // are deliberately left in place here for manual recovery (design
    // doc, Merge-back step 8), so this function's caller (pipeline.rs) is
    // free to just propagate the error as a task failure.
    merge_worktree_into_primary(&worktree_path, primary_root, &row.base_commit_sha, task_id).await?;

    // By this point the merge has already committed successfully on
    // primary_root — the task's user-visible work is done. A failure past
    // this point is pure housekeeping (a file lock on Windows, a transient
    // DB error), not work loss: log it and leave the worktree/row for the
    // next server-startup cleanup pass to retry, rather than reporting the
    // task itself as failed.
    //
    // git worktree remove requires --force here: the worktree is still
    // "dirty" relative to its own base_commit_sha (nothing was reset inside
    // it), even though its diff already landed on primary_root.
    if let Err(error) = remove_worktree(primary_root, &worktree_path).await {
        tracing::warn!(%task_id, %error, "failed to remove worktree after a successful merge; leaving it for startup cleanup to retry");
        // Deliberately does NOT call `delete_task_worktree` here even though
        // the merge itself already succeeded: keeping the row is what lets
        // `cleanup_stale_worktrees` (Task 7) retry through the git-aware
        // `remove_worktree` (which also runs `git worktree prune`) on the
        // next pass. Deleting the row instead would only hand this
        // directory to `cleanup_orphaned_worktree_directories`'s plain
        // `remove_dir_all` sweep — which never touches `.git/worktrees/`
        // metadata in `primary_root` — reintroducing the same leftover-
        // metadata problem Task 2's `remove_worktree` fix exists to close.
        return Ok(());
    }

    if let Err(error) = evohime_storage::task_worktrees::delete_task_worktree(&state.pool, task_id).await {
        tracing::warn!(%task_id, %error, "failed to delete task_worktrees row after a successful merge; leaving it for startup cleanup to retry");
    }

    Ok(())
}
```

- [ ] **Step 2: Override `workspace_root` and call `finalize_worktree` in `pipeline.rs`**

In `crates/server/src/task/pipeline.rs`, the current block (lines 73–81) reads:

```rust
    let workspace_scope = task
        .workspace_path
        .clone()
        .unwrap_or_else(|| state.workspace_root.to_string_lossy().into_owned());
    let workspace_root = task
        .workspace_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| state.workspace_root.clone());
```

Replace it with:

```rust
    let workspace_scope = task
        .workspace_path
        .clone()
        .unwrap_or_else(|| state.workspace_root.to_string_lossy().into_owned());
    // `primary_workspace_root` is the task's own semantic project root — the
    // same value used for memory scoping/UI display, and what merge-back
    // must land its commit on. `workspace_root` may be overridden below to
    // point at an isolated worktree; memory/UI scoping never is.
    let primary_workspace_root = task
        .workspace_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| state.workspace_root.clone());
    let worktree_row = evohime_storage::task_worktrees::get_task_worktree(&state.pool, task.id)
        .await
        .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    let workspace_root = worktree_row
        .as_ref()
        .map(|row| PathBuf::from(&row.worktree_path))
        .unwrap_or_else(|| primary_workspace_root.clone());
```

A few lines down, the `demo_file_path` computation inside `AgentConfig { .. }` (around line 204–209) currently reads:

```rust
        demo_file_path: task
            .workspace_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| state.workspace_root.clone())
            .join("docs/sample-context.md"),
```

Replace it with (reusing the already-resolved, possibly-isolated `workspace_root`):

```rust
        demo_file_path: workspace_root.join("docs/sample-context.md"),
```

Immediately after the `let agent_result = match agent_result { ... };` block finishes (right before the `evohime_storage::insert_message(...)` call at line 476), insert:

```rust
    if let Some(row) = &worktree_row {
        crate::task::worktree::finalize_worktree(state, task.id, &primary_workspace_root, row)
            .await
            .map_err(|error| (task.id, ApiError::Internal(error.to_string())))?;
    }
```

This placement matters: it runs only on the success path (the `match agent_result` block already returned early with `Err(...)` for approval-pause, agent failure, or join errors), and it runs *before* `process_user_message` returns — so `crates/server/src/ws.rs`'s `release_task_cancellation_if_terminal` call (Task 5, which only runs after `process_user_message(...).await` resolves, and only actually removes the entry once the task's status is terminal) naturally waits for merge-back to finish first. A failure in `merge_worktree_into_primary` itself propagates as `Err((task.id, ApiError::Internal(...)))`, which the `ws.rs` spawn wrapper already turns into `fail_task` + a `TaskFailed` event — no new error-handling path needed there. A failure only in `finalize_worktree`'s post-merge cleanup (Task 6, Step 1) does **not** propagate — `finalize_worktree` returns `Ok(())` in that case, since the merge itself already succeeded and the task is genuinely done; only the scratch directory's removal is deferred to startup cleanup.

**Caution for whoever applies this step:** `workspace_root` is a pre-existing name in `pipeline.rs`, reused here for the (now possibly worktree-overridden) value — and at least one pre-existing call site a few lines above this block, `claim_attachment_context(state, session_id, task.id, &workspace_root)`, implicitly assumed `workspace_root` always meant the *primary* root, since that was the only meaning it ever had before this task. Attachments are uploaded into the primary root and their stored paths are relative to it; a worktree (populated purely from a git commit) never contains an attachment uploaded after that commit. Search the function for every existing use of `workspace_root` before this step's edit lands, and change any that are primary-root-relative data (not agent tool-call sandboxing) to use `primary_workspace_root` instead — `claim_attachment_context`'s call is a confirmed instance of this, found by direct inspection; there is no guarantee it's the only one, since introducing a new meaning for an existing variable name silently changes every pre-existing reader of it, not just the ones this step's own diff touches.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p evohime-server`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/worktree.rs crates/server/src/task/pipeline.rs
git commit -m "feat(server): merge isolated worktree back into primary checkout on task success (7.107)"
```

---

### Task 7: Startup cleanup of stale worktrees

**Files:**
- Modify: `crates/server/src/task/worktree.rs` (add `cleanup_stale_worktrees`)
- Modify: `crates/server/src/startup.rs`

**Interfaces:**
- Consumes: `evohime_storage::task_worktrees::{list_task_worktrees_with_status, get_task_worktree, delete_task_worktree}` (Task 1); `remove_worktree` (Task 2).
- Produces:
  - `pub(crate) async fn cleanup_stale_worktrees(state: &Arc<AppState>, retention: Duration)`
  - `pub(crate) async fn cleanup_orphaned_worktree_directories(state: &Arc<AppState>)`
  - `pub(crate) async fn worktree_cleanup_loop(state: Arc<AppState>, interval: Duration, retention: Duration)`
  - `pub(crate) const TERMINAL_TASK_STATUSES: &[&str]`

Cleanup decides per row from the owning task's **current** status (`task_status`, joined in `list_task_worktrees_with_status`), not from a restart-scoped snapshot. This matters concretely: a task `Paused` on an `approval.required` round-trip never appears in `recover_after_restart`'s return value on a later, unrelated restart (it wasn't crashed — pipeline.rs's `NeedsApproval` branch returns `Ok(())` normally), yet its worktree is still in active use and must never be swept. Querying live status instead of that snapshot handles this correctly and makes the function's signature simpler (no `resumable_task_ids` parameter to keep in sync with anything).

- [ ] **Step 1: Add `cleanup_stale_worktrees` to `worktree.rs`**

Add after `finalize_worktree` (add `use std::time::Duration;` and `use tracing::warn;` to the imports):

```rust
/// The single, centralized definition of "terminal" for a task's
/// `evohime_storage::TaskRow.status` string. Both this module's cleanup and
/// `crate::task::helpers::release_task_cancellation_if_terminal` (Task 5)
/// key off exactly this list — if a new status is ever added upstream
/// (`evohime_protocol::TaskStatus`), it must be classified here in one
/// place, not re-derived independently in multiple call sites where one of
/// them could silently drift and either leak a worktree that should have
/// been cleaned, or — worse — sweep one still needed by an active task.
pub(crate) const TERMINAL_TASK_STATUSES: &[&str] = &["completed", "failed", "cancelled"];

/// Removes worktree directories (and their `task_worktrees` rows) whose
/// owning task has reached a terminal status (`completed`/`failed`/
/// `cancelled`) and is older than `retention`. A `completed` row should
/// never actually be found here — a successful merge-back deletes its own
/// row before the task transitions to `completed` — but is handled the
/// same way rather than treated as a bug if a crash landed one anyway.
///
/// Rows for non-terminal tasks (`running`, `paused`, `cancelling`,
/// `retrying`) are never touched regardless of age: the task may still
/// resume and reuse that exact worktree. Terminal rows younger than
/// `retention` are left for a later startup, giving an operator a window
/// to inspect a worktree kept around after a merge conflict before it's
/// swept.
pub(crate) async fn cleanup_stale_worktrees(state: &Arc<AppState>, retention: Duration) {
    let entries = match evohime_storage::task_worktrees::list_task_worktrees_with_status(&state.pool).await {
        Ok(entries) => entries,
        Err(error) => {
            warn!(%error, "failed to list task_worktrees for startup cleanup");
            return;
        }
    };

    // `chrono::Duration::from_std` returns `Err` for out-of-range inputs.
    // Falling back to `.unwrap_or_default()` there would silently give a
    // *zero*-length window — making `cutoff` equal to "now" and making every
    // terminal row immediately eligible for deletion regardless of its real
    // age, the exact opposite of a retention grace period. Fall back to an
    // effectively-unbounded window instead, so an extreme/misconfigured
    // value degrades to "keep everything this run" rather than "keep
    // nothing."
    // `365 * 100` is ~99.7 real years (leap years aren't accounted for), not
    // exactly 100 — irrelevant to this fallback's actual purpose ("keep
    // everything indefinitely for this run"), but naming it precisely
    // avoids implying a guarantee this constant doesn't make.
    let retention_chrono =
        chrono::Duration::from_std(retention).unwrap_or_else(|_| chrono::Duration::days(365 * 100));
    let cutoff = chrono::Utc::now() - retention_chrono;

    for entry in entries {
        let row = entry.row;
        if !TERMINAL_TASK_STATUSES.contains(&entry.task_status.as_str()) {
            continue;
        }
        if row.created_at > cutoff {
            continue;
        }

        let worktree_path = PathBuf::from(&row.worktree_path);
        let primary_root = PathBuf::from(&row.primary_workspace_root);

        if !primary_root.exists() {
            // The repo itself is gone (moved/deleted) — `git -C primary_root
            // worktree prune` can never succeed, so retrying forever would
            // leave this row stuck permanently. Best-effort remove the
            // worktree directory directly (no git involved, nothing left to
            // prune metadata from) and drop the row regardless of outcome.
            let _ = tokio::fs::remove_dir_all(&worktree_path).await;
            if let Err(error) =
                evohime_storage::task_worktrees::delete_task_worktree(&state.pool, row.task_id).await
            {
                warn!(task_id = %row.task_id, %error, "failed to delete task_worktrees row for a worktree whose primary_workspace_root no longer exists");
            }
            continue;
        }

        // Take the same per-primary-root lock a normal merge-back would
        // (`AppState::merge_lock_for`, Task 4). Without it, this cleanup
        // pass's `git worktree remove`/`prune` (both touch `.git/worktrees/`
        // metadata under primary_root's own `.git` directory) could run
        // concurrently with a *different*, still-active task's
        // `finalize_worktree` targeting the same primary_root — two git
        // invocations racing on the same repository's internal state.
        let lock = state.merge_lock_for(&primary_root).await;
        let _guard = lock.lock().await;
        if let Err(error) = remove_worktree(&primary_root, &worktree_path).await {
            warn!(
                task_id = %row.task_id,
                %error,
                "failed to remove stale worktree during startup cleanup"
            );
            // Keep the row — retry on the next restart rather than losing
            // track of a directory that may still exist.
            continue;
        }
        if let Err(error) =
            evohime_storage::task_worktrees::delete_task_worktree(&state.pool, row.task_id).await
        {
            warn!(task_id = %row.task_id, %error, "failed to delete stale task_worktrees row");
        }
    }
}

/// Sweeps `evohime-worktrees/` for directories with no matching
/// `task_worktrees` row. `task_worktrees.task_id` is a foreign key on
/// `tasks(id) ON DELETE CASCADE` — if a task or its owning session is ever
/// deleted directly (session archival, restore/import flows), Postgres
/// removes the `task_worktrees` row as a side effect without running any of
/// this application's cleanup code, permanently leaking the physical
/// directory. The row-driven pass above has no way to know such a directory
/// ever existed; this reconciles against the filesystem directly instead.
/// A missing row means nothing will ever reference this directory again, so
/// a plain `remove_dir_all` is sufficient — there's no primary repository
/// left worth pruning `git worktree` metadata against.
pub(crate) async fn cleanup_orphaned_worktree_directories(state: &Arc<AppState>) {
    let root = std::env::temp_dir().join("evohime-worktrees");
    let mut entries = match tokio::fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            warn!(%error, path = %root.display(), "failed to scan for orphaned worktree directories");
            return;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let Some(task_id) = entry
            .file_name()
            .to_str()
            .and_then(|name| Uuid::parse_str(name).ok())
        else {
            continue; // not a task-id-named directory; leave it alone
        };
        let has_row = matches!(
            evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id).await,
            Ok(Some(_))
        );
        if has_row {
            continue;
        }
        // A UUID-shaped directory name alone isn't proof this is actually
        // one of our worktrees — an unrelated directory could coincidentally
        // land here with a UUID-like name. Require the `.git` marker file
        // every real worktree checkout has before deleting anything.
        if !entry.path().join(".git").exists() {
            continue;
        }
        if let Err(error) = tokio::fs::remove_dir_all(entry.path()).await {
            warn!(%task_id, %error, "failed to remove orphaned worktree directory");
        }
    }
}
```

- [ ] **Step 2: Centralize terminal-status handling — remove the duplication in `helpers.rs`**

`release_task_cancellation_if_terminal` (Task 5) was written against its own inline `matches!(..., "completed" | "failed" | "cancelled")` because `TERMINAL_TASK_STATUSES` (just defined above) didn't exist yet at that point in the plan. Now that it does, update `crates/server/src/task/helpers.rs` so there's exactly one definition of "terminal" for the whole feature — a status added later in only one of the two places would either leak a worktree that should've been cleaned, or sweep one an active task still needs. Replace:

```rust
        Ok(Some(task)) => matches!(task.status.as_str(), "completed" | "failed" | "cancelled"),
```

with:

```rust
        Ok(Some(task)) => crate::task::worktree::TERMINAL_TASK_STATUSES.contains(&task.status.as_str()),
```

- [ ] **Step 3: Call both from `startup.rs`, and add a periodic background pass alongside the startup one**

In `crates/server/src/startup.rs`, right after the existing block that builds `recovered` and before the `if !recovered.is_empty() { ... }` loop (around line 273–278):

```rust
    let recovered = evohime_task_engine::recover_after_restart(&state.pool)
        .await
        .context("recover tasks after restart")?;
    let resume_policy = evohime_task_engine::RestartResumePolicy::from_env();
```

Insert directly after the `resume_policy` line, before `if !recovered.is_empty() {`:

```rust
    let worktree_retention = duration_secs_env_local("EVOHIME_WORKTREE_RETENTION_SECS", 24 * 60 * 60);
    crate::task::worktree::cleanup_stale_worktrees(&state, worktree_retention).await;
    crate::task::worktree::cleanup_orphaned_worktree_directories(&state).await;
```

This reuses the `duration_secs_env_local` helper already defined at the top of `startup.rs`, matching the pattern used for `WORKER_HEALTH_INTERVAL_SECS` etc. — default retention is 24 hours. It deliberately runs independently of the `recovered`/`resume_policy` logic right below it — cleanup consults live task status itself, not that restart-scoped list. The orphan sweep runs after the row-driven pass so it never races against a row that pass is still processing.

Running cleanup only at startup means a long-lived server process (weeks without a restart) never sweeps a stuck worktree — e.g. one whose `remove_worktree` in `finalize_worktree` (Task 6) kept failing (a persistent Windows file lock). This project already has an established pattern for exactly this shape of problem: `crates/server/src/worker_api.rs`'s `worker_retention_loop(state, retention_days)`, spawned once in `startup.rs` and ticking hourly. Add the same shape here.

In `crates/server/src/task/worktree.rs`, add after `cleanup_orphaned_worktree_directories`:

```rust
/// Runs both cleanup passes on a fixed interval so a stuck worktree (e.g.
/// one whose removal in `finalize_worktree` kept failing) doesn't have to
/// wait for the next server restart to be swept — mirrors
/// `crate::worker_api::worker_retention_loop`'s shape.
pub(crate) async fn worktree_cleanup_loop(state: Arc<AppState>, interval: Duration, retention: Duration) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        cleanup_stale_worktrees(&state, retention).await;
        cleanup_orphaned_worktree_directories(&state).await;
    }
}
```

Then, in `crates/server/src/startup.rs`, spawn it near the existing `worker_retention_loop`/`worker_health_loop` spawns (around line 144-152). Make the interval configurable the same way `worktree_retention` already is, rather than hardcoding it — a stuck worktree otherwise waits up to a fixed hour regardless of how urgently an operator might want to shorten that window:

```rust
    let worktree_cleanup_interval =
        duration_secs_env_local("EVOHIME_WORKTREE_CLEANUP_INTERVAL_SECS", 60 * 60);
    let worktree_cleanup_state = state.clone();
    tokio::spawn(async move {
        crate::task::worktree::worktree_cleanup_loop(
            worktree_cleanup_state,
            worktree_cleanup_interval,
            worktree_retention,
        )
        .await;
    });
```

This must come *after* the `worktree_retention` binding introduced earlier in this step (it reuses the same value the startup-time pass used, so both agree on the same retention window).

- [ ] **Step 3.5: Repopulate `task_cancellations` for tasks that were already paused before this restart**

(See the note at the end of Task 5, Step 7 for why this is needed.) Right after the `cleanup_stale_worktrees`/`cleanup_orphaned_worktree_directories` calls added in Step 3 above (so any row for a task that no longer exists has already been dropped), seed `task_cancellations` from every task still in a non-terminal status:

```rust
    let non_terminal_tasks = evohime_storage::list_tasks(&state.pool, None)
        .await
        .context("list tasks for task_cancellations startup seed")?
        .into_iter()
        .filter(|task| !crate::task::worktree::TERMINAL_TASK_STATUSES.contains(&task.status.as_str()));
    {
        let mut cancellations = state.task_cancellations.lock().await;
        for task in non_terminal_tasks {
            // A fresh token here is never wired to anything that can
            // actually cancel this specific task mid-flight — it exists
            // purely so this entry's *presence* makes `is_concurrent` true
            // for any task starting before this one resumes or is
            // cancelled. Real cancellation of an already-paused task goes
            // through `evohime_task_engine::cancel_task` directly (see
            // `TaskCancel`, Task 5 Step 4), which doesn't depend on this
            // token at all.
            cancellations
                .entry(task.id)
                .or_insert_with(tokio_util::sync::CancellationToken::new);
        }
    }
```

This runs for every non-terminal status, not just `paused` — a task caught mid-`retrying` by a crash (a narrow window inside `retry_task`'s two back-to-back transitions) has just as much unfinished business as one that's cleanly `paused`. Reusing `TERMINAL_TASK_STATUSES` (defined earlier in this same task) rather than a second hardcoded status list is what keeps this in sync with `release_task_cancellation_if_terminal` (Task 5) and `cleanup_stale_worktrees` (this task) automatically if a status is ever added or renamed later.

This deliberately loads every task row (`evohime_storage::list_tasks(pool, None)`, no status filter pushed into SQL) rather than a query pre-filtered server-side to non-terminal statuses. `TERMINAL_TASK_STATUSES` lives in `crates/server` (Task 7, Step 1); `evohime_storage` is a lower-level crate `crates/server` depends on, not the reverse, so a SQL-side `WHERE status NOT IN (...)` filter would have to hardcode its own separate copy of the same status list — reintroducing exactly the two-places-to-keep-in-sync problem `TERMINAL_TASK_STATUSES` exists to eliminate (see its own doc comment, Task 7 Step 1). Filtering in Rust against the one real definition costs one full `tasks` table scan at startup only, not per-request — acceptable for a table whose growth is bounded by actual task volume, and worth revisiting only if that table's size becomes a real startup-latency concern on its own.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p evohime-server`
Expected: no errors. Step 3.5's use of `evohime_storage::list_tasks` and `.context(...)` follows the same import pattern `recover_after_restart`'s own call already uses a few lines above it in this same function.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/task/worktree.rs crates/server/src/startup.rs
git commit -m "feat(server): clean up stale worktrees at startup, on an hourly loop, and re-arm task_cancellations for tasks paused across a restart (7.107)"
```

---

### Task 8: Orchestration-level integration test (two concurrent tasks, one conflict)

**Files:**
- Modify: `crates/server/src/task/worktree.rs` (add tests exercising `provision_worktree`/`finalize_worktree`/`cleanup_stale_worktrees` against a real Postgres, gated the same way as `plugin_audit.rs`)

**Interfaces:**
- Consumes: everything from Tasks 1–7.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `crates/server/src/task/worktree.rs`. These need a real `AppState`; build a minimal one directly (only the fields these functions touch: `pool`, `workspace_merge_locks` — `task_cancellations` is not needed here since we call `provision_worktree`/`finalize_worktree` directly). Add `use crate::app::AppState;` (already added in Task 5) and construct via `AppState` struct literal — since not all fields are `pub(crate)`-constructible outside `crate::app`, add a `#[cfg(test)] impl AppState { pub(crate) fn for_worktree_tests(pool: PgPool) -> Arc<Self> }` helper in `crates/server/src/app.rs` instead of duplicating the full literal in the test:

There is exactly one existing `AppState { ... }` construction site today (`crates/server/src/startup.rs:121`, confirmed by searching for `AppState {` across the crate) — no test-only builder exists yet, so this is genuinely new, not a duplicate.

`ModelGatewayConfig` (`crates/model-gateway/src/config.rs`) has no `Default` impl and only two fields (`default_route: String`, `routes: HashMap<String, ModelRouteConfig>`) — construct it directly with an empty route table instead of going through `from_env()`, so this test builder has zero dependency on the process environment (a stray `MODEL_ROUTES_JSON`/`MODEL_PROVIDER` value set in the test-runner's environment must never be able to change what this builder produces).

In `crates/server/src/app.rs`, add at the end of the `impl AppState` block:

```rust
    #[cfg(test)]
    pub(crate) fn for_worktree_tests(pool: sqlx::PgPool) -> Arc<Self> {
        use std::collections::HashMap;
        use tokio::sync::{Mutex, RwLock};
        Arc::new(Self {
            pool,
            workspace_root: std::env::temp_dir(),
            // `AuthConfig` derives `Default` (`api_token: None`) — use that
            // directly instead of `from_env()`, so a stray `EVOHIME_API_TOKEN`
            // set in the test-runner's own environment can't change what
            // this builder produces.
            auth: crate::auth::AuthConfig::default(),
            tools: evohime_tool_runtime::ToolRegistry::bootstrap_with_permissions(
                evohime_permissions::PermissionEngine::new(),
            ),
            permissions: evohime_permissions::PermissionEngine::new(),
            model_gateway: Arc::new(RwLock::new(None)),
            model_config: Arc::new(RwLock::new(ModelGatewayConfig {
                default_route: "default".to_string(),
                routes: HashMap::new(),
            })),
            mcp_servers: Arc::new(Mutex::new(Vec::new())),
            session_buses: Arc::new(Mutex::new(HashMap::new())),
            task_cancellations: Arc::new(Mutex::new(HashMap::new())),
            workspace_merge_locks: Arc::new(Mutex::new(HashMap::new())),
            worker: crate::worker::WorkerClient::new("http://127.0.0.1:8090".to_string())
                .expect("worker client"),
            worker_job_stall: std::time::Duration::from_secs(30),
            plugin_catalog_cache: crate::plugins::PluginCatalogCache::default(),
            metrics: Arc::new(crate::observability::PipelineMetrics::new()),
            worker_metrics: Arc::new(crate::worker_observability::WorkerMetrics::new()),
            // Same reasoning as `auth` above: construct `RateLimitConfig`
            // directly (mirroring `from_env()`'s own defaults) rather than
            // reading the environment, so a stray `EVOHIME_RATE_LIMIT_*`
            // value can't affect these tests.
            rate_limiter: Arc::new(crate::rate_limit::RateLimiter::new(
                crate::rate_limit::RateLimitConfig {
                    session_per_minute: 30,
                    task_per_minute: 60,
                    worker_job_per_minute: 30,
                    max_concurrent_tasks: 16,
                    max_concurrent_worker_jobs: 32,
                    disabled: false,
                },
            )),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            local_shutdown_secret: None,
        })
    }
```

Also add a `seed_task` test helper — `task_worktrees.task_id` is a foreign key into `tasks(id)` (Task 1's migration), so every orchestration test needs a real row there first, not a bare `Uuid::new_v4()`:

```rust
    async fn seed_task(pool: &sqlx::PgPool) -> Uuid {
        let session_id: Uuid =
            sqlx::query_scalar("INSERT INTO sessions DEFAULT VALUES RETURNING id")
                .fetch_one(pool)
                .await
                .expect("insert session");
        sqlx::query_scalar(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await
        .expect("insert task")
    }
```

Then add the tests:

```rust
    #[tokio::test]
    async fn provision_and_finalize_round_trip_through_a_shared_repo() {
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping worktree orchestration test: database unavailable");
            return;
        };
        let task_id_a = seed_task(&pool).await;
        let task_id_b = seed_task(&pool).await;
        let state = AppState::for_worktree_tests(pool);

        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());

        // Task A starts unisolated (nothing running yet) — nothing to provision.
        // Task B starts while A is "running" (simulated: A's worktree provisioned first).
        provision_worktree(&state, task_id_a, repo.path())
            .await
            .expect("provision A");
        provision_worktree(&state, task_id_b, repo.path())
            .await
            .expect("provision B");

        let row_a = evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id_a)
            .await
            .expect("get A")
            .expect("row A present");
        let row_b = evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id_b)
            .await
            .expect("get B")
            .expect("row B present");
        assert_ne!(row_a.worktree_path, row_b.worktree_path);

        std::fs::write(PathBuf::from(&row_a.worktree_path).join("a.txt"), "from a\n")
            .expect("write a");
        std::fs::write(PathBuf::from(&row_b.worktree_path).join("b.txt"), "from b\n")
            .expect("write b");

        finalize_worktree(&state, task_id_a, repo.path(), &row_a)
            .await
            .expect("finalize A");
        finalize_worktree(&state, task_id_b, repo.path(), &row_b)
            .await
            .expect("finalize B");

        assert!(repo.path().join("a.txt").exists());
        assert!(repo.path().join("b.txt").exists());
        assert!(evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id_a)
            .await
            .expect("get A after")
            .is_none());
        assert!(evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id_b)
            .await
            .expect("get B after")
            .is_none());
    }

    #[tokio::test]
    async fn cleanup_removes_only_terminal_rows_past_retention() {
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping worktree cleanup test: database unavailable");
            return;
        };
        let failed_task = seed_task(&pool).await; // starts 'running'
        let paused_task = seed_task(&pool).await;
        let recent_failed_task = seed_task(&pool).await;
        let state = AppState::for_worktree_tests(pool);

        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());

        provision_worktree(&state, failed_task, repo.path())
            .await
            .expect("provision failed_task");
        provision_worktree(&state, paused_task, repo.path())
            .await
            .expect("provision paused_task");
        provision_worktree(&state, recent_failed_task, repo.path())
            .await
            .expect("provision recent_failed_task");

        sqlx::query("UPDATE tasks SET status = 'failed' WHERE id = $1")
            .bind(failed_task)
            .execute(&state.pool)
            .await
            .expect("mark failed");
        sqlx::query("UPDATE tasks SET status = 'paused' WHERE id = $1")
            .bind(paused_task)
            .execute(&state.pool)
            .await
            .expect("mark paused");
        sqlx::query("UPDATE tasks SET status = 'failed' WHERE id = $1")
            .bind(recent_failed_task)
            .execute(&state.pool)
            .await
            .expect("mark recent_failed");

        // Retention of 0 makes every terminal row immediately eligible;
        // back-date recent_failed_task's row so it still falls inside a
        // real (non-zero) window in the second half of this test.
        cleanup_stale_worktrees(&state, Duration::from_secs(0)).await;

        assert!(
            evohime_storage::task_worktrees::get_task_worktree(&state.pool, failed_task)
                .await
                .expect("get failed_task")
                .is_none(),
            "a terminal task's row past retention must be removed"
        );
        assert!(
            evohime_storage::task_worktrees::get_task_worktree(&state.pool, paused_task)
                .await
                .expect("get paused_task")
                .is_some(),
            "a non-terminal task's row must never be removed by age alone"
        );
        assert!(
            evohime_storage::task_worktrees::get_task_worktree(&state.pool, recent_failed_task)
                .await
                .expect("get recent_failed_task")
                .is_none(),
            "retention of 0 makes even a just-created terminal row eligible"
        );

        // Re-provision recent_failed_task to exercise a real retention
        // window: with a long retention, a just-failed row must survive.
        provision_worktree(&state, recent_failed_task, repo.path())
            .await
            .expect("re-provision recent_failed_task");
        sqlx::query("UPDATE tasks SET status = 'failed' WHERE id = $1")
            .bind(recent_failed_task)
            .execute(&state.pool)
            .await
            .expect("mark recent_failed again");
        cleanup_stale_worktrees(&state, Duration::from_secs(24 * 60 * 60)).await;
        assert!(
            evohime_storage::task_worktrees::get_task_worktree(&state.pool, recent_failed_task)
                .await
                .expect("get recent_failed_task after real retention")
                .is_some(),
            "a terminal row younger than the retention window must survive"
        );

        // Clean up the still-live rows manually so the test doesn't leak worktrees.
        for task_id in [paused_task, recent_failed_task] {
            let row = evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id)
                .await
                .expect("get row for manual cleanup")
                .expect("row present");
            remove_worktree(repo.path(), &PathBuf::from(&row.worktree_path))
                .await
                .expect("manual cleanup");
            evohime_storage::task_worktrees::delete_task_worktree(&state.pool, task_id)
                .await
                .expect("delete row for manual cleanup");
        }
    }

    #[tokio::test]
    async fn cleanup_drops_a_row_whose_primary_root_no_longer_exists() {
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping worktree cleanup test: database unavailable");
            return;
        };
        let task_id = seed_task(&pool).await;
        let state = AppState::for_worktree_tests(pool);

        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        provision_worktree(&state, task_id, repo.path())
            .await
            .expect("provision");
        sqlx::query("UPDATE tasks SET status = 'failed' WHERE id = $1")
            .bind(task_id)
            .execute(&state.pool)
            .await
            .expect("mark failed");

        // The repo itself disappears (moved/deleted) before the next
        // server restart — simulate that by dropping the tempdir.
        drop(repo);

        cleanup_stale_worktrees(&state, Duration::from_secs(0)).await;

        assert!(
            evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id)
                .await
                .expect("get after cleanup")
                .is_none(),
            "a row whose primary_workspace_root is gone must still be dropped, not stuck forever"
        );
    }

    #[tokio::test]
    async fn cleanup_retention_overflow_does_not_delete_everything() {
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping worktree cleanup test: database unavailable");
            return;
        };
        let task_id = seed_task(&pool).await;
        let state = AppState::for_worktree_tests(pool);

        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        provision_worktree(&state, task_id, repo.path())
            .await
            .expect("provision");
        sqlx::query("UPDATE tasks SET status = 'failed' WHERE id = $1")
            .bind(task_id)
            .execute(&state.pool)
            .await
            .expect("mark failed");

        // An out-of-range retention must degrade to "keep everything this
        // run", not silently collapse to a zero-length window.
        cleanup_stale_worktrees(&state, Duration::MAX).await;

        assert!(
            evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id)
                .await
                .expect("get after cleanup")
                .is_some(),
            "an extreme retention value must not delete a just-created row"
        );

        let row = evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id)
            .await
            .expect("get row for manual cleanup")
            .expect("row present");
        remove_worktree(repo.path(), &PathBuf::from(&row.worktree_path))
            .await
            .expect("manual cleanup");
        evohime_storage::task_worktrees::delete_task_worktree(&state.pool, task_id)
            .await
            .expect("delete row for manual cleanup");
    }

    #[tokio::test]
    async fn orphan_sweep_removes_a_directory_with_no_matching_row() {
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping worktree orphan-sweep test: database unavailable");
            return;
        };
        let state = AppState::for_worktree_tests(pool);

        let orphan_id = Uuid::new_v4();
        let orphan_dir = std::env::temp_dir()
            .join("evohime-worktrees")
            .join(orphan_id.to_string());
        tokio::fs::create_dir_all(&orphan_dir)
            .await
            .expect("create orphan dir");
        // The sweep requires a `.git` marker before deleting anything (only
        // confirming a directory is actually one of our worktree checkouts,
        // not an unrelated directory that happens to have a UUID-shaped
        // name) — a real worktree always has this, so the test must too.
        tokio::fs::write(orphan_dir.join(".git"), "gitdir: /fake/for/test\n")
            .await
            .expect("create .git marker");
        // No task_worktrees row for orphan_id at all — simulates a row that
        // disappeared via ON DELETE CASCADE without this app's own code
        // ever running to clean up the directory.

        cleanup_orphaned_worktree_directories(&state).await;

        assert!(
            !orphan_dir.exists(),
            "a worktree directory with no matching row must be swept"
        );
    }

    #[tokio::test]
    async fn merge_lock_serializes_two_concurrent_merges_into_the_same_primary() {
        // The unit tests above (Task 3) only ever call
        // `merge_worktree_into_primary` sequentially — none of them prove
        // `AppState::merge_lock_for` (Task 4) actually serializes two
        // merges racing for the *same* `primary_root` at the true OS-thread
        // level, only that the function is correct when called one at a
        // time. This test drives two real concurrent `finalize_worktree`
        // calls through `tokio::join!` and asserts the result is exactly
        // what serialized execution would produce: two clean commits, no
        // corrupted index, no lost file.
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping worktree merge-lock test: database unavailable");
            return;
        };
        let task_id_a = seed_task(&pool).await;
        let task_id_b = seed_task(&pool).await;
        let state = AppState::for_worktree_tests(pool);

        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());

        provision_worktree(&state, task_id_a, repo.path())
            .await
            .expect("provision A");
        provision_worktree(&state, task_id_b, repo.path())
            .await
            .expect("provision B");
        let row_a = evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id_a)
            .await
            .expect("get A")
            .expect("row A present");
        let row_b = evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id_b)
            .await
            .expect("get B")
            .expect("row B present");

        std::fs::write(PathBuf::from(&row_a.worktree_path).join("from-a.txt"), "a\n")
            .expect("write a");
        std::fs::write(PathBuf::from(&row_b.worktree_path).join("from-b.txt"), "b\n")
            .expect("write b");

        // Both finalize calls race for the same `merge_lock_for(repo.path())`
        // lock. If it didn't actually serialize them, two concurrent
        // `git add -A` / `git commit` invocations against the same
        // `primary_root` would be expected to corrupt the index or drop one
        // side's file outright on at least some fraction of runs.
        let (result_a, result_b) = tokio::join!(
            finalize_worktree(&state, task_id_a, repo.path(), &row_a),
            finalize_worktree(&state, task_id_b, repo.path(), &row_b),
        );
        result_a.expect("finalize A");
        result_b.expect("finalize B");

        assert!(repo.path().join("from-a.txt").exists(), "task A's file must survive");
        assert!(repo.path().join("from-b.txt").exists(), "task B's file must survive");

        let status_output = StdCommand::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo.path())
            .output()
            .expect("git status --porcelain");
        assert!(
            status_output.stdout.is_empty(),
            "primary_root must be fully clean after two concurrent merges"
        );

        let log_output = StdCommand::new("git")
            .args(["log", "--oneline"])
            .current_dir(repo.path())
            .output()
            .expect("git log --oneline");
        let commit_count = String::from_utf8_lossy(&log_output.stdout).lines().count();
        assert_eq!(
            commit_count, 3,
            "expected the initial commit plus one commit per task, in some order"
        );
    }
```

`evohime_storage::connect_integration_pool` is already `pub`, re-exported from `crates/storage/src/lib.rs:90` — no new helper needed, this calls it directly exactly like `crates/storage/src/plugin_audit.rs`'s own tests do.

- [ ] **Step 2: Run the tests**

Run: `cargo test -p evohime-server task::worktree:: -- --nocapture`
Expected: all tests `PASS` (or print `skipping ... database unavailable` and pass trivially if no integration Postgres is configured).

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/app.rs crates/server/src/task/worktree.rs
git commit -m "test(server): orchestration-level coverage for worktree provision/finalize/cleanup (7.107)"
```

---

### Task 9: Full workspace verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite**

Run: `cargo test --workspace --all-features --all-targets`
Expected: all tests pass (integration tests requiring Postgres either pass or self-skip per the existing convention; if `DATABASE_URL`/integration Postgres is available in this environment, confirm they actually ran rather than skipped, per `AGENTS.md` rule 13 — don't claim a check passed without actually running it).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace --all-features --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Manual smoke test via the real launcher**

Start the stack with `.\start-dev.ps1` (per `AGENTS.md`/project convention — never substitute `cargo run` + `npm run dev`).

3a. Open two chat sessions against the same workspace and send a long-running message in each within the same few seconds. Confirm in the server logs that the second task's `AgentConfig.workspace_root` is a path under the OS temp `evohime-worktrees` directory, and that after both complete, `git log` in the primary workspace shows two new commits (or one, if the tasks touched disjoint files and merged into a single commit sequence) and no leftover `.evohime`-adjacent worktree directories.

3b. Approval-pause concurrency check (Task 5, Step 3/4's fix — no automated end-to-end test covers this; simulating the full WebSocket approval round-trip is disproportionate to the size of that fix, so this manual pass is the verification). Send a message that triggers an `approval.required` pause (e.g. a `filesystem.patch` in `Ask` mode). While it's paused, start a *second*, unrelated task against the same workspace and confirm — via server logs — that it gets isolated into a worktree (i.e. `is_concurrent` was `true`), proving the first task's paused-but-not-terminal status still counted. Approve the first task and confirm it resumes and completes normally, still pointed at the primary checkout, without colliding with the second task.

3c. Cancel-while-paused check (Task 5, Step 4's fix): trigger another `approval.required` pause, then send `TaskCancel` for it instead of approving. Confirm a task started immediately afterward is *not* needlessly isolated (i.e. `is_concurrent` is `false` again) — proving the cancelled task's `task_cancellations` entry was actually released.

3d. Stale-worktree retry check (Task 5, Step 5's fix): start a task that gets isolated (start it while another is running, per 3a), and force it to fail while isolated — e.g. deny a required approval, or otherwise induce a merge conflict on its merge-back — so its `task_worktrees` row and worktree directory are left behind per `finalize_worktree`'s design. Confirm via server logs the row/directory exist (`SELECT * FROM task_worktrees WHERE task_id = ...`, or check `%TEMP%/evohime-worktrees/<task_id>` on disk). Send `TaskRetry` for it and confirm in the logs that the *old* worktree path is removed and a *new* one is provisioned (different path) before the retried run starts — not the same stale directory reused as-is.

3e. Restart-survives-pause check (Task 7, Step 3.5's fix): trigger an `approval.required` pause, then restart the server process (`.\start-dev.ps1` again) without approving or cancelling it first. Immediately after the restart completes, start a new, unrelated task against the same workspace and confirm via server logs that it gets isolated into a worktree (`is_concurrent` was `true`) — proving the paused task's `task_cancellations` entry was correctly re-seeded from its DB status rather than starting the post-restart map empty.

- [ ] **Step 4: Clean build artifacts**

Per `AGENTS.md` rule 15, remove the workspace `target/` directory once verification is complete and nothing else in this session still needs it:

Run: `cargo clean` (only if no further verification in this session depends on the build)

- [ ] **Step 5: Update roadmap status**

In `docs/roadmap.md`, change the `7.107` row's status from `⬜` to `✅` and fill in the evidence column, e.g.:

```
| 7.107 | Worktree-aware multi-checkout agent (parallel tasks isolated) | L | ✅ | `task_worktrees` table; detached-HEAD worktrees under OS temp dir, provisioned atomically alongside `task_cancellations` (idempotent, rolled back on DB failure, entries kept alive through approval pauses, `TaskCancel`, `TaskRetry`'s stale-worktree teardown, a server restart, and a panic via `TaskCancellationGuard`); path-scoped squash merge-back (`git apply --3way --index` + scoped `commit`/`checkout HEAD` restore, never a blanket reset/commit) under a per-workspace `workspace_merge_locks` registry, serialization verified under real concurrent execution; startup cleanup keyed to live task status with retention-overflow and cascade-orphan handling |
```

Before editing, run `grep -rn "7.107" AGENTS.md docs/roadmap.md docs/current-state.md docs/architecture.md docs/development-plan.md` to see every current mention verbatim rather than assuming the "остался `7.107`" phrasing is identical across all four files — update each occurrence to match its own file's actual wording, then update the "остался `7.107`" sentences in `AGENTS.md`, `docs/current-state.md`, `docs/architecture.md`, and `docs/development-plan.md` to reflect Stage 7 being fully complete.

- [ ] **Step 6: Commit**

```bash
git add docs/roadmap.md AGENTS.md docs/current-state.md docs/architecture.md docs/development-plan.md
git commit -m "docs: mark 7.107 worktree-aware multi-checkout agent complete, close Stage 7"
```
