//! Versioned Core-owned contract for repeatable automation triggers (plan 16.1).
//!
//! This contract deliberately does not contain an executor or a scheduler.  A
//! definition is immutable input; the runtime binds a run to its revision and
//! to immutable policy/approval snapshots before it can execute any effect.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current version identifier for serialized automation definitions.
pub const AUTOMATION_CONTRACT_VERSION: &str = "automation/v1";
/// Maximum byte length accepted for automation identifiers.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum activity references in one automation graph.
pub const MAX_GRAPH_ACTIVITIES: usize = 64;
/// Maximum serialized input or definition payload size.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// Maximum retained automation history events.
pub const MAX_HISTORY_EVENTS: usize = 256;
/// Maximum concurrent automation runs accepted by this contract.
pub const MAX_PARALLELISM: u32 = 8;
/// Minimum idempotency-key retention interval in milliseconds.
pub const IDEMPOTENCY_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Immutable, versioned trigger and execution policy for one automation.
pub struct AutomationDefinitionV1 {
    /// Version identifier that must match the supported contract.
    pub contract: String,
    /// Automation identifier selected by the trigger.
    pub definition_id: String,
    /// Exact definition revision requested.
    pub revision: u64,
    /// Owner namespace that authorizes the trigger.
    pub owner_scope: String,
    /// Reference to the workflow graph executed by the automation.
    pub graph_ref: String,
    /// Activity references permitted in the graph.
    pub activities: Vec<ActivityRef>,
    /// Trigger sources enabled for the automation.
    pub trigger_policy: TriggerPolicy,
    /// Concurrent run and queue limits.
    pub concurrency: ConcurrencyPolicy,
    /// Retry behavior applied to retryable failures.
    pub retry: RetryPolicy,
    /// Capabilities the automation may request, still subject to runtime policy.
    pub capabilities: Vec<String>,
    /// Approval boundary required before effects execute.
    pub approval_mode: ApprovalMode,
    /// Schema used to validate trigger input.
    pub input_schema: String,
    /// History retention period in days.
    pub retention_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Reference to an activity inside the automation graph.
pub struct ActivityRef {
    /// Activity that produced this outcome.
    pub activity_id: String,
    /// Reference to the activity implementation or block.
    pub block_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Allowed manual, scheduled, and keyed trigger sources.
pub struct TriggerPolicy {
    /// Whether an explicit manual trigger is allowed.
    pub manual: bool,
    /// Optional schedule expression for recurring triggers.
    pub schedule: Option<String>,
    /// Allowlisted event keys that may start a run.
    pub trigger_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Queue and concurrent-run bounds for an automation.
pub struct ConcurrencyPolicy {
    /// Maximum simultaneous runs.
    pub max_concurrent: u32,
    /// Maximum trigger requests waiting in the queue.
    pub queue_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Bounded retry rules for failed automation work.
pub struct RetryPolicy {
    /// Maximum attempts for one activity execution.
    pub max_attempts: u32,
    /// Error codes eligible for another attempt.
    pub retryable_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Approval requirement applied before automation effects.
pub enum ApprovalMode {
    /// Do not require a separate approval gate for automation effects.
    Never,
    /// Require approval when the run is about to perform an effect.
    OnEffect,
    /// Require approval before every automation run.
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Unsupported contract or invalid automation input.
pub enum AutomationValidationError {
    /// The definition uses an unsupported contract version.
    UnsupportedContract,
    /// A required field is empty.
    Empty(&'static str),
    /// A configured size or count bound was exceeded.
    Limit(&'static str),
    /// A field or invariant failed validation.
    Invalid(&'static str),
    /// The contract major version is not recognized.
    UnknownMajor,
}

impl std::fmt::Display for AutomationValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AutomationValidationError {}

impl AutomationDefinitionV1 {
    /// Validates this automation definition against supported contract and size bounds.
    pub fn validate(&self) -> Result<(), AutomationValidationError> {
        if self.contract != AUTOMATION_CONTRACT_VERSION {
            return Err(AutomationValidationError::UnsupportedContract);
        }
        for (name, value) in [
            ("definition_id", &self.definition_id),
            ("owner_scope", &self.owner_scope),
            ("graph_ref", &self.graph_ref),
            ("input_schema", &self.input_schema),
        ] {
            if value.is_empty() {
                return Err(AutomationValidationError::Empty(name));
            }
            if value.len() > MAX_INPUT_BYTES {
                return Err(AutomationValidationError::Limit(name));
            }
        }
        if self.revision == 0 {
            return Err(AutomationValidationError::Invalid("revision"));
        }
        if self.activities.is_empty() || self.activities.len() > MAX_GRAPH_ACTIVITIES {
            return Err(AutomationValidationError::Limit("activities"));
        }
        if self.activities.iter().any(|a| {
            a.activity_id.is_empty()
                || a.activity_id.len() > MAX_ID_BYTES
                || a.block_ref.is_empty()
                || a.block_ref.len() > MAX_ID_BYTES
        }) {
            return Err(AutomationValidationError::Invalid("activity"));
        }
        if self
            .capabilities
            .iter()
            .any(|v| v.is_empty() || v.len() > MAX_ID_BYTES)
        {
            return Err(AutomationValidationError::Invalid("capability"));
        }
        if self.concurrency.max_concurrent == 0
            || self.concurrency.max_concurrent > MAX_PARALLELISM
            || self.concurrency.queue_limit > 256
        {
            return Err(AutomationValidationError::Limit("concurrency"));
        }
        if self.retry.max_attempts > 2 {
            return Err(AutomationValidationError::Limit("retry"));
        }
        if self.retention_days == 0 || self.retention_days > 365 {
            return Err(AutomationValidationError::Limit("retention_days"));
        }
        if self
            .trigger_policy
            .trigger_keys
            .iter()
            .any(|v| v.is_empty() || v.len() > MAX_ID_BYTES)
        {
            return Err(AutomationValidationError::Invalid("trigger_key"));
        }
        Ok(())
    }
    /// Validates the definition and returns its SHA-256 content hash.
    pub fn canonical_hash(&self) -> Result<String, AutomationValidationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| AutomationValidationError::Invalid("definition"))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Validated request to start one automation run.
pub struct TriggerRequestV1 {
    /// Owner namespace that authorizes the trigger.
    pub owner_scope: String,
    /// Automation identifier selected by the trigger.
    pub definition_id: String,
    /// Exact definition revision requested.
    pub revision: u64,
    /// Manual, scheduled, or event key that caused the request.
    pub trigger_key: String,
    /// Optional canonical scheduled time slot used for deduplication.
    pub scheduled_slot: Option<String>,
    /// Serialized trigger input validated against the definition schema.
    pub input_json: String,
    /// Request tracing identifier shared with downstream events.
    pub correlation_id: String,
    /// Stable key used to deduplicate trigger retries.
    pub idempotency_key: String,
}

impl TriggerRequestV1 {
    /// Validates this automation definition against supported contract and size bounds.
    pub fn validate(&self) -> Result<(), AutomationValidationError> {
        for (name, value) in [
            ("owner_scope", &self.owner_scope),
            ("definition_id", &self.definition_id),
            ("trigger_key", &self.trigger_key),
            ("correlation_id", &self.correlation_id),
            ("idempotency_key", &self.idempotency_key),
        ] {
            if value.is_empty() {
                return Err(AutomationValidationError::Empty(name));
            }
            if value.len() > MAX_ID_BYTES {
                return Err(AutomationValidationError::Limit(name));
            }
        }
        if self.input_json.len() > MAX_INPUT_BYTES {
            return Err(AutomationValidationError::Limit("input_json"));
        }
        if self.revision == 0 {
            return Err(AutomationValidationError::Invalid("revision"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of an admitted automation run.
pub enum AutomationRunState {
    /// The trigger passed admission and is bound to policy snapshots.
    Admitted,
    /// The run is waiting for an execution slot.
    Queued,
    /// The runtime is preparing the execution context.
    Starting,
    /// Activities are executing.
    Running,
    /// The run is paused until its approval request is resolved.
    WaitingApproval,
    /// The run is paused by an explicit control decision.
    Paused,
    /// A retryable activity failure is scheduled for another attempt.
    Retrying,
    /// Cancellation was requested and cleanup is in progress.
    Cancelling,
    /// All required activities completed successfully.
    Completed,
    /// The run ended with an unrecoverable failure.
    Failed,
    /// The run was cancelled.
    Cancelled,
    /// The run exhausted recovery or retry handling and needs intervention.
    DeadLetter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Run identity and immutable policy snapshots captured at admission.
pub struct AutomationRunV1 {
    /// Automation run that emitted this event.
    pub run_id: String,
    /// Validated trigger request that admitted the run.
    pub request: TriggerRequestV1,
    /// Integrity hash of the immutable automation definition.
    pub definition_hash: String,
    /// Permission policy snapshot bound to this run.
    pub permission_snapshot: String,
    /// Approval policy snapshot bound to this run.
    pub approval_snapshot: String,
    /// Run recovery generation for stale-event detection.
    pub generation: u64,
    /// Current run lifecycle state.
    pub state: AutomationRunState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Ordered activity outcome emitted by one automation run.
pub struct ActivityEventV1 {
    /// Stable activity event identifier.
    pub event_id: String,
    /// Automation run that emitted this event.
    pub run_id: String,
    /// Run recovery generation for stale-event detection.
    pub generation: u64,
    /// Monotonic event sequence within the run.
    pub sequence: u64,
    /// Activity that produced this outcome.
    pub activity_id: String,
    /// Attempt number for the activity.
    pub attempt: u32,
    /// Stable activity outcome code.
    pub outcome: String,
    /// Bounded diagnostic summary for the activity.
    pub diagnostics: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Summary of active and recently completed automation runs.
pub struct AutomationHealthV1 {
    /// Automation identifier selected by the trigger.
    pub definition_id: String,
    /// Exact definition revision requested.
    pub revision: u64,
    /// Number of runs currently executing or awaiting approval.
    pub active_runs: u32,
    /// Number of admitted runs awaiting execution.
    pub queued_runs: u32,
    /// Terminal state of the most recently completed run, if any.
    pub last_terminal_state: Option<AutomationRunState>,
    /// Stable error code from the latest failed run, if any.
    pub last_error_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition() -> AutomationDefinitionV1 {
        AutomationDefinitionV1 {
            contract: AUTOMATION_CONTRACT_VERSION.into(),
            definition_id: "daily.sync".into(),
            revision: 1,
            owner_scope: "owner".into(),
            graph_ref: "workflow:sync".into(),
            activities: vec![ActivityRef {
                activity_id: "sync".into(),
                block_ref: "workspace.read".into(),
            }],
            trigger_policy: TriggerPolicy {
                manual: true,
                schedule: None,
                trigger_keys: vec!["manual".into()],
            },
            concurrency: ConcurrencyPolicy {
                max_concurrent: 1,
                queue_limit: 8,
            },
            retry: RetryPolicy {
                max_attempts: 2,
                retryable_errors: vec!["provider_timeout".into()],
            },
            capabilities: vec!["workspace.read".into()],
            approval_mode: ApprovalMode::OnEffect,
            input_schema: "{}".into(),
            retention_days: 30,
        }
    }
    #[test]
    fn validates_and_hashes_stably() {
        let d = definition();
        assert!(d.validate().is_ok());
        assert_eq!(d.canonical_hash().unwrap(), d.canonical_hash().unwrap());
    }
    #[test]
    fn rejects_unsafe_retry_and_unknown_contract() {
        let mut d = definition();
        d.retry.max_attempts = 3;
        assert!(matches!(
            d.validate(),
            Err(AutomationValidationError::Limit("retry"))
        ));
        d.contract = "automation/v9".into();
        assert!(matches!(
            d.validate(),
            Err(AutomationValidationError::UnsupportedContract)
        ));
    }
    #[test]
    fn trigger_requires_bounded_identity_and_input() {
        let mut r = TriggerRequestV1 {
            owner_scope: "o".into(),
            definition_id: "d".into(),
            revision: 1,
            trigger_key: "k".into(),
            scheduled_slot: None,
            input_json: "{}".into(),
            correlation_id: "c".into(),
            idempotency_key: "i".into(),
        };
        assert!(r.validate().is_ok());
        r.input_json = "x".repeat(MAX_INPUT_BYTES + 1);
        assert!(matches!(
            r.validate(),
            Err(AutomationValidationError::Limit("input_json"))
        ));
    }
}
