use serde::{Deserialize, Serialize};

use crate::StorageError;

/// Versioned reference to a role captured when a run was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRef {
    /// Stable role identifier.
    pub id: String,
    /// Role contract version.
    pub version: String,
    /// Hash of the role definition.
    pub hash: String,
}

/// Versioned reference to a skill captured when a run was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRef {
    /// Stable skill identifier.
    pub id: String,
    /// Skill contract version.
    pub version: String,
    /// Hash of the skill definition.
    pub hash: String,
}

/// Canonical policy and effective-permission snapshot for a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySnapshot {
    /// Snapshot schema version.
    pub schema_version: u32,
    /// Policy contract version.
    pub policy_version: u32,
    /// Hash of the effective permissions captured for this run.
    pub effective_permissions_hash: String,
    /// Canonical serialized policy snapshot.
    pub canonical_json: Vec<u8>,
}

/// Resolved provider and model route captured for a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRouteSnapshot {
    /// Route requested by the caller.
    pub requested_route: String,
    /// Provider selected after routing.
    pub resolved_provider: String,
    /// Model selected after routing.
    pub resolved_model: String,
    /// Version of the route policy used for resolution.
    pub route_policy_version: u32,
    /// Canonical serialized route snapshot.
    pub canonical_json: Vec<u8>,
}

/// Immutable configuration snapshots captured at run creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSnapshots {
    /// Role identity and version selected for the run.
    pub role_ref: RoleRef,
    /// Skill identity and version selected for the run.
    pub skill_ref: SkillRef,
    /// Policy state captured for the run.
    pub policy: PolicySnapshot,
    /// Model route captured for the run.
    pub model_route: ModelRouteSnapshot,
}

/// Stored run identity, status, and serialized configuration snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    /// Stable run identifier.
    pub id: String,
    /// Work item associated with the run.
    pub work_item_id: String,
    /// Current run lifecycle state.
    pub status: String,
    /// Serialized policy snapshot.
    pub policy_snapshot: Vec<u8>,
    /// Serialized role reference.
    pub role_snapshot: Vec<u8>,
    /// Serialized skill reference.
    pub skill_snapshot: Vec<u8>,
    /// Serialized model route snapshot.
    pub model_route_snapshot: Vec<u8>,
}

/// Durable checkpoint of a run node's state and pending effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCheckpointRecord {
    /// Run that owns the checkpoint.
    pub run_id: String,
    /// Stable checkpoint identifier.
    pub checkpoint_id: String,
    /// Execution stage captured by the checkpoint.
    pub stage: String,
    /// Graph node captured by the checkpoint.
    pub node_id: String,
    /// Attempt number for the node.
    pub attempt: u32,
    /// Hash of the checkpoint input.
    pub input_hash: String,
    /// Serialized node state.
    pub state_json: Vec<u8>,
    /// Serialized effects awaiting completion.
    pub pending_effects_json: Vec<u8>,
    /// Timestamp at which the checkpoint was committed.
    pub committed_at: String,
}

/// Durable execution state for an external or otherwise non-transactional run effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEffectRecord {
    /// Stable effect identifier.
    pub effect_id: String,
    /// Run that owns the effect.
    pub run_id: String,
    /// Node that produced the effect.
    pub node_id: String,
    /// Effect category.
    pub kind: String,
    /// Key used to make effect execution idempotent.
    pub idempotency_key: String,
    /// Hash of the immutable effect intent.
    pub immutable_intent_hash: String,
    /// Current effect lifecycle state.
    pub state: String,
    /// Time at which execution began.
    pub started_at: Option<String>,
    /// Time at which execution completed.
    pub completed_at: Option<String>,
    /// Hash of the effect result, when available.
    pub result_hash: Option<String>,
}

/// Run/effect pair recovered after an interruption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredRunRecord {
    /// Recovered run identifier.
    pub run_id: String,
    /// Work item associated with the recovered run.
    pub work_item_id: String,
    /// Effect whose outcome required recovery.
    pub effect_id: String,
    /// Effect category.
    pub kind: String,
}

/// Fenced lease metadata for one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLeaseRecord {
    /// Run protected by this lease.
    pub run_id: String,
    /// Lease identifier supplied by its owner.
    pub lease_id: String,
    /// Identity of the lease owner.
    pub owner_id: String,
    /// Monotonically increasing fencing generation.
    pub generation: u64,
    /// Lease expiration timestamp.
    pub lease_expires_at: String,
    /// Timestamp of the most recent heartbeat.
    pub heartbeat_at: String,
}

/// Persisted verifier evidence for reconciling a run effect's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReconciliationRecord {
    /// Effect whose outcome was checked.
    pub effect_id: String,
    /// Reconciliation state.
    pub state: String,
    /// Verifier that produced the evidence.
    pub verifier: String,
    /// Serialized reconciliation evidence.
    pub evidence_json: Vec<u8>,
    /// Reconciliation timestamp.
    pub reconciled_at: String,
}

/// Durable states of an interrupted run's recovery process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryState {
    /// Recovery has been started but evidence collection is pending.
    Recovering,
    /// Recovery is checking the outcome of pending effects.
    Reconciling,
    /// The run can safely resume.
    Resumable,
    /// Recovery cannot proceed without resolving a blocker.
    Blocked,
    /// Recovery is waiting for human approval.
    WaitingApproval,
    /// Recovery ended in failure.
    Failed,
}

impl RecoveryState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Recovering => "RECOVERING",
            Self::Reconciling => "RECONCILING",
            Self::Resumable => "RESUMABLE",
            Self::Blocked => "BLOCKED",
            Self::WaitingApproval => "WAITING_APPROVAL",
            Self::Failed => "FAILED",
        }
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Resumable | Self::Blocked | Self::WaitingApproval | Self::Failed
        )
    }

    pub(crate) fn parse(value: &str) -> Result<Self, StorageError> {
        match value {
            "RECOVERING" => Ok(Self::Recovering),
            "RECONCILING" => Ok(Self::Reconciling),
            "RESUMABLE" => Ok(Self::Resumable),
            "BLOCKED" => Ok(Self::Blocked),
            "WAITING_APPROVAL" => Ok(Self::WaitingApproval),
            "FAILED" => Ok(Self::Failed),
            other => Err(StorageError::InvalidRecovery(format!(
                "unknown recovery state {other}"
            ))),
        }
    }
}

/// Durable recovery state and evidence for one run effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecoveryRecord {
    /// Database row identifier.
    pub id: i64,
    /// Run being recovered.
    pub run_id: String,
    /// Current recovery lifecycle state.
    pub state: RecoveryState,
    /// Effect whose outcome is being recovered.
    pub effect_id: String,
    /// Key used to deduplicate recovery transitions.
    pub idempotency_key: String,
    /// Verifier responsible for the recovery evidence.
    pub verifier: String,
    /// Serialized evidence used to make the recovery decision.
    pub evidence_json: Vec<u8>,
    /// Recovery decision recorded by Core.
    pub decision: String,
    /// Recovery record creation timestamp.
    pub created_at: String,
}

/// Row count for one table in a storage diagnostics report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsTableCount {
    /// Table name.
    pub table: String,
    /// Number of rows in the table.
    pub rows: i64,
}

/// Event count for one event type in a storage diagnostics report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsEventCount {
    /// Event type name.
    pub event_type: String,
    /// Number of events of that type.
    pub rows: i64,
}

/// Bounded summary of database tables and event types for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSummary {
    /// Row counts for selected storage tables.
    pub table_counts: Vec<DiagnosticsTableCount>,
    /// Counts for event types, bounded by [`MAX_DIAGNOSTICS_EVENT_TYPES`].
    pub event_counts: Vec<DiagnosticsEventCount>,
    /// Total number of events across all event types.
    pub total_events: i64,
    /// Whether event-type counts were truncated to the configured bound.
    pub event_types_truncated: bool,
}

/// Maximum number of distinct event types included in a diagnostics summary.
pub const MAX_DIAGNOSTICS_EVENT_TYPES: usize = 128;

/// Bounded, read-only recovery health facts. Distinct from
/// `recover_unknown_effects`, which mutates run/effect state; this snapshot
/// performs only SELECTs so it is safe for diagnostic use (e.g. Core Doctor).
/// Read-only recovery health facts safe to expose in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryHealthSnapshot {
    /// Number of effects whose outcome is still unknown.
    pub unknown_effects: i64,
    /// Whether a relevant run lease has expired.
    pub lease_expired: bool,
    /// Number of runs currently available to resume.
    pub resumable_runs: i64,
}

/// Input fields for one idempotent recovery-state transition.
pub struct RecoveryTransitionInput<'a> {
    /// Run whose recovery state is changing.
    pub run_id: &'a str,
    /// Target recovery state.
    pub next: RecoveryState,
    /// Effect associated with the transition.
    pub effect_id: &'a str,
    /// Key used to deduplicate the transition.
    pub idempotency_key: &'a str,
    /// Verifier that produced the supplied evidence.
    pub verifier: &'a str,
    /// Serialized evidence supporting the transition.
    pub evidence_json: &'a [u8],
    /// Decision recorded by the transition.
    pub decision: &'a str,
}

/// Input values recorded for one tool invocation metric.
pub struct ToolMetricInput<'a> {
    /// Task that invoked the tool.
    pub task_id: &'a str,
    /// Name of the invoked tool.
    pub tool_name: &'a str,
    /// Tool invocation iteration within the task.
    pub iteration: i64,
    /// Whether the invocation succeeded.
    pub ok: bool,
    /// Failure category, if the invocation failed.
    pub failure_kind: Option<&'a str>,
    /// Whether a recovery hint was available.
    pub recovery_hint: bool,
    /// Whether the outcome was escalated.
    pub escalated: bool,
}
