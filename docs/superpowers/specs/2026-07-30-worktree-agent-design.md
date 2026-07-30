# Worktree-Aware Multi-Checkout Agent (`7.107`) Design

**Date:** 2026-07-30
**Status:** Ready for implementation
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
- Concurrency detection (`is_concurrent`) stays global across
  `task_cancellations`, not scoped per distinct `workspace_root`. A task
  can carry its own `workspace_root` via `task.workspace_path` (Sites
  feature, `crates/server/src/task/pipeline.rs:73-81`) that differs from
  `AppState.workspace_root`; this design does not check whether a
  concurrently-running task actually targets the *same* directory before
  triggering isolation. The only cost of this simplification is an
  occasional unnecessary worktree (isolation triggered for two tasks on
  unrelated Sites workspaces); it is never a correctness risk, since
  merge-back always targets the isolated task's own resolved
  `workspace_root`, never a different one. Scoping the trigger per-path is
  future work if the waste turns out to matter in practice.

## Architecture

### Trigger

Isolation is decided once, at task start, using the existing
`task_cancellations` map as the source of truth for "is another task
already running against this workspace". `task_cancellations` is
`Arc<tokio::sync::Mutex<HashMap<Uuid, CancellationToken>>>` — the same
`tokio::sync::Mutex` type used for every other `AppState` map, which never
poisons on panic (its `lock()` is infallible, unlike `std::sync::Mutex`).
`workspace_merge_locks` (below) must use the same type for the same reason.

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

**The `task_cancellations` entry is the concurrency signal other tasks rely
on, so it must stay for as long as this task can still touch
`workspace_root`** — that includes the entire merge-back critical section
(steps 1–6 below), not just the agent loop's own work. Today,
`crates/server/src/ws.rs:193-197` removes the entry immediately after
`process_user_message` returns. Merge-back must happen *inside*
`process_user_message` (or whatever wraps it), before that removal, on
every path — success, failure, and cancellation. If the entry were removed
before merge-back finishes running `git apply`/`git commit` against
`workspace_root`, a new task starting in that window would see an empty
map, skip isolation, and run directly against `workspace_root` while this
task's merge-back is still touching it — reintroducing the exact race this
design exists to prevent.

### Worktree lifecycle

- Location: **outside** `workspace_root` entirely —
  `std::env::temp_dir().join("evohime-worktrees").join(task_id)`. Not
  `.evohime/worktrees/` inside the repo: `.evohime/` is not gitignored
  (`.evohime/plugins/...` is tracked today), so a worktree nested inside
  the tracked tree would itself be a full checkout of the repo sitting at a
  path git in `workspace_root` can see. `git add -A` — run either inside
  the worktree during merge-back, or by any other unisolated task running
  ordinary agent operations against `workspace_root` — would then pick up
  that entire nested checkout as untracked content and could get it
  committed. Keeping worktrees off the tracked tree entirely removes this
  class of bug structurally instead of depending on a `.gitignore` entry
  staying correct forever.
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
- Idempotent by construction: provisioning first checks `task_worktrees`
  for an existing row for this `task_id` and returns immediately if one is
  found, instead of calling `git worktree add` again (which would fail —
  the directory already exists). Nothing in this design calls provisioning
  twice today, but this makes it safe if a future retry path does.
- If `git worktree add` succeeds but the `task_worktrees` insert then fails
  (a transient Postgres error), the worktree is removed immediately
  (`git worktree remove --force` + `prune`) before returning the error, so
  provisioning fails cleanly rather than leaving an orphan directory with
  no row — the row is the only thing startup cleanup or merge-back knows
  to look for.
- Locking: **not** a single global lock. A worktree merge-back only ever
  touches one `primary_workspace_root` (a task's own resolved workspace,
  which can differ across concurrent tasks under the Sites feature — see
  Non-goals). Serializing every merge-back behind one process-wide
  `Mutex<()>` would make an unrelated task on workspace B wait on a slow
  merge for workspace A for no reason. Instead, `AppState` holds a small
  lock registry:
  `workspace_merge_locks: Arc<tokio::sync::Mutex<HashMap<PathBuf,
  Arc<tokio::sync::Mutex<()>>>>>`. Acquiring the lock for a given
  `primary_workspace_root` means: briefly lock the outer map, get-or-insert
  an `Arc<tokio::sync::Mutex<()>>` for that path, drop the outer map lock,
  then lock the per-path mutex for the duration of the actual merge. The
  outer map lock is never held across a merge, so two different primary
  roots never wait on each other; two tasks merging into the *same* root
  still serialize correctly.

### Merge-back

On successful task completion, before the task transitions to
`Completed`:

1. Acquire the per-`primary_workspace_root` merge lock from
   `workspace_merge_locks` (`tokio::sync::Mutex<()>`, same rationale as
   `task_cancellations` above — must not poison).
2. Log `base_commit_sha` alongside a fresh `git rev-parse HEAD` on
   `workspace_root` at this point, before touching anything. The apply step
   below already handles a moved `HEAD` correctly (that's what `--3way` is
   for), so this is diagnostic only — when a conflict *does* happen, the
   log line already on hand shows whether it was caused by drift (another
   task's merge landed in between) or a same-base textual collision,
   instead of requiring that to be reconstructed after the fact.
3. Inside the worktree, run `git add -A` first. `git diff <base_commit>`
   alone never shows untracked files regardless of the commit it's
   compared against — a file the agent created but never staged would
   silently be dropped from the merge otherwise. Staging everything first
   makes the following diff/apply steps see the complete set of changes.
4. Compute the diff against `base_commit_sha` read back from
   `task_worktrees` (`git diff --cached <base_commit_sha>`).
5. Apply that diff to `workspace_root` with `git apply --3way --index`, so
   that changes landed on `workspace_root` by *other* tasks in the meantime
   (each merged the same way) are tolerated as long as they don't textually
   conflict. Plain `git apply` only ever touches the working tree; `--3way`
   alone does not reliably update `workspace_root`'s index for cleanly
   merged hunks — `--index` is required to actually stage them so the
   following commit captures the full result.
6. On a clean apply: run `git commit` on `workspace_root` to land the
   result as an actual commit, not a dirty working tree sitting on top of
   an otherwise-clean `HEAD`. The task's own `git.commit` call(s) made
   *inside* the worktree (per the "commit continuously" rule) already
   captured proper messages there; replaying each of those message
   verbatim on `workspace_root` is unnecessary complexity, so this is a
   single squash commit, message `"agent: task <task_id> (worktree
   merge)"`. This keeps `workspace_root` in the same clean-HEAD state the
   "commit continuously" rule expects, whether or not this task actually
   needed isolation.
7. Remove the worktree directory (`git worktree remove --force` + `git
   worktree prune`), *then* delete the `task_worktrees` row, then release
   the lock. A filesystem removal and a Postgres delete can't be made truly
   atomic together, so this is ordered instead: if the process dies between
   the two, the row survives pointing at an already-removed directory,
   never the reverse (a directory surviving with no row, which the startup
   cleanup pass wouldn't know to look for). Startup cleanup (below) treats
   "directory already gone" as a normal no-op, not an error, when it
   processes such a row.
8. On failure (patch does not apply cleanly, or the commit step fails):
   **first restore `workspace_root` to a clean state** — `git apply
   --3way --index` can leave the primary checkout's live working
   tree/index with conflict markers and unmerged entries (visible via
   `git ls-files -u`) when the 3-way merge can't reconcile automatically.
   Nothing from this failed attempt was ever committed, so it's always safe
   to run `git reset --hard HEAD` (discarding only this merge attempt,
   never a real commit) before returning the error — otherwise the *next*
   task to touch `workspace_root`, isolated or not, would inherit a
   half-merged git state it had nothing to do with. Only after that
   restoration: leave the worktree itself in place (its own state is
   unaffected by the primary-side reset), fail the task with a message
   that includes the worktree path, and release the lock. No automatic
   conflict resolution — this is a rare edge case (two concurrent tasks
   touching the same lines) and is surfaced for manual inspection rather
   than guessed at. **The `task_worktrees` row is deliberately not deleted
   here** — it, and the worktree directory it points at, are the only
   record of the unmerged work, kept so an operator can inspect and
   manually recover it (see Cleanup on server restart for how long that
   window lasts).

Cancellation follows the same shape as failure: the worktree is left in
place (not silently deleted) so in-progress work isn't lost, and the task
transitions to `Cancelled` as today.

**Merge-back is keyed to whether the agent finished its work, not to
whether the rest of the response pipeline succeeds.** It runs as soon as
`agent_result` is `Ok(..)` — before message persistence, memory feedback,
or `complete_task` — so a later failure in one of *those* steps doesn't
skip merge-back or leave it half-done; the isolated changes are already
safely landed on `workspace_root` by that point regardless of what happens
next in the response pipeline.

### Cleanup on server restart

Worktree directories under the OS temp dir can be left on disk if the
server crashes mid-task, and rows are deliberately left behind after a
merge conflict for manual inspection (Merge-back step 8). At startup, the
server reads all rows from `task_worktrees` (the source of truth, not a
directory scan) and decides per row using the owning task's **current
status in the `tasks` table** — not the transient, restart-scoped list
`recover_after_restart` (`crates/task-engine/src/lib.rs:136`) returns.

That distinction matters: `recover_after_restart`'s return value only
reflects tasks that were literally `Running`/`Cancelling` at the moment of
*this* restart. A task sitting in `Paused` because it's mid-way through an
`approval.required` round-trip (an ordinary pause, not a crash — see
`crates/server/src/task/pipeline.rs`'s `NeedsApproval` branch, which
returns `Ok(())` without ever reaching merge-back) would **not** appear in
that list on a later, unrelated restart, even though its `task_worktrees`
row is still very much in use and resuming it reuses the same worktree.
Keying cleanup off `recover_after_restart` instead of live status would
eventually delete that worktree out from under a task waiting on the
user's approval.

The rule instead: query each row's task status directly.

- Status `Running`, `Paused`, or any other **non-terminal** status: always
  kept, regardless of age. The task may still resume and reuse this exact
  worktree.
- Status `Completed`: should never appear here — a successful merge-back
  deletes its own row (step 7) before the task transitions to `Completed`.
  If one is found anyway (a crash between commit and row-delete), treat it
  like `Failed`/`Cancelled` below rather than treating it as a bug to
  surface.
- Status `Failed` or `Cancelled` (terminal, and either genuinely orphaned
  crash debris or a deliberately-preserved merge-conflict row): removed
  only once older than a retention window (`created_at` older than
  `EVOHIME_WORKTREE_RETENTION_SECS`, default 24h) — directory removed if
  present (`NotFound` is not an error — see the ordering note in Merge-back
  step 7), `git worktree prune` run against the row's own
  `primary_workspace_root`, and the row deleted. Rows newer than the
  window are left for the *next* startup check, giving an operator a real
  window to inspect a conflict before it's swept.

This runs unconditionally at startup (cheap query when the table is empty)
so orphaned worktrees never accumulate indefinitely, without ever deleting
a worktree a non-terminal task still depends on.

### Error handling / fallback

Falling back to the shared `workspace_root` is only ever safe when
`is_concurrent == false` — that's the situation the fallback matches
anyway (no other task is touching `workspace_root`, so using it directly
carries none of the risk this design exists to prevent):

- `is_concurrent == false` and `workspace_root` is not a git repository, or
  worktree creation fails for any other reason (disk full, permissions):
  isolation is skipped, log a warning once, use `workspace_root` directly.
  Identical to today's behavior.
- `is_concurrent == true` and worktree creation fails for *any* reason:
  isolation cannot be provided but another task is actively using
  `workspace_root` — falling back to the shared root here would recreate
  the exact race/corruption this design exists to prevent. The task fails
  immediately instead, with an error naming the underlying cause (e.g.
  "failed to allocate isolated worktree: disk full"), and the
  `task_cancellations` entry is removed as part of that failure. It does
  not touch `workspace_root`.

## Data flow summary

```
task start (single task_cancellations lock guard covers both steps)
  -> is_concurrent = !task_cancellations.is_empty(); insert(task_id, token)
       false -> workspace_root (unchanged path)
       true  -> base_commit_sha = git rev-parse HEAD
                git worktree add --detach <temp_dir>/evohime-worktrees/<task_id> <base_commit_sha>
                insert task_worktrees(task_id, base_commit_sha, worktree_path)
                -> AgentLoopConfig.workspace_root = worktree path
task runs (filesystem/shell/git tools operate on worktree path)
task completes (success)
  -> lock workspace_merge_locks[primary_workspace_root]
  -> log base_commit_sha vs current rev-parse HEAD (diagnostic only)
  -> git add -A (in worktree, catches untracked files)
  -> git diff --cached <base_commit_sha> (in worktree; sha from task_worktrees)
  -> git apply --3way --index (against workspace_root, stages clean hunks)
  -> ok: git commit (workspace_root), remove worktree, delete task_worktrees row, unlock
  -> conflict: git reset --hard HEAD (workspace_root), fail task, keep worktree + row, unlock
```

## Implementation notes

- `git worktree remove` must be called with `--force`: after the squash
  commit lands on `workspace_root`, the worktree itself is still "dirty"
  relative to its own `base_commit_sha` (nothing was reset inside it), so
  a plain `git worktree remove` would fail with `fatal: '<path>' contains
  uncommitted changes`.
- A rename in the worktree diff can appear to `git apply --3way` as a
  delete+create pair. If another concurrent task edits that same file's
  content on `workspace_root` in the meantime, the 3-way merge may not
  reconcile cleanly. This is exactly the conflict path already specified
  above (`git reset --hard HEAD` on `workspace_root`, fail merge-back, keep
  the worktree for manual inspection) — no extra rename-detection logic is
  needed for it.

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
  still present at `worktree_path` (from `task_worktrees`) afterward.
- A fallback test: `workspace_root` is a plain (non-git) temp directory;
  task execution proceeds without isolation and without error.
- A merge-back test asserting `workspace_root`'s `HEAD` actually advances
  (a real commit exists) after a successful merge, including a case where
  the worktree only ever created an untracked file and never called
  `git.commit` itself.
- A startup-cleanup test: a stale `task_worktrees` row for a `Failed` task
  older than the retention window is removed (row and directory both
  gone); a `Paused` task's row is kept regardless of age and regardless of
  whether that task appears in `recover_after_restart`'s return value for
  *this* restart; a `Failed` task's row younger than the retention window
  is kept; a row whose directory is already missing is deleted without
  error.
- A concurrent-failure test: `is_concurrent == true` and worktree creation
  is forced to fail; the task fails immediately and `workspace_root` is
  left untouched (no fallback to the shared root).
- A provisioning-rollback test: `git worktree add` succeeds but the
  `task_worktrees` insert is forced to fail; the worktree directory is
  removed before the error is returned (no orphan directory with no row).
- An idempotent-provisioning test: `provision_worktree` called twice for
  the same `task_id` succeeds both times and only one worktree/row exists.
- A per-path lock test: two merge-backs targeting *different*
  `primary_workspace_root`s proceed concurrently (neither blocks on the
  other); two merge-backs targeting the *same* root still serialize.
- A conflict-recovery test: after a forced merge conflict, `workspace_root`
  has no unmerged index entries (`git ls-files -u` empty) and matches its
  pre-attempt `HEAD` afterward.
