# Failure Learning Wave 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining `7.103` scope: escalate confidence/importance when a `failure_pattern`/`verification_rule` repeats (duplicate at admit), and rank those lessons above other experience memories at retrieval time — no new migration, no protocol/frontend changes.

**Architecture:** Add `FeedbackSignal::Repeated` to the existing feedback pipeline (`crates/memory/src/feedback.rs` + `feedback_service.rs`) and invoke it as a side effect inside `admit_memory_item` (`crates/memory/src/service.rs`) whenever a duplicate hits a `Candidate`-status `Experience`-scope `FailurePattern`/`VerificationRule` row. Separately, bump the experience-kind retrieval bonus in `crates/memory/src/retrieve.rs::score_item` for those two kinds.

**Tech Stack:** Rust, existing `evohime-memory` crate, existing `evohime-storage` feedback/admit plumbing, `sqlx` Postgres integration test pattern (`connect_integration_pool`).

## Global Constraints

- No new DB migration for a repeat-count column — repeat count is derivable later from `memory_feedback_events` if ever needed. (Amended during Task 3 real-DB verification: the `memory_feedback_events.signal` column has a `CHECK` constraint, `memory_feedback_signal_check`, listing allowed values; `'repeated'` must be added to that list via a small migration, or every `FeedbackSignal::Repeated` write silently rolls back the whole transaction — see Task 3a.)
- Confidence escalation must never be able to reach `FAILURE_CONFIDENCE_CAP` (0.6) or beyond in a way that unlocks auto-promote — hard-capped at exactly `FAILURE_CONFIDENCE_CAP`, reusing the existing `crates/memory/src/extract.rs` constant, not a new magic number.
- Importance escalation has no upper cap beyond the standard `clamp01` (0..=1) — `decide_gate` never reads importance, so this is safe and is what drives retrieval priority.
- Escalation only applies when the existing duplicate row's status is `Candidate`. `Active`/`Rejected`/`Conflict` rows are left untouched (respect the operator's prior decision).
- `AdmitOutcome` and `GateDecision` shapes do not change. No new `MemoryProposed`/`MemoryAsk` events. Callers of `admit_memory_item` (e.g. `crates/server/src/task/memory.rs`) require zero changes.
- Non-experience or non-failure-lane duplicates (regular facts, preferences, success patterns, playbooks) behave exactly as before.
- Commit after each task per repo rule (`AGENTS.md` rule 11): finished work is committed without waiting to be asked; push only on explicit request.

---

### Task 1: `FeedbackSignal::Repeated`

**Files:**
- Modify: `crates/memory/src/feedback.rs`

**Interfaces:**
- Consumes: `crate::extract::FAILURE_CONFIDENCE_CAP` (existing, `pub const FAILURE_CONFIDENCE_CAP: f64 = 0.6;` in `crates/memory/src/extract.rs:357`).
- Produces: `FeedbackSignal::Repeated` variant, `FAILURE_REPEAT_CONFIDENCE_BUMP: f64`, `FAILURE_REPEAT_IMPORTANCE_BUMP: f64` constants — used by Task 2's `record_memory_repeated`.

- [ ] **Step 1: Write the failing tests**

Add at the end of the `#[cfg(test)] mod tests` block in `crates/memory/src/feedback.rs` (after the existing `idle_decay_archives_when_low` test, before the final closing `}`):

```rust
    #[test]
    fn repeated_bumps_confidence_and_importance_below_cap() {
        let adj = apply_feedback_signal(
            0.5,
            0.5,
            false,
            MemoryStatus::Candidate,
            FeedbackSignal::Repeated,
        );
        assert!((adj.confidence - 0.55).abs() < 1e-9);
        assert!((adj.importance - 0.6).abs() < 1e-9);
        assert!(adj.next_status.is_none());
    }

    #[test]
    fn repeated_confidence_never_exceeds_failure_cap() {
        let near_cap = apply_feedback_signal(
            0.58,
            0.5,
            false,
            MemoryStatus::Candidate,
            FeedbackSignal::Repeated,
        );
        assert!((near_cap.confidence - FAILURE_CONFIDENCE_CAP).abs() < 1e-9);

        let already_at_cap = apply_feedback_signal(
            FAILURE_CONFIDENCE_CAP,
            0.5,
            false,
            MemoryStatus::Candidate,
            FeedbackSignal::Repeated,
        );
        assert!((already_at_cap.confidence - FAILURE_CONFIDENCE_CAP).abs() < 1e-9);
    }

    #[test]
    fn repeated_importance_has_no_cap_besides_unit_clamp() {
        let adj = apply_feedback_signal(
            0.3,
            0.95,
            false,
            MemoryStatus::Candidate,
            FeedbackSignal::Repeated,
        );
        assert!((adj.importance - 1.0).abs() < 1e-9);
    }

    #[test]
    fn repeated_signal_parses_and_formats() {
        assert_eq!(FeedbackSignal::Repeated.as_str(), "repeated");
        assert_eq!(FeedbackSignal::parse("repeated"), Some(FeedbackSignal::Repeated));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p evohime-memory feedback:: -- --nocapture`
Expected: FAIL to compile — `FeedbackSignal::Repeated` variant does not exist.

- [ ] **Step 3: Implement**

At the top of `crates/memory/src/feedback.rs`, change:

```rust
use evohime_storage::MemoryStatus;
```

to:

```rust
use crate::extract::FAILURE_CONFIDENCE_CAP;
use evohime_storage::MemoryStatus;
```

After the existing `USED_CONFIDENCE_BUMP` constant (currently the last const before the enum), add:

```rust
/// Bump applied when an experience failure-lesson (failure_pattern/verification_rule)
/// repeats as an admit duplicate — a repeated mistake is stronger evidence than a guess.
pub const FAILURE_REPEAT_CONFIDENCE_BUMP: f64 = 0.05;
pub const FAILURE_REPEAT_IMPORTANCE_BUMP: f64 = 0.1;
```

Change the enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackSignal {
    Used,
    Helpful,
    Harmful,
    Corrected,
    Rejected,
    IdleDecay,
    Repeated,
}
```

Change `as_str`:

```rust
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Used => "used",
            Self::Helpful => "helpful",
            Self::Harmful => "harmful",
            Self::Corrected => "corrected",
            Self::Rejected => "rejected",
            Self::IdleDecay => "idle_decay",
            Self::Repeated => "repeated",
        }
    }
```

Change `parse`:

```rust
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "used" => Some(Self::Used),
            "helpful" => Some(Self::Helpful),
            "harmful" => Some(Self::Harmful),
            "corrected" => Some(Self::Corrected),
            "rejected" => Some(Self::Rejected),
            "idle_decay" => Some(Self::IdleDecay),
            "repeated" => Some(Self::Repeated),
            _ => None,
        }
    }
```

In `apply_feedback_signal`, add a new match arm right after the `FeedbackSignal::IdleDecay` arm (before the closing `};` of the `match signal` block):

```rust
        FeedbackSignal::IdleDecay => {
            let conf = clamp01(before - IDLE_CONFIDENCE_DECAY);
            if !pinned
                && conf < ARCHIVE_CONFIDENCE_THRESHOLD
                && matches!(status, MemoryStatus::Active | MemoryStatus::Candidate)
            {
                next_status = Some(MemoryStatus::Archived);
            }
            conf
        }
        FeedbackSignal::Repeated => {
            next_importance = clamp01(next_importance + FAILURE_REPEAT_IMPORTANCE_BUMP);
            clamp01(before + FAILURE_REPEAT_CONFIDENCE_BUMP).min(FAILURE_CONFIDENCE_CAP)
        }
    };
```

(Replace only the trailing `};` of the existing `match` — the `FeedbackSignal::IdleDecay` arm body itself is unchanged, this just adds a new arm after it.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p evohime-memory feedback:: -- --nocapture`
Expected: PASS — all `feedback::tests::*` tests green, including the 4 new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/memory/src/feedback.rs
git commit -m "feat(memory): add Repeated feedback signal for failure-lesson escalation"
```

---

### Task 2: `record_memory_repeated` service function

**Files:**
- Modify: `crates/memory/src/feedback_service.rs`
- Modify: `crates/memory/src/lib.rs`

**Interfaces:**
- Consumes: `FeedbackSignal::Repeated` (Task 1), existing private `apply_one(pool, id, signal, task_id)` helper already in `feedback_service.rs`.
- Produces: `pub async fn record_memory_repeated(pool: &PgPool, memory_id: Uuid, task_id: Option<Uuid>) -> Result<Option<FeedbackApplyResult>, MemoryError>` — consumed by Task 3.

- [ ] **Step 1: Implement (no new unit test needed — `apply_one` is already covered; this is a thin wrapper identical in shape to `record_memory_corrected`)**

In `crates/memory/src/feedback_service.rs`, after the `record_memory_corrected_for_operator` function (ends at line 214, right before the `/// Decay a batch...` comment on line 216), add:

```rust
/// Escalate a repeated failure-lesson duplicate (7.103 wave 2): confidence/importance
/// rise, but confidence is hard-capped by `apply_feedback_signal`'s `Repeated` arm — this
/// never unlocks auto-promote for a failure-derived lesson.
pub async fn record_memory_repeated(
    pool: &PgPool,
    memory_id: Uuid,
    task_id: Option<Uuid>,
) -> Result<Option<FeedbackApplyResult>, MemoryError> {
    apply_one(pool, memory_id, FeedbackSignal::Repeated, task_id).await
}
```

- [ ] **Step 2: Export from the crate root**

In `crates/memory/src/lib.rs`, change:

```rust
pub use feedback_service::{
    decay_unused_memory, record_memory_corrected, record_memory_corrected_for_operator,
    record_memory_harmful, record_memory_helpful, record_memory_rejected,
    record_memory_rejected_for_operator, record_memory_used, FeedbackApplyResult,
    DEFAULT_IDLE_BATCH, DEFAULT_IDLE_DAYS,
};
```

to:

```rust
pub use feedback_service::{
    decay_unused_memory, record_memory_corrected, record_memory_corrected_for_operator,
    record_memory_harmful, record_memory_helpful, record_memory_rejected,
    record_memory_rejected_for_operator, record_memory_repeated, record_memory_used,
    FeedbackApplyResult, DEFAULT_IDLE_BATCH, DEFAULT_IDLE_DAYS,
};
```

- [ ] **Step 3: Check it builds**

Run: `cargo check -p evohime-memory`
Expected: builds clean (this task adds no new tests of its own; Task 3's integration test exercises this function through `admit_memory_item`).

- [ ] **Step 4: Commit**

```bash
git add crates/memory/src/feedback_service.rs crates/memory/src/lib.rs
git commit -m "feat(memory): expose record_memory_repeated"
```

---

### Task 3: Wire escalation into `admit_memory_item`

**Files:**
- Modify: `crates/memory/src/service.rs`
- Modify: `crates/memory/src/lib.rs` (add integration test to the existing `#[cfg(test)] mod tests` block)

**Interfaces:**
- Consumes: `record_memory_repeated` (Task 2), existing `ExistingMemory { id, kind, status, content, pinned, embedding, embedding_version }`, existing `NewMemoryItem.scope: MemoryScope`, `NewMemoryItem.source_task_id: Option<Uuid>`.
- Produces: no new public signature — `admit_memory_item`'s existing signature and `AdmitOutcome` enum are unchanged; this task only adds an internal side effect.

- [ ] **Step 1: Write the failing integration test**

In `crates/memory/src/lib.rs`, inside the existing `#[cfg(test)] mod tests` block, add after `admit_inserts_and_dedupes_against_database` (which ends right before the final closing braces of the file):

```rust
    #[tokio::test]
    async fn repeated_failure_duplicate_escalates_importance_and_confidence() {
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping escalation integration test: database unavailable");
            return;
        };

        let mut item = NewMemoryItem::candidate_fact(
            MemoryScope::Experience,
            LOCAL_OPERATOR_SCOPE_KEY,
            "deploy times out: retry without exponential backoff",
        );
        item.kind = MemoryKind::FailurePattern;
        item.confidence = 0.5;
        item.importance = 0.5;

        let first = admit_memory_item(&pool, item.clone())
            .await
            .expect("admit 1");
        let AdmitOutcome::Inserted(inserted) = first else {
            panic!("expected first admit to insert a new row");
        };

        let second = admit_memory_item(&pool, item).await.expect("admit 2");
        assert!(matches!(
            second,
            AdmitOutcome::Duplicate { existing_id } if existing_id == inserted.id
        ));

        let escalated = evohime_storage::get_memory_item(&pool, inserted.id)
            .await
            .expect("load escalated row")
            .expect("row still exists");
        assert!(escalated.confidence > inserted.confidence);
        assert!(escalated.confidence <= FAILURE_CONFIDENCE_CAP);
        assert!(escalated.importance > inserted.importance);

        let _ = evohime_storage::delete_memory_item(&pool, inserted.id).await;
    }

    #[tokio::test]
    async fn duplicate_of_already_accepted_failure_lesson_is_not_escalated() {
        // `load_existing` only loads Candidate/Active/Conflict rows (a Rejected row is
        // invisible to dedup matching, so a duplicate of a Rejected lesson would insert
        // fresh rather than hit `Duplicate` at all). Active is the reachable "operator
        // already decided, don't silently touch it" case worth guarding here.
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping escalation integration test: database unavailable");
            return;
        };

        let mut item = NewMemoryItem::candidate_fact(
            MemoryScope::Experience,
            LOCAL_OPERATOR_SCOPE_KEY,
            "migration fails when backup file is missing",
        );
        item.kind = MemoryKind::VerificationRule;
        item.confidence = 0.5;
        item.importance = 0.5;

        let first = admit_memory_item(&pool, item.clone())
            .await
            .expect("admit 1");
        let AdmitOutcome::Inserted(inserted) = first else {
            panic!("expected first admit to insert a new row");
        };
        accept_memory_item(&pool, inserted.id)
            .await
            .expect("accept")
            .expect("row existed");

        let second = admit_memory_item(&pool, item).await.expect("admit 2");
        assert!(matches!(
            second,
            AdmitOutcome::Duplicate { existing_id } if existing_id == inserted.id
        ));

        let untouched = evohime_storage::get_memory_item(&pool, inserted.id)
            .await
            .expect("load")
            .expect("row still exists");
        assert_eq!(untouched.confidence, inserted.confidence);
        assert_eq!(untouched.importance, inserted.importance);

        let _ = evohime_storage::delete_memory_item(&pool, inserted.id).await;
    }
```

No new import is needed for `FAILURE_CONFIDENCE_CAP` — it's already re-exported at the crate root (`pub use extract::{..., FAILURE_CONFIDENCE_CAP, ...};` in `lib.rs`), and the test module's existing `use super::*;` already pulls it into scope.

- [ ] **Step 2: Run tests to verify they fail (or skip cleanly without a DB)**

Run: `cargo test -p evohime-memory repeated_failure_duplicate_escalates -- --nocapture`
Expected: if `DATABASE_URL`/integration DB is reachable, FAIL because escalation isn't wired yet (`escalated.confidence > inserted.confidence` fails, both equal). If no DB reachable, the test prints "skipping escalation integration test: database unavailable" and passes trivially — that's expected either way at this step; the important failure to see (when a DB is available) is the assertion failure, confirming the test exercises real behavior.

- [ ] **Step 3: Implement**

In `crates/memory/src/service.rs`, add an import right after the existing `use` block (after `use uuid::Uuid;`):

```rust
use crate::feedback_service::record_memory_repeated;
```

Add a new private helper function right after the `MemoryService::evaluate` method's closing brace (i.e., right before the `#[derive(Debug, Clone)] pub enum Evaluation { ... }` block):

```rust
/// Escalate a duplicate of an experience failure-lesson (7.103 wave 2): a repeated
/// failure_pattern/verification_rule is stronger evidence than a first guess. Only
/// touches rows still awaiting an operator decision (`Candidate`) — an already
/// accepted/rejected/conflicted row keeps the operator's call. Confidence is capped
/// by `FeedbackSignal::Repeated` itself, so this can never unlock auto-promote.
async fn escalate_repeated_failure_lesson(
    pool: &PgPool,
    prepared: &PreparedMemoryItem,
    existing: &[ExistingMemory],
    existing_id: Uuid,
) {
    if prepared.item.scope != MemoryScope::Experience {
        return;
    }
    let Some(hit) = existing.iter().find(|item| item.id == existing_id) else {
        return;
    };
    if hit.status != MemoryStatus::Candidate {
        return;
    }
    if !matches!(
        hit.kind,
        MemoryKind::FailurePattern | MemoryKind::VerificationRule
    ) {
        return;
    }
    if let Err(error) =
        record_memory_repeated(pool, existing_id, prepared.item.source_task_id).await
    {
        tracing::warn!(%existing_id, %error, "failure-lesson repeat escalation failed");
    }
}
```

Change the `Evaluation::Duplicate` arm inside `admit_memory_item` from:

```rust
    match MemoryService::evaluate(&prepared, &existing)? {
        Evaluation::Duplicate { existing_id } => Ok(AdmitOutcome::Duplicate { existing_id }),
```

to:

```rust
    match MemoryService::evaluate(&prepared, &existing)? {
        Evaluation::Duplicate { existing_id } => {
            escalate_repeated_failure_lesson(pool, &prepared, &existing, existing_id).await;
            Ok(AdmitOutcome::Duplicate { existing_id })
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p evohime-memory repeated_failure_duplicate_escalates -- --nocapture` and `cargo test -p evohime-memory duplicate_of_already_accepted_failure_lesson_is_not_escalated -- --nocapture`
Expected: PASS (or clean skip if no integration DB is configured — check `DATABASE_URL` is set per `AGENTS.md`; if `evohime-storage`'s `connect_integration_pool` needs a running Postgres, start it via `.\start-dev.ps1` prerequisites or `docker compose -f .devcontainer/docker-compose.yml up -d` first so the test actually exercises the new code path instead of skipping).

- [ ] **Step 5: Run the full memory crate test suite**

Run: `cargo test -p evohime-memory`
Expected: PASS, no regressions in dedupe/conflict/admit/retrieve/extract tests.

- [ ] **Step 6: Commit**

```bash
git add crates/memory/src/service.rs crates/memory/src/lib.rs
git commit -m "feat(memory): escalate repeated failure-lesson duplicates"
```

---

### Task 3a: Allow `'repeated'` in the feedback signal CHECK constraint

**Discovered during Task 3 real-database verification:** the implementer's first pass reported tests passing, but they had silently skipped (no reachable integration DB in that run). Once a real local Postgres was verified and used, `record_memory_repeated` failed on every call: `memory_feedback_events` has `CONSTRAINT memory_feedback_signal_check CHECK (signal IN ('used', 'helpful', 'harmful', 'corrected', 'rejected', 'idle_decay'))` (`migrations/0016_memory_feedback.sql:26-28`) — `'repeated'` is not in that list. The failed `INSERT` rolls back the whole transaction in `apply_memory_item_feedback` (`crates/storage/src/memory.rs`), so the `UPDATE ... SET confidence=..., importance=...` on `memory_items` never commits either. Escalation silently no-ops in production exactly like it did in the unverified test run — this is not a test-only gap, it is a real correctness bug that had to be caught with an actual database.

**Files:**
- Create: `migrations/0028_memory_feedback_repeated_signal.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Allow the 'repeated' feedback signal (7.103 wave 2: failure-lesson escalation).

ALTER TABLE memory_feedback_events
    DROP CONSTRAINT IF EXISTS memory_feedback_signal_check;

ALTER TABLE memory_feedback_events
    ADD CONSTRAINT memory_feedback_signal_check CHECK (
        signal IN ('used', 'helpful', 'harmful', 'corrected', 'rejected', 'idle_decay', 'repeated')
    );
```

- [ ] **Step 2: Apply against a real local Postgres and re-run Task 3's integration tests**

This step requires an actual reachable Postgres (`DATABASE_URL`, default `postgres://evohime:evohime@localhost:5432/evohime`) — not the soft-skip path. Set `EVOHIME_REQUIRE_DB=1` so a connection/migration failure panics instead of silently skipping (per `crates/storage/src/test_db.rs`'s documented behavior).

Run: `DATABASE_URL="postgres://evohime:evohime@localhost:5432/evohime" EVOHIME_REQUIRE_DB=1 cargo test -p evohime-memory repeated_failure_duplicate_escalates -- --nocapture`
Expected: PASS — `escalated.confidence > inserted.confidence`, `escalated.confidence <= FAILURE_CONFIDENCE_CAP`, `escalated.importance > inserted.importance` all hold now that the `INSERT` into `memory_feedback_events` succeeds and the transaction commits.

Run: `DATABASE_URL="postgres://evohime:evohime@localhost:5432/evohime" EVOHIME_REQUIRE_DB=1 cargo test -p evohime-memory duplicate_of_already_accepted_failure_lesson_is_not_escalated -- --nocapture`
Expected: PASS.

If `EVOHIME_REQUIRE_DB=1` panics with a connection error (not a migration/constraint error), stop and report BLOCKED — that means no local Postgres is reachable in this environment and the two integration tests cannot be verified for real; do not report DONE with only the soft-skip path exercised.

- [ ] **Step 3: Run the full memory crate suite against the real DB once more**

Run: `DATABASE_URL="postgres://evohime:evohime@localhost:5432/evohime" EVOHIME_REQUIRE_DB=1 cargo test -p evohime-memory`
Expected: PASS, no regressions — this also re-verifies the pre-existing `admit_inserts_and_dedupes_against_database` test actually exercises the DB path this time.

- [ ] **Step 4: Commit**

```bash
git add migrations/0028_memory_feedback_repeated_signal.sql
git commit -m "fix(memory): allow repeated signal in feedback CHECK constraint"
```

---

### Task 4: Retrieval priority for failure lessons

**Files:**
- Modify: `crates/memory/src/retrieve.rs`

**Interfaces:**
- Consumes: existing `score_item(query, query_embedding, item, item_embedding) -> f64` (unchanged signature).
- Produces: no new public API — pure scoring-behavior change inside the existing function.

- [ ] **Step 1: Write the failing test**

In `crates/memory/src/retrieve.rs`, inside the existing `#[cfg(test)] mod tests` block, add after `active_fact_outranks_experience_candidate_when_query_ties` (the last test, right before the closing `}` of the `mod tests` block):

```rust
    #[tokio::test]
    async fn failure_lessons_outrank_success_patterns_at_equal_relevance() {
        let mut failure = sample_item(
            Uuid::new_v4(),
            "timeout when deploying: retry without backoff",
            "candidate",
            0.5,
            false,
        );
        failure.scope = "experience".into();
        failure.kind = MemoryKind::FailurePattern.as_str().into();

        let mut success = sample_item(
            Uuid::new_v4(),
            "timeout when deploying: retry without backoff",
            "candidate",
            0.5,
            false,
        );
        success.scope = "experience".into();
        success.kind = MemoryKind::SuccessPattern.as_str().into();

        let failure_id = failure.id;
        let ranked = search_memory("timeout deploying", &[success, failure], 5).await;
        assert_eq!(ranked[0].item.id, failure_id);
        assert!(ranked[0].score > ranked[1].score);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p evohime-memory failure_lessons_outrank_success_patterns -- --nocapture`
Expected: FAIL — `ranked[0].score > ranked[1].score` is false (both currently score identically, tie-broken arbitrarily or by insertion order, not guaranteed to put failure first).

- [ ] **Step 3: Implement**

In `crates/memory/src/retrieve.rs`, change this block inside `score_item`:

```rust
    let kind = MemoryKind::parse(&item.kind);
    let scope = MemoryScope::parse(&item.scope);
    if scope == Some(MemoryScope::Experience)
        || matches!(
            kind,
            Some(
                MemoryKind::SuccessPattern
                    | MemoryKind::FailurePattern
                    | MemoryKind::VerificationRule
                    | MemoryKind::Playbook
            )
        )
    {
        // Between active facts and weak candidates.
        score += 0.3;
    }
```

to:

```rust
    let kind = MemoryKind::parse(&item.kind);
    let scope = MemoryScope::parse(&item.scope);
    let is_experience = scope == Some(MemoryScope::Experience)
        || matches!(
            kind,
            Some(
                MemoryKind::SuccessPattern
                    | MemoryKind::FailurePattern
                    | MemoryKind::VerificationRule
                    | MemoryKind::Playbook
            )
        );
    if is_experience {
        // Between active facts and weak candidates.
        score += 0.3;
        // Lessons that prevent a repeat mistake outrank generic playbooks/success
        // patterns at equal relevance (7.103 wave 2).
        if matches!(
            kind,
            Some(MemoryKind::FailurePattern | MemoryKind::VerificationRule)
        ) {
            score += 0.2;
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p evohime-memory failure_lessons_outrank_success_patterns -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Run the full retrieve test module**

Run: `cargo test -p evohime-memory retrieve::`
Expected: PASS, all existing ranking tests (pinned-first, semantic paraphrase, budget truncation, playbook suggestions, active-fact-vs-experience tie) still green — the new bonus only affects `FailurePattern`/`VerificationRule` relative to other experience kinds, not relative to non-experience facts.

- [ ] **Step 6: Commit**

```bash
git add crates/memory/src/retrieve.rs
git commit -m "feat(memory): prioritize failure lessons in retrieval ranking"
```

---

### Task 5: Full checks and docs

**Files:**
- Modify: `AGENTS.md`
- Modify: `docs/roadmap.md`
- Modify: `docs/current-state.md`

- [ ] **Step 1: Full workspace checks**

Run: `cargo test --workspace`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings/errors.

- [ ] **Step 2: Update `AGENTS.md`**

In `AGENTS.md`, in the "Incomplete / next" bullet that currently ends with:

```
`7.103` wave 1 ✅ — обучение на провалах: `extract_failure_candidates` (≤2 урока `failure_pattern`/`verification_rule`, confidence cap 0.6 — только Ask, без auto-promote), `FAILURE_EXTRACT_PROMPT` в post-task extract
```

append (same bullet, comma-separated continuation):

```
; wave 2 ✅ — эскалация повторов: `FeedbackSignal::Repeated` поднимает confidence (жёсткий кап 0.6, auto-promote по-прежнему невозможен) и importance (без верхнего капа) существующей experience-записи при повторном admit-дубликате `failure_pattern`/`verification_rule` в статусе `Candidate`; retrieval даёт этим двум kind'ам дополнительный бонус ранжирования над `success_pattern`/`playbook`
```

In the roadmap table row `| 7 Hardening + Product | 🟡 In progress; ... 7.103 wave 1 (failure learning) done |`, change to reference wave 2 done as well:

```
| 7 Hardening + Product | 🟡 In progress; `7.1`–`7.102` complete, `7.103` waves 1–2 done |
```

- [ ] **Step 3: Update `docs/roadmap.md`**

Find the `7.103` row (currently ending `остаток — эскалация повторов, retrieval-приоритизация уроков`) and change the trailing clause to:

```
wave 1 ✅: обучение на провалах — ограниченная полоса extract (≤2 кандидата `failure_pattern`/`verification_rule`, scope experience, confidence cap 0.6 → только Ask-гейт, без auto-promote); harmful-фидбек использованной памяти уже был; wave 2 ✅: эскалация confidence/importance при повторе через `FeedbackSignal::Repeated` (confidence кап 0.6 не снимается) и retrieval-бонус для failure_pattern/verification_rule над success_pattern/playbook
```

Update the status cell for this row from `🟡` to `✅` if this was the last open item for `7.103`; otherwise leave `🟡` if other sub-items remain tracked elsewhere in the same row (check the row's full text before editing — only flip the status marker if no other open sub-bullet remains under `7.103`).

- [ ] **Step 4: Update `docs/current-state.md`**

Find the `7.103` sentence (currently ending `Остаток `7.103` — эскалация повторяющихся паттернов и retrieval-приоритизация уроков.`) and append:

```
Wave 2 закрыла остаток: `FeedbackSignal::Repeated` эскалирует confidence (кап 0.6, auto-promote из провала по-прежнему невозможен) и importance существующей experience-записи при повторном admit-дубликате failure_pattern/verification_rule в статусе Candidate; `score_item` даёт этим двум kind'ам дополнительный ранжирующий бонус над success_pattern/playbook. `7.103` закрыт.
```

Also update the summary line `1. **Stage 7** — Waves A–D ✅; Wave E `7.84`–`7.98` ✅; `7.99`–`7.102` ✅; `7.103` wave 1 ✅` to:

```
1. **Stage 7** — Waves A–D ✅; Wave E `7.84`–`7.98` ✅; `7.99`–`7.102` ✅; `7.103` ✅
```

- [ ] **Step 5: Commit**

```bash
git add AGENTS.md docs/roadmap.md docs/current-state.md
git commit -m "docs: mark failure-learning wave 2 complete (7.103 closed)"
```
