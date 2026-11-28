//! Core-owned Team Resource Budget contract and validation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema version accepted by team resource budget contracts.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum identifier length accepted by this contract.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum allocation entries in one team budget policy.
pub const MAX_ALLOCATIONS: usize = 64;
/// Maximum byte length for a request reason or next-work description.
pub const MAX_REASON_BYTES: usize = 512;

/// Policy for assigning unspent capacity to additional work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReallocationMode {
    /// Keep allocation amounts fixed for the policy revision.
    Fixed,
    /// Allow configured allocations to borrow from shared unspent capacity.
    AutoFromUnspentPool,
    /// Allow automatic movement while preserving hard caps.
    AutoWithinCap,
    /// Require a human approval before reallocation.
    HumanApproved,
}
/// Aggregate state of budget availability and reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    /// Within configured soft and hard resource limits.
    Active,
    /// The charge fits the hard limit but crosses a warning threshold.
    SoftWarning,
    /// The proposed charge would exceed a hard limit.
    BudgetBlocked,
    /// Some usage remains unreported or unreconciled.
    Incomplete,
    /// Budget health cannot currently be determined.
    Unknown,
}
/// Elapsed-time accounting policy for budget wall-clock limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallClockMode {
    /// Count only time while work is actively executing.
    ActiveOnly,
    /// Count execution and time spent waiting on dependencies.
    ActiveAndWaiting,
    /// Count all elapsed time since the work began.
    AllElapsed,
}

/// Optional ceilings for cost, token, call, and elapsed-time resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    /// Optional maximum monetary cost in millionths of the configured currency unit.
    pub max_cost_micros: Option<u64>,
    /// Optional maximum number of input tokens.
    pub max_input_tokens: Option<u64>,
    /// Optional maximum number of output tokens.
    pub max_output_tokens: Option<u64>,
    /// Optional maximum number of model requests.
    pub max_model_calls: Option<u64>,
    /// Optional maximum number of tool invocations.
    pub max_tool_calls: Option<u64>,
    /// Optional wall-clock duration limit in milliseconds.
    pub max_wall_clock_ms: Option<u64>,
}
/// Resource limits and reserve permissions for one team subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetAllocation {
    /// Stable identifier for this budget object.
    pub id: String,
    /// Category of team member or phase this allocation applies to.
    pub subject_kind: String,
    /// Identifier of the allocated subject.
    pub subject_ref: String,
    /// Threshold that triggers a warning for the subject.
    pub soft_limit: ResourceLimits,
    /// Limit that blocks additional resource use.
    pub hard_limit: ResourceLimits,
    /// Relative priority used for allocation decisions.
    pub priority: u8,
    /// Whether unused shared capacity may be borrowed.
    pub borrow_from_unspent_pool: bool,
    /// Whether this allocation may consume protected reserve capacity.
    pub reserve_access: bool,
}
/// Versioned aggregate budget, allocation, and reserve policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamBudgetPolicy {
    /// Serialized schema version supported by this contract.
    pub schema_version: u32,
    /// Stable identifier for this budget object.
    pub id: String,
    /// Monotonic policy or state revision.
    pub version: u64,
    /// Hard aggregate resource limits for the team session.
    pub total_limits: ResourceLimits,
    /// Per-subject resource limits and borrowing policy.
    pub allocations: Vec<BudgetAllocation>,
    /// Capacity held back for explicitly authorized use.
    pub protected_reserve: ResourceLimits,
    /// Policy for moving unused allocation between subjects.
    pub reallocation_mode: ReallocationMode,
    /// Which elapsed intervals count against wall-clock limits.
    pub wall_clock_mode: WallClockMode,
    /// Percent of a soft limit that raises a warning.
    pub warning_threshold_percent: u8,
    /// Whether work with unpriced resource usage may proceed.
    pub allow_unknown_cost: bool,
    /// Integrity hash of canonical policy content.
    pub content_hash: String,
}
/// Spent and reserved usage associated with one allocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationState {
    /// Identifier of the allocation represented by this state.
    pub allocation_id: String,
    /// Resources charged to this allocation so far.
    pub spent: ResourceLimits,
    /// Resources reserved for work not yet reconciled.
    pub reserved: ResourceLimits,
}
/// Versioned usage and reservation totals for one team session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamBudgetState {
    /// Serialized schema version supported by this contract.
    pub schema_version: u32,
    /// Team session that owns this state or usage event.
    pub team_session_id: String,
    /// Policy revision used to interpret the accumulated usage.
    pub policy_version: u64,
    /// Aggregate usage charged to the team session.
    pub total_spent: ResourceLimits,
    /// Aggregate capacity reserved for pending work.
    pub total_reserved: ResourceLimits,
    /// Usage and reservation totals for each allocation.
    pub allocations_state: Vec<AllocationState>,
    /// Remaining protected reserve capacity.
    pub reserve_remaining: ResourceLimits,
    /// Number of estimates awaiting reconciliation.
    pub pending_estimates: u64,
    /// Current aggregate budget health.
    pub status: BudgetStatus,
    /// Monotonic policy or state revision.
    pub version: u64,
    /// Last state update time in Unix epoch milliseconds.
    pub updated_at_ms: i64,
}
/// Observed or estimated resource consumption for one operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUsageEvent {
    /// Serialized schema version supported by this contract.
    pub schema_version: u32,
    /// Stable identifier for this budget object.
    pub id: String,
    /// Team session that owns this state or usage event.
    pub team_session_id: String,
    /// Optional participant instance that produced the usage.
    pub role_instance_id: Option<String>,
    /// Optional workflow phase associated with the usage.
    pub phase_id: Option<String>,
    /// Execution run that incurred the usage.
    pub run_id: String,
    /// Category of operation that consumed resources.
    pub operation_kind: String,
    /// Optional provider that reported the usage.
    pub provider: Option<String>,
    /// Optional model identifier that incurred the usage.
    pub model: Option<String>,
    /// Optional tool identifier that incurred the usage.
    pub tool_ref: Option<String>,
    /// Observed or estimated input token usage.
    pub input_tokens: Option<u64>,
    /// Observed or estimated output token usage.
    pub output_tokens: Option<u64>,
    /// Observed or estimated monetary cost in millionths.
    pub cost_micros: Option<u64>,
    /// Elapsed duration charged by this event in milliseconds.
    pub duration_ms: u64,
    /// Whether the values were estimated before execution.
    pub estimated_before: bool,
    /// Whether the usage requires reconciliation.
    pub uncertain: bool,
    /// Observation time as Unix epoch milliseconds.
    pub observed_at_ms: i64,
}
/// Bounded request for additional team-session resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRequest {
    /// Stable identifier for this budget object.
    pub id: String,
    /// Team session that owns this state or usage event.
    pub team_session_id: String,
    /// Identity requesting additional budget.
    pub requester: String,
    /// Additional resource limits requested.
    pub requested: ResourceLimits,
    /// Stable reason for the requested budget change.
    pub reason_code: String,
    /// Bounded description of the next planned work.
    pub expected_next_work: String,
}

/// Preflight outcome for a proposed resource charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeDecision {
    /// The proposed charge fits the configured budget.
    Allowed,
    /// The charge fits the hard limit but crosses a warning threshold.
    SoftWarning,
    /// The proposed charge would exceed a hard limit.
    BudgetBlocked,
    /// The charge cannot be priced under the active policy.
    UnknownCost,
}

/// Validation, limit, reserve, or reconciliation failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BudgetError {
    /// The budget schema version is unsupported.
    #[error("unsupported team budget schema")]
    Version,
    /// A budget field or invariant is invalid.
    #[error("team budget field is invalid")]
    Invalid,
    /// A serialized value exceeds its byte bound.
    #[error("team budget field is too large")]
    TooLarge,
    /// A configured resource limit would be exceeded.
    #[error("team budget limit exceeded")]
    Limit,
    /// The requester cannot consume protected reserve capacity.
    #[error("protected reserve access denied")]
    ReserveDenied,
    /// Usage must be reconciled before another charge is accepted.
    #[error("unknown usage requires reconciliation")]
    UsageUncertain,
}

fn pair(soft: Option<u64>, hard: Option<u64>) -> bool {
    soft.zip(hard).is_none_or(|(s, h)| s <= h)
}
/// Validates the budget policy, allocation bounds, and soft/hard limit ordering.
pub fn validate_policy(p: &TeamBudgetPolicy) -> Result<(), BudgetError> {
    if p.schema_version != SCHEMA_VERSION {
        return Err(BudgetError::Version);
    }
    if p.id.is_empty()
        || p.id.len() > MAX_ID_BYTES
        || p.version == 0
        || p.allocations.len() > MAX_ALLOCATIONS
        || p.warning_threshold_percent == 0
        || p.warning_threshold_percent > 100
        || p.content_hash.len() != 64
    {
        return Err(BudgetError::Invalid);
    }
    for a in &p.allocations {
        if a.id.is_empty()
            || a.id.len() > MAX_ID_BYTES
            || a.subject_kind.is_empty()
            || a.subject_ref.is_empty()
            || !pair(a.soft_limit.max_cost_micros, a.hard_limit.max_cost_micros)
            || !pair(a.soft_limit.max_input_tokens, a.hard_limit.max_input_tokens)
            || !pair(
                a.soft_limit.max_output_tokens,
                a.hard_limit.max_output_tokens,
            )
            || !pair(a.soft_limit.max_model_calls, a.hard_limit.max_model_calls)
            || !pair(a.soft_limit.max_tool_calls, a.hard_limit.max_tool_calls)
            || !pair(
                a.soft_limit.max_wall_clock_ms,
                a.hard_limit.max_wall_clock_ms,
            )
        {
            return Err(BudgetError::Invalid);
        }
    }
    Ok(())
}
/// Returns the SHA-256 hash of the policy with its content hash cleared.
pub fn canonical_hash(p: &TeamBudgetPolicy) -> Result<String, BudgetError> {
    let mut copy = p.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| BudgetError::Invalid)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
/// Validates policy constraints and verifies its canonical integrity hash.
pub fn validate_hash(p: &TeamBudgetPolicy) -> Result<(), BudgetError> {
    validate_policy(p)?;
    if canonical_hash(p)? != p.content_hash {
        return Err(BudgetError::Invalid);
    }
    Ok(())
}
/// Checks identifiers, bounds, and required fields in a budget request.
pub fn validate_request(r: &BudgetRequest) -> Result<(), BudgetError> {
    if r.id.is_empty()
        || r.id.len() > MAX_ID_BYTES
        || r.team_session_id.is_empty()
        || r.requester.is_empty()
        || r.reason_code.is_empty()
        || r.reason_code.len() > MAX_REASON_BYTES
        || r.expected_next_work.len() > MAX_REASON_BYTES
    {
        return Err(BudgetError::Invalid);
    }
    Ok(())
}

fn exceeds(
    used: Option<u64>,
    reservation: Option<u64>,
    estimate: Option<u64>,
    limit: Option<u64>,
) -> bool {
    limit.is_some_and(|max| {
        used.unwrap_or(0)
            .saturating_add(reservation.unwrap_or(0))
            .saturating_add(estimate.unwrap_or(0))
            > max
    })
}

/// Checks whether an estimated charge is permitted by the current policy and usage state.
pub fn preflight_charge(
    state: &TeamBudgetState,
    policy: &TeamBudgetPolicy,
    estimate: &ResourceLimits,
    reserve_access: bool,
    unknown_cost: bool,
) -> Result<ChargeDecision, BudgetError> {
    validate_hash(policy)?;
    if unknown_cost && !policy.allow_unknown_cost {
        return Ok(ChargeDecision::UnknownCost);
    }
    if reserve_access && !policy.allocations.iter().any(|a| a.reserve_access) {
        return Err(BudgetError::ReserveDenied);
    }
    if exceeds(
        state.total_spent.max_cost_micros,
        state.total_reserved.max_cost_micros,
        estimate.max_cost_micros,
        policy.total_limits.max_cost_micros,
    ) || exceeds(
        state.total_spent.max_input_tokens,
        state.total_reserved.max_input_tokens,
        estimate.max_input_tokens,
        policy.total_limits.max_input_tokens,
    ) || exceeds(
        state.total_spent.max_output_tokens,
        state.total_reserved.max_output_tokens,
        estimate.max_output_tokens,
        policy.total_limits.max_output_tokens,
    ) || exceeds(
        state.total_spent.max_model_calls,
        state.total_reserved.max_model_calls,
        estimate.max_model_calls,
        policy.total_limits.max_model_calls,
    ) || exceeds(
        state.total_spent.max_tool_calls,
        state.total_reserved.max_tool_calls,
        estimate.max_tool_calls,
        policy.total_limits.max_tool_calls,
    ) || exceeds(
        state.total_spent.max_wall_clock_ms,
        state.total_reserved.max_wall_clock_ms,
        estimate.max_wall_clock_ms,
        policy.total_limits.max_wall_clock_ms,
    ) {
        return Ok(ChargeDecision::BudgetBlocked);
    }
    Ok(ChargeDecision::Allowed)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> TeamBudgetPolicy {
        let mut p = TeamBudgetPolicy {
            schema_version: 1,
            id: "team".into(),
            version: 1,
            total_limits: ResourceLimits {
                max_cost_micros: Some(100),
                ..Default::default()
            },
            allocations: vec![BudgetAllocation {
                id: "reviewer".into(),
                subject_kind: "role_slot".into(),
                subject_ref: "reviewer".into(),
                soft_limit: ResourceLimits {
                    max_cost_micros: Some(50),
                    ..Default::default()
                },
                hard_limit: ResourceLimits {
                    max_cost_micros: Some(80),
                    ..Default::default()
                },
                priority: 1,
                borrow_from_unspent_pool: false,
                reserve_access: true,
            }],
            protected_reserve: ResourceLimits {
                max_cost_micros: Some(20),
                ..Default::default()
            },
            reallocation_mode: ReallocationMode::HumanApproved,
            wall_clock_mode: WallClockMode::ActiveOnly,
            warning_threshold_percent: 80,
            allow_unknown_cost: false,
            content_hash: String::new(),
        };
        p.content_hash = canonical_hash(&p).unwrap();
        p
    }
    #[test]
    fn policy_hash_is_validated() {
        assert!(validate_hash(&p()).is_ok());
    }
    #[test]
    fn invalid_soft_limit_fails_closed() {
        let mut x = p();
        x.allocations[0].soft_limit.max_cost_micros = Some(90);
        assert_eq!(validate_policy(&x), Err(BudgetError::Invalid));
    }

    #[test]
    fn preflight_blocks_shared_cap_and_unknown_cost() {
        let p = p();
        let state = TeamBudgetState {
            schema_version: 1,
            team_session_id: "s".into(),
            policy_version: 1,
            total_spent: ResourceLimits {
                max_cost_micros: Some(90),
                ..Default::default()
            },
            total_reserved: ResourceLimits::default(),
            allocations_state: Vec::new(),
            reserve_remaining: ResourceLimits::default(),
            pending_estimates: 0,
            status: BudgetStatus::Active,
            version: 1,
            updated_at_ms: 1,
        };
        assert_eq!(
            preflight_charge(
                &state,
                &p,
                &ResourceLimits {
                    max_cost_micros: Some(20),
                    ..Default::default()
                },
                false,
                false
            )
            .unwrap(),
            ChargeDecision::BudgetBlocked
        );
        assert_eq!(
            preflight_charge(
                &TeamBudgetState {
                    total_spent: ResourceLimits::default(),
                    ..state
                },
                &p,
                &ResourceLimits::default(),
                false,
                true
            )
            .unwrap(),
            ChargeDecision::UnknownCost
        );
    }
}
