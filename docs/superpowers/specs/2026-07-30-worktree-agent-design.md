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
already running against this workspace":

- Before registering the new task's `CancellationToken`, check whether the
  map is non-empty.
- If empty: proceed exactly as today, `AgentLoopConfig.workspace_root =
  state.workspace_root.clone()`.
- If non-empty: allocate a worktree (below) and set
  `AgentLoopConfig.workspace_root` to the worktree path instead.

This means the *first* task to start after an idle period always uses the
primary checkout directly; only the second and later concurrently-running
tasks get worktrees. This keeps the zero-concurrency path (the overwhelming
common case) exactly as fast and simple as it is today.

### Worktree lifecycle

- Location: `<workspace_root>/.evohime/worktrees/<task_id>/`, consistent
  with the existing `.evohime/` convention (plugins, plugins.lock.json).
- Creation: `git worktree add --detach <path> HEAD` run against
  `workspace_root`. Detached HEAD — no branch is created or checked out,
  satisfying the no-new-branches rule.
- The agent runtime, tool registry, and all filesystem/shell/git tool calls
  for that task operate against this path exactly as they would against
  `workspace_root` — no other code path changes.
- A new `AppState` field, `Arc<Mutex<()>>` (`workspace_merge_lock`), is
  added to serialize the merge-back step (below) across concurrently
  finishing tasks.

### Merge-back

On successful task completion, before the task transitions to
`Completed`:

1. Acquire `workspace_merge_lock`.
2. Compute the diff of the worktree against the commit it was created from
   (`git diff <base_commit>` inside the worktree; the base commit is
   recorded when the worktree is created).
3. Apply that diff to `workspace_root` with a 3-way merge
   (`git apply --3way`), so that changes landed on `workspace_root` by
   *other* tasks in the meantime (each merged the same way) are tolerated
   as long as they don't textually conflict.
4. On success: remove the worktree (`git worktree remove --force` +
   `git worktree prune`) and release the lock.
5. On failure (patch does not apply cleanly): leave the worktree in place,
   fail the task with a message that includes the worktree path, and
   release the lock. No automatic conflict resolution — this is a rare
   edge case (two concurrent tasks touching the same lines) and is surfaced
   for manual inspection rather than guessed at.

Cancellation follows the same shape as failure: the worktree is left in
place (not silently deleted) so in-progress work isn't lost, and the task
transitions to `Cancelled` as today.

No commits happen inside the worktree, and no commits happen automatically
on `workspace_root` either — merge-back only updates the working tree/index
of the primary checkout, exactly like the agent's own `filesystem.write`
calls would. The existing `git.commit` tool (used by the agent itself
during the task, per the "commit continuously" rule) is what actually
creates commits; that tool call itself happened inside the worktree and its
effect (the staged/committed state) is part of the diff carried back in
step 2–3.

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
task start
  -> task_cancellations non-empty?
       no  -> workspace_root (unchanged path)
       yes -> git worktree add --detach .evohime/worktrees/<task_id> HEAD
              -> AgentLoopConfig.workspace_root = worktree path
task runs (filesystem/shell/git tools operate on worktree path)
task completes (success)
  -> lock workspace_merge_lock
  -> git diff <base_commit> (in worktree)
  -> git apply --3way (against workspace_root)
  -> ok: remove worktree, unlock
  -> conflict: fail task, keep worktree, unlock
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
