//! Core-owned durable background execution contract (plan 132).
//!
//! This module owns validation and deterministic decisions only.  It does not
//! execute workflow nodes, tools or provider effects: those remain owned by
//! the existing runtime boundaries.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CONTRACT_VERSION: &str = "background-execution/v1";
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_SCOPE_BYTES: usize = 256;
pub const MAX_REF_BYTES: usize = 512;
pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024;
pub const MAX_QUEUE_DEPTH: u32 = 4096;
pub const MAX_ACTIVE: u32 = 256;
pub const MAX_CATCH_UP: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundRunKind {
    WorkflowRun,
    AgentRun,
    GoalContinuation,
    MaintenanceRun,
    RegisteredBackgroundTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Accepted,
    Scheduled,
    Queued,
    Dispatching,
    Running,
    Waiting,
    RetryScheduled,
    Blocked,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    DeadLettered,
    Superseded,
}

impl RunState {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled | Self::DeadLettered | Self::Superseded)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityClass {
    UserBlocking,
    ForegroundContinuation,
    BackgroundWorkflow,
    ScheduledWorkflow,
    Maintenance,
    BulkResearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverflowPolicy {
    RejectNew,
    DropOldestAllowed,
    CoalesceByKey,
    BackpressureCaller,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissedFirePolicy {
    Skip,
    FireOnceNow,
    Coalesce,
    CatchUpBounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlapPolicy {
    Allow,
    QueueNext,
    SkipIfRunning,
    CancelPreviousThenRun,
    Coalesce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ScheduleSpec {
    OneShotAt { wake_at_ms: i64 },
    Interval { every_ms: i64, first_at_ms: i64 },
    Cron { minute: u8, hour: u8, timezone_minutes: i32 },
}

impl ScheduleSpec {
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::OneShotAt { wake_at_ms } if *wake_at_ms <= 0 => Err(ValidationError::Invalid("wake_at_ms")),
            Self::Interval { every_ms, first_at_ms } if *every_ms <= 0 || *first_at_ms <= 0 => Err(ValidationError::Invalid("interval")),
            Self::Cron { minute, hour, timezone_minutes } if *minute >= 60 || *hour >= 24 || !(-14 * 60..=14 * 60).contains(timezone_minutes) => Err(ValidationError::Invalid("cron")),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WaitCondition {
    WaitUntil { wake_at_ms: i64 },
    WaitForDuration { duration_ms: i64, resolved_wake_at_ms: i64 },
    WaitForRunState { run_id: String, state: String, timeout_at_ms: Option<i64> },
    WaitForHumanWorkItem { work_item_id: String, timeout_at_ms: Option<i64> },
}

impl WaitCondition {
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::WaitUntil { wake_at_ms } if *wake_at_ms <= 0 => Err(ValidationError::Invalid("wake_at_ms")),
            Self::WaitForDuration { duration_ms, resolved_wake_at_ms } if *duration_ms <= 0 || *resolved_wake_at_ms <= 0 => Err(ValidationError::Invalid("duration")),
            Self::WaitForRunState { run_id, state, timeout_at_ms } => {
                validate_key("run_id", run_id)?;
                validate_ref("state", state, MAX_ID_BYTES)?;
                if timeout_at_ms.is_some_and(|value| value <= 0) { return Err(ValidationError::Invalid("timeout_at_ms")); }
                Ok(())
            }
            Self::WaitForHumanWorkItem { work_item_id, timeout_at_ms } => {
                validate_key("work_item_id", work_item_id)?;
                if timeout_at_ms.is_some_and(|value| value <= 0) { return Err(ValidationError::Invalid("timeout_at_ms")); }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundQueue {
    pub queue_id: String,
    pub owner_scope: String,
    pub revision: u64,
    pub max_active: u32,
    pub max_queued: u32,
    pub priority: PriorityClass,
    pub overflow: OverflowPolicy,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundRunSnapshot {
    pub schema_version: u32,
    pub run_id: String,
    pub kind: BackgroundRunKind,
    pub owner_scope: String,
    pub source_definition_ref: String,
    pub source_definition_revision: u64,
    pub queue_ref: String,
    pub concurrency_key: Option<String>,
    pub priority: PriorityClass,
    pub environment_snapshot_ref: String,
    pub execution_policy_ref: String,
    pub approval_policy_ref: Option<String>,
    pub idempotency_key: Option<String>,
    pub state: RunState,
    pub attempt: u32,
    pub next_wakeup_at_ms: Option<i64>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    UnsupportedContract,
    Empty(&'static str),
    TooLong(&'static str),
    Invalid(&'static str),
    Limit(&'static str),
    SecretLike(&'static str),
    Serialization,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for ValidationError {}

fn validate_ref(name: &'static str, value: &str, limit: usize) -> Result<(), ValidationError> {
    if value.is_empty() { return Err(ValidationError::Empty(name)); }
    if value.len() > limit { return Err(ValidationError::TooLong(name)); }
    let lower = value.to_ascii_lowercase();
    if ["bearer ", "password=", "token=", "secret=", "api_key="].iter().any(|m| lower.contains(m)) {
        return Err(ValidationError::SecretLike(name));
    }
    Ok(())
}

fn validate_key(name: &'static str, value: &str) -> Result<(), ValidationError> {
    validate_ref(name, value, MAX_REF_BYTES)?;
    if value.bytes().any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'/' | b'.'))) {
        return Err(ValidationError::Invalid(name));
    }
    Ok(())
}

impl BackgroundQueue {
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_ref("queue_id", &self.queue_id, MAX_ID_BYTES)?;
        validate_ref("owner_scope", &self.owner_scope, MAX_SCOPE_BYTES)?;
        if self.revision == 0 { return Err(ValidationError::Invalid("revision")); }
        if self.max_active == 0 || self.max_active > MAX_ACTIVE { return Err(ValidationError::Limit("max_active")); }
        if self.max_queued > MAX_QUEUE_DEPTH { return Err(ValidationError::Limit("max_queued")); }
        validate_ref("content_hash", &self.content_hash, MAX_ID_BYTES)
    }
}

impl BackgroundRunSnapshot {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != SCHEMA_VERSION { return Err(ValidationError::UnsupportedContract); }
        validate_ref("run_id", &self.run_id, MAX_ID_BYTES)?;
        validate_ref("owner_scope", &self.owner_scope, MAX_SCOPE_BYTES)?;
        validate_ref("source_definition_ref", &self.source_definition_ref, MAX_REF_BYTES)?;
        validate_key("queue_ref", &self.queue_ref)?;
        validate_key("environment_snapshot_ref", &self.environment_snapshot_ref)?;
        validate_key("execution_policy_ref", &self.execution_policy_ref)?;
        if let Some(value) = &self.concurrency_key { validate_key("concurrency_key", value)?; }
        if let Some(value) = &self.approval_policy_ref { validate_key("approval_policy_ref", value)?; }
        if self.source_definition_revision == 0 || self.attempt > 4096 { return Err(ValidationError::Invalid("snapshot_revision_or_attempt")); }
        if self.content_hash.len() > MAX_ID_BYTES { return Err(ValidationError::TooLong("content_hash")); }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<String, ValidationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| ValidationError::Serialization)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES { return Err(ValidationError::Limit("snapshot")); }
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

pub fn transition_allowed(from: RunState, to: RunState) -> bool {
    use RunState::*;
    if from.terminal() { return false; }
    matches!((from, to),
        (Accepted, Scheduled | Queued | Blocked | Cancelled) |
        (Scheduled, Queued | Blocked | Cancelling | Cancelled) |
        (Queued, Dispatching | Blocked | Cancelling | Cancelled | DeadLettered) |
        (Dispatching, Running | Blocked | Cancelling | Cancelled) |
        (Running, Waiting | RetryScheduled | Completed | Failed | Cancelling | Blocked) |
        (Waiting, Queued | RetryScheduled | Blocked | Cancelling | Cancelled) |
        (RetryScheduled, Queued | Blocked | Cancelling | Cancelled) |
        (Blocked, Queued | Cancelling | Cancelled | DeadLettered) |
        (Cancelling, Cancelled))
}

pub fn schedule_fire_key(schedule_id: &str, revision: u64, logical_fire_ms: i64) -> Result<String, ValidationError> {
    validate_ref("schedule_id", schedule_id, MAX_ID_BYTES)?;
    if revision == 0 { return Err(ValidationError::Invalid("schedule_revision")); }
    Ok(format!("{schedule_id}:{revision}:{logical_fire_ms}"))
}

pub fn due(now_ms: i64, wake_at_ms: Option<i64>) -> bool { wake_at_ms.is_some_and(|wake| now_ms >= wake) }

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> BackgroundRunSnapshot {
        BackgroundRunSnapshot { schema_version: SCHEMA_VERSION, run_id: "run-1".into(), kind: BackgroundRunKind::WorkflowRun, owner_scope: "workspace-1".into(), source_definition_ref: "workflow:1".into(), source_definition_revision: 1, queue_ref: "workflow-default".into(), concurrency_key: None, priority: PriorityClass::BackgroundWorkflow, environment_snapshot_ref: "env:1".into(), execution_policy_ref: "policy:1".into(), approval_policy_ref: None, idempotency_key: Some("fire-1".into()), state: RunState::Accepted, attempt: 0, next_wakeup_at_ms: None, content_hash: "hash".into() }
    }

    #[test]
    fn snapshot_hash_is_bounded_and_stable() { assert_eq!(run().canonical_hash().unwrap(), run().canonical_hash().unwrap()); }

    #[test]
    fn transitions_are_fail_closed_and_terminal() {
        assert!(transition_allowed(RunState::Accepted, RunState::Queued));
        assert!(transition_allowed(RunState::Running, RunState::Completed));
        assert!(!transition_allowed(RunState::Completed, RunState::Queued));
        assert!(!transition_allowed(RunState::Running, RunState::Accepted));
    }

    #[test]
    fn secret_like_refs_are_rejected() { let mut value = run(); value.execution_policy_ref = "token=raw".into(); assert!(matches!(value.validate(), Err(ValidationError::SecretLike("execution_policy_ref")))); }

    #[test]
    fn fire_identity_is_deterministic() { assert_eq!(schedule_fire_key("schedule", 2, 42).unwrap(), "schedule:2:42"); }

    #[test]
    fn fake_clock_wait_and_schedule_policies_are_bounded() {
        assert!(due(1_000, Some(1_000)));
        assert!(!due(999, Some(1_000)));
        assert!(ScheduleSpec::OneShotAt { wake_at_ms: 2_000 }.validate().is_ok());
        assert!(ScheduleSpec::Interval { every_ms: 0, first_at_ms: 2_000 }.validate().is_err());
        assert!(WaitCondition::WaitForHumanWorkItem { work_item_id: "item-1".into(), timeout_at_ms: Some(2_000) }.validate().is_ok());
    }
}
