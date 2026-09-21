use serde::{Deserialize, Serialize};

use crate::StorageError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRef {
    pub id: String,
    pub version: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRef {
    pub id: String,
    pub version: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySnapshot {
    pub schema_version: u32,
    pub policy_version: u32,
    pub effective_permissions_hash: String,
    pub canonical_json: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRouteSnapshot {
    pub requested_route: String,
    pub resolved_provider: String,
    pub resolved_model: String,
    pub route_policy_version: u32,
    pub canonical_json: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSnapshots {
    pub role_ref: RoleRef,
    pub skill_ref: SkillRef,
    pub policy: PolicySnapshot,
    pub model_route: ModelRouteSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub id: String,
    pub work_item_id: String,
    pub status: String,
    pub policy_snapshot: Vec<u8>,
    pub role_snapshot: Vec<u8>,
    pub skill_snapshot: Vec<u8>,
    pub model_route_snapshot: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCheckpointRecord {
    pub run_id: String,
    pub checkpoint_id: String,
    pub stage: String,
    pub node_id: String,
    pub attempt: u32,
    pub input_hash: String,
    pub state_json: Vec<u8>,
    pub pending_effects_json: Vec<u8>,
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEffectRecord {
    pub effect_id: String,
    pub run_id: String,
    pub node_id: String,
    pub kind: String,
    pub idempotency_key: String,
    pub immutable_intent_hash: String,
    pub state: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub result_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredRunRecord {
    pub run_id: String,
    pub work_item_id: String,
    pub effect_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLeaseRecord {
    pub run_id: String,
    pub lease_id: String,
    pub owner_id: String,
    pub generation: u64,
    pub lease_expires_at: String,
    pub heartbeat_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReconciliationRecord {
    pub effect_id: String,
    pub state: String,
    pub verifier: String,
    pub evidence_json: Vec<u8>,
    pub reconciled_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryState {
    Recovering,
    Reconciling,
    Resumable,
    Blocked,
    WaitingApproval,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecoveryRecord {
    pub id: i64,
    pub run_id: String,
    pub state: RecoveryState,
    pub effect_id: String,
    pub idempotency_key: String,
    pub verifier: String,
    pub evidence_json: Vec<u8>,
    pub decision: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsTableCount {
    pub table: String,
    pub rows: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsEventCount {
    pub event_type: String,
    pub rows: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSummary {
    pub table_counts: Vec<DiagnosticsTableCount>,
    pub event_counts: Vec<DiagnosticsEventCount>,
    pub total_events: i64,
    pub event_types_truncated: bool,
}

pub const MAX_DIAGNOSTICS_EVENT_TYPES: usize = 128;

/// Bounded, read-only recovery health facts. Distinct from
/// `recover_unknown_effects`, which mutates run/effect state; this snapshot
/// performs only SELECTs so it is safe for diagnostic use (e.g. Core Doctor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryHealthSnapshot {
    pub unknown_effects: i64,
    pub lease_expired: bool,
    pub resumable_runs: i64,
}

pub struct RecoveryTransitionInput<'a> {
    pub run_id: &'a str,
    pub next: RecoveryState,
    pub effect_id: &'a str,
    pub idempotency_key: &'a str,
    pub verifier: &'a str,
    pub evidence_json: &'a [u8],
    pub decision: &'a str,
}

pub struct ToolMetricInput<'a> {
    pub task_id: &'a str,
    pub tool_name: &'a str,
    pub iteration: i64,
    pub ok: bool,
    pub failure_kind: Option<&'a str>,
    pub recovery_hint: bool,
    pub escalated: bool,
}
