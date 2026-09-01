//! Core-owned Team Resource Budget contract and validation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_ALLOCATIONS: usize = 64;
pub const MAX_REASON_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReallocationMode {
    Fixed,
    AutoFromUnspentPool,
    AutoWithinCap,
    HumanApproved,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    Active,
    SoftWarning,
    BudgetBlocked,
    Incomplete,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallClockMode {
    ActiveOnly,
    ActiveAndWaiting,
    AllElapsed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    pub max_cost_micros: Option<u64>,
    pub max_input_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub max_model_calls: Option<u64>,
    pub max_tool_calls: Option<u64>,
    pub max_wall_clock_ms: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetAllocation {
    pub id: String,
    pub subject_kind: String,
    pub subject_ref: String,
    pub soft_limit: ResourceLimits,
    pub hard_limit: ResourceLimits,
    pub priority: u8,
    pub borrow_from_unspent_pool: bool,
    pub reserve_access: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamBudgetPolicy {
    pub schema_version: u32,
    pub id: String,
    pub version: u64,
    pub total_limits: ResourceLimits,
    pub allocations: Vec<BudgetAllocation>,
    pub protected_reserve: ResourceLimits,
    pub reallocation_mode: ReallocationMode,
    pub wall_clock_mode: WallClockMode,
    pub warning_threshold_percent: u8,
    pub allow_unknown_cost: bool,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationState {
    pub allocation_id: String,
    pub spent: ResourceLimits,
    pub reserved: ResourceLimits,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamBudgetState {
    pub schema_version: u32,
    pub team_session_id: String,
    pub policy_version: u64,
    pub total_spent: ResourceLimits,
    pub total_reserved: ResourceLimits,
    pub allocations_state: Vec<AllocationState>,
    pub reserve_remaining: ResourceLimits,
    pub pending_estimates: u64,
    pub status: BudgetStatus,
    pub version: u64,
    pub updated_at_ms: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUsageEvent {
    pub schema_version: u32,
    pub id: String,
    pub team_session_id: String,
    pub role_instance_id: Option<String>,
    pub phase_id: Option<String>,
    pub run_id: String,
    pub operation_kind: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub tool_ref: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_micros: Option<u64>,
    pub duration_ms: u64,
    pub estimated_before: bool,
    pub uncertain: bool,
    pub observed_at_ms: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRequest {
    pub id: String,
    pub team_session_id: String,
    pub requester: String,
    pub requested: ResourceLimits,
    pub reason_code: String,
    pub expected_next_work: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeDecision {
    Allowed,
    SoftWarning,
    BudgetBlocked,
    UnknownCost,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BudgetError {
    #[error("unsupported team budget schema")]
    Version,
    #[error("team budget field is invalid")]
    Invalid,
    #[error("team budget field is too large")]
    TooLarge,
    #[error("team budget limit exceeded")]
    Limit,
    #[error("protected reserve access denied")]
    ReserveDenied,
    #[error("unknown usage requires reconciliation")]
    UsageUncertain,
}

fn pair(soft: Option<u64>, hard: Option<u64>) -> bool {
    soft.zip(hard).is_none_or(|(s, h)| s <= h)
}
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
pub fn canonical_hash(p: &TeamBudgetPolicy) -> Result<String, BudgetError> {
    let mut copy = p.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| BudgetError::Invalid)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
pub fn validate_hash(p: &TeamBudgetPolicy) -> Result<(), BudgetError> {
    validate_policy(p)?;
    if canonical_hash(p)? != p.content_hash {
        return Err(BudgetError::Invalid);
    }
    Ok(())
}
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
