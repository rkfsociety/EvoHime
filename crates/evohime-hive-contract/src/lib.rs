#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Minimal boundary traits, error types, and DTOs between `evohime-hive` and
//! `evohime-core`.
//!
//! `evohime-hive` depends only on this contract crate; `evohime-core` implements
//! these traits to supply durable journal access, task dispatch, and workspace
//! configuration without exposing Core internals to the Hive.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Boxed `Send` future used for object-safe async contract traits.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Typed error returned across the `evohime-hive` / `evohime-core` boundary.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HiveContractError {
    /// Durable journal or configuration storage operation failed.
    #[error("hive storage error: {0}")]
    Storage(String),
    /// Coordinator failed to accept or dispatch a Hive task.
    #[error("hive dispatch error: {0}")]
    Dispatch(String),
    /// Serialization or deserialization across the boundary failed.
    #[error("hive serialization error: {0}")]
    Serialization(String),
    /// Caller supplied invalid or out-of-bounds contract input.
    #[error("hive invalid input: {0}")]
    InvalidInput(String),
    /// Required Core capability or route is currently unavailable.
    #[error("hive dependency unavailable: {0}")]
    Unavailable(String),
}

/// Minimal projection of a durable journal event needed by Hive observers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiveEventRecord {
    /// Monotonic journal sequence identifier.
    pub sequence_id: i64,
    /// Task identifier associated with the event.
    pub task_id: String,
    /// Machine-readable event type (e.g. `child.workflow`, `eva_pulse.signal.accepted`).
    pub event_type: String,
    /// Raw JSON payload bytes.
    pub payload: Vec<u8>,
    /// Event timestamp in Unix milliseconds.
    pub created_at_ms: i64,
}

/// Minimal request to start a Hive-owned task via the Core coordinator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiveTaskDispatchRequest {
    /// Hive-owned task identifier (for example, `eva-pulse-<uuid>`).
    pub task_id: String,
    /// Prompt or instruction for the coordinator/Queen.
    pub prompt: String,
    /// Optional workspace root override.
    pub workspace_root: Option<String>,
    /// Isolated route hint (for example, `"hive"` or `"hive_fallback"`).
    pub preferred_route_hint: Option<String>,
}

/// Request to record a detached Git worktree lease allocated for a Hive task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiveWorktreeLeaseRequest {
    /// Sanitized worktree identifier.
    pub worktree_id: String,
    /// Task identifier owning the worktree lease.
    pub task_id: String,
    /// Base repository root path.
    pub base_root: String,
    /// Branch mode (e.g. `"detached"`).
    pub branch: String,
    /// Relative worktree directory reference (e.g. `.evohime/worktrees/<id>`).
    pub root_ref: String,
    /// Resolved base commit hash.
    pub base_commit: String,
}

/// Rolling dispatch window counts and oldest request timestamps for a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HiveDispatchWindows {
    /// Requests dispatched in the last 60 seconds.
    pub last_minute: u32,
    /// Requests dispatched in the last hour.
    pub last_hour: u32,
    /// Requests dispatched in the last 24 hours.
    pub last_day: u32,
    /// Oldest dispatch timestamp inside the minute window, if any.
    pub oldest_minute_ms: Option<i64>,
    /// Oldest dispatch timestamp inside the hour window, if any.
    pub oldest_hour_ms: Option<i64>,
    /// Oldest dispatch timestamp inside the day window, if any.
    pub oldest_day_ms: Option<i64>,
}

/// Bounded per-request token usage record for daily cost estimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiveModelUsage {
    /// Request dispatch timestamp in Unix milliseconds.
    pub request_at: i64,
    /// Prompt tokens consumed by the request.
    pub prompt_tokens: u32,
    /// Completion tokens produced by the response.
    pub completion_tokens: u32,
    /// Cached prompt tokens read, if reported by the provider.
    pub cache_read_input_tokens: Option<u32>,
}

/// Durable journal and settings operations required by the Hive.
pub trait HiveJournalProvider: Send + Sync {
    /// Returns the latest committed journal sequence number.
    fn latest_sequence(&self) -> BoxFuture<'_, i64>;

    /// Replays up to `limit` journal events starting after `after_sequence`.
    fn replay_events(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<HiveEventRecord>, HiveContractError>>;

    /// Records an audit event for a Hive task or dead-letter record.
    fn record_audit<'a>(
        &'a self,
        task_id: &'a str,
        event_type: &'a str,
        payload: &'a [u8],
    ) -> BoxFuture<'a, Result<(), HiveContractError>>;

    /// Prunes old signal audit records, keeping at most `keep_limit` per type.
    fn prune_signal_records<'a>(
        &'a self,
        event_types: &'a [&'a str],
        keep_limit: usize,
    ) -> BoxFuture<'a, ()>;

    /// Loads serialized `HiveSettings` JSON from the durable configuration store.
    fn load_hive_settings_json(
        &self,
    ) -> BoxFuture<'_, Result<Option<serde_json::Value>, HiveContractError>>;

    /// Persists serialized `HiveSettings` JSON into the durable configuration store.
    fn save_hive_settings_json<'a>(
        &'a self,
        settings: &'a serde_json::Value,
    ) -> BoxFuture<'a, Result<(), HiveContractError>>;

    /// Returns active persistent agent role identifiers.
    fn persistent_agent_roles(&self) -> BoxFuture<'_, Vec<String>>;

    /// Returns `(tasks_today, average_task_time_sec)` for the UTC day of `now_ms`.
    fn task_metrics(&self, now_ms: i64) -> BoxFuture<'_, (u32, u64)>;

    /// Filters `roots` down to task identifiers belonging to the Hive world.
    fn filter_hive_root_tasks(&self, roots: Vec<String>) -> BoxFuture<'_, Vec<String>>;

    /// Returns rolling dispatch window counters for `(provider, model)` at `now_ms`.
    fn model_dispatch_windows<'a>(
        &'a self,
        provider: &'a str,
        model: &'a str,
        now_ms: i64,
        minute_window_ms: i64,
        hour_window_ms: i64,
        day_window_ms: i64,
    ) -> BoxFuture<'a, Result<HiveDispatchWindows, HiveContractError>>;

    /// Loads bounded usage rows for `(provider, model)` since `start_ms`.
    fn model_usage_since<'a>(
        &'a self,
        start_ms: i64,
        provider: &'a str,
        model: &'a str,
    ) -> BoxFuture<'a, Result<Vec<HiveModelUsage>, HiveContractError>>;

    /// Sums total tokens recorded for `route_scope` since `start_ms`.
    fn route_tokens_since<'a>(
        &'a self,
        start_ms: i64,
        route_scope: &'a str,
    ) -> BoxFuture<'a, Result<u64, HiveContractError>>;

    /// Records a newly created Hive task worktree lease in durable storage.
    fn register_worktree_lease(&self, request: HiveWorktreeLeaseRequest) -> BoxFuture<'_, ()>;

    /// Returns whether `task_id` still owns an active workflow run.
    fn has_active_workflow_run<'a>(&'a self, task_id: &'a str) -> BoxFuture<'a, bool>;

    /// Transitions a cleaned worktree lease to `cleanup_pending`.
    fn mark_worktree_cleanup_pending<'a>(&'a self, worktree_id: &'a str) -> BoxFuture<'a, ()>;
}

/// Task dispatch seam implemented by Core's `TaskCoordinator`.
pub trait HiveCoordinator: Send + Sync {
    /// Dispatches a new Hive-owned task into the Core execution pipeline.
    fn dispatch_task(
        &self,
        request: HiveTaskDispatchRequest,
    ) -> BoxFuture<'_, Result<(), HiveContractError>>;
}

/// Provides validated workspace paths and model selection state for the Hive.
pub trait WorkspaceProvider: Send + Sync {
    /// Returns the configured and validated `eva_workspace_path`, if any.
    fn eva_workspace_path(&self) -> BoxFuture<'_, Option<String>>;

    /// Returns the currently selected model name, if any.
    fn selected_model_name(&self) -> Option<String>;
}

/// Optional structured logging sink so Hive does not depend on `StructuredLogger`.
pub trait HiveTelemetrySink: Send + Sync {
    /// Emits a structured log record from the Hive daemon.
    fn log_event(&self, level: &str, event: &str, fields: serde_json::Value);
}
