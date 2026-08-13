# Protocol design review — durable recovery / replay / resync (Этап 0c)

Date: 2026-08-13
Reviewer: Claude (agent), pass requested as the final checklist item of
section 13.6 in `docs/plans/evohime-master-plan.md`.

Scope: the full durable recovery/replay/resync protocol surface as it exists
after commit `048c0ff` (wiring of `recover_and_reconcile_after_restart()`
into Core startup):

- `run_leases` (owner/generation/heartbeat/expiry) — `crates/evohime-local-storage/src/lib.rs`
- reconciliation verifier + `run_reconciliations` — same file, `reconcile_run_effect`
- bounded recovery state machine `RECOVERING → RECONCILING → RESUMABLE|BLOCKED|WAITING_APPROVAL|FAILED`
  — `RecoveryState`, `transition_recovery`, `run_recovery` table
- `ResyncRequest` / replay-gap / `FullSnapshot` envelopes — `crates/desktop-ipc/src/lib.rs`,
  `crates/evohime-core/src/ipc_bridge.rs`
- `protocol_version` + capability negotiation — `negotiate_protocol` in `crates/desktop-ipc/src/lib.rs`
- live wiring — `recover_and_reconcile_after_restart()` in `crates/evohime-core/src/lib.rs:1807`

## Method

Read `048c0ff` end to end, the storage-layer implementations of leases,
effects, reconciliation and recovery state, the IPC handlers for
`ResyncRequest`/`ReplayEvents`, the protocol negotiation code, and the
existing test suites (`crates/evohime-local-storage/src/lib.rs` unit tests
around line 2280–2670, `crates/evohime-core/tests/recovery_state_machine.rs`).
No code was changed as part of the review itself.

## 1. Idempotency correctness

**Verdict: correct for the paths that are wired up.**

- `transition_recovery` requires a caller-supplied `idempotency_key` per
  transition. A repeat call with the same key against the *same* target
  state is a verified no-op (returns the existing record without a second
  `INSERT`). A repeat call with the same key against a *different* target
  state is rejected with `StorageError::InvalidRecovery` rather than
  silently overwriting history — this is the correct "double-fire with
  divergent outcome" guard.
- The state machine itself is validated server-side (not just
  client-trusted): `(None → Recovering)`, `(Recovering → Reconciling)`,
  `(Reconciling → any terminal)`, and `(terminal → same terminal)` are the
  only legal transitions; everything else (e.g. `Reconciling → Recovering`,
  or skipping straight to a terminal state) is rejected. This closes the
  "blind retry re-applies an effect" risk the exit criteria call out.
- `recover_and_reconcile_after_restart()` derives a distinct idempotency key
  per stage (`{run}:{effect}:recovering`, `...:reconciling`,
  `...:resumable`/`...:blocked`), so a crash between any two stages of
  *recovery itself* replays safely on the next startup — verified directly
  by `recovery_state_machine.rs`, which asserts a second recovery pass
  after a completed BLOCKED transition returns an empty reconciliation list
  (no re-processing, no duplicate `run.recovery.decision` events).
- `prepare_run_effect` uses `idempotency_key TEXT NOT NULL UNIQUE` at the
  schema level plus `INSERT OR IGNORE`, so effect creation is idempotent at
  the DB constraint layer, not just in application logic.
- Every `transition_recovery` call appends a `run.recovery.decision` audit
  event inside the same transaction as the state row insert — so the audit
  trail and the state transition can't diverge (no state change without an
  event, no event without a committed state change, since both are in one
  `unchecked_transaction()`).

No idempotency gap was found in the code that is actually invoked from the
live Core startup path.

## 2. Generation/lease correctness

**Verdict: the storage primitive is correct; the live recovery path does not use it. This is a real gap (see Finding A).**

- `acquire_run_lease` is race-safe as written: it only ever updates a lease
  row when `lease_id` matches (same owner re-acquiring/renewing) or the
  existing lease has expired (`lease_expires_at <= datetime('now')`);
  otherwise `updated == 0` and it returns
  `StorageError::InvalidRunEffect("run lease is held by another owner")`.
  A stale owner cannot silently keep mutating state past lease expiry
  because `heartbeat_run_lease`'s `UPDATE ... WHERE ... AND lease_expires_at
  > datetime('now')` fails once the lease has lapsed, and the caller is
  expected to treat that as `LeaseRejected`.
- `release_run_lease` deletes by the full tuple
  `(run_id, lease_id, owner_id, generation)`, so a caller cannot release a
  lease it doesn't currently hold.
- The generation column is threaded through every lease operation
  correctly at the SQL level — a stale generation on any of
  `acquire/heartbeat/release` simply matches zero rows and fails safe.

**Finding A (gap, low severity given current single-instance deployment):**
`recover_and_reconcile_after_restart()` never calls `acquire_run_lease`,
`get_run_lease`, or bumps the generation for the run it is recovering. It
goes straight from `recover_unknown_effects()` to `transition_recovery(...,
Recovering, ...)`. The only place a lease is acquired at all is
`prepare_run_effect`'s caller path (`crates/evohime-core/src/lib.rs:1751`),
at a hardcoded `generation = 1`, and it is only released on successful
`complete_build_effect`. Concretely:
  - After a hard kill, the `run_leases` row for the crashed run is still
    present and only expires after its TTL (30s in the current caller).
    Recovery does not check or take over that lease, so a second Core
    process racing the same run during that 30s window would not be
    blocked by the lease mechanism from also touching `run_effects` — it
    would only be blocked by SQLite's own transaction serialization on the
    same DB file, which is real but incidental protection, not protocol
    protection.
  - There is no generation bump on restart, so the lease table can never
    distinguish "the same Core process that died" from "a new Core process
    that came back" — both would present as generation 1, owner `"core"`.
  - In the current architecture (single Core process per supervisor,
    supervisor guarantees at most one live child via Job Object) this is
    not exploitable today. It becomes a real risk only if the deployment
    model changes to allow two Core instances against the same DB
    concurrently (e.g. a future multi-window or crash-restart-race
    scenario where the old process hasn't fully exited when the new one
    starts). Recommend wiring `acquire_run_lease` with a bumped generation
    into `recover_and_reconcile_after_restart()` before it starts
    reconciling, so recovery itself is lease-guarded, not just the
    original effect execution.

## 3. Gap/resync correctness

**Verdict: correct — no silent-data-loss path found.**

- `ResyncRequest` handling (`ipc_bridge.rs:133`) always checks
  `batch.gap_detected` and, if set, writes an explicit `replay.gap` event
  *before* continuing to serve whatever event range or snapshot is
  actually available — the client is told about the gap rather than
  silently receiving a truncated stream that looks contiguous.
  `ReplayEvents` (non-resync path) does the same
  (`ipc_bridge.rs:217-232`).
- `include_full_snapshot` gives the client an explicit way to force a clean
  full-state resync instead of relying on incremental replay, and
  `validate_full_snapshot` bounds its size (`MAX_RESYNC_SNAPSHOT_BYTES =
  MAX_FRAME_BYTES - 1024`) so a huge snapshot can't blow past the frame
  limit and get silently truncated by the transport.
- `validate_resync_request` bounds `max_events` to
  `DEFAULT_RESYNC_MAX_EVENTS` (512), preventing an unbounded resync from
  being requested; a caller who wants more must page.
- Every resync response ends with an explicit `resync.end` envelope
  carrying `last_sequence`, giving the client a definite boundary to
  resume from rather than inferring completion from stream EOF.
- The recovery state machine's only two terminal states reachable from
  `Reconciling` for an unverified effect are `Resumable` (confirmed by a
  matching snapshot) or `Blocked` (unconfirmed) — there's no third "assume
  success" path. `recover_and_reconcile_after_restart()` computes `success`
  strictly from `snapshot.run_id == record.run_id`; any missing or
  mismatched snapshot is `false`, which lands on `Blocked`, never silently
  marks the run `completed`.

No gap in this area was found: a replay gap always surfaces as
`replay.gap`, and a recovered effect with unconfirmed outcome always lands
on `BLOCKED`, never resumes automatically. This matches the exit criteria
in section 13.6 ("partial gap корректно восстанавливается или приводит к
full snapshot").

## 4. Protocol version + capability negotiation

**Verdict: correct.**

- `negotiate_protocol` rejects on major-version mismatch
  (`NegotiationError::MajorMismatch`) and otherwise negotiates
  `min(local.minor, peer.minor)` — a strictly additive-only compatibility
  model, matching the "backward compatibility matrix" requirement.
- Capability lists are bounded (`MAX_CAPABILITIES = 64`) and each name is
  validated for length and absence of control bytes
  (`normalize_capabilities`), so a malformed or adversarial capability list
  can't be used to smuggle unbounded data through the handshake or corrupt
  log output.
- The negotiated capability set is the intersection of local and peer
  capabilities (`local_caps.filter(peer_caps.binary_search(...).is_ok())`),
  so an old client that doesn't advertise `resync` simply doesn't get it
  offered, rather than the newer server assuming it's present.

## 5. Type-specific outcome verifiers

**Finding B (gap, real but currently inert):** `crates/evohime-local-storage/src/reconciliation_verifier.rs`
implements exactly what the checklist calls for — `SnapshotKind::{File,
Database, Process}`, typed `FileSnapshotOutcome` / `DatabaseSnapshotOutcome`
/ `ProcessSnapshotOutcome`, a `VerificationStatus::{Confirmed, Unconfirmed,
Blocked}` result, and `verify_snapshot()` with generation checks — and it is
unit-tested (`file_hash_match_is_confirmed`, `mismatch_is_unconfirmed_without_retry`,
`missing_database_evidence_is_blocked`, `process_generation_mismatch_is_unconfirmed`,
`oversized_input_is_rejected`). However, grepping Core and the storage
crate's own `lib.rs` for callers of `verify_snapshot` outside its own test
module turns up **nothing** — `recover_and_reconcile_after_restart()`, the
only live recovery entry point, never calls into
`reconciliation_verifier` at all. It hardcodes `"bounded_build_snapshot"`
as the verifier name for every recovered effect and computes `success`
itself via a direct `snapshot.run_id == record.run_id` comparison against
`latest_snapshot_for_task`, bypassing the typed verifier module entirely.
`RecoveredRunRecord` (returned by `recover_unknown_effects()`,
`crates/evohime-local-storage/src/lib.rs:216-220`) also doesn't carry an
effect `kind`, so there is no way for the live recovery loop to even choose
`SnapshotKind::File` vs `Database` vs `Process` if it wanted to.

Net effect: the type-specific verifier logic is correctly implemented and
tested in isolation, but is dead code from the live recovery path's point
of view. This is not a correctness bug today — the only effect kind Core
currently produces is `"bounded_build"`, and that one path's ad hoc
snapshot-match check is itself correct (see §3). But it means the checklist
line is only half true: the *bounded verifiers* exist and are bounded, but
they are not *wired into* recovery. Recommend either wiring
`recover_and_reconcile_after_restart()` to dispatch through
`reconciliation_verifier::verify_snapshot` (adding a `kind` field to
`RecoveredRunRecord`), or, if `verify_snapshot` is intentionally reserved
for a future effect kind, saying so explicitly next to its definition so a
future reader doesn't assume it's already load-bearing.

## Summary of findings

| # | Severity | Area | Finding |
|---|----------|------|---------|
| A | Low (today) / Medium (future multi-instance) | lease/generation | Recovery path never acquires or bumps the run lease before reconciling; protection today is incidental (single-process + SQLite transaction serialization), not protocol-level. |
| B | Low | verifier dispatch | `reconciliation_verifier::verify_snapshot` (typed file/database/process verifiers) is implemented and unit-tested but has no caller outside its own tests; `recover_and_reconcile_after_restart()` bypasses it with a hardcoded ad hoc snapshot check. Correct today (only one effect kind exists) but the module is currently dead code from production's perspective. |

No idempotency-correctness bug, no gap/resync silent-data-loss path, and no
protocol-negotiation issue were found. Both findings above are pre-existing
scope limitations rather than bugs in what's implemented — they are
recorded here as the honest output of the review rather than left
unmentioned, per the instruction to report "nothing wrong" explicitly only
if that's actually true. It is not fully true: Finding A is a genuine
protocol gap worth tracking (recommend a follow-up task before any
multi-instance or hot-restart-race scenario is supported), Finding B is a
documentation/scope note rather than an action item until a second effect
kind is added.
