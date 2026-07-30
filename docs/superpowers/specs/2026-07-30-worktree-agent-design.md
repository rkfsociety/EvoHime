# Worktree-Aware Multi-Checkout Agent (`7.107`) Design

**Date:** 2026-07-30
**Status:** Draft
**Roadmap item:** `7.107` — Worktree-aware multi-checkout agent (parallel tasks isolated)

## Problem

`AppState.workspace_root` is a single physical directory shared by every
task. `AgentLoopConfig.workspace_root` is always a clone of that same path
(`crates/server/src/ws.rs` spawns each task's agent loop with
`state.workspace_root.clone()`). When two tasks are active at the same time
(the server already spawns each task as an independent `tokio::spawn` and
tracks them in `AppState.task_cancellations: Arc<Mutex<HashMap<Uuid,
CancellationToken>>>`), their `filesystem.*`, `shell.execute`, and `git.*`
tool calls all operate on the same on-disk checkout: concurrent writes can
clobber each other, and concurrent `git` invocations can race on
`.git/index.lock`.

## Goal

When a task starts while another task is already running against the same
workspace, give it an isolated `git worktree` checkout instead of the
shared `workspace_root`, so its filesystem/shell/git tool calls cannot
interfere with the other task. When the task finishes successfully, fold
its changes back into the primary checkout automatically.

## Non-goals

- No feature branches are ever created. This project's standing rule is
  that branches are only created on the user's explicit request; isolation
  must not introduce any.
- No new approval/review UI. Per-write approvals already happen through the
  existing permissions engine while the task runs inside the worktree;
  merge-back does not re-request approval for changes already approved.
- No isolation for the common case (only one task running against a given
  workspace at a time) — no worktree is created, behavior is unchanged from
  today.
- No support for workspaces that are not git repositories beyond graceful
  fallback (see Error handling).

## Architecture

### Trigger

Isolation is decided once, at task start, using the existing
`task_cancellations` map as the source of truth for "is another task
already running against this workspace". `task_cancellations` is
`Arc<tokio::sync::Mutex<HashMap<Uuid, CancellationToken>>>` — the same
`tokio::sync::Mutex` type used for every other `AppState` map, which never
poisons on panic (its `lock()` is infallible, unlike `std::sync::Mutex`).
`workspace_merge_lock` (below) must use the same type for the same reason.

The existing code at `crates/server/src/ws.rs:126` reads
`task_cancellations.lock().await.len()` for rate limiting and drops the
guard immediately; a separate lock/insert happens later at
`ws.rs:169-173`. That gap is a real TOCTOU race — two tasks starting within
the same window can both observe an empty map and both run unisolated
against the shared checkout. The isolation decision must not repeat this
mistake: the "is the map non-empty" check and the `insert` of the new
task's `CancellationToken` happen under a **single** lock acquisition:

```rust
let is_concurrent = {
    let mut guard = state.task_cancellations.lock().await;
    let is_concurrent = !guard.is_empty();
    guard.insert(task_id, token.clone());
    is_concurrent
}; // guard dropped here, decision already made atomically
```

- `is_concurrent == false`: proceed exactly as today,
  `AgentLoopConfig.workspace_root = state.workspace_root.clone()`.
- `is_concurrent == true`: allocate a worktree (below) and set
  `AgentLoopConfig.workspace_root` to the worktree path instead.

This means the *first* task to start after an idle period always uses the
primary checkout directly; only the second and later concurrently-running
tasks get worktrees. This keeps the zero-concurrency path (the overwhelming
common case) exactly as fast and simple as it is today, and closes the race
the existing rate-limit check already had a milder version of.

### Worktree lifecycle

- Location: `<workspace_root>/.evohime/worktrees/<task_id>/`, consistent
  with the existing `.evohime/` convention (plugins, plugins.lock.json).
- Creation: first `git rev-parse HEAD` against `workspace_root` to capture
  an explicit `base_commit_sha`, then `git worktree add --detach <path>
  <base_commit_sha>` — pinning the worktree to that exact commit rather
  than the moving ref `HEAD`. This matters because `workspace_root`'s
  `HEAD` can itself move between the `rev-parse` and the `worktree add`
  call (another task's merge-back landing a commit); pinning to the SHA
  makes the worktree's base unambiguous regardless. Detached HEAD — no
  branch is created or checked out, satisfying the no-new-branches rule.
- Persisted state: a new `task_worktrees` table (migration), columns
  `task_id` (PK, FK to `tasks`), `base_commit_sha`, `worktree_path`,
  `created_at`. Following this project's convention of persisting
  task-related state in PostgreSQL rather than side-channel files (same
  pattern as checkpoints, `sync_runs`, `plugin_audit`), this is what both
  merge-back and the startup cleanup step (below) read from — not the
  worktree directory's git metadata, and not a file inside the worktree
  itself (which `git add -A` would otherwise pick up as part of the diff).
  The row is deleted when the worktree is removed.
- The agent runtime, tool registry, and all filesystem/shell/git tool calls
  for that task operate against this path exactly as they would against
  `workspace_root` — no other code path changes.
- A new `AppState` field, `Arc<tokio::sync::Mutex<()>>`
  (`workspace_merge_lock`), is added to serialize the merge-back step
  (below) across concurrently finishing tasks.

### Merge-back

On successful task completion, before the task transitions to
`Completed`:

1. Acquire `workspace_merge_lock` (`tokio::sync::Mutex<()>`, same rationale
   as `task_cancellations` above — must not poison).
2. Inside the worktree, run `git add -A` first. `git diff <base_commit>`
   alone never shows untracked files regardless of the commit it's
   compared against — a file the agent created but never staged would
   silently be dropped from the merge otherwise. Staging everything first
   makes the following diff/apply steps see the complete set of changes.
3. Compute the diff against `base_commit_sha` read back from
   `task_worktrees` (`git diff --cached <base_commit_sha>`).
4. Apply that diff to `workspace_root` with `git apply --3way --index`, so
   that changes landed on `workspace_root` by *other* tasks in the meantime
   (each merged the same way) are tolerated as long as they don't textually
   conflict. Plain `git apply` only ever touches the working tree; `--3way`
   alone does not reliably update `workspace_root`'s index for cleanly
   merged hunks — `--index` is required to actually stage them so the
   following commit captures the full result.
5. On a clean apply: run `git commit` on `workspace_root` to land the
   result as an actual commit, not a dirty working tree sitting on top of
   an otherwise-clean `HEAD`. The task's own `git.commit` call(s) made
   *inside* the worktree (per the "commit continuously" rule) already
   captured proper messages there; replaying each of those message
   verbatim on `workspace_root` is unnecessary complexity, so this is a
   single squash commit, message `"agent: task <task_id> (worktree
   merge)"`. This keeps `workspace_root` in the same clean-HEAD state the
   "commit continuously" rule expects, whether or not this task actually
   needed isolation.
6. Remove the worktree (`git worktree remove --force` + `git worktree
   prune`) and release the lock.
7. On failure (patch does not apply cleanly, or the commit step fails):
   leave the worktree in place, fail the task with a message that includes
   the worktree path, and release the lock. No automatic conflict
   resolution — this is a rare edge case (two concurrent tasks touching the
   same lines) and is surfaced for manual inspection rather than guessed
   at.

Cancellation follows the same shape as failure: the worktree is left in
place (not silently deleted) so in-progress work isn't lost, and the task
transitions to `Cancelled` as today.

### Cleanup on server restart

`.evohime/worktrees/<task_id>/` directories can be left on disk if the
server crashes mid-task. At startup, after `recover_after_restart`
(`crates/task-engine/src/lib.rs:136`) determines which tasks are
resumable, the server reads all rows from `task_worktrees` (the source of
truth, not a directory scan):

- a row whose `task_id` is among the resumable tasks is kept — the task
  will continue using `worktree_path`;
- every other row (task already terminal, or not going to be auto-resumed
  per `RestartResumePolicy`) has its directory removed, its `git worktree
  prune` run against `workspace_root`, and the row deleted.

This runs unconditionally at startup (cheap query when the table is empty)
so orphaned worktrees never accumulate indefinitely.

### Error handling / fallback

- If `workspace_root` is not a git repository (`git worktree add` fails
  immediately), isolation is skipped for that task: log a warning once and
  fall back to the shared `workspace_root`, matching the existing
  behavior. The server must not crash or fail the task because isolation
  wasn't available.
- If worktree creation fails for any other reason (disk full, permissions),
  same fallback: log and use the shared root.

## Data flow summary

```
task start (single task_cancellations lock guard covers both steps)
  -> is_concurrent = !task_cancellations.is_empty(); insert(task_id, token)
       false -> workspace_root (unchanged path)
       true  -> base_commit_sha = git rev-parse HEAD
                git worktree add --detach .evohime/worktrees/<task_id> <base_commit_sha>
                insert task_worktrees(task_id, base_commit_sha, worktree_path)
                -> AgentLoopConfig.workspace_root = worktree path
task runs (filesystem/shell/git tools operate on worktree path)
task completes (success)
  -> lock workspace_merge_lock
  -> git add -A (in worktree, catches untracked files)
  -> git diff --cached <base_commit_sha> (in worktree; sha from task_worktrees)
  -> git apply --3way --index (against workspace_root, stages clean hunks)
  -> ok: git commit (workspace_root), remove worktree, delete task_worktrees row, unlock
  -> conflict: fail task, keep worktree + row, unlock
```

## Testing

- Unit tests for the worktree helper (create/remove/base-commit tracking)
  against a temp git repo, mirroring the existing `tempfile`-based patterns
  in `crates/tool-runtime/src/tools/git.rs`.
- A server-level test that starts two tasks concurrently against a shared
  temp git repo, has each write to a different file, and asserts both
  changes land on the primary checkout with no worktree directories left
  behind.
- A conflict-path test: two concurrent tasks edit the same file/line; the
  second to merge fails with its worktree preserved, and its files are
  still present in `.evohime/worktrees/<task_id>/` afterward.
- A fallback test: `workspace_root` is a plain (non-git) temp directory;
  task execution proceeds without isolation and without error.
- A merge-back test asserting `workspace_root`'s `HEAD` actually advances
  (a real commit exists) after a successful merge, including a case where
  the worktree only ever created an untracked file and never called
  `git.commit` itself.
- A startup-cleanup test: a stale `task_worktrees` row + directory whose
  task is terminal is removed on server start (row and directory both
  gone); one whose task is resumable is kept.
