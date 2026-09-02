//! Core-owned Team Coordinator contract.
//!
//! The coordinator proposes routing decisions.  It never creates identities,
//! grants capabilities, or replaces security/acceptance gates.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_WORK_ITEMS: usize = 64;
pub const MAX_PARTICIPANTS: usize = 32;
pub const MAX_CONSULTATIONS: usize = 16;
pub const MAX_DECOMPOSITION_CHILDREN: usize = 16;
pub const MAX_REASSIGNMENTS: u32 = 3;
pub const MAX_ACTIVE_ASSIGNMENTS: usize = 32;
pub const MAX_PROPOSAL_BYTES: usize = 64 * 1024;
pub const MAX_CONSULTATION_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Unassigned,
    Proposed,
    Assigned,
    InProgress,
    Submitted,
    UnderReview,
    Accepted,
    NeedsRevision,
    Blocked,
    Escalated,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagerialVerdict {
    Accept,
    Revise,
    Escalate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationReason {
    NoCompatibleParticipant,
    AllCandidatesBusy,
    CapabilityMissing,
    BudgetInsufficient,
    RepeatedFailure,
    ConflictingResults,
    HumanDecisionRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamWorkItem {
    pub schema_version: u32,
    pub id: String,
    pub objective: String,
    pub required_output_contract: String,
    pub required_capabilities: Vec<String>,
    pub preferred_role_tags: Vec<String>,
    pub dependencies: Vec<String>,
    pub priority: u8,
    pub estimated_cost_class: Option<String>,
    pub status: WorkItemStatus,
    pub assigned_instance_id: Option<String>,
    pub attempt: u32,
    pub max_attempts: u32,
    pub created_by: String,
    pub evidence_refs: Vec<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamCoordinatorPolicy {
    pub schema_version: u32,
    pub max_work_items: usize,
    pub max_active_assignments: usize,
    pub max_reassignments: u32,
    pub max_consultations: usize,
}

pub fn default_policy() -> TeamCoordinatorPolicy {
    TeamCoordinatorPolicy {
        schema_version: SCHEMA_VERSION,
        max_work_items: MAX_WORK_ITEMS,
        max_active_assignments: MAX_ACTIVE_ASSIGNMENTS,
        max_reassignments: MAX_REASSIGNMENTS,
        max_consultations: MAX_CONSULTATIONS,
    }
}

pub fn validate_policy(policy: &TeamCoordinatorPolicy) -> Result<(), CoordinatorError> {
    if policy.schema_version != SCHEMA_VERSION {
        return Err(CoordinatorError::UnsupportedVersion(policy.schema_version));
    }
    if policy.max_work_items == 0
        || policy.max_work_items > MAX_WORK_ITEMS
        || policy.max_active_assignments == 0
        || policy.max_active_assignments > MAX_ACTIVE_ASSIGNMENTS
        || policy.max_reassignments > MAX_REASSIGNMENTS
        || policy.max_consultations == 0
        || policy.max_consultations > MAX_CONSULTATIONS
    {
        return Err(CoordinatorError::Bounds);
    }
    Ok(())
}

pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, CoordinatorError> {
    let bytes = serde_json::to_vec(value).map_err(|_| CoordinatorError::SensitiveData)?;
    if bytes.len() > MAX_PROPOSAL_BYTES {
        return Err(CoordinatorError::Bounds);
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantCandidate {
    pub instance_id: String,
    pub role_profile_id: String,
    pub role_version: String,
    pub specialization_tags: Vec<String>,
    pub effective_capability_summary: Vec<String>,
    pub supported_output_contracts: Vec<String>,
    pub current_load: u32,
    pub current_status: String,
    pub remaining_budget_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationProposal {
    pub schema_version: u32,
    pub work_item_id: String,
    pub target_instance_id: String,
    pub rationale_codes: Vec<String>,
    pub context_refs: Vec<String>,
    pub requested_budget_class: Option<String>,
    pub expected_output_contract: String,
    pub coordinator_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecialistQuery {
    pub schema_version: u32,
    pub id: String,
    pub requester: String,
    pub specialist: String,
    pub question: String,
    pub context_refs: Vec<String>,
    pub response_contract: String,
    pub deadline_ms: Option<u64>,
    pub budget_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationReview {
    pub schema_version: u32,
    pub work_item_id: String,
    pub verdict: ManagerialVerdict,
    pub findings: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub required_changes: Vec<String>,
    pub security_gate_passed: bool,
    pub acceptance_gate_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecompositionProposal {
    pub schema_version: u32,
    pub parent_work_item_id: String,
    pub children: Vec<TeamWorkItem>,
    pub dependencies: Vec<(String, String)>,
    pub join_contract: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorProjection {
    pub schema_version: u32,
    pub operation: String,
    pub work_item: Option<TeamWorkItem>,
    pub proposal: Option<DelegationProposal>,
    pub consultation: Option<SpecialistQuery>,
    pub review: Option<CoordinationReview>,
    pub escalation: Option<EscalationReason>,
    pub candidate_count: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoordinatorError {
    #[error("unsupported coordinator schema version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid coordinator identifier or text")]
    InvalidText,
    #[error("coordinator collection bound exceeded")]
    Bounds,
    #[error("sensitive or authority-bearing coordinator data is forbidden")]
    SensitiveData,
    #[error("work item is stale")]
    StaleRevision,
    #[error("work item transition is invalid")]
    InvalidTransition,
    #[error("no compatible participant")]
    NoCompatibleParticipant,
    #[error("all compatible participants are busy")]
    AllCandidatesBusy,
    #[error("participant is not in the current roster")]
    NotInRoster,
    #[error("participant capability or output contract is incompatible")]
    IncompatibleParticipant,
    #[error("reassignment limit reached")]
    ReassignmentLimit,
    #[error("managerial review cannot replace a required gate")]
    GateRequired,
    #[error("decomposition contains a cycle or invalid child")]
    InvalidDecomposition,
}

fn valid_text(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn forbidden(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            let lower = key.to_ascii_lowercase();
            [
                "secret",
                "password",
                "token",
                "credential",
                "raw_prompt",
                "raw_output",
                "grant",
            ]
            .iter()
            .any(|part| lower.contains(part))
                || forbidden(value)
        }),
        serde_json::Value::Array(items) => items.iter().any(forbidden),
        _ => false,
    }
}

pub fn validate_work_item(item: &TeamWorkItem) -> Result<(), CoordinatorError> {
    if item.schema_version != SCHEMA_VERSION {
        return Err(CoordinatorError::UnsupportedVersion(item.schema_version));
    }
    if !valid_text(&item.id)
        || !valid_text(&item.objective)
        || !valid_text(&item.required_output_contract)
        || !valid_text(&item.created_by)
        || item.required_capabilities.len() > MAX_PARTICIPANTS
        || item.preferred_role_tags.len() > MAX_PARTICIPANTS
        || item.dependencies.len() > MAX_WORK_ITEMS
        || item.evidence_refs.len() > MAX_WORK_ITEMS
        || item.max_attempts == 0
        || item.max_attempts > MAX_REASSIGNMENTS + 1
    {
        return Err(
            if item.max_attempts == 0 || item.max_attempts > MAX_REASSIGNMENTS + 1 {
                CoordinatorError::Bounds
            } else {
                CoordinatorError::InvalidText
            },
        );
    }
    let value = serde_json::to_value(item).map_err(|_| CoordinatorError::SensitiveData)?;
    if forbidden(&value) {
        return Err(CoordinatorError::SensitiveData);
    }
    if serde_json::to_vec(item)
        .map_err(|_| CoordinatorError::Bounds)?
        .len()
        > MAX_PROPOSAL_BYTES
    {
        return Err(CoordinatorError::Bounds);
    }
    Ok(())
}

pub fn compatible(item: &TeamWorkItem, candidate: &ParticipantCandidate) -> bool {
    item.required_capabilities
        .iter()
        .all(|required| candidate.effective_capability_summary.contains(required))
        && candidate
            .supported_output_contracts
            .contains(&item.required_output_contract)
}

pub fn validate_candidate(candidate: &ParticipantCandidate) -> Result<(), CoordinatorError> {
    if !valid_text(&candidate.instance_id)
        || !valid_text(&candidate.role_profile_id)
        || !valid_text(&candidate.role_version)
        || candidate.specialization_tags.len() > MAX_PARTICIPANTS
        || candidate.effective_capability_summary.len() > MAX_PARTICIPANTS
        || candidate.supported_output_contracts.len() > MAX_PARTICIPANTS
        || !valid_text(&candidate.current_status)
    {
        return Err(CoordinatorError::InvalidText);
    }
    if forbidden(&serde_json::to_value(candidate).map_err(|_| CoordinatorError::SensitiveData)?) {
        return Err(CoordinatorError::SensitiveData);
    }
    Ok(())
}

pub fn propose_assignment(
    item: &TeamWorkItem,
    candidates: &[ParticipantCandidate],
) -> Result<DelegationProposal, CoordinatorError> {
    validate_work_item(item)?;
    if candidates.len() > MAX_PARTICIPANTS {
        return Err(CoordinatorError::Bounds);
    }
    for candidate in candidates {
        validate_candidate(candidate)?;
    }
    let compatible_candidates: Vec<_> = candidates
        .iter()
        .filter(|candidate| compatible(item, candidate))
        .collect();
    if compatible_candidates.is_empty() {
        return Err(CoordinatorError::NoCompatibleParticipant);
    }
    let mut idle_candidates: Vec<_> = compatible_candidates
        .into_iter()
        .filter(|candidate| candidate.current_status == "idle")
        .collect();
    if idle_candidates.is_empty() {
        return Err(CoordinatorError::AllCandidatesBusy);
    }
    idle_candidates.sort_by(|left, right| {
        let left_tags = item
            .preferred_role_tags
            .iter()
            .filter(|tag| left.specialization_tags.contains(tag))
            .count();
        let right_tags = item
            .preferred_role_tags
            .iter()
            .filter(|tag| right.specialization_tags.contains(tag))
            .count();
        right_tags
            .cmp(&left_tags)
            .then_with(|| left.current_load.cmp(&right.current_load))
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    let candidate = idle_candidates[0];
    Ok(DelegationProposal {
        schema_version: SCHEMA_VERSION,
        work_item_id: item.id.clone(),
        target_instance_id: candidate.instance_id.clone(),
        rationale_codes: vec!["capability_match".into(), "load_aware_tiebreak".into()],
        context_refs: item.evidence_refs.clone(),
        requested_budget_class: item.estimated_cost_class.clone(),
        expected_output_contract: item.required_output_contract.clone(),
        coordinator_revision: item.revision,
    })
}

pub fn validate_proposal(
    item: &TeamWorkItem,
    proposal: &DelegationProposal,
    candidate: &ParticipantCandidate,
) -> Result<(), CoordinatorError> {
    validate_work_item(item)?;
    if proposal.schema_version != SCHEMA_VERSION
        || proposal.work_item_id != item.id
        || proposal.target_instance_id != candidate.instance_id
        || proposal.coordinator_revision != item.revision
        || !compatible(item, candidate)
        || candidate.current_status != "idle"
    {
        return Err(CoordinatorError::IncompatibleParticipant);
    }
    let bytes = serde_json::to_vec(proposal).map_err(|_| CoordinatorError::Bounds)?;
    if bytes.len() > MAX_PROPOSAL_BYTES {
        return Err(CoordinatorError::Bounds);
    }
    Ok(())
}

pub fn validate_consultation(query: &SpecialistQuery) -> Result<(), CoordinatorError> {
    if query.schema_version != SCHEMA_VERSION {
        return Err(CoordinatorError::UnsupportedVersion(query.schema_version));
    }
    if !valid_text(&query.id)
        || !valid_text(&query.id)
        || !valid_text(&query.requester)
        || !valid_text(&query.specialist)
        || !valid_text(&query.question)
        || !valid_text(&query.response_contract)
        || query.context_refs.len() > MAX_WORK_ITEMS
        || forbidden(&serde_json::to_value(query).map_err(|_| CoordinatorError::SensitiveData)?)
        || serde_json::to_vec(query)
            .map_err(|_| CoordinatorError::Bounds)?
            .len()
            > MAX_CONSULTATION_BYTES
    {
        return Err(
            if forbidden(&serde_json::to_value(query).map_err(|_| CoordinatorError::SensitiveData)?)
            {
                CoordinatorError::SensitiveData
            } else {
                CoordinatorError::InvalidText
            },
        );
    }
    Ok(())
}

pub fn validate_review(review: &CoordinationReview) -> Result<(), CoordinatorError> {
    if review.schema_version != SCHEMA_VERSION
        || !valid_text(&review.work_item_id)
        || review.findings.len() > MAX_WORK_ITEMS
        || review.evidence_refs.len() > MAX_WORK_ITEMS
        || review.required_changes.len() > MAX_WORK_ITEMS
        || forbidden(&serde_json::to_value(review).map_err(|_| CoordinatorError::SensitiveData)?)
    {
        return Err(CoordinatorError::SensitiveData);
    }
    if review.verdict == ManagerialVerdict::Accept
        && (!review.security_gate_passed || !review.acceptance_gate_passed)
    {
        return Err(CoordinatorError::GateRequired);
    }
    Ok(())
}

pub fn validate_decomposition(proposal: &DecompositionProposal) -> Result<(), CoordinatorError> {
    if proposal.schema_version != SCHEMA_VERSION
        || !valid_text(&proposal.parent_work_item_id)
        || !valid_text(&proposal.join_contract)
        || proposal.children.is_empty()
        || proposal.children.len() > MAX_DECOMPOSITION_CHILDREN
    {
        return Err(CoordinatorError::InvalidDecomposition);
    }
    for child in &proposal.children {
        validate_work_item(child)?;
        if child.id == proposal.parent_work_item_id {
            return Err(CoordinatorError::InvalidDecomposition);
        }
    }
    let mut visiting = Vec::new();
    for child in &proposal.children {
        if proposal.dependencies.iter().any(|(from, to)| {
            from == to || (from == &child.id && to == &proposal.parent_work_item_id)
        }) {
            return Err(CoordinatorError::InvalidDecomposition);
        }
        if visiting.contains(&child.id) {
            return Err(CoordinatorError::InvalidDecomposition);
        }
        visiting.push(child.id.clone());
    }
    Ok(())
}

pub fn validate_reassignment(item: &TeamWorkItem) -> Result<(), CoordinatorError> {
    validate_work_item(item)?;
    if item.attempt > MAX_REASSIGNMENTS {
        return Err(CoordinatorError::ReassignmentLimit);
    }
    Ok(())
}

pub fn transition(
    item: &mut TeamWorkItem,
    target: WorkItemStatus,
    expected_revision: u64,
) -> Result<(), CoordinatorError> {
    validate_work_item(item)?;
    if item.revision != expected_revision {
        return Err(CoordinatorError::StaleRevision);
    }
    let allowed = matches!(
        (item.status, target),
        (WorkItemStatus::Unassigned, WorkItemStatus::Proposed)
            | (WorkItemStatus::Unassigned, WorkItemStatus::Assigned)
            | (WorkItemStatus::Proposed, WorkItemStatus::Assigned)
            | (WorkItemStatus::Assigned, WorkItemStatus::InProgress)
            | (WorkItemStatus::InProgress, WorkItemStatus::Submitted)
            | (WorkItemStatus::Submitted, WorkItemStatus::UnderReview)
            | (WorkItemStatus::UnderReview, WorkItemStatus::Accepted)
            | (WorkItemStatus::UnderReview, WorkItemStatus::NeedsRevision)
            | (WorkItemStatus::NeedsRevision, WorkItemStatus::InProgress)
            | (_, WorkItemStatus::Blocked)
            | (_, WorkItemStatus::Escalated)
            | (_, WorkItemStatus::Cancelled)
    );
    if !allowed {
        return Err(CoordinatorError::InvalidTransition);
    }
    item.status = target;
    item.revision = item.revision.saturating_add(1);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> TeamWorkItem {
        TeamWorkItem {
            schema_version: SCHEMA_VERSION,
            id: "work-1".into(),
            objective: "inspect migration".into(),
            required_output_contract: "report-v1".into(),
            required_capabilities: vec!["repo.read".into()],
            preferred_role_tags: vec!["rust".into()],
            dependencies: vec![],
            priority: 1,
            estimated_cost_class: Some("small".into()),
            status: WorkItemStatus::Unassigned,
            assigned_instance_id: None,
            attempt: 0,
            max_attempts: 4,
            created_by: "coordinator".into(),
            evidence_refs: vec!["artifact-1".into()],
            revision: 1,
        }
    }

    fn candidate(status: &str, load: u32, id: &str) -> ParticipantCandidate {
        ParticipantCandidate {
            instance_id: id.into(),
            role_profile_id: "rust".into(),
            role_version: "1".into(),
            specialization_tags: vec!["rust".into()],
            effective_capability_summary: vec!["repo.read".into()],
            supported_output_contracts: vec!["report-v1".into()],
            current_load: load,
            current_status: status.into(),
            remaining_budget_class: Some("small".into()),
        }
    }

    #[test]
    fn matching_is_capability_checked_and_deterministic_by_load_then_id() {
        let item = item();
        let proposal = propose_assignment(
            &item,
            &[candidate("idle", 2, "b"), candidate("idle", 1, "a")],
        )
        .unwrap();
        assert_eq!(proposal.target_instance_id, "a");
        assert!(validate_proposal(&item, &proposal, &candidate("idle", 1, "a")).is_ok());
        assert_eq!(
            propose_assignment(&item, &[candidate("busy", 1, "a")]),
            Err(CoordinatorError::AllCandidatesBusy)
        );
    }

    #[test]
    fn managerial_accept_requires_independent_gates_and_decomposition_is_bounded() {
        let review = CoordinationReview {
            schema_version: SCHEMA_VERSION,
            work_item_id: "work-1".into(),
            verdict: ManagerialVerdict::Accept,
            findings: vec![],
            evidence_refs: vec![],
            required_changes: vec![],
            security_gate_passed: false,
            acceptance_gate_passed: true,
        };
        assert_eq!(
            validate_review(&review),
            Err(CoordinatorError::GateRequired)
        );
        let proposal = DecompositionProposal {
            schema_version: SCHEMA_VERSION,
            parent_work_item_id: "work-1".into(),
            children: vec![item()],
            dependencies: vec![],
            join_contract: "join".into(),
        };
        assert_eq!(
            validate_decomposition(&proposal),
            Err(CoordinatorError::InvalidDecomposition)
        );
    }
}
