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

**The entry must also survive an approval pause, not just merge-back —
this is a live bug in the code the entry pattern already follows.**
`crates/server/src/ws.rs` has four places that spawn a task run and remove
its `task_cancellations` entry unconditionally once the run's `await`
resolves: the initial `UserMessage` handler (~ws.rs:193-197) and three
resume paths (plan-approval-granted ~ws.rs:352-356, tool-approval-granted,
and manual task-resume — all following the identical insert-before-spawn /
remove-after-await shape). A task that pauses for `approval.required`
(`crates/server/src/task/pipeline.rs`'s `NeedsApproval` branch) returns
`Ok(())` from `process_user_message`/`resume_task_run` *without finishing*
— it is `Paused`, not done, and will keep using the same
`primary_workspace_root` once resumed. All four call sites currently treat
that `Ok(())` exactly like true completion and remove the entry anyway.
Concretely: task A (first, unisolated) pauses for approval and its entry
is removed → task B starts, sees an empty map, also runs unisolated
directly against the same `primary_workspace_root` → the user approves A →
A resumes, still unisolated, now potentially running *at the same time* as
B in the same directory. This reintroduces the exact concurrent-write race
this whole design exists to prevent, and it is a mainline flow (approval
pauses are a normal, frequently-used path), not an edge case.

Fix: at all four removal sites, only remove the entry once the task's
*current* status (re-read from `tasks` via `evohime_storage::load_task`)
is terminal (`completed`/`failed`/`cancelled`) — never on a bare `Ok(()))`.
A `Paused` task keeps its entry indefinitely, which is correct: as long as
it's paused, some other task starting concurrently must still isolate
against the same directory. The entry is only actually removed when the
task reaches a real terminal state — either by finishing normally (the
resumed run eventually completes) or by being explicitly cancelled.

That last case needs one more fix: `ClientCommand::TaskCancel`
(`ws.rs:200-218`) cancels a task by calling `evohime_task_engine::cancel_task`
directly — for a currently-*paused* task, there is no active spawned
future whose `await` will ever resolve again to trigger the
status-recheck-and-remove above. `TaskCancel`'s handler must remove the
`task_cancellations` entry itself, right after transitioning the task to
`Cancelled`, or a cancelled-while-paused task's entry would never be
cleaned up and every later task would be needlessly isolated forever.

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
- Defense in depth: before calling `git worktree add`, verify
  `worktree_path` does not start with `primary_root` (it never will in
  practice, since the path is built from `std::env::temp_dir()`, but this
  is a one-line, zero-cost assertion against `std::env::temp_dir()` ever
  being misconfigured — e.g. a `TMPDIR`/`TEMP` environment variable
  pointing inside the repository — turning what would otherwise be the
  exact nested-checkout hazard the OS-temp-dir choice above exists to
  avoid into a clear startup error instead of a silent repeat of it).
- Every `git` subprocess call in this feature runs under a timeout
  (`tokio::time::timeout`), matching the existing convention of explicit
  per-operation `Duration` constants in
  `crates/tool-runtime/src/tools/git.rs` (`STATUS_TIMEOUT`,
  `COMMIT_TIMEOUT`, etc.). Without one, a hung `git` process (a stale lock
  file, an unresponsive network filesystem) would block merge-back
  indefinitely — and since merge-back holds the per-path merge lock for
  its duration, every other task later targeting the same
  `primary_workspace_root` would queue up behind it forever too.
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
4. Get the list of changed paths (`git diff --cached --name-only
   <base_commit_sha>`), then the full patch content (`git diff --cached
   --binary <base_commit_sha>`; `--binary` is required or a changed binary
   file's actual bytes are omitted from the diff and can never be
   reconstructed by `git apply` — without it, a binary file change would
   silently vanish from the merge). If the path list is empty, nothing
   changed relative to base; skip straight to step 7 (remove worktree,
   delete row) — there's nothing to apply or commit.

   **Every remaining step below operates on this exact path list only,
   never on `workspace_root`'s tree/index as a whole.** `workspace_root`
   is not a scratch space reserved for this merge — whenever `is_concurrent
   == false` for the currently-first task, that unisolated task's own
   tool calls are writing directly into this same directory, uncommitted,
   for the *entire* time other tasks are running isolated. An operation
   that touches "everything currently staged/dirty" (`git commit` with no
   pathspec, `git reset --hard`) would destroy or silently absorb that
   task's in-progress, unrelated work. Scoping every git invocation below
   to the specific paths this merge's own patch touches is what makes
   merge-back safe to run next to a live unisolated task.
5. Apply the patch to `workspace_root` with `git apply --3way --index`, so
   that changes landed on `workspace_root` by *other* tasks in the meantime
   (each merged the same way) are tolerated as long as they don't textually
   conflict. Plain `git apply` only ever touches the working tree; `--3way`
   alone does not reliably update `workspace_root`'s index for cleanly
   merged hunks — `--index` is required to actually stage them so the
   following commit captures the full result. (`git apply` itself already
   only ever touches the paths named in the patch — this step doesn't need
   an explicit pathspec, only the steps that operate on "whatever is
   currently staged" do.)
6. On a clean apply: check whether anything is actually staged *for this
   merge's own paths* (`git diff --cached --quiet -- <path-list>`) — a
   3-way merge can legitimately produce no staged diff if the primary side
   already matched. If something is staged, commit **scoped to exactly
   this merge's path list**: `git commit -m "agent: task <task_id>
   (worktree merge)" -- <path-list>`. Per `git commit`'s own semantics,
   passing pathspecs commits only changes to those paths regardless of
   what else happens to be staged or dirty elsewhere in the index —
   this is what keeps a concurrently-running unisolated task's unrelated
   staged/dirty state out of this commit. If the commit itself fails
   (rare — e.g. a hook rejects it), restore just this merge's paths (next
   step) and return an `Io` error rather than leaving them staged.
   The task's own `git.commit` call(s) made *inside* the worktree (per the
   "commit continuously" rule) already captured proper messages there;
   replaying each of those verbatim on `workspace_root` is unnecessary
   complexity, so this is a single squash commit. This keeps
   `workspace_root` in the same clean-HEAD state the "commit continuously"
   rule expects, whether or not this task actually needed isolation.
7. Remove the worktree directory (`git worktree remove --force` + `git
   worktree prune`), *then* delete the `task_worktrees` row, then release
   the lock. A filesystem removal and a Postgres delete can't be made truly
   atomic together, so this is ordered instead: if the process dies between
   the two, the row survives pointing at an already-removed directory,
   never the reverse (a directory surviving with no row, which the startup
   cleanup pass wouldn't know to look for). Startup cleanup (below) treats
   "directory already gone" as a normal no-op, not an error, when it
   processes such a row. **Failure at this step is not a merge failure —
   see the note at the end of this list.**
8. On a failed apply, or a failed scoped commit (step 6): **restore only
   this merge's own path list to `HEAD`** —
   `git checkout HEAD -- <path-list>` (plus, for paths the patch newly
   created that never existed at `HEAD`, `git reset -- <path-list>` to
   unstage before removing the now-untracked file directly — a created
   file has nothing at `HEAD` for `checkout` to restore). A failed 3-way
   apply can leave conflict markers and unmerged entries (visible via
   `git ls-files -u`) in the specific files it touched; nothing from this
   merge attempt was ever committed, so restoring exactly those paths is
   always safe. This is deliberately **not** `git reset --hard HEAD` — a
   blanket reset would also discard any unrelated uncommitted work
   belonging to a concurrently-running unisolated task elsewhere in
   `workspace_root` (see the note in step 4). Restoring only this merge's
   own paths leaves everything else in `workspace_root` exactly as it was.
   The residual risk this doesn't eliminate — the unisolated task happens
   to be editing the *same* file this merge's patch also touches — is
   exactly what a content conflict already means; there's no way to
   automatically untangle two concurrent edits to identical lines, and
   that's why this stays a manual-recovery path rather than an
   auto-resolve. After restoring: leave the worktree itself in place (its
   own state is unaffected), fail the task with a message that includes
   the worktree path, and release the lock. **The `task_worktrees` row is
   deliberately not deleted here** — it, and the worktree directory it
   points at, are the only record of the unmerged work, kept so an
   operator can inspect and manually recover it (see Cleanup on server
   restart for how long that window lasts).

**Cleanup failure (step 7) must not be reported as a task failure.** By
the time step 7 runs, the merge already committed successfully on
`workspace_root` — the user-visible outcome of the task is already
correct. If `git worktree remove` fails (file lock, permissions — not
uncommon on Windows) or the `task_worktrees` delete fails (transient DB
error), that's a housekeeping problem, not a work-loss problem: log a
warning and leave the worktree directory / row in place for the next
server-startup cleanup pass to retry (same "row outlives a mid-removal
crash" tolerance already designed into the ordering above). The task still
completes successfully.

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
  - If `primary_workspace_root` itself no longer exists on disk (the repo
    was moved or deleted), `git -C <primary_root> worktree prune` can never
    succeed — retrying it forever would leave the row stuck permanently.
    In that specific case, skip straight to deleting the row (there is no
    repository left to prune metadata from) after a best-effort plain
    `remove_dir_all` on the worktree directory itself if it still exists.
- Converting `retention` (a `Duration`) to a `chrono::Duration` for the
  cutoff comparison must not silently produce a zero-length window:
  `chrono::Duration::from_std` returns `Err` for out-of-range inputs, and
  naively falling back to `.unwrap_or_default()` gives a **zero** duration
  — making `cutoff` equal to "now" and immediately eligible-for-deletion
  every terminal row regardless of its actual age, the opposite of the
  intended retention grace period. The fallback must be a very large
  window (effectively "never expire this run") instead of zero.

**Orphaned-directory reconciliation.** `task_worktrees.task_id` is a
foreign key on `tasks(id) ON DELETE CASCADE`. If a task or its owning
session is ever deleted directly (session archival/deletion, restore/import
flows), Postgres removes the `task_worktrees` row as a side effect of that
cascade — without running any of this application's cleanup code, so the
physical directory is never touched and leaks permanently; the row-driven
pass above has no way to know it ever existed. To bound this: after the
row-driven pass, list the subdirectories of
`std::env::temp_dir().join("evohime-worktrees")` and remove any whose name
(parsed as a task ID) has no matching `task_worktrees` row — its owning row
is gone by definition, so nothing else will ever reference it again. This
pass doesn't need a primary root (there's no way to recover one for an
orphan) — a plain `remove_dir_all` on the directory is sufficient; it's no
longer registered with any git repository worth pruning against.

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
  -> path_list = git diff --cached --name-only <base_commit_sha> (in worktree)
  -> patch = git diff --cached --binary <base_commit_sha> (in worktree)
  -> git apply --3way --index (against workspace_root; patch already scoped to path_list)
  -> ok: git commit -- <path_list> (workspace_root, scoped — never touches unrelated
        state e.g. a concurrently-running unisolated task's own uncommitted work)
     -> remove worktree, delete task_worktrees row, unlock (failure here: log + retry
        later, task still succeeds — the commit already landed)
  -> conflict/commit-failure: git checkout HEAD -- <path_list> (workspace_root,
        scoped restore, never a blanket reset), fail task, keep worktree + row, unlock
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
  above (scoped `git checkout HEAD --` restore on `workspace_root`, fail
  merge-back, keep the worktree for manual inspection) — no extra
  rename-detection logic is needed for it.

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
- A scoped-operation test: `workspace_root` has unrelated uncommitted
  changes (simulating a live unisolated task) in a *different* file before
  merge-back runs; those changes are still present, untouched, both after
  a successful merge-back commit and after a forced conflict's recovery
  restore.
- A binary-file test: a worktree creates/modifies a binary file; merge-back
  lands it correctly on `workspace_root` (byte-for-byte), proving `--binary`
  is wired through the diff.
- A cleanup-failure-is-not-task-failure test: `finalize_worktree`'s merge
  step succeeds but `remove_worktree` is forced to fail; the function still
  reports success (the task completes), and the row/directory remain for a
  later retry.
- An approval-pause concurrency test: task A starts unisolated, pauses for
  approval (its `task_cancellations` entry must still be present); task B
  starting at that point must observe `is_concurrent == true` and get
  isolated.
- A `TaskCancel`-while-paused test: cancelling a paused task removes its
  `task_cancellations` entry directly (not just via the terminal-status
  recheck path, which never re-runs for an already-idle paused task).
- A retention-overflow test: an extreme `retention` value must not make
  `cutoff` collapse to "now" (i.e. must not delete a row created a second
  ago).
- A missing-primary-root test: a row's `primary_workspace_root` no longer
  exists on disk; cleanup deletes the row without retrying forever.
- An orphaned-directory test: a directory under `evohime-worktrees/` with
  no matching `task_worktrees` row (simulating a cascade-deleted task) is
  removed by the reconciliation pass.
