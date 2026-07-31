# Multi-Model Review Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a reusable backend engine that runs an artifact (a spec or a plan, as plain text) through a configurable pool of reviewer LLMs, synthesizes their feedback into one report, has the main model revise the artifact, self-checks the revision, and persists the whole round — plus the Settings UI to configure the reviewer pool and synthesizer.

**Architecture:** `crates/agent-runtime/src/review.rs` implements the engine: reviewers run strictly sequentially over the existing `ModelGateway` (never in parallel — rate limits), each reviewing the artifact from scratch with no visibility into other reviewers' output. A synthesizer model merges all reviewer output into one report. The main model then revises the artifact and silently self-checks its own revision in a bounded loop (safety cap, not a cost optimization — see Global Constraints). Reviewer/synthesizer models are configured as ordinary named routes (`reviewer_0..reviewer_{N-1}`, `synthesizer`) in the existing `ModelGatewayConfig.routes` map — the same mechanism already used for the `"orchestrator"` route — so no new route-resolution machinery is needed. Each round is persisted to a new `artifact_reviews` table, modeled directly on `planning_history`. A new Settings → "Планирование" tab lets the user pick reviewer count (1–5) and each reviewer's + the synthesizer's provider/model, reusing the existing `ModelRouteDraft` editing pattern.

**Tech Stack:** Rust (agent-runtime, storage, server crates), sqlx/Postgres, React + TypeScript (frontend/web).

**Out of scope (follow-up plan):** wiring this engine into the task lifecycle — the "Review" button, pausable spec/plan draft states, HTTP endpoint to trigger a round, and any protocol/`ServerEvent` changes. This plan produces a standalone, unit-testable engine plus its settings UI; nothing calls it from a running task yet.

## Global Constraints

- Reviewers run **strictly sequentially**, never concurrently (all providers have rate limits/delays).
- Each reviewer reviews the artifact **from scratch**; reviewers never see other reviewers' output.
- Reviewer call failure: retry **exactly once** (2 attempts total). If both fail, mark that reviewer `failed: true` and continue the round without it.
- If **every** reviewer fails in a round, the round errors out (`ReviewEngineError::AllReviewersFailed`) — there is nothing to synthesize.
- The self-check loop (main model re-checking its own revision) is silent/internal and bounded by `max_self_check_iterations` — this is a stability safety cap against runaway loops, not a cost/latency optimization (time/cost are explicitly not a concern for this feature).
- If the self-check step itself fails (gateway error, or the model replies without calling the expected tool), the loop stops and keeps the last good revision instead of discarding the round — a misbehaving self-check must never lose already-produced work.
- One reviewer pool + one synthesizer configuration is shared by every artifact kind (spec review and plan review both use it) — no per-kind config.
- Reviewer/synthesizer model routes are named `reviewer_0`, `reviewer_1`, ... `reviewer_{N-1}`, and `synthesizer` inside the existing `ModelGatewayConfig.routes: HashMap<String, ModelRouteConfig>` — reuse the existing named-route mechanism (`"orchestrator"` precedent in `crates/server/src/models_api.rs`), do not add a parallel config system.
- `artifact_reviews` table follows the exact `planning_history` pattern: any jsonb column holding an array must be bound as a single `Value::Array(...)`, not relied on to auto-encode as a Postgres array (see `crates/storage/src/planning_history.rs:66-74` for the bug this avoids).

---

## Task 1: Storage — `artifact_reviews` table + repo module

**Files:**
- Create: `migrations/0037_artifact_reviews.sql`
- Create: `crates/storage/src/artifact_reviews.rs`
- Modify: `crates/storage/src/lib.rs` (register module + re-exports, add `StorageError::InvalidArtifactReview` variant)

**Interfaces:**
- Produces: `ReviewArtifactKind::{Spec, Plan}` (with `as_str() -> &'static str`), `ReviewerCommentEntry { route_name: String, comments: String, failed: bool }`, `ArtifactReviewRow`, `NewArtifactReview`, `insert_artifact_review(pool, NewArtifactReview) -> Result<ArtifactReviewRow, StorageError>`, `list_artifact_reviews_by_task(pool, task_id: Uuid) -> Result<Vec<ArtifactReviewRow>, StorageError>` — Task 5 consumes these.

- [ ] **Step 1: Write the migration**

```sql
-- migrations/0037_artifact_reviews.sql
CREATE TABLE IF NOT EXISTS artifact_reviews (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id uuid NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    session_id uuid NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    artifact_kind text NOT NULL CHECK (artifact_kind IN ('spec', 'plan')),
    round_number integer NOT NULL CHECK (round_number >= 1),
    original_content text NOT NULL,
    reviewer_comments jsonb NOT NULL, -- array of {route_name, comments, failed}
    synthesized_feedback text NOT NULL,
    revised_content text NOT NULL,
    self_check_iterations integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS artifact_reviews_task_id_idx ON artifact_reviews (task_id);
```

- [ ] **Step 2: Write `crates/storage/src/artifact_reviews.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewArtifactKind {
    Spec,
    Plan,
}

impl ReviewArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spec => "spec",
            Self::Plan => "plan",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerCommentEntry {
    pub route_name: String,
    pub comments: String,
    pub failed: bool,
}

/// One persisted review round for a task's spec or plan.
#[derive(Debug, Clone, FromRow)]
pub struct ArtifactReviewRow {
    pub id: Uuid,
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub artifact_kind: String,
    pub round_number: i32,
    pub original_content: String,
    pub reviewer_comments: Value, // JSON array of ReviewerCommentEntry
    pub synthesized_feedback: String,
    pub revised_content: String,
    pub self_check_iterations: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewArtifactReview {
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub artifact_kind: ReviewArtifactKind,
    pub round_number: i32,
    pub original_content: String,
    pub reviewer_comments: Vec<ReviewerCommentEntry>,
    pub synthesized_feedback: String,
    pub revised_content: String,
    pub self_check_iterations: i32,
}

pub async fn insert_artifact_review(
    pool: &PgPool,
    entry: NewArtifactReview,
) -> Result<ArtifactReviewRow, StorageError> {
    if entry.round_number < 1 {
        return Err(StorageError::InvalidArtifactReview(format!(
            "round_number must be >= 1, got {}",
            entry.round_number
        )));
    }

    // The column is a scalar jsonb holding a JSON array, not a Postgres
    // array of jsonb — bind one Value::Array, same fix as planning_history.
    let comments_json: Value = Value::Array(
        entry
            .reviewer_comments
            .iter()
            .map(|comment| serde_json::to_value(comment).unwrap_or(Value::Null))
            .collect(),
    );

    let row = sqlx::query_as::<_, ArtifactReviewRow>(
        r#"
        INSERT INTO artifact_reviews
            (task_id, session_id, artifact_kind, round_number, original_content,
             reviewer_comments, synthesized_feedback, revised_content, self_check_iterations)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, task_id, session_id, artifact_kind, round_number, original_content,
                  reviewer_comments, synthesized_feedback, revised_content, self_check_iterations, created_at
        "#,
    )
    .bind(entry.task_id)
    .bind(entry.session_id)
    .bind(entry.artifact_kind.as_str())
    .bind(entry.round_number)
    .bind(entry.original_content)
    .bind(&comments_json)
    .bind(entry.synthesized_feedback)
    .bind(entry.revised_content)
    .bind(entry.self_check_iterations)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn list_artifact_reviews_by_task(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Vec<ArtifactReviewRow>, StorageError> {
    let rows = sqlx::query_as::<_, ArtifactReviewRow>(
        r#"
        SELECT id, task_id, session_id, artifact_kind, round_number, original_content,
               reviewer_comments, synthesized_feedback, revised_content, self_check_iterations, created_at
        FROM artifact_reviews
        WHERE task_id = $1
        ORDER BY round_number ASC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::connect_integration_pool;

    async fn seed_session_and_task(pool: &PgPool) -> (Uuid, Uuid) {
        let session = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000001'::uuid) RETURNING id",
        )
        .fetch_one(pool)
        .await
        .expect("create session");

        let task = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(pool)
        .await
        .expect("create task");

        (session, task)
    }

    #[tokio::test]
    async fn insert_and_list_artifact_reviews() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping artifact_reviews test: database unavailable");
            return;
        };
        let (session, task) = seed_session_and_task(&pool).await;

        let entry = NewArtifactReview {
            task_id: task,
            session_id: session,
            artifact_kind: ReviewArtifactKind::Plan,
            round_number: 1,
            original_content: "step 1\nstep 2".to_string(),
            reviewer_comments: vec![
                ReviewerCommentEntry {
                    route_name: "reviewer_0".to_string(),
                    comments: "missing error handling".to_string(),
                    failed: false,
                },
                ReviewerCommentEntry {
                    route_name: "reviewer_1".to_string(),
                    comments: String::new(),
                    failed: true,
                },
            ],
            synthesized_feedback: "Add error handling to step 2.".to_string(),
            revised_content: "step 1\nstep 2 (with error handling)".to_string(),
            self_check_iterations: 1,
        };

        let inserted = insert_artifact_review(&pool, entry)
            .await
            .expect("insert artifact review");

        assert_eq!(inserted.task_id, task);
        assert_eq!(inserted.artifact_kind, "plan");
        assert_eq!(inserted.round_number, 1);
        assert_eq!(inserted.reviewer_comments.as_array().map(Vec::len), Some(2));

        let rows = list_artifact_reviews_by_task(&pool, task)
            .await
            .expect("list artifact reviews");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, inserted.id);
    }

    #[tokio::test]
    async fn rejects_round_number_below_one() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping round_number validation test: database unavailable");
            return;
        };
        let (session, task) = seed_session_and_task(&pool).await;

        let entry = NewArtifactReview {
            task_id: task,
            session_id: session,
            artifact_kind: ReviewArtifactKind::Spec,
            round_number: 0,
            original_content: "spec".to_string(),
            reviewer_comments: vec![],
            synthesized_feedback: String::new(),
            revised_content: "spec".to_string(),
            self_check_iterations: 0,
        };

        let error = insert_artifact_review(&pool, entry)
            .await
            .expect_err("round_number 0 should be rejected");
        assert!(matches!(error, StorageError::InvalidArtifactReview(_)));
    }
}
```

- [ ] **Step 3: Register the module in `crates/storage/src/lib.rs`**

Add to the `pub mod` block (alphabetically, after `apply...`/before `attachments`... follow existing alpha order at line 8):

```rust
pub mod artifact_reviews;
```

Add a re-export next to the `planning_history` re-export (around line 70):

```rust
pub use artifact_reviews::{
    insert_artifact_review, list_artifact_reviews_by_task, ArtifactReviewRow, NewArtifactReview,
    ReviewArtifactKind, ReviewerCommentEntry,
};
```

Add a new `StorageError` variant next to `InvalidPlanningHistory` (line 118):

```rust
    #[error("invalid artifact review: {0}")]
    InvalidArtifactReview(String),
```

- [ ] **Step 4: Run the storage tests**

Run: `cargo test -p evohime-storage artifact_reviews`
Expected: PASS (or "skipping ... database unavailable" printed twice if no local Postgres — that's an acceptable pass, matching the existing `planning_history` test convention).

- [ ] **Step 5: Commit**

```bash
git add migrations/0037_artifact_reviews.sql crates/storage/src/artifact_reviews.rs crates/storage/src/lib.rs
git commit -m "feat(storage): add artifact_reviews table for multi-model review rounds"
```

---

## Task 2: Server — allow `reviewer_*`/`synthesizer` routes to inherit the default API key

**Files:**
- Modify: `crates/server/src/models_api.rs:264-286` (`build_model_config`)

**Interfaces:**
- Consumes: `ModelSettingsRequest.routes: Vec<ModelRouteRequest>` (unchanged shape) — the frontend (Task 6) will send entries named `reviewer_0`, `reviewer_1`, ..., `synthesizer` through the exact same `PUT` endpoint already used for `"orchestrator"`.
- Produces: `inherits_default_key(name: &str) -> bool` helper, reused by `build_model_config`.

Today only the literal route name `"orchestrator"` inherits the default route's API key when the user leaves the key field blank (see the `(name == "orchestrator").then(...)` closure and the `if name == "orchestrator"` branch at `models_api.rs:269` and `:279`). Reviewer routes are dynamically named (`reviewer_0`..`reviewer_4`) so the same convenience needs a predicate instead of a literal match.

- [ ] **Step 1: Add the predicate and use it in both branches**

In `crates/server/src/models_api.rs`, add near the top of `build_model_config` (before the `for route in request.routes` loop, i.e. just after line 222):

```rust
    fn inherits_default_key(name: &str) -> bool {
        name == "orchestrator" || name == "synthesizer" || name.starts_with("reviewer_")
    }
```

Replace the existing_key lookup (currently lines 264-278):

```rust
        let existing_key = current
            .routes
            .get(&name)
            .map(|item| item.literouter.api_key.clone())
            .or_else(|| {
                inherits_default_key(&name).then(|| {
                    current
                        .routes
                        .get(&current.default_route)
                        .map(|item| item.literouter.api_key.clone())
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| requested_default_key.clone())
                })
            })
            .unwrap_or_default();
        let api_key = if inherits_default_key(&name) {
            requested_default_key.clone()
        } else {
            route
                .api_key
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(existing_key)
        };
```

- [ ] **Step 2: Write a test mirroring `carries_default_api_key_to_new_orchestrator_route`**

Add to the `#[cfg(test)] mod tests` block in the same file:

```rust
    #[test]
    fn carries_default_api_key_to_new_reviewer_and_synthesizer_routes() {
        let current = evohime_model_gateway::ModelGatewayConfig {
            default_route: "default".to_string(),
            routes: HashMap::from([(
                "default".to_string(),
                ModelRouteConfig::literouter("", "https://api.literouter.com/v1", "deepseek:free"),
            )]),
        };
        let config = build_model_config(
            ModelSettingsRequest {
                default_route: "default".to_string(),
                routes: vec![
                    ModelRouteRequest {
                        name: "default".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: Some("lr_test_key".to_string()),
                        billing_mode: "free".to_string(),
                    },
                    ModelRouteRequest {
                        name: "reviewer_0".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: None,
                        billing_mode: "free".to_string(),
                    },
                    ModelRouteRequest {
                        name: "synthesizer".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: None,
                        billing_mode: "free".to_string(),
                    },
                ],
            },
            &current,
        )
        .expect("model config is valid");

        assert_eq!(config.routes["reviewer_0"].literouter.api_key, "lr_test_key");
        assert_eq!(config.routes["synthesizer"].literouter.api_key, "lr_test_key");
    }
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p evohime-server models_api`
Expected: PASS, including the new `carries_default_api_key_to_new_reviewer_and_synthesizer_routes` test and the two pre-existing tests unmodified.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/models_api.rs
git commit -m "feat(server): let reviewer_*/synthesizer model routes inherit the default API key"
```

---

## Task 3: agent-runtime — review engine types + sequential reviewer loop

**Files:**
- Create: `crates/agent-runtime/src/review.rs`
- Modify: `crates/agent-runtime/src/lib.rs` (register module — mirror however `planning` is registered)

**Interfaces:**
- Produces: `ArtifactKind::{Spec, Plan}`, `ReviewerRoute { route_name: String, model: Option<String> }`, `ReviewerComment { route_name: String, comments: String, failed: bool }`, `ReviewEngineError`, `pub(crate) async fn call_reviewer(gateway, reviewer: &ReviewerRoute, artifact_kind: ArtifactKind, content: &str) -> ReviewerComment` — consumed by Task 4's `run_review_round`.

First, check how `crates/agent-runtime/src/lib.rs` registers existing sibling modules (e.g. `planning`) and mirror that exactly — do not guess the visibility/re-export pattern.

- [ ] **Step 1: Write `crates/agent-runtime/src/review.rs`**

```rust
//! Multi-model review engine: sequential reviewers -> synthesizer -> reviser -> self-check.
//! See docs/superpowers/plans/2026-07-31T1600-multi-model-review-engine.md.

use evohime_model_gateway::providers::{ChatMessage, ChatRole, ProviderError};
use evohime_model_gateway::ModelGateway;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Spec,
    Plan,
}

impl ArtifactKind {
    pub fn label(self) -> &'static str {
        match self {
            ArtifactKind::Spec => "specification",
            ArtifactKind::Plan => "implementation plan",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReviewerRoute {
    pub route_name: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewerComment {
    pub route_name: String,
    pub comments: String,
    pub failed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewEngineError {
    #[error("no reviewers configured")]
    NoReviewers,
    #[error("all reviewers failed for this round")]
    AllReviewersFailed,
    #[error("model gateway error: {0}")]
    Gateway(#[from] ProviderError),
    #[error("storage error: {0}")]
    Storage(#[from] evohime_storage::StorageError),
}

/// Calls one reviewer with the artifact, from scratch (no visibility into
/// other reviewers). Retries exactly once; on a second failure returns a
/// `failed: true` comment instead of propagating the error, so one bad
/// reviewer never aborts the round (Global Constraints).
pub(crate) async fn call_reviewer(
    gateway: &ModelGateway,
    reviewer: &ReviewerRoute,
    artifact_kind: ArtifactKind,
    content: &str,
) -> ReviewerComment {
    let messages = vec![
        ChatMessage::text(
            ChatRole::System,
            format!(
                "You are an independent reviewer critiquing a software {}. \
                 List concrete issues, gaps, and risks, one per line. \
                 If you find nothing wrong, reply with exactly: NO ISSUES.",
                artifact_kind.label()
            ),
        ),
        ChatMessage::text(ChatRole::User, content),
    ];

    for _attempt in 0..2 {
        if let Ok(result) = gateway
            .chat_with_tools_for_route(&reviewer.route_name, reviewer.model.as_deref(), &messages, &[])
            .await
        {
            return ReviewerComment {
                route_name: reviewer.route_name.clone(),
                comments: result.content,
                failed: false,
            };
        }
    }

    ReviewerComment {
        route_name: reviewer.route_name.clone(),
        comments: String::new(),
        failed: true,
    }
}

/// Runs every configured reviewer strictly sequentially (rate limits — Global
/// Constraints), each reviewing `content` from scratch.
pub(crate) async fn run_reviewers(
    gateway: &ModelGateway,
    reviewer_routes: &[ReviewerRoute],
    artifact_kind: ArtifactKind,
    content: &str,
) -> Result<Vec<ReviewerComment>, ReviewEngineError> {
    if reviewer_routes.is_empty() {
        return Err(ReviewEngineError::NoReviewers);
    }

    let mut comments = Vec::with_capacity(reviewer_routes.len());
    for reviewer in reviewer_routes {
        comments.push(call_reviewer(gateway, reviewer, artifact_kind, content).await);
    }

    if comments.iter().all(|comment| comment.failed) {
        return Err(ReviewEngineError::AllReviewersFailed);
    }

    Ok(comments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_model_gateway::providers::mock::MockProvider;
    use evohime_model_gateway::tools::ChatResult;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn gateway_with_routes(routes: Vec<(&str, MockProvider)>) -> ModelGateway {
        let mut map: HashMap<String, Arc<dyn evohime_model_gateway::providers::ModelProvider>> =
            HashMap::new();
        for (name, provider) in routes {
            map.insert(name.to_string(), Arc::new(provider));
        }
        ModelGateway::from_routes("reviewer_0", map)
    }

    #[tokio::test]
    async fn collects_comments_from_all_reviewers() {
        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult {
                        content: "missing tests".into(),
                        ..Default::default()
                    }],
                ),
            ),
            (
                "reviewer_1",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult {
                        content: "NO ISSUES".into(),
                        ..Default::default()
                    }],
                ),
            ),
        ]);
        let routes = vec![
            ReviewerRoute { route_name: "reviewer_0".into(), model: None },
            ReviewerRoute { route_name: "reviewer_1".into(), model: None },
        ];

        let comments = run_reviewers(&gateway, &routes, ArtifactKind::Plan, "plan text")
            .await
            .expect("reviewers run");

        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].comments, "missing tests");
        assert!(!comments[0].failed);
        assert_eq!(comments[1].comments, "NO ISSUES");
    }

    #[tokio::test]
    async fn skips_a_reviewer_after_two_failed_attempts() {
        // Route "reviewer_0" has no provider registered at all, so every
        // chat_with_tools_for_route call errors with "unknown model route" —
        // this simulates a reviewer that fails both attempts.
        let gateway = gateway_with_routes(vec![(
            "reviewer_1",
            MockProvider::with_tool_call_sequence(
                "m",
                vec![ChatResult { content: "ok".into(), ..Default::default() }],
            ),
        )]);
        let routes = vec![
            ReviewerRoute { route_name: "reviewer_0".into(), model: None },
            ReviewerRoute { route_name: "reviewer_1".into(), model: None },
        ];

        let comments = run_reviewers(&gateway, &routes, ArtifactKind::Plan, "plan text")
            .await
            .expect("round still succeeds — one reviewer survives");

        assert!(comments[0].failed);
        assert!(!comments[1].failed);
    }

    #[tokio::test]
    async fn errors_when_every_reviewer_fails() {
        let gateway = gateway_with_routes(vec![]);
        let routes = vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }];

        let error = run_reviewers(&gateway, &routes, ArtifactKind::Plan, "plan text")
            .await
            .expect_err("all reviewers failed");

        assert!(matches!(error, ReviewEngineError::AllReviewersFailed));
    }
}
```

- [ ] **Step 2: Register the module**

Read `crates/agent-runtime/src/lib.rs` and add `pub mod review;` following the exact same visibility as the existing `pub mod planning;` (or equivalent) declaration.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p evohime-agent-runtime review::`
Expected: PASS — `collects_comments_from_all_reviewers`, `skips_a_reviewer_after_two_failed_attempts`, `errors_when_every_reviewer_fails`.

- [ ] **Step 4: Commit**

```bash
git add crates/agent-runtime/src/review.rs crates/agent-runtime/src/lib.rs
git commit -m "feat(agent-runtime): add sequential multi-reviewer loop (review engine, part 1)"
```

---

## Task 4: agent-runtime — synthesizer, reviser, bounded self-check, `run_review_round`

**Files:**
- Modify: `crates/agent-runtime/src/review.rs` (append to the module built in Task 3)

**Interfaces:**
- Consumes: `run_reviewers` from Task 3.
- Produces: `ReviewEngineConfig { reviewer_routes: Vec<ReviewerRoute>, synthesizer_route: ReviewerRoute, main_route: ReviewerRoute, max_self_check_iterations: u32 }`, `ReviewRoundResult { reviewer_comments: Vec<ReviewerComment>, synthesized_feedback: String, revised_content: String, self_check_iterations: u32 }`, `pub async fn run_review_round(gateway: &ModelGateway, config: &ReviewEngineConfig, artifact_kind: ArtifactKind, content: &str) -> Result<ReviewRoundResult, ReviewEngineError>` — consumed by Task 5.

- [ ] **Step 1: Append the config/result types and the pipeline functions to `review.rs`**

```rust
use serde_json::json;

#[derive(Debug, Clone)]
pub struct ReviewEngineConfig {
    pub reviewer_routes: Vec<ReviewerRoute>,
    pub synthesizer_route: ReviewerRoute,
    /// Model used for the reviser + self-check steps (the main agent model).
    pub main_route: ReviewerRoute,
    /// Hard cap on self-check iterations — a stability safety valve, not a
    /// cost optimization (Global Constraints).
    pub max_self_check_iterations: u32,
}

#[derive(Debug, Clone)]
pub struct ReviewRoundResult {
    pub reviewer_comments: Vec<ReviewerComment>,
    pub synthesized_feedback: String,
    pub revised_content: String,
    pub self_check_iterations: u32,
}

struct SelfCheckDecision {
    complete: bool,
    content: String,
}

#[derive(Deserialize)]
struct SelfCheckArgs {
    complete: bool,
    content: String,
}

fn self_check_tool() -> evohime_model_gateway::ToolSpec {
    evohime_model_gateway::ToolSpec::function(
        "submit_self_check",
        "Report whether the revised artifact fully addresses the review report.",
        json!({
            "type": "object",
            "properties": {
                "complete": {
                    "type": "boolean",
                    "description": "true if every issue from the review report is addressed"
                },
                "content": {
                    "type": "string",
                    "description": "the (possibly further-revised) artifact content"
                }
            },
            "required": ["complete", "content"]
        }),
    )
}

async fn synthesize_feedback(
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    artifact_kind: ArtifactKind,
    reviewer_comments: &[ReviewerComment],
) -> Result<String, ReviewEngineError> {
    let joined = reviewer_comments
        .iter()
        .filter(|comment| !comment.failed)
        .enumerate()
        .map(|(index, comment)| format!("Reviewer {}:\n{}", index + 1, comment.comments))
        .collect::<Vec<_>>()
        .join("\n\n");

    let messages = vec![
        ChatMessage::text(
            ChatRole::System,
            format!(
                "You merge multiple independent reviews of a software {} into one report. \
                 Deduplicate overlapping points and keep every substantive issue. \
                 If every reviewer found nothing wrong, reply with exactly: NO ISSUES.",
                artifact_kind.label()
            ),
        ),
        ChatMessage::text(ChatRole::User, joined),
    ];

    let result = gateway
        .chat_with_tools_for_route(
            &config.synthesizer_route.route_name,
            config.synthesizer_route.model.as_deref(),
            &messages,
            &[],
        )
        .await?;
    Ok(result.content)
}

async fn revise_artifact(
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    artifact_kind: ArtifactKind,
    content: &str,
    synthesized_feedback: &str,
) -> Result<String, ReviewEngineError> {
    let messages = vec![
        ChatMessage::text(
            ChatRole::System,
            format!(
                "Revise the {label} below to address every issue in the review report. \
                 Reply with the complete revised {label} only — no preamble, no commentary.",
                label = artifact_kind.label()
            ),
        ),
        ChatMessage::text(
            ChatRole::User,
            format!("--- ORIGINAL ---\n{content}\n\n--- REVIEW REPORT ---\n{synthesized_feedback}"),
        ),
    ];

    let result = gateway
        .chat_with_tools_for_route(
            &config.main_route.route_name,
            config.main_route.model.as_deref(),
            &messages,
            &[],
        )
        .await?;
    Ok(result.content)
}

async fn self_check(
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    artifact_kind: ArtifactKind,
    content: &str,
    synthesized_feedback: &str,
) -> Result<SelfCheckDecision, ReviewEngineError> {
    let tool = self_check_tool();
    let messages = vec![
        ChatMessage::text(
            ChatRole::System,
            format!(
                "Check your own revision of this {label} against the review report. \
                 If anything is still unaddressed, fix it silently and call submit_self_check \
                 with complete=false and the fixed content. If everything is addressed, call \
                 submit_self_check with complete=true and the content unchanged.",
                label = artifact_kind.label()
            ),
        ),
        ChatMessage::text(
            ChatRole::User,
            format!("--- REVISED ---\n{content}\n\n--- REVIEW REPORT ---\n{synthesized_feedback}"),
        ),
    ];

    let result = gateway
        .chat_with_tools_for_route(
            &config.main_route.route_name,
            config.main_route.model.as_deref(),
            &messages,
            std::slice::from_ref(&tool),
        )
        .await?;

    let call = result
        .tool_calls
        .first()
        .ok_or_else(|| ReviewEngineError::Gateway(ProviderError::Api(
            "self-check did not call submit_self_check".into(),
        )))?;
    let parsed: SelfCheckArgs = serde_json::from_str(&call.arguments)
        .map_err(|error| ReviewEngineError::Gateway(ProviderError::Api(error.to_string())))?;

    Ok(SelfCheckDecision { complete: parsed.complete, content: parsed.content })
}

/// Runs one full review round: sequential reviewers -> synthesizer -> reviser
/// -> bounded silent self-check. See Global Constraints for the ordering and
/// failure-handling rules this implements.
pub async fn run_review_round(
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    artifact_kind: ArtifactKind,
    content: &str,
) -> Result<ReviewRoundResult, ReviewEngineError> {
    let reviewer_comments =
        run_reviewers(gateway, &config.reviewer_routes, artifact_kind, content).await?;

    let synthesized_feedback =
        synthesize_feedback(gateway, config, artifact_kind, &reviewer_comments).await?;

    let mut revised_content =
        revise_artifact(gateway, config, artifact_kind, content, &synthesized_feedback).await?;

    let mut self_check_iterations = 0;
    while self_check_iterations < config.max_self_check_iterations {
        // If the self-check call itself fails (gateway error, or the model
        // replies without calling submit_self_check), keep the last good
        // revision and stop instead of discarding the whole round — a
        // misbehaving self-check step must never lose already-good work
        // (Global Constraints: stability over cost/latency).
        let decision = match self_check(
            gateway,
            config,
            artifact_kind,
            &revised_content,
            &synthesized_feedback,
        )
        .await
        {
            Ok(decision) => decision,
            Err(_) => break,
        };
        self_check_iterations += 1;
        revised_content = decision.content;
        if decision.complete {
            break;
        }
    }

    Ok(ReviewRoundResult {
        reviewer_comments,
        synthesized_feedback,
        revised_content,
        self_check_iterations,
    })
}
```

- [ ] **Step 2: Add pipeline tests to the existing `#[cfg(test)] mod tests` block in `review.rs`**

```rust
    #[tokio::test]
    async fn run_review_round_completes_self_check_on_first_pass() {
        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "add tests".into(), ..Default::default() }],
                ),
            ),
            (
                "synthesizer",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "Add tests for step 2.".into(), ..Default::default() }],
                ),
            ),
            (
                "main",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![
                        // 1st call: revise_artifact (plain content)
                        ChatResult { content: "step 1\nstep 2 (with tests)".into(), ..Default::default() },
                        // 2nd call: self_check tool call, complete=true
                        ChatResult {
                            content: String::new(),
                            tool_calls: vec![evohime_model_gateway::NativeToolCall {
                                id: "call_1".into(),
                                name: "submit_self_check".into(),
                                arguments: r#"{"complete":true,"content":"step 1\nstep 2 (with tests)"}"#.into(),
                            }],
                            ..Default::default()
                        },
                    ],
                ),
            ),
        ]);
        let config = ReviewEngineConfig {
            reviewer_routes: vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }],
            synthesizer_route: ReviewerRoute { route_name: "synthesizer".into(), model: None },
            main_route: ReviewerRoute { route_name: "main".into(), model: None },
            max_self_check_iterations: 5,
        };

        let result = run_review_round(&gateway, &config, ArtifactKind::Plan, "step 1\nstep 2")
            .await
            .expect("round succeeds");

        assert_eq!(result.self_check_iterations, 1);
        assert_eq!(result.revised_content, "step 1\nstep 2 (with tests)");
        assert_eq!(result.synthesized_feedback, "Add tests for step 2.");
    }

    #[tokio::test]
    async fn run_review_round_stops_at_max_self_check_iterations() {
        let never_complete = ChatResult {
            content: String::new(),
            tool_calls: vec![evohime_model_gateway::NativeToolCall {
                id: "call_1".into(),
                name: "submit_self_check".into(),
                arguments: r#"{"complete":false,"content":"still working"}"#.into(),
            }],
            ..Default::default()
        };
        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "issue".into(), ..Default::default() }],
                ),
            ),
            (
                "synthesizer",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "fix the issue".into(), ..Default::default() }],
                ),
            ),
            (
                "main",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![
                        ChatResult { content: "revised".into(), ..Default::default() },
                        never_complete.clone(),
                        never_complete,
                    ],
                ),
            ),
        ]);
        let config = ReviewEngineConfig {
            reviewer_routes: vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }],
            synthesizer_route: ReviewerRoute { route_name: "synthesizer".into(), model: None },
            main_route: ReviewerRoute { route_name: "main".into(), model: None },
            max_self_check_iterations: 2,
        };

        let result = run_review_round(&gateway, &config, ArtifactKind::Plan, "content")
            .await
            .expect("round still returns instead of looping forever");

        assert_eq!(result.self_check_iterations, 2);
    }

    #[tokio::test]
    async fn run_review_round_survives_self_check_not_calling_the_tool() {
        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "issue".into(), ..Default::default() }],
                ),
            ),
            (
                "synthesizer",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "fix it".into(), ..Default::default() }],
                ),
            ),
            (
                "main",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![
                        ChatResult { content: "revised once".into(), ..Default::default() },
                        // Self-check replies with plain text instead of calling
                        // submit_self_check — simulates a model that ignores tool_choice.
                        ChatResult { content: "looks fine to me".into(), ..Default::default() },
                    ],
                ),
            ),
        ]);
        let config = ReviewEngineConfig {
            reviewer_routes: vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }],
            synthesizer_route: ReviewerRoute { route_name: "synthesizer".into(), model: None },
            main_route: ReviewerRoute { route_name: "main".into(), model: None },
            max_self_check_iterations: 5,
        };

        let result = run_review_round(&gateway, &config, ArtifactKind::Plan, "content")
            .await
            .expect("round still succeeds even if self-check misbehaves");

        assert_eq!(result.self_check_iterations, 0);
        assert_eq!(result.revised_content, "revised once");
    }
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p evohime-agent-runtime review::`
Expected: PASS — all 6 tests (3 from Task 3, 3 new).

- [ ] **Step 4: Commit**

```bash
git add crates/agent-runtime/src/review.rs
git commit -m "feat(agent-runtime): add synthesizer/reviser/self-check to review engine (part 2)"
```

---

## Task 5: agent-runtime — persist each review round

**Files:**
- Modify: `crates/agent-runtime/src/review.rs` (append)

**Interfaces:**
- Consumes: `run_review_round` (Task 4), `evohime_storage::{insert_artifact_review, NewArtifactReview, ReviewArtifactKind, ReviewerCommentEntry}` (Task 1).
- Produces: `pub async fn run_and_persist_review_round(pool: &PgPool, gateway: &ModelGateway, config: &ReviewEngineConfig, task_id: Uuid, session_id: Uuid, artifact_kind: ArtifactKind, round_number: i32, content: &str) -> Result<ReviewRoundResult, ReviewEngineError>` — this is the entry point the follow-up "wire into task lifecycle" plan will call from the Review button handler.

- [ ] **Step 1: Append to `review.rs`**

```rust
use evohime_storage::{
    insert_artifact_review, NewArtifactReview, ReviewArtifactKind, ReviewerCommentEntry,
};
use sqlx::PgPool;
use uuid::Uuid;

fn to_storage_kind(kind: ArtifactKind) -> ReviewArtifactKind {
    match kind {
        ArtifactKind::Spec => ReviewArtifactKind::Spec,
        ArtifactKind::Plan => ReviewArtifactKind::Plan,
    }
}

/// Runs one review round and persists it to `artifact_reviews`. `round_number`
/// is 1-based and caller-supplied (the caller tracks how many rounds have run
/// for this task+artifact_kind so far).
pub async fn run_and_persist_review_round(
    pool: &PgPool,
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    task_id: Uuid,
    session_id: Uuid,
    artifact_kind: ArtifactKind,
    round_number: i32,
    content: &str,
) -> Result<ReviewRoundResult, ReviewEngineError> {
    let result = run_review_round(gateway, config, artifact_kind, content).await?;

    let entry = NewArtifactReview {
        task_id,
        session_id,
        artifact_kind: to_storage_kind(artifact_kind),
        round_number,
        original_content: content.to_string(),
        reviewer_comments: result
            .reviewer_comments
            .iter()
            .map(|comment| ReviewerCommentEntry {
                route_name: comment.route_name.clone(),
                comments: comment.comments.clone(),
                failed: comment.failed,
            })
            .collect(),
        synthesized_feedback: result.synthesized_feedback.clone(),
        revised_content: result.revised_content.clone(),
        self_check_iterations: result.self_check_iterations as i32,
    };

    insert_artifact_review(pool, entry).await?;

    Ok(result)
}
```

- [ ] **Step 2: Add an integration test**

Append to the `tests` module in `review.rs`:

```rust
    #[tokio::test]
    async fn run_and_persist_review_round_writes_a_row() {
        let Some(pool) = evohime_storage::test_db::connect_integration_pool().await else {
            eprintln!("skipping run_and_persist_review_round test: database unavailable");
            return;
        };

        let session = sqlx::query_scalar::<_, uuid::Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000001'::uuid) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("create session");
        let task = sqlx::query_scalar::<_, uuid::Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(&pool)
        .await
        .expect("create task");

        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "NO ISSUES".into(), ..Default::default() }],
                ),
            ),
            (
                "synthesizer",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "NO ISSUES".into(), ..Default::default() }],
                ),
            ),
            (
                "main",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![
                        ChatResult { content: "plan v2".into(), ..Default::default() },
                        ChatResult {
                            content: String::new(),
                            tool_calls: vec![evohime_model_gateway::NativeToolCall {
                                id: "call_1".into(),
                                name: "submit_self_check".into(),
                                arguments: r#"{"complete":true,"content":"plan v2"}"#.into(),
                            }],
                            ..Default::default()
                        },
                    ],
                ),
            ),
        ]);
        let config = ReviewEngineConfig {
            reviewer_routes: vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }],
            synthesizer_route: ReviewerRoute { route_name: "synthesizer".into(), model: None },
            main_route: ReviewerRoute { route_name: "main".into(), model: None },
            max_self_check_iterations: 3,
        };

        run_and_persist_review_round(
            &pool, &gateway, &config, task, session, ArtifactKind::Plan, 1, "plan v1",
        )
        .await
        .expect("round persists");

        let rows = evohime_storage::list_artifact_reviews_by_task(&pool, task)
            .await
            .expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].revised_content, "plan v2");
        assert_eq!(rows[0].round_number, 1);
    }
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p evohime-agent-runtime review::`
Expected: PASS (7 tests; the new one prints "skipping ... database unavailable" and passes trivially if no local Postgres is running).

- [ ] **Step 4: Commit**

```bash
git add crates/agent-runtime/src/review.rs
git commit -m "feat(agent-runtime): persist review rounds to artifact_reviews"
```

---

## Task 6: Frontend — Settings → "Планирование" tab

**Files:**
- Modify: `frontend/web/src/types.ts:173-183` (`SettingsTab` union)
- Create: `frontend/web/src/panels/PlanningSettingsSection.tsx`
- Modify: `frontend/web/src/panels/SettingsPanel.tsx` (register tab + render section)
- Modify: `frontend/web/src/app.tsx` (derive reviewer/synthesizer routes, add `setReviewerCount`, pass new props)

**Interfaces:**
- Consumes: `ModelRouteDraft`, `modelDrafts`/`setModelDrafts`, `updateModelDraft`, `saveModelConfig` — all already defined in `app.tsx` (Task 6 adds no new persistence path; reviewer/synthesizer routes save through the exact same `saveModelConfig` → `PUT /api/models/config` flow the orchestrator route already uses, made possible by Task 2's `inherits_default_key`).
- Produces: nothing consumed elsewhere in this plan (this is the last task) — but the route names it writes (`reviewer_0..N-1`, `synthesizer`) are exactly what the follow-up "wire into task lifecycle" plan will read via `ReviewEngineConfig`.

- [ ] **Step 1: Add the tab to `SettingsTab`**

In `frontend/web/src/types.ts:173-183`, add `"planning"`:

```typescript
export type SettingsTab =
  | "model"
  | "permissions"
  | "mcp"
  | "tools"
  | "worker"
  | "metrics"
  | "thinking"
  | "spend"
  | "planning"
  | "launcher"
  | "archive";
```

- [ ] **Step 2: Create `frontend/web/src/panels/PlanningSettingsSection.tsx`**

```tsx
import type { ModelRouteDraft } from "../types";

type ProviderModelPickerProps = {
  route: ModelRouteDraft;
  models: string[];
  onUpdate: (patch: Partial<ModelRouteDraft>) => void;
};

function ProviderModelPicker({ route, models, onUpdate }: ProviderModelPickerProps) {
  return (
    <div className="modelProviderForm">
      <label>
        <span>Провайдер</span>
        <select
          value={route.provider}
          onChange={(event) => {
            const provider = event.target.value;
            onUpdate({
              provider,
              base_url: provider === "literouter" ? "https://api.literouter.com/v1" : "https://api.openai.com/v1",
              model: provider === "literouter" ? "deepseek:free" : "gpt-4o-mini",
              billing_mode: provider === "literouter" ? "free" : "paid",
            });
          }}
        >
          <option value="literouter">LiteRouter</option>
          <option value="openai-compatible">OpenAI-compatible</option>
        </select>
      </label>
      <label>
        <span>Модель</span>
        <select value={route.model} onChange={(event) => onUpdate({ model: event.target.value })}>
          {[route.model, ...models]
            .filter((model, index, list) => model && list.indexOf(model) === index)
            .map((model) => (
              <option key={model} value={model}>
                {model}
              </option>
            ))}
        </select>
      </label>
    </div>
  );
}

type PlanningSettingsSectionProps = {
  reviewerRoutes: Array<{ route: ModelRouteDraft; index: number }>;
  reviewerModels: string[];
  synthesizerRoute: ModelRouteDraft | null;
  synthesizerRouteIndex: number;
  synthesizerModels: string[];
  onSetReviewerCount: (count: number) => void;
  onUpdateModelDraft: (index: number, patch: Partial<ModelRouteDraft>) => void;
  onSave: () => void;
};

export function PlanningSettingsSection({
  reviewerRoutes,
  reviewerModels,
  synthesizerRoute,
  synthesizerRouteIndex,
  synthesizerModels,
  onSetReviewerCount,
  onUpdateModelDraft,
  onSave,
}: PlanningSettingsSectionProps) {
  return (
    <section className="settingsSection">
      <h3>Планирование</h3>
      <p className="settingsHint">
        Пул моделей, которые ревьюят спецификацию и план перед реализацией, и модель-синтезатор,
        сводящая их замечания в один отчёт.
      </p>

      <label>
        <span>Количество ревьюверов</span>
        <select
          value={reviewerRoutes.length || 1}
          onChange={(event) => onSetReviewerCount(Number(event.target.value))}
        >
          {[1, 2, 3, 4, 5].map((count) => (
            <option key={count} value={count}>
              {count}
            </option>
          ))}
        </select>
      </label>

      {reviewerRoutes.map(({ route, index }, position) => (
        <div key={route.name} className="orchestratorSettings">
          <h4>{`Ревьювер ${position + 1}`}</h4>
          <ProviderModelPicker
            route={route}
            models={reviewerModels}
            onUpdate={(patch) => onUpdateModelDraft(index, patch)}
          />
        </div>
      ))}

      {synthesizerRoute ? (
        <div className="orchestratorSettings">
          <h4>Синтезатор</h4>
          <ProviderModelPicker
            route={synthesizerRoute}
            models={synthesizerModels}
            onUpdate={(patch) => onUpdateModelDraft(synthesizerRouteIndex, patch)}
          />
        </div>
      ) : null}
    </section>
  );
}
```

- [ ] **Step 3: Derive reviewer/synthesizer routes and `setReviewerCount` in `app.tsx`**

Near the existing `orchestratorRouteIndex`/`orchestratorRoute` derivation (`app.tsx:666-667`), add:

```typescript
  const reviewerRoutes = modelDrafts
    .map((route, index) => ({ route, index }))
    .filter(({ route }) => /^reviewer_\d+$/.test(route.name))
    .sort((a, b) => a.route.name.localeCompare(b.route.name, undefined, { numeric: true }));
  const synthesizerRouteIndex = modelDrafts.findIndex((route) => route.name === "synthesizer");
  const synthesizerRoute = synthesizerRouteIndex >= 0 ? modelDrafts[synthesizerRouteIndex] : null;
```

Near `updateModelDraft` (`app.tsx:914-918`), add:

```typescript
  function defaultPlanningRouteDraft(name: string): ModelRouteDraft {
    return {
      name,
      provider: "literouter",
      model: "deepseek:free",
      base_url: "https://api.literouter.com/v1",
      api_key: "",
      billing_mode: "free",
    };
  }

  function setReviewerCount(count: number) {
    setModelDrafts((current) => {
      const withoutReviewers = current.filter((route) => !/^reviewer_\d+$/.test(route.name));
      const reviewers = Array.from({ length: count }, (_, index) => {
        const name = `reviewer_${index}`;
        return current.find((route) => route.name === name) ?? defaultPlanningRouteDraft(name);
      });
      const hasSynthesizer = current.some((route) => route.name === "synthesizer");
      const synthesizer = hasSynthesizer ? [] : [defaultPlanningRouteDraft("synthesizer")];
      return [...withoutReviewers, ...reviewers, ...synthesizer];
    });
  }
```

The reviewer-count `<select>` in Step 2 defaults its displayed value to `reviewerRoutes.length || 1` when nothing is configured yet — but a `<select>` only fires `onChange` when the chosen value *differs* from the current one, so a user who opens the tab, sees "1" already selected, and wants exactly one reviewer has no event to hook: nothing would ever get created. Seed one reviewer + the synthesizer as soon as the Planning tab is opened with none configured, next to the other tab-driven `useEffect`s in `app.tsx` (e.g. the one at `app.tsx:636-649`):

```typescript
  useEffect(() => {
    if (settingsTab !== "planning" || !modelConfig) {
      return;
    }
    if (reviewerRoutes.length === 0) {
      setReviewerCount(1);
    }
  }, [settingsTab, modelConfig, reviewerRoutes.length]);
```

- [ ] **Step 4: Register the tab and render the section in `SettingsPanel.tsx`**

Add the import (near line 20):

```typescript
import { PlanningSettingsSection } from "./PlanningSettingsSection";
```

Add to `SETTINGS_TABS` (after the `"spend"` entry, `SettingsPanel.tsx:65`):

```typescript
  ["planning", "Планирование", "Ревьюверы и синтезатор"],
```

Add the corresponding props to `SettingsPanelProps` (near `orchestratorModels`, line 34) and to the function's destructured parameters (near line 93):

```typescript
  reviewerRoutes: Array<{ route: ModelRouteDraft; index: number }>;
  reviewerModels: string[];
  synthesizerRoute: ModelRouteDraft | null;
  synthesizerRouteIndex: number;
  synthesizerModels: string[];
  onSetReviewerCount: (count: number) => void;
```

Render it alongside the other tab blocks (after the `"spend"` block, before `"launcher"`):

```tsx
        {settingsTab === "planning" ? (
          <PlanningSettingsSection
            reviewerRoutes={reviewerRoutes}
            reviewerModels={orchestratorModels}
            synthesizerRoute={synthesizerRoute}
            synthesizerRouteIndex={synthesizerRouteIndex}
            synthesizerModels={orchestratorModels}
            onSetReviewerCount={onSetReviewerCount}
            onUpdateModelDraft={onUpdateModelDraft}
            onSave={onSaveModelConfig}
          />
        ) : null}
```

(Reusing `orchestratorModels` as the available-models list for reviewers/synthesizer is intentional — all three route kinds resolve against the same `GET /api/models/available?route=orchestrator` fallback behavior already in `models_api.rs:58-63` for any route name not yet saved.)

- [ ] **Step 5: Wire the new props at the `<SettingsPanel>` call site in `app.tsx`**

Near `orchestratorModels={orchestratorModels}` (`app.tsx:1185`), add:

```tsx
          reviewerRoutes={reviewerRoutes}
          reviewerModels={orchestratorModels}
          synthesizerRoute={synthesizerRoute}
          synthesizerRouteIndex={synthesizerRouteIndex}
          synthesizerModels={orchestratorModels}
          onSetReviewerCount={setReviewerCount}
```

Provider/model `<select>` changes (unlike the API-key `<input>`) have no `onBlur` to hook, so `PlanningSettingsSection` needs its own explicit save affordance rather than relying on autosave. Add an `onSave: () => void` prop to `PlanningSettingsSectionProps` and render a save button at the bottom of the section:

```tsx
      <button type="button" className="settingsSaveButton" onClick={onSave}>
        Сохранить
      </button>
```

Pass `onSave={onSaveModelConfig}` from `SettingsPanel.tsx`'s render call (Step 4) and thread `onSaveModelConfig` (already a `SettingsPanelProps` field, line 36) through unchanged — no new prop needed at the `app.tsx` call site for this part.

For the reviewer-count `<select>` specifically, also call `saveModelConfig()` right after `setReviewerCount` finishes updating state, since changing the count is itself a save-worthy action independent of the button:

```typescript
  function setReviewerCount(count: number) {
    setModelDrafts((current) => {
      const withoutReviewers = current.filter((route) => !/^reviewer_\d+$/.test(route.name));
      const reviewers = Array.from({ length: count }, (_, index) => {
        const name = `reviewer_${index}`;
        return current.find((route) => route.name === name) ?? defaultPlanningRouteDraft(name);
      });
      const hasSynthesizer = current.some((route) => route.name === "synthesizer");
      const synthesizer = hasSynthesizer ? [] : [defaultPlanningRouteDraft("synthesizer")];
      return [...withoutReviewers, ...reviewers, ...synthesizer];
    });
    setTimeout(() => void saveModelConfig(), 0);
  }
```

(`setTimeout(..., 0)` defers the save until after the `setModelDrafts` update has been applied, since `saveModelConfig` reads `modelDrafts` from closure state — the same reason the existing `onBlur={onSaveModelConfig}` pattern works from a separate event rather than inline in the same setter call.)

- [ ] **Step 6: Manual verification**

Run: `npm run dev` (or the project's existing dev script) inside `frontend/web/`, open Settings → "Планирование", set reviewer count to 3, confirm 3 provider/model picker blocks plus one "Синтезатор" block render, change a model, save, reload the page, and confirm the selections persisted (i.e. `GET /api/models/config` now returns `reviewer_0`, `reviewer_1`, `reviewer_2`, `synthesizer` routes).

Expected: 3 reviewer blocks + 1 synthesizer block, persisted across reload.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/types.ts frontend/web/src/panels/PlanningSettingsSection.tsx frontend/web/src/panels/SettingsPanel.tsx frontend/web/src/app.tsx
git commit -m "feat(frontend): add Settings > Planning tab for reviewer pool + synthesizer"
```

---

## Follow-up (separate plan, not part of this one)

- Turn planning into a real LLM call (currently `MockLlmClient` in `run_planning_phase_inner`).
- Introduce pausable "spec draft" and "plan draft" states in the task lifecycle.
- Add the "Ревью" / "Ревью ещё раз" / "Начать реализацию" UI, calling `run_and_persist_review_round` from Task 5 once per click.
- Add `ServerEvent` variants for review progress if the UI needs live status during a round.
