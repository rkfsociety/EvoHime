# Worktree-Aware Multi-Checkout Agent (`7.107`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a second task starts while another task is already running against the same server, isolate it in a detached-HEAD git worktree instead of the shared `workspace_root`, and automatically fold its changes back into the primary checkout when it finishes.

**Architecture:** A new `task_worktrees` Postgres table tracks one row per isolated task (`base_commit_sha`, `worktree_path`, `primary_workspace_root`). `crates/server/src/ws.rs` decides isolation atomically alongside its existing `task_cancellations` bookkeeping and provisions the worktree via `git worktree add --detach` into the OS temp directory (never inside the tracked repo tree), idempotently and with rollback if persisting the row fails. `crates/server/src/task/pipeline.rs` points the agent's `workspace_root` at the worktree when a row exists, then on success squash-merges the worktree's diff back onto the primary checkout under a per-`primary_workspace_root` lock (`workspace_merge_locks`, not one global lock), using `git add -A` + `git diff --cached` + `git apply --3way --index` + `git commit`, resetting the primary checkout back to a clean `HEAD` on conflict. Startup cleanup keys off each row's owning task's *live* status, not a restart-scoped snapshot. All git subprocess calls follow the existing `tokio::process::Command` pattern already used in `crates/tool-runtime/src/tools/git.rs` and `crates/server/src/github_api.rs`.

**Tech Stack:** Rust (axum, sqlx/Postgres, tokio), `git` CLI via `tokio::process::Command`.

## Global Constraints

- No feature branches are ever created — worktrees are detached HEAD only. (Design §Non-goals)
- No new approval/review UI — per-write approvals already happened inside the worktree via the existing permissions engine. (Design §Non-goals)
- `tokio::sync::Mutex` only for the new lock registry (`workspace_merge_locks`) and any reuse of `task_cancellations` — never `std::sync::Mutex`, since tokio's mutex never poisons on panic. (Design §Trigger)
- Every DB schema change goes through `migrations/`, next free number is `0035`. (`AGENTS.md` rule 7; confirmed via `ls migrations/`)
- After finishing this plan: `cargo test --workspace --all-features --all-targets`, `cargo clippy --workspace --all-features --all-targets -- -D warnings`, and clean up `target/` when no longer needed for a later step. (`AGENTS.md` rules 5, 15)
- Commit after every task; never push unless the user explicitly asks. (`AGENTS.md` rule 11)

---

## File Structure

- **Create** `migrations/0035_task_worktrees.sql` — new table.
- **Create** `crates/storage/src/task_worktrees.rs` — DAO for the table (mirrors `crates/storage/src/plugin_audit.rs`).
- **Modify** `crates/storage/src/lib.rs` — register `pub mod task_worktrees;`.
- **Create** `crates/server/src/task/worktree.rs` — all git-subprocess mechanics (pure functions taking explicit paths) plus the `AppState`-aware orchestration functions (`provision_worktree`, `finalize_worktree`, `cleanup_stale_worktrees`).
- **Modify** `crates/server/src/task/mod.rs` — register the new module.
- **Modify** `crates/server/src/app.rs` — add `workspace_merge_locks` field and `merge_lock_for` helper method to `AppState`.
- **Modify** `crates/server/src/startup.rs` — construct the new field; add the startup cleanup pass after `recover_after_restart`.
- **Modify** `crates/server/src/ws.rs` — atomic trigger decision, worktree provisioning, fail-fast on concurrent provisioning failure.
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
    worktree_path text NOT NULL,
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
  - `pub(crate) const WORKTREE_OP_TIMEOUT: Duration` (30s) — quick, metadata-only git operations.
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
pub(crate) const WORKTREE_OP_TIMEOUT: Duration = Duration::from_secs(30);

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
    run_git(repo, &["rev-parse", "HEAD"], WORKTREE_OP_TIMEOUT)
        .await
        .map_err(|error| WorktreeError::NotAGitRepo(error.to_string()))
}

pub(crate) async fn add_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_sha: &str,
) -> Result<(), WorktreeError> {
    if repo == worktree_path || worktree_path.starts_with(repo) {
        // Defense in depth: the OS-temp-dir choice for worktree_path already
        // avoids this structurally, but a misconfigured TMPDIR/TEMP could in
        // principle point inside the repo — fail clearly instead of letting
        // `git worktree add` create a nested checkout under `repo`.
        return Err(WorktreeError::Io(format!(
            "refusing to create worktree {} nested inside primary root {}",
            worktree_path.display(),
            repo.display()
        )));
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
        WORKTREE_OP_TIMEOUT,
    )
    .await?;
    Ok(())
}

pub(crate) async fn remove_worktree(repo: &Path, worktree_path: &Path) -> Result<(), WorktreeError> {
    if worktree_path.exists() {
        let worktree_path_str = worktree_path.to_string_lossy().into_owned();
        run_git(
            repo,
            &["worktree", "remove", "--force", &worktree_path_str],
            WORKTREE_OP_TIMEOUT,
        )
        .await?;
    }
    run_git(repo, &["worktree", "prune"], WORKTREE_OP_TIMEOUT).await?;
    Ok(())
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
}
```

- [ ] **Step 2: Run the tests to verify they fail to compile first, then pass**

Run: `cargo test -p evohime-server task::worktree:: -- --nocapture`
Expected: compiles and all five tests `PASS` (git must be on `PATH`, as it already is elsewhere in this workspace's tests, e.g. `crates/tool-runtime/src/tools/git.rs`).

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
  - `pub(crate) const MERGE_TIMEOUT: Duration` (5 minutes — diff/apply/commit can be slower than metadata ops on a large patch).
  - `pub(crate) async fn merge_worktree_into_primary(worktree_path: &Path, primary_root: &Path, base_sha: &str, task_id: Uuid) -> Result<(), WorktreeError>`

**Every operation below is scoped to exactly the paths this merge's own patch touches — never to `workspace_root`'s whole index/tree.** Whenever the currently-*first* task is running unisolated (the common case: `is_concurrent == false` at its own start), its tool calls write directly into `primary_root`, uncommitted, for the entire time any *other* task is running isolated. A blanket `git commit` (with no pathspec) could sweep that first task's unrelated staged changes into this merge's commit; a blanket `git reset --hard` on conflict could destroy its in-progress uncommitted work outright. Scoping every git invocation to this merge's own path list is what makes it safe to run next to a live unisolated task.

- [ ] **Step 1: Write the failing tests**

Append to `crates/server/src/task/worktree.rs` (add `use uuid::Uuid;` and `use tracing::{info, warn};` to the top-level `use` block, and add these functions above the `#[cfg(test)]` module):

```rust
/// Diff/apply/commit can be slower than metadata-only git operations on a
/// large patch.
pub(crate) const MERGE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

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
    let output = run_git(
        worktree_path,
        &["diff", "--cached", "--name-status", base_sha],
        MERGE_TIMEOUT,
    )
    .await?;
    let mut existing = Vec::new();
    let mut added = Vec::new();
    for line in output.lines() {
        let mut fields = line.split('\t');
        let status = fields.next().unwrap_or_default();
        match status.chars().next() {
            Some('A') => added.extend(fields.next().map(str::to_string)),
            Some('R') => existing.extend(fields.map(str::to_string)), // old + new name
            _ => existing.extend(fields.next().map(str::to_string)),
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
    let output = tokio::time::timeout(MERGE_TIMEOUT, run)
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
    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = child.stdin.take().expect("stdin piped");
        stdin
            .write_all(patch)
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to write patch to git apply: {error}")))?;
    }
    let output = tokio::time::timeout(MERGE_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| WorktreeError::Io("git apply timed out".to_string()))?
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
async fn restore_paths(repo: &Path, changed: &ChangedPaths) {
    if !changed.existing.is_empty() {
        let mut args: Vec<&str> = vec!["checkout", "HEAD", "--"];
        args.extend(changed.existing.iter().map(String::as_str));
        if let Err(error) = run_git(repo, &args, MERGE_TIMEOUT).await {
            warn!(%error, "failed to restore existing paths to HEAD after a failed merge-back");
        }
    }
    if !changed.added.is_empty() {
        let mut args: Vec<&str> = vec!["reset", "--"];
        args.extend(changed.added.iter().map(String::as_str));
        if let Err(error) = run_git(repo, &args, WORKTREE_OP_TIMEOUT).await {
            warn!(%error, "failed to unstage added paths after a failed merge-back");
        }
        for path in &changed.added {
            let _ = tokio::fs::remove_file(repo.join(path)).await;
        }
    }
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
    run_git(worktree_path, &["add", "-A"], MERGE_TIMEOUT).await?;

    let changed = changed_paths(worktree_path, base_sha).await?;
    let all_paths = changed.all();
    if all_paths.is_empty() {
        // Nothing changed relative to base — nothing to merge or commit.
        return Ok(());
    }

    let patch = diff_cached_patch(worktree_path, base_sha).await?;

    if let Err(apply_error) = apply_patch(primary_root, &patch).await {
        restore_paths(primary_root, &changed).await;
        return Err(WorktreeError::Conflict(apply_error.to_string()));
    }

    // Only commit if THIS merge's own paths actually have staged changes —
    // a 3-way merge can legitimately produce no diff if primary already
    // matched. Scoped to all_paths, not the whole index: a concurrently
    // running unisolated task could have unrelated staged changes too.
    let mut quiet_args: Vec<&str> = vec!["diff", "--cached", "--quiet", "--"];
    quiet_args.extend(all_paths.iter().copied());
    let staged = Command::new("git")
        .arg("-C")
        .arg(primary_root)
        .args(&quiet_args)
        .status()
        .await
        .map_err(|error| WorktreeError::Io(format!("failed to run git diff --cached --quiet: {error}")))?;
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
    if let Err(commit_error) = run_git(primary_root, &commit_args, MERGE_TIMEOUT).await {
        restore_paths(primary_root, &changed).await;
        return Err(WorktreeError::Io(format!(
            "commit failed after a clean apply: {commit_error}"
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
        // uncommitted edits directly in the primary checkout.
        std::fs::write(repo.path().join("live-task.txt"), "wip\n").expect("write");
        run(repo.path(), &["git", "add", "live-task.txt"]);

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        assert!(repo.path().join("from-worktree.txt").exists());
        // The concurrently "live" task's staged file must survive, untouched
        // and still staged — not committed into this merge, not reset away.
        assert!(repo.path().join("live-task.txt").exists());
        let status = StdCommand::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(repo.path())
            .output()
            .expect("git diff --cached --name-only");
        let staged = String::from_utf8_lossy(&status.stdout);
        assert!(
            staged.contains("live-task.txt"),
            "unrelated staged file must remain staged, not swept into this merge's commit"
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
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p evohime-server task::worktree:: -- --nocapture`
Expected: all eight tests `PASS`.

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
    // Idempotent: nothing in this design calls provision_worktree twice for
    // the same task today, but if a future retry path ever did, calling
    // `git worktree add` again would fail outright since the directory
    // already exists. Treat an existing row as already-done instead.
    if evohime_storage::task_worktrees::get_task_worktree(&state.pool, task_id)
        .await
        .map_err(|error| WorktreeError::Io(format!("failed to check for existing task_worktrees row: {error}")))?
        .is_some()
    {
        return Ok(());
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
        // permanent orphan otherwise.
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

`is_concurrent` (Step 2) is only correct if `task_cancellations` actually reflects every task that still has unfinished business in a workspace — including one that's `Paused` waiting on `approval.required`, not just one currently executing. Today it doesn't: `crates/server/src/ws.rs` has four places that spawn a task run and unconditionally remove its `task_cancellations` entry once the run's `.await` resolves — the `UserMessage` handler this task is already editing (originally ~ws.rs:193-197, now shifted by Step 2's edit), and three resume paths (plan-approval-granted, tool-approval-granted, manual task-resume) that all follow the identical insert-before-spawn / remove-after-await shape. `process_user_message`/`resume_task_run` return `Ok(())` both when a task truly completes *and* when it merely pauses for approval (`crates/server/src/task/pipeline.rs`'s `NeedsApproval` branch returns early). All four sites currently treat both cases identically and remove the entry either way — so a paused task's directory stops being protected by `is_concurrent` the moment it pauses, even though it's still going to resume into that same directory later. A second task starting in that window sees an empty map, runs unisolated, and can end up running *at the same time* as the first task once it's approved and resumes — the exact race this whole feature exists to prevent, on a mainline (not edge-case) flow.

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
```

Now replace all four `.task_cancellations.lock().await.remove(&task_id);` calls in `crates/server/src/ws.rs` (the one inside this task's own spawned block, plus the three resume-path spawned blocks at the plan-approval-granted, tool-approval-granted, and manual-resume handlers — find each with `grep -n "task_cancellations" crates/server/src/ws.rs` and match the `.remove(&task_id)` pattern specifically, not the `.insert(...)` calls) with:

```rust
                                crate::task::helpers::release_task_cancellation_if_terminal(
                                    &state_for_task,
                                    task_id,
                                )
                                .await;
```

- [ ] **Step 4: Fix `TaskCancel` for a currently-paused task**

`ClientCommand::TaskCancel` (`ws.rs:200-218`) transitions the task to `Cancelled` by calling `evohime_task_engine::cancel_task` directly. For a task that's actively running, cancellation triggers the token, the spawned future's `await` resolves, and Step 3's new helper removes the entry correctly. But for a task that's currently *paused* (no active spawned future — its `.await` already resolved when it first paused), nothing will ever re-run Step 3's check, so cancelling a paused task would transition it to `Cancelled` in the database while leaving its `task_cancellations` entry stuck forever — needlessly isolating every future task indefinitely. Add a direct removal right after the cancellation:

```rust
                        ClientCommand::TaskCancel { task_id } => {
                            let cancellation =
                                state.task_cancellations.lock().await.get(&task_id).cloned();
                            if let Some(token) = cancellation {
                                token.cancel();
                            }
                            let _ = evohime_task_engine::cancel_task(&state.pool, task_id).await;
                            state.task_cancellations.lock().await.remove(&task_id);
                            let _ = finalize_open_task_steps(&state, task_id, "cancelled").await;
```

This replaces the existing first five lines of the `TaskCancel` arm (find it via `grep -n "ClientCommand::TaskCancel" crates/server/src/ws.rs`) — everything from `emit_event(...)` onward in that arm is unchanged. Removing here is safe/idempotent even for an actively-running task too (Step 3's helper would find the row already gone and just no-op).

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p evohime-server`
Expected: no errors. `fail_task` is already imported via `use evohime_task_engine::{fail_task, resume_task, retry_task, start_task};` at the top of `ws.rs`, so the unqualified call in Step 2 resolves correctly. `evohime_storage::load_task` is already used elsewhere in this file (`TaskPlanReject`), so no new import is needed for it in `helpers.rs` beyond what's already there (`evohime_storage` is used unqualified via its crate name throughout this module already).

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/task/worktree.rs crates/server/src/task/helpers.rs crates/server/src/ws.rs
git commit -m "feat(server): trigger worktree isolation atomically; keep task_cancellations alive through an approval pause (7.107)"
```

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

Cleanup decides per row from the owning task's **current** status (`task_status`, joined in `list_task_worktrees_with_status`), not from a restart-scoped snapshot. This matters concretely: a task `Paused` on an `approval.required` round-trip never appears in `recover_after_restart`'s return value on a later, unrelated restart (it wasn't crashed — pipeline.rs's `NeedsApproval` branch returns `Ok(())` normally), yet its worktree is still in active use and must never be swept. Querying live status instead of that snapshot handles this correctly and makes the function's signature simpler (no `resumable_task_ids` parameter to keep in sync with anything).

- [ ] **Step 1: Add `cleanup_stale_worktrees` to `worktree.rs`**

Add after `finalize_worktree` (add `use std::time::Duration;` and `use tracing::warn;` to the imports):

```rust
const TERMINAL_TASK_STATUSES: &[&str] = &["completed", "failed", "cancelled"];

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
        if !has_row {
            if let Err(error) = tokio::fs::remove_dir_all(entry.path()).await {
                warn!(%task_id, %error, "failed to remove orphaned worktree directory");
            }
        }
    }
}
```

- [ ] **Step 2: Call both from `startup.rs`**

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

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p evohime-server`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/worktree.rs crates/server/src/startup.rs
git commit -m "feat(server): clean up stale worktrees on server startup (7.107)"
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
            auth: crate::auth::AuthConfig::from_env(),
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
            rate_limiter: Arc::new(crate::rate_limit::RateLimiter::from_env()),
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
        // No task_worktrees row for orphan_id at all — simulates a row that
        // disappeared via ON DELETE CASCADE without this app's own code
        // ever running to clean up the directory.

        cleanup_orphaned_worktree_directories(&state).await;

        assert!(
            !orphan_dir.exists(),
            "a worktree directory with no matching row must be swept"
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

- [ ] **Step 4: Clean build artifacts**

Per `AGENTS.md` rule 15, remove the workspace `target/` directory once verification is complete and nothing else in this session still needs it:

Run: `cargo clean` (only if no further verification in this session depends on the build)

- [ ] **Step 5: Update roadmap status**

In `docs/roadmap.md`, change the `7.107` row's status from `⬜` to `✅` and fill in the evidence column, e.g.:

```
| 7.107 | Worktree-aware multi-checkout agent (parallel tasks isolated) | L | ✅ | `task_worktrees` table; detached-HEAD worktrees under OS temp dir, provisioned atomically alongside `task_cancellations` (idempotent, rolled back on DB failure, entries kept alive through approval pauses); path-scoped squash merge-back (`git apply --3way --index` + scoped `commit`/`checkout HEAD` restore, never a blanket reset/commit) under a per-workspace `workspace_merge_locks` registry; startup cleanup keyed to live task status with retention-overflow and cascade-orphan handling |
```

Also update the "остался `7.107`" sentences in `AGENTS.md`, `docs/current-state.md`, `docs/architecture.md`, and `docs/development-plan.md` to reflect Stage 7 being fully complete.

- [ ] **Step 6: Commit**

```bash
git add docs/roadmap.md AGENTS.md docs/current-state.md docs/architecture.md docs/development-plan.md
git commit -m "docs: mark 7.107 worktree-aware multi-checkout agent complete, close Stage 7"
```
