//! Core-owned durable background execution contract (plan 132).
//!
//! This module owns validation and deterministic decisions only.  It does not
//! execute workflow nodes, tools or provider effects: those remain owned by
//! the existing runtime boundaries.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Stable contract identifier for durable background execution.
pub const CONTRACT_VERSION: &str = "background-execution/v1";
/// Serialized schema version supported by this contract.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum identifier length accepted by this contract.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum owner-scope length accepted by this contract.
pub const MAX_SCOPE_BYTES: usize = 256;
/// Maximum reference length accepted by this contract.
pub const MAX_REF_BYTES: usize = 512;
/// Maximum serialized run snapshot size in bytes.
pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024;
/// Maximum queued runs accepted by one background queue.
pub const MAX_QUEUE_DEPTH: u32 = 4096;
/// Maximum active runs accepted by one background queue.
pub const MAX_ACTIVE: u32 = 256;
/// Maximum missed scheduled fires processed in one catch-up pass.
pub const MAX_CATCH_UP: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Source category for a durable background run.
pub enum BackgroundRunKind {
    /// A workflow definition was submitted as background work.
    WorkflowRun,
    /// An agent run was submitted as background work.
    AgentRun,
    /// A goal continuation was scheduled.
    GoalContinuation,
    /// A maintenance task was scheduled.
    MaintenanceRun,
    /// A task from the registered background-task catalog was scheduled.
    RegisteredBackgroundTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of one queued or active background run.
pub enum RunState {
    /// Request passed admission checks.
    Accepted,
    /// Run is waiting for its schedule time.
    Scheduled,
    /// Run is waiting for a queue slot.
    Queued,
    /// Runtime is preparing to start the run.
    Dispatching,
    /// Run is executing.
    Running,
    /// Run is waiting for a durable condition.
    Waiting,
    /// A retry is waiting for its scheduled time.
    RetryScheduled,
    /// Policy, capacity, or dependency prevents execution.
    Blocked,
    /// Cancellation was requested and is being processed.
    Cancelling,
    /// Run completed successfully.
    Completed,
    /// Run ended with a non-retryable failure.
    Failed,
    /// Run was cancelled.
    Cancelled,
    /// Run exhausted automatic handling and needs intervention.
    DeadLettered,
    /// A newer run or schedule revision replaced this run.
    Superseded,
}

impl RunState {
    /// Returns whether the run state is terminal.
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::Cancelled
                | Self::DeadLettered
                | Self::Superseded
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Scheduling priority class for background work.
pub enum PriorityClass {
    /// Highest priority for work directly blocking a user request.
    UserBlocking,
    /// Priority for a foreground task continuation.
    ForegroundContinuation,
    /// Priority for non-urgent workflow execution.
    BackgroundWorkflow,
    /// Priority for due scheduled workflow work.
    ScheduledWorkflow,
    /// Priority for background maintenance.
    Maintenance,
    /// Lowest priority for bulk research or batch work.
    BulkResearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Queue behavior when its capacity is reached.
pub enum OverflowPolicy {
    /// Reject a new run when the queue is full.
    RejectNew,
    /// Drop the oldest eligible queued run to admit a new one.
    DropOldestAllowed,
    /// Merge equivalent requests that share an idempotency or concurrency key.
    CoalesceByKey,
    /// Return capacity pressure to the caller.
    BackpressureCaller,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Behavior when a schedule fires while the runtime is unavailable.
pub enum MissedFirePolicy {
    /// Do not run missed schedule occurrences.
    Skip,
    /// Run one occurrence immediately for any missed fires.
    FireOnceNow,
    /// Combine missed occurrences into one run.
    Coalesce,
    /// Process missed occurrences up to the configured catch-up cap.
    CatchUpBounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Behavior when a scheduled run overlaps an existing run.
pub enum OverlapPolicy {
    /// Allow overlapping runs.
    Allow,
    /// Queue the next run until the current run ends.
    QueueNext,
    /// Skip a fire while an earlier run remains active.
    SkipIfRunning,
    /// Cancel the existing run before starting the new one.
    CancelPreviousThenRun,
    /// Combine missed occurrences into one run.
    Coalesce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
/// Supported deterministic wake-up schedule forms.
pub enum ScheduleSpec {
    /// Run once at an absolute timestamp.
    OneShotAt {
        /// Absolute scheduled wake time as Unix epoch milliseconds.
        wake_at_ms: i64,
    },
    /// Run repeatedly at a fixed interval after its first scheduled fire.
    Interval {
        /// Interval between fires in milliseconds.
        every_ms: i64,
        /// Absolute time of the first interval fire.
        first_at_ms: i64,
    },
    /// Run at a minute and hour in a fixed UTC offset.
    Cron {
        /// Minute component, from 0 through 59.
        minute: u8,
        /// Hour component, from 0 through 23.
        hour: u8,
        /// UTC offset in minutes used to evaluate the schedule.
        timezone_minutes: i32,
    },
}

impl ScheduleSpec {
    /// Validates schedule or wait-condition fields and time bounds.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::OneShotAt { wake_at_ms } if *wake_at_ms <= 0 => {
                Err(ValidationError::Invalid("wake_at_ms"))
            }
            Self::Interval {
                every_ms,
                first_at_ms,
            } if *every_ms <= 0 || *first_at_ms <= 0 => Err(ValidationError::Invalid("interval")),
            Self::Cron {
                minute,
                hour,
                timezone_minutes,
            } if *minute >= 60
                || *hour >= 24
                || !(-14 * 60..=14 * 60).contains(timezone_minutes) =>
            {
                Err(ValidationError::Invalid("cron"))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
/// Durable condition that keeps a run waiting.
pub enum WaitCondition {
    /// Resume at an absolute wake time.
    WaitUntil {
        /// Absolute wake time as Unix epoch milliseconds.
        wake_at_ms: i64,
    },
    /// Resume after a duration resolved at the time this condition is stored.
    WaitForDuration {
        /// Requested delay in milliseconds.
        duration_ms: i64,
        /// Absolute wake time computed when the wait was created.
        resolved_wake_at_ms: i64,
    },
    /// Resume when another run reaches the requested state.
    WaitForRunState {
        /// Run identifier whose lifecycle is awaited.
        run_id: String,
        /// Desired run state represented by a stable state name.
        state: String,
        /// Optional timeout as Unix epoch milliseconds.
        timeout_at_ms: Option<i64>,
    },
    /// Resume when a human work item receives a terminal decision.
    WaitForHumanWorkItem {
        /// Human work-item identifier whose completion is awaited.
        work_item_id: String,
        /// Optional timeout as Unix epoch milliseconds.
        timeout_at_ms: Option<i64>,
    },
}

impl WaitCondition {
    /// Validates schedule or wait-condition fields and time bounds.
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::WaitUntil { wake_at_ms } if *wake_at_ms <= 0 => {
                Err(ValidationError::Invalid("wake_at_ms"))
            }
            Self::WaitForDuration {
                duration_ms,
                resolved_wake_at_ms,
            } if *duration_ms <= 0 || *resolved_wake_at_ms <= 0 => {
                Err(ValidationError::Invalid("duration"))
            }
            Self::WaitForRunState {
                run_id,
                state,
                timeout_at_ms,
            } => {
                validate_key("run_id", run_id)?;
                validate_ref("state", state, MAX_ID_BYTES)?;
                if timeout_at_ms.is_some_and(|value| value <= 0) {
                    return Err(ValidationError::Invalid("timeout_at_ms"));
                }
                Ok(())
            }
            Self::WaitForHumanWorkItem {
                work_item_id,
                timeout_at_ms,
            } => {
                validate_key("work_item_id", work_item_id)?;
                if timeout_at_ms.is_some_and(|value| value <= 0) {
                    return Err(ValidationError::Invalid("timeout_at_ms"));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Versioned queue capacity, priority, and overflow contract.
pub struct BackgroundQueue {
    /// Stable queue identifier.
    pub queue_id: String,
    /// Owner namespace controlling this queue.
    pub owner_scope: String,
    /// Monotonic queue revision.
    pub revision: u64,
    /// Maximum simultaneous active runs.
    pub max_active: u32,
    /// Maximum waiting runs retained in the queue.
    pub max_queued: u32,
    /// Scheduling priority applied to runs in this queue.
    pub priority: PriorityClass,
    /// Behavior applied when queue capacity is exhausted.
    pub overflow: OverflowPolicy,
    /// Integrity hash of canonical queue or run content.
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Immutable, validated execution and policy references captured for a run.
pub struct BackgroundRunSnapshot {
    /// Serialized schema version supported by this snapshot.
    pub schema_version: u32,
    /// Run identifier whose state is awaited.
    pub run_id: String,
    /// Source category of the background run.
    pub kind: BackgroundRunKind,
    /// Owner namespace controlling this queue.
    pub owner_scope: String,
    /// Reference to the immutable workflow or task definition.
    pub source_definition_ref: String,
    /// Exact source definition revision captured at admission.
    pub source_definition_revision: u64,
    /// Queue used for scheduling and capacity enforcement.
    pub queue_ref: String,
    /// Optional key used to serialize runs sharing a resource.
    pub concurrency_key: Option<String>,
    /// Scheduling priority applied to runs in this queue.
    pub priority: PriorityClass,
    /// Immutable environment snapshot used by the run.
    pub environment_snapshot_ref: String,
    /// Immutable execution policy snapshot reference.
    pub execution_policy_ref: String,
    /// Optional immutable approval policy snapshot reference.
    pub approval_policy_ref: Option<String>,
    /// Stable key used to deduplicate equivalent trigger requests.
    pub idempotency_key: Option<String>,
    /// Current lifecycle state of the run.
    pub state: RunState,
    /// Current execution attempt number.
    pub attempt: u32,
    /// Optional next wake time as Unix epoch milliseconds.
    pub next_wakeup_at_ms: Option<i64>,
    /// Integrity hash of canonical queue or run content.
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Unsupported contract, invalid reference, exceeded bound, or serialization failure.
pub enum ValidationError {
    /// Contract or schema version is unsupported.
    UnsupportedContract,
    /// A required identifier or reference is empty.
    Empty(&'static str),
    /// A reference exceeds its byte limit.
    TooLong(&'static str),
    /// A value violates a scheduling or state invariant.
    Invalid(&'static str),
    /// A queue, snapshot, or catch-up bound was exceeded.
    Limit(&'static str),
    /// A reference contains a secret-shaped value.
    SecretLike(&'static str),
    /// A value could not be serialized safely.
    Serialization,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ValidationError {}

fn validate_ref(name: &'static str, value: &str, limit: usize) -> Result<(), ValidationError> {
    if value.is_empty() {
        return Err(ValidationError::Empty(name));
    }
    if value.len() > limit {
        return Err(ValidationError::TooLong(name));
    }
    let lower = value.to_ascii_lowercase();
    if ["bearer ", "password=", "token=", "secret=", "api_key="]
        .iter()
        .any(|m| lower.contains(m))
    {
        return Err(ValidationError::SecretLike(name));
    }
    Ok(())
}

fn validate_key(name: &'static str, value: &str) -> Result<(), ValidationError> {
    validate_ref(name, value, MAX_REF_BYTES)?;
    if value.bytes().any(|byte| {
        !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'/' | b'.'))
    }) {
        return Err(ValidationError::Invalid(name));
    }
    Ok(())
}

impl BackgroundQueue {
    /// Validates schedule or wait-condition fields and time bounds.
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_ref("queue_id", &self.queue_id, MAX_ID_BYTES)?;
        validate_ref("owner_scope", &self.owner_scope, MAX_SCOPE_BYTES)?;
        if self.revision == 0 {
            return Err(ValidationError::Invalid("revision"));
        }
        if self.max_active == 0 || self.max_active > MAX_ACTIVE {
            return Err(ValidationError::Limit("max_active"));
        }
        if self.max_queued > MAX_QUEUE_DEPTH {
            return Err(ValidationError::Limit("max_queued"));
        }
        validate_ref("content_hash", &self.content_hash, MAX_ID_BYTES)
    }
}

impl BackgroundRunSnapshot {
    /// Validates schedule or wait-condition fields and time bounds.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedContract);
        }
        validate_ref("run_id", &self.run_id, MAX_ID_BYTES)?;
        validate_ref("owner_scope", &self.owner_scope, MAX_SCOPE_BYTES)?;
        validate_ref(
            "source_definition_ref",
            &self.source_definition_ref,
            MAX_REF_BYTES,
        )?;
        validate_key("queue_ref", &self.queue_ref)?;
        validate_key("environment_snapshot_ref", &self.environment_snapshot_ref)?;
        validate_key("execution_policy_ref", &self.execution_policy_ref)?;
        if let Some(value) = &self.concurrency_key {
            validate_key("concurrency_key", value)?;
        }
        if let Some(value) = &self.approval_policy_ref {
            validate_key("approval_policy_ref", value)?;
        }
        if self.source_definition_revision == 0 || self.attempt > 4096 {
            return Err(ValidationError::Invalid("snapshot_revision_or_attempt"));
        }
        if self.content_hash.len() > MAX_ID_BYTES {
            return Err(ValidationError::TooLong("content_hash"));
        }
        Ok(())
    }

    /// Validates the run snapshot and computes its bounded SHA-256 hash.
    pub fn canonical_hash(&self) -> Result<String, ValidationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| ValidationError::Serialization)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(ValidationError::Limit("snapshot"));
        }
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

/// Checks whether a lifecycle state transition is permitted.
pub fn transition_allowed(from: RunState, to: RunState) -> bool {
    use RunState::*;
    if from.terminal() {
        return false;
    }
    matches!(
        (from, to),
        (Accepted, Scheduled | Queued | Blocked | Cancelled)
            | (Scheduled, Queued | Blocked | Cancelling | Cancelled)
            | (
                Queued,
                Dispatching | Blocked | Cancelling | Cancelled | DeadLettered
            )
            | (Dispatching, Running | Blocked | Cancelling | Cancelled)
            | (
                Running,
                Waiting | RetryScheduled | Completed | Failed | Cancelling | Blocked
            )
            | (
                Waiting,
                Queued | RetryScheduled | Blocked | Cancelling | Cancelled
            )
            | (RetryScheduled, Queued | Blocked | Cancelling | Cancelled)
            | (Blocked, Queued | Cancelling | Cancelled | DeadLettered)
            | (Cancelling, Cancelled)
    )
}

/// Builds a deterministic idempotency key for a schedule occurrence.
pub fn schedule_fire_key(
    schedule_id: &str,
    revision: u64,
    logical_fire_ms: i64,
) -> Result<String, ValidationError> {
    validate_ref("schedule_id", schedule_id, MAX_ID_BYTES)?;
    if revision == 0 {
        return Err(ValidationError::Invalid("schedule_revision"));
    }
    Ok(format!("{schedule_id}:{revision}:{logical_fire_ms}"))
}

/// Returns whether a configured wake time has arrived.
pub fn due(now_ms: i64, wake_at_ms: Option<i64>) -> bool {
    wake_at_ms.is_some_and(|wake| now_ms >= wake)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> BackgroundRunSnapshot {
        BackgroundRunSnapshot {
            schema_version: SCHEMA_VERSION,
            run_id: "run-1".into(),
            kind: BackgroundRunKind::WorkflowRun,
            owner_scope: "workspace-1".into(),
            source_definition_ref: "workflow:1".into(),
            source_definition_revision: 1,
            queue_ref: "workflow-default".into(),
            concurrency_key: None,
            priority: PriorityClass::BackgroundWorkflow,
            environment_snapshot_ref: "env:1".into(),
            execution_policy_ref: "policy:1".into(),
            approval_policy_ref: None,
            idempotency_key: Some("fire-1".into()),
            state: RunState::Accepted,
            attempt: 0,
            next_wakeup_at_ms: None,
            content_hash: "hash".into(),
        }
    }

    #[test]
    fn snapshot_hash_is_bounded_and_stable() {
        assert_eq!(
            run().canonical_hash().unwrap(),
            run().canonical_hash().unwrap()
        );
    }

    #[test]
    fn transitions_are_fail_closed_and_terminal() {
        assert!(transition_allowed(RunState::Accepted, RunState::Queued));
        assert!(transition_allowed(RunState::Running, RunState::Completed));
        assert!(!transition_allowed(RunState::Completed, RunState::Queued));
        assert!(!transition_allowed(RunState::Running, RunState::Accepted));
    }

    #[test]
    fn secret_like_refs_are_rejected() {
        let mut value = run();
        value.execution_policy_ref = "token=raw".into();
        assert!(matches!(
            value.validate(),
            Err(ValidationError::SecretLike("execution_policy_ref"))
        ));
    }

    #[test]
    fn fire_identity_is_deterministic() {
        assert_eq!(
            schedule_fire_key("schedule", 2, 42).unwrap(),
            "schedule:2:42"
        );
    }

    #[test]
    fn fake_clock_wait_and_schedule_policies_are_bounded() {
        assert!(due(1_000, Some(1_000)));
        assert!(!due(999, Some(1_000)));
        assert!(ScheduleSpec::OneShotAt { wake_at_ms: 2_000 }
            .validate()
            .is_ok());
        assert!(ScheduleSpec::Interval {
            every_ms: 0,
            first_at_ms: 2_000
        }
        .validate()
        .is_err());
        assert!(WaitCondition::WaitForHumanWorkItem {
            work_item_id: "item-1".into(),
            timeout_at_ms: Some(2_000)
        }
        .validate()
        .is_ok());
    }
}
