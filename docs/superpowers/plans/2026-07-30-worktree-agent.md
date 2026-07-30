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
  - `pub(crate) async fn rev_parse_head(repo: &Path) -> Result<String, WorktreeError>`
  - `pub(crate) async fn add_worktree(repo: &Path, worktree_path: &Path, base_sha: &str) -> Result<(), WorktreeError>`
  - `pub(crate) async fn remove_worktree(repo: &Path, worktree_path: &Path) -> Result<(), WorktreeError>` (tolerates a missing `worktree_path`)

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

async fn run_git(repo: &Path, args: &[&str]) -> Result<String, WorktreeError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| WorktreeError::Io(format!("failed to run git: {error}")))?;

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
    run_git(repo, &["rev-parse", "HEAD"])
        .await
        .map_err(|error| WorktreeError::NotAGitRepo(error.to_string()))
}

pub(crate) async fn add_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_sha: &str,
) -> Result<(), WorktreeError> {
    if let Some(parent) = worktree_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to create {}: {error}", parent.display())))?;
    }
    let worktree_path_str = worktree_path.to_string_lossy().into_owned();
    run_git(
        repo,
        &["worktree", "add", "--detach", &worktree_path_str, base_sha],
    )
    .await?;
    Ok(())
}

pub(crate) async fn remove_worktree(repo: &Path, worktree_path: &Path) -> Result<(), WorktreeError> {
    if worktree_path.exists() {
        let worktree_path_str = worktree_path.to_string_lossy().into_owned();
        run_git(repo, &["worktree", "remove", "--force", &worktree_path_str]).await?;
    }
    run_git(repo, &["worktree", "prune"]).await?;
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
}
```

- [ ] **Step 2: Run the tests to verify they fail to compile first, then pass**

Run: `cargo test -p evohime-server task::worktree:: -- --nocapture`
Expected: compiles and all four tests `PASS` (git must be on `PATH`, as it already is elsewhere in this workspace's tests, e.g. `crates/tool-runtime/src/tools/git.rs`).

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
- Consumes: `WorktreeError` from Task 2.
- Produces:
  - `pub(crate) async fn reset_hard_head(repo: &Path) -> Result<(), WorktreeError>`
  - `pub(crate) async fn merge_worktree_into_primary(worktree_path: &Path, primary_root: &Path, base_sha: &str, task_id: Uuid) -> Result<(), WorktreeError>`

- [ ] **Step 1: Write the failing tests**

Append to `crates/server/src/task/worktree.rs` (add `use uuid::Uuid;` and `use tracing::info;` to the top-level `use` block, and add these functions above the `#[cfg(test)]` module):

```rust
/// Discards any uncommitted state in `repo` — used only to recover from a
/// failed `git apply --3way --index`, which can leave the working
/// tree/index with conflict markers and unmerged entries. Nothing from a
/// failed apply was ever committed, so resetting to `HEAD` only ever
/// discards that failed attempt, never a real commit.
pub(crate) async fn reset_hard_head(repo: &Path) -> Result<(), WorktreeError> {
    run_git(repo, &["reset", "--hard", "HEAD"]).await?;
    Ok(())
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
    run_git(worktree_path, &["add", "-A"]).await?;

    let diff_output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(["diff", "--cached", base_sha])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| WorktreeError::Io(format!("failed to run git diff: {error}")))?;
    if !diff_output.status.success() {
        let stderr = String::from_utf8_lossy(&diff_output.stderr).trim().to_string();
        return Err(WorktreeError::Io(format!("git diff --cached failed: {stderr}")));
    }
    let patch = diff_output.stdout;
    if patch.is_empty() {
        // Nothing changed relative to base — nothing to merge or commit.
        return Ok(());
    }

    let mut apply = Command::new("git")
        .arg("-C")
        .arg(primary_root)
        .args(["apply", "--3way", "--index"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| WorktreeError::Io(format!("failed to spawn git apply: {error}")))?;
    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = apply.stdin.take().expect("stdin piped");
        stdin
            .write_all(&patch)
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to write patch to git apply: {error}")))?;
    }
    let apply_output = apply
        .wait_with_output()
        .await
        .map_err(|error| WorktreeError::Io(format!("failed to wait on git apply: {error}")))?;
    if !apply_output.status.success() {
        let stderr = String::from_utf8_lossy(&apply_output.stderr).trim().to_string();
        // A failed 3-way apply can leave primary_root's working tree/index
        // with conflict markers and unmerged entries. Nothing from this
        // attempt was ever committed, so it's always safe to discard it —
        // and necessary, or the *next* task to touch primary_root (isolated
        // or not) would inherit a half-merged git state it had nothing to
        // do with.
        if let Err(reset_error) = reset_hard_head(primary_root).await {
            return Err(WorktreeError::Conflict(format!(
                "git apply --3way --index failed: {stderr}; additionally failed to reset primary_root afterward: {reset_error}"
            )));
        }
        return Err(WorktreeError::Conflict(format!(
            "git apply --3way --index failed: {stderr}"
        )));
    }

    // Only commit if the apply actually staged something (an empty 3-way
    // merge result is possible if the primary side already had the change).
    let staged = Command::new("git")
        .arg("-C")
        .arg(primary_root)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .await
        .map_err(|error| WorktreeError::Io(format!("failed to run git diff --cached --quiet: {error}")))?;
    if staged.success() {
        // Exit code 0 means no staged differences.
        return Ok(());
    }

    run_git(
        primary_root,
        &["commit", "-m", &format!("agent: task {task_id} (worktree merge)")],
    )
    .await?;
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
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p evohime-server task::worktree:: -- --nocapture`
Expected: all six tests `PASS`.

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
- Consumes: `WorktreeError`, `rev_parse_head`, `add_worktree` (Task 2); `evohime_storage::task_worktrees::{insert_task_worktree, NewTaskWorktree}` (Task 1); `AppState` (Task 4).
- Produces: `pub(crate) async fn provision_worktree(state: &Arc<AppState>, task_id: Uuid, primary_root: &Path) -> Result<(), WorktreeError>`

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

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p evohime-server`
Expected: no errors. `fail_task` is already imported via `use evohime_task_engine::{fail_task, resume_task, retry_task, start_task};` at the top of `ws.rs`, so the unqualified call above resolves correctly.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/task/worktree.rs crates/server/src/ws.rs
git commit -m "feat(server): trigger worktree isolation atomically on concurrent task start (7.107)"
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
    // On Err (conflict), merge_worktree_into_primary has already reset
    // primary_root back to a clean HEAD internally — the row and worktree
    // are deliberately left in place here for manual recovery (design
    // doc, Merge-back step 8), so this function's caller (pipeline.rs) is
    // free to just propagate the error as a task failure.
    merge_worktree_into_primary(&worktree_path, primary_root, &row.base_commit_sha, task_id).await?;

    // git worktree remove requires --force here: the worktree is still
    // "dirty" relative to its own base_commit_sha (nothing was reset inside
    // it), even though its diff already landed on primary_root.
    remove_worktree(primary_root, &worktree_path).await?;

    evohime_storage::task_worktrees::delete_task_worktree(&state.pool, task_id)
        .await
        .map_err(|error| WorktreeError::Io(format!("failed to delete task_worktrees row: {error}")))?;

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

This placement matters: it runs only on the success path (the `match agent_result` block already returned early with `Err(...)` for approval-pause, agent failure, or join errors), and it runs *before* `process_user_message` returns — so `crates/server/src/ws.rs`'s existing `task_cancellations.lock().await.remove(&task_id)` (which only runs after `process_user_message(...).await` resolves) naturally waits for merge-back to finish. A merge-back failure propagates as `Err((task.id, ApiError::Internal(...)))`, which the `ws.rs` spawn wrapper already turns into `fail_task` + a `TaskFailed` event — no new error-handling path needed there.

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
- Consumes: `evohime_storage::task_worktrees::{list_task_worktrees_with_status, delete_task_worktree}` (Task 1); `remove_worktree` (Task 2).
- Produces: `pub(crate) async fn cleanup_stale_worktrees(state: &Arc<AppState>, retention: Duration)`

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

    let cutoff = chrono::Utc::now() - chrono::Duration::from_std(retention).unwrap_or_default();

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
```

- [ ] **Step 2: Call it from `startup.rs`**

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
```

This reuses the `duration_secs_env_local` helper already defined at the top of `startup.rs`, matching the pattern used for `WORKER_HEALTH_INTERVAL_SECS` etc. — default retention is 24 hours. It deliberately runs independently of the `recovered`/`resume_policy` logic right below it — cleanup consults live task status itself, not that restart-scoped list.

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

`ModelGatewayConfig` (`crates/model-gateway/src/config.rs`) has no `Default` impl; its `from_env()` returns `Result<Self, ProviderError>` but never actually errors when no `MODEL_ROUTES_JSON`/`MODEL_PROVIDER` env vars are set — it falls back to `ProviderKind::LiteRouter` with empty-string config. Use `.expect(...)` on it rather than `unwrap_or_default()`.

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
            model_config: Arc::new(RwLock::new(
                ModelGatewayConfig::from_env().expect("model gateway config from env"),
            )),
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

Start the stack with `.\start-dev.ps1` (per `AGENTS.md`/project convention — never substitute `cargo run` + `npm run dev`). Open two chat sessions against the same workspace and send a long-running message in each within the same few seconds. Confirm in the server logs that the second task's `AgentConfig.workspace_root` is a path under the OS temp `evohime-worktrees` directory, and that after both complete, `git log` in the primary workspace shows two new commits (or one, if the tasks touched disjoint files and merged into a single commit sequence) and no leftover `.evohime`-adjacent worktree directories.

- [ ] **Step 4: Clean build artifacts**

Per `AGENTS.md` rule 15, remove the workspace `target/` directory once verification is complete and nothing else in this session still needs it:

Run: `cargo clean` (only if no further verification in this session depends on the build)

- [ ] **Step 5: Update roadmap status**

In `docs/roadmap.md`, change the `7.107` row's status from `⬜` to `✅` and fill in the evidence column, e.g.:

```
| 7.107 | Worktree-aware multi-checkout agent (parallel tasks isolated) | L | ✅ | `task_worktrees` table; detached-HEAD worktrees under OS temp dir, provisioned atomically alongside `task_cancellations` (idempotent, rolled back on DB failure); squash merge-back via `git apply --3way --index` under a per-workspace `workspace_merge_locks` registry, with primary-checkout reset on conflict; startup cleanup keyed to live task status, retained for terminal tasks past `EVOHIME_WORKTREE_RETENTION_SECS` |
```

Also update the "остался `7.107`" sentences in `AGENTS.md`, `docs/current-state.md`, `docs/architecture.md`, and `docs/development-plan.md` to reflect Stage 7 being fully complete.

- [ ] **Step 6: Commit**

```bash
git add docs/roadmap.md AGENTS.md docs/current-state.md docs/architecture.md docs/development-plan.md
git commit -m "docs: mark 7.107 worktree-aware multi-checkout agent complete, close Stage 7"
```
