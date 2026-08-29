//! Core-owned Continuation Policy v1.
//!
//! This module is intentionally pure: it validates a bounded policy and
//! derives a decision from typed evidence. Dispatch, approvals and persistence
//! remain explicit callers, so model text cannot become authority.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const POLICY_SCHEMA_VERSION: u32 = 1;
pub const MAX_POLICY_BYTES: usize = 64 * 1024;
pub const MAX_GATES: usize = 32;
pub const MAX_GATE_ARGS: usize = 32;
pub const MAX_STRING: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    Workspace,
    Goal,
    Run,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeRef {
    pub kind: ScopeKind,
    pub owner_scope: String,
    pub workspace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetV1 {
    pub max_continuations: u32,
    pub max_model_turns: u32,
    pub max_tokens: Option<u64>,
    pub max_cost_micros: Option<u64>,
    pub max_wall_clock_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateArg {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GateArgs {
    Empty,
    Named { values: Vec<GateArg> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    Tool,
    Workflow,
    Evidence,
    Approval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredStatus {
    Passed,
    Approved,
    Fresh,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateV1 {
    pub id: String,
    pub kind: GateKind,
    pub capability_ref: String,
    pub args: GateArgs,
    pub required_status: RequiredStatus,
    pub timeout_ms: u64,
    pub retry: RetryPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionMode {
    GoalCriteriaAndGates,
    GatesOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationPolicyV1 {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub scope: ScopeRef,
    pub actor: String,
    pub enabled: bool,
    pub linked_goal_id: Option<String>,
    pub budget: BudgetV1,
    pub require_workspace_change_before_retry: bool,
    pub stop_on_user_interaction: bool,
    pub stop_on_approval_required: bool,
    pub stop_on_unknown_outcome: bool,
    pub gates: Vec<GateV1>,
    pub completion_mode: CompletionMode,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    Invalid(String),
    UnsupportedVersion(u32),
    Oversized,
    HashMismatch,
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(value) => write!(f, "invalid continuation policy: {value}"),
            Self::UnsupportedVersion(value) => {
                write!(f, "unsupported continuation policy version {value}")
            }
            Self::Oversized => f.write_str("continuation policy is oversized"),
            Self::HashMismatch => f.write_str("continuation policy content hash mismatch"),
        }
    }
}

impl std::error::Error for ContractError {}

impl ContinuationPolicyV1 {
    pub fn seal(mut self) -> Result<Self, ContractError> {
        self.content_hash.clear();
        self.validate_body()?;
        self.content_hash = self.compute_hash_unchecked()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != POLICY_SCHEMA_VERSION {
            return Err(ContractError::UnsupportedVersion(self.schema_version));
        }
        self.validate_body()?;
        if self.content_hash != self.compute_hash_unchecked()? {
            return Err(ContractError::HashMismatch);
        }
        if self.canonical_json()?.len() > MAX_POLICY_BYTES {
            return Err(ContractError::Oversized);
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, ContractError> {
        let mut unsigned = self.clone();
        unsigned.content_hash.clear();
        serde_json::to_vec(&unsigned).map_err(|e| ContractError::Invalid(e.to_string()))
    }

    pub fn compute_hash(&self) -> Result<String, ContractError> {
        self.compute_hash_unchecked()
    }

    fn compute_hash_unchecked(&self) -> Result<String, ContractError> {
        let bytes = self.canonical_json()?;
        let mut hasher = Sha256::new();
        hasher.update(b"evohime/continuation-policy/v1\0");
        hasher.update(bytes);
        Ok(hex::encode(hasher.finalize()))
    }

    fn validate_body(&self) -> Result<(), ContractError> {
        for (name, value) in [
            ("id", &self.id),
            ("actor", &self.actor),
            ("owner_scope", &self.scope.owner_scope),
            ("workspace_id", &self.scope.workspace_id),
        ] {
            if value.is_empty() || value.len() > MAX_STRING {
                return Err(ContractError::Invalid(format!("{name} bounds")));
            }
        }
        if self.revision == 0
            || self.scope.owner_scope.contains('\\')
            || self.scope.owner_scope.contains('/')
        {
            return Err(ContractError::Invalid("scope or revision".into()));
        }
        if self.gates.len() > MAX_GATES {
            return Err(ContractError::Invalid("too many gates".into()));
        }
        if self.budget.max_continuations == 0 || self.budget.max_model_turns == 0 {
            return Err(ContractError::Invalid("budgets must be positive".into()));
        }
        if self
            .budget
            .max_tokens
            .is_some_and(|value| value > i64::MAX as u64)
            || self
                .budget
                .max_cost_micros
                .is_some_and(|value| value > i64::MAX as u64)
            || self
                .budget
                .max_wall_clock_ms
                .is_some_and(|value| value > i64::MAX as u64)
        {
            return Err(ContractError::Invalid("budget value is too large".into()));
        }
        for gate in &self.gates {
            if gate.id.is_empty()
                || gate.id.len() > MAX_STRING
                || gate.capability_ref.is_empty()
                || gate.capability_ref.len() > MAX_STRING
            {
                return Err(ContractError::Invalid("gate identity bounds".into()));
            }
            if gate.timeout_ms == 0 || gate.retry.max_attempts == 0 || gate.retry.max_attempts > 16
            {
                return Err(ContractError::Invalid("gate retry/timeout bounds".into()));
            }
            if let GateArgs::Named { values } = &gate.args {
                if values.len() > MAX_GATE_ARGS
                    || values.windows(2).any(|pair| pair[0].key >= pair[1].key)
                {
                    return Err(ContractError::Invalid(
                        "gate args must be bounded and sorted".into(),
                    ));
                }
                if values.iter().any(|arg| {
                    arg.key.is_empty() || arg.key.len() > MAX_STRING || arg.value.len() > MAX_STRING
                }) {
                    return Err(ContractError::Invalid("gate arg bounds".into()));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Continue,
    Complete,
    PauseForApproval,
    Blocked,
    BudgetLimited,
    StopFailed,
    StopUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DecisionEvidence {
    pub required_gates_passed: bool,
    pub goal_criteria_complete: bool,
    pub pending_approval: bool,
    pub unknown_outcome: bool,
    pub retryable_failure: bool,
    pub non_retryable_failure: bool,
    pub workspace_changed: bool,
    pub no_progress_cycles: u32,
    pub max_no_progress_cycles: u32,
    pub user_stop: bool,
    pub continuation_index: u32,
    pub max_continuations: u32,
    pub model_turns: u32,
    pub max_model_turns: u32,
}

pub fn decide(evidence: &DecisionEvidence) -> Decision {
    if evidence.user_stop {
        return Decision::StopUser;
    }
    if evidence.pending_approval {
        return Decision::PauseForApproval;
    }
    if evidence.unknown_outcome {
        return Decision::Blocked;
    }
    if evidence.non_retryable_failure {
        return Decision::StopFailed;
    }
    if evidence.required_gates_passed && evidence.goal_criteria_complete {
        return Decision::Complete;
    }
    if evidence.model_turns >= evidence.max_model_turns
        || evidence.continuation_index >= evidence.max_continuations
    {
        return Decision::BudgetLimited;
    }
    if evidence.no_progress_cycles >= evidence.max_no_progress_cycles {
        return Decision::Blocked;
    }
    if evidence.retryable_failure && (!evidence.workspace_changed) {
        return Decision::Blocked;
    }
    Decision::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ContinuationPolicyV1 {
        ContinuationPolicyV1 {
            schema_version: 1,
            id: "p1".into(),
            revision: 1,
            scope: ScopeRef {
                kind: ScopeKind::Workspace,
                owner_scope: "owner-1".into(),
                workspace_id: "workspace-1".into(),
            },
            actor: "user".into(),
            enabled: true,
            linked_goal_id: None,
            budget: BudgetV1 {
                max_continuations: 2,
                max_model_turns: 3,
                max_tokens: None,
                max_cost_micros: None,
                max_wall_clock_ms: None,
            },
            require_workspace_change_before_retry: true,
            stop_on_user_interaction: true,
            stop_on_approval_required: true,
            stop_on_unknown_outcome: true,
            gates: vec![],
            completion_mode: CompletionMode::GatesOnly,
            created_at_ms: 1,
            updated_at_ms: 1,
            content_hash: String::new(),
        }
    }

    #[test]
    fn sealing_is_canonical_and_typed() {
        let sealed = policy().seal().unwrap();
        sealed.validate().unwrap();
        assert_eq!(sealed.content_hash, sealed.compute_hash().unwrap());
        assert!(serde_json::to_vec(&sealed).unwrap().len() < MAX_POLICY_BYTES);
    }

    #[test]
    fn decision_has_hard_stop_precedence() {
        assert_eq!(
            decide(&DecisionEvidence {
                user_stop: true,
                required_gates_passed: true,
                goal_criteria_complete: true,
                ..Default::default()
            }),
            Decision::StopUser
        );
        assert_eq!(
            decide(&DecisionEvidence {
                pending_approval: true,
                ..Default::default()
            }),
            Decision::PauseForApproval
        );
        assert_eq!(
            decide(&DecisionEvidence {
                required_gates_passed: true,
                goal_criteria_complete: true,
                max_continuations: 1,
                max_model_turns: 1,
                ..Default::default()
            }),
            Decision::Complete
        );
    }
}
