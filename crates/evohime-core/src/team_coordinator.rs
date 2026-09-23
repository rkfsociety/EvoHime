//! Core-owned Team Coordinator contract.
//!
//! The coordinator proposes routing decisions.  It never creates identities,
//! grants capabilities, or replaces security/acceptance gates.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema version accepted by team coordinator contracts.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum byte length of text and identifiers accepted by this contract.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum work items stored in one coordinator scope.
pub const MAX_WORK_ITEMS: usize = 64;
/// Maximum participant candidates considered at once.
pub const MAX_PARTICIPANTS: usize = 32;
/// Maximum specialist consultations per coordinator policy.
pub const MAX_CONSULTATIONS: usize = 16;
/// Maximum child items accepted in a decomposition proposal.
pub const MAX_DECOMPOSITION_CHILDREN: usize = 16;
/// Maximum reassignment attempts for a work item.
pub const MAX_REASSIGNMENTS: u32 = 3;
/// Maximum active assignments allowed by policy.
pub const MAX_ACTIVE_ASSIGNMENTS: usize = 32;
/// Maximum serialized coordinator proposal size.
pub const MAX_PROPOSAL_BYTES: usize = 64 * 1024;
/// Maximum serialized specialist consultation size.
pub const MAX_CONSULTATION_BYTES: usize = 32 * 1024;

/// Coordinator-owned lifecycle states for one work item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    /// The work item has no current participant.
    Unassigned,
    /// A participant assignment has been proposed but not committed.
    Proposed,
    /// A participant owns the work item.
    Assigned,
    /// The assigned participant is actively working.
    InProgress,
    /// The participant submitted a result for review.
    Submitted,
    /// The result is being evaluated.
    UnderReview,
    /// The result passed required gates and was accepted.
    Accepted,
    /// The result requires changes before acceptance.
    NeedsRevision,
    /// Progress cannot continue until a dependency or decision is resolved.
    Blocked,
    /// The work item requires a higher-level decision.
    Escalated,
    /// The work item was cancelled.
    Cancelled,
}

/// Managerial recommendation after reviewing a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagerialVerdict {
    /// Recommend accepting the reviewed result, subject to required gates.
    Accept,
    /// Recommend returning the result for changes.
    Revise,
    /// Recommend escalation for a decision outside the coordinator.
    Escalate,
}

/// Reasons a coordinator should request higher-level intervention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationReason {
    /// No participant is compatible with the work item.
    NoCompatibleParticipant,
    /// All compatible participants are at their active assignment limit.
    AllCandidatesBusy,
    /// A required participant capability is unavailable.
    CapabilityMissing,
    /// No candidate has the requested budget class.
    BudgetInsufficient,
    /// The work item exhausted repeated attempts.
    RepeatedFailure,
    /// Candidate results conflict and need adjudication.
    ConflictingResults,
    /// The decision requires explicit human input.
    HumanDecisionRequired,
}

/// Versioned objective, constraints, ownership, and lifecycle for a unit of team work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamWorkItem {
    /// Serialized contract version supported by this module.
    pub schema_version: u32,
    /// Stable identifier within the coordinator contract.
    pub id: String,
    /// Bounded statement of the work item outcome.
    pub objective: String,
    /// Output shape the assigned participant must satisfy.
    pub required_output_contract: String,
    /// Capabilities required for a compatible assignment.
    pub required_capabilities: Vec<String>,
    /// Optional role specializations that improve participant fit.
    pub preferred_role_tags: Vec<String>,
    /// Directed child dependency pairs required for execution order.
    pub dependencies: Vec<String>,
    /// Relative routing priority; larger values receive preference.
    pub priority: u8,
    /// Optional budget class estimate for scheduling.
    pub estimated_cost_class: Option<String>,
    /// Current coordinator-owned lifecycle status.
    pub status: WorkItemStatus,
    /// Participant instance owning the current assignment, if any.
    pub assigned_instance_id: Option<String>,
    /// Current assignment attempt number.
    pub attempt: u32,
    /// Maximum allowed attempts before escalation or rejection.
    pub max_attempts: u32,
    /// Identity of the work item creator.
    pub created_by: String,
    /// References supporting work item progress or completion.
    pub evidence_refs: Vec<String>,
    /// Monotonic revision used for stale-write detection.
    pub revision: u64,
}

/// Bounds controlling work item routing and consultation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamCoordinatorPolicy {
    /// Serialized contract version supported by this module.
    pub schema_version: u32,
    /// Maximum work items accepted by this policy.
    pub max_work_items: usize,
    /// Maximum simultaneous assignments allowed.
    pub max_active_assignments: usize,
    /// Maximum number of times an item may be reassigned.
    pub max_reassignments: u32,
    /// Maximum specialist queries allowed by the policy.
    pub max_consultations: usize,
}

/// Returns the conservative default coordinator limits.
pub fn default_policy() -> TeamCoordinatorPolicy {
    TeamCoordinatorPolicy {
        schema_version: SCHEMA_VERSION,
        max_work_items: MAX_WORK_ITEMS,
        max_active_assignments: MAX_ACTIVE_ASSIGNMENTS,
        max_reassignments: MAX_REASSIGNMENTS,
        max_consultations: MAX_CONSULTATIONS,
    }
}

/// Validates coordinator limits against supported hard bounds.
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

/// Serializes a bounded coordinator value and returns its SHA-256 hash.
pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, CoordinatorError> {
    let bytes = serde_json::to_vec(value).map_err(|_| CoordinatorError::SensitiveData)?;
    if bytes.len() > MAX_PROPOSAL_BYTES {
        return Err(CoordinatorError::Bounds);
    }
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Current roster facts used to validate assignment compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantCandidate {
    /// Stable identifier for the participant instance.
    pub instance_id: String,
    /// Role profile used by the participant.
    pub role_profile_id: String,
    /// Version of the participant role profile.
    pub role_version: String,
    /// Specializations advertised by the participant.
    pub specialization_tags: Vec<String>,
    /// Capabilities currently available to the participant.
    pub effective_capability_summary: Vec<String>,
    /// Output contracts the participant can produce.
    pub supported_output_contracts: Vec<String>,
    /// Number of active assignments held by the participant.
    pub current_load: u32,
    /// Availability state used for routing.
    pub current_status: String,
    /// Optional budget class remaining for this participant.
    pub remaining_budget_class: Option<String>,
}

/// Proposed assignment constrained to a current work item and roster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationProposal {
    /// Serialized contract version supported by this module.
    pub schema_version: u32,
    /// Identifier of the work item being assigned or reviewed.
    pub work_item_id: String,
    /// Participant instance selected for assignment.
    pub target_instance_id: String,
    /// Bounded machine-readable reasons for the proposed assignment.
    pub rationale_codes: Vec<String>,
    /// References to context authorized for this operation.
    pub context_refs: Vec<String>,
    /// Optional budget requested for completing the work.
    pub requested_budget_class: Option<String>,
    /// Output contract expected from the assigned participant.
    pub expected_output_contract: String,
    /// Coordinator revision used to create the proposal.
    pub coordinator_revision: u64,
}

/// Bounded request for consultation from a specialist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecialistQuery {
    /// Serialized contract version supported by this module.
    pub schema_version: u32,
    /// Stable identifier within the coordinator contract.
    pub id: String,
    /// Identity requesting specialist input.
    pub requester: String,
    /// Identity of the requested specialist.
    pub specialist: String,
    /// Bounded question sent to the specialist.
    pub question: String,
    /// References to context authorized for this operation.
    pub context_refs: Vec<String>,
    /// Expected shape of the specialist response.
    pub response_contract: String,
    /// Optional deadline as Unix epoch milliseconds.
    pub deadline_ms: Option<u64>,
    /// Optional budget class for the consultation.
    pub budget_class: Option<String>,
}

/// Managerial review result that remains subordinate to independent gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationReview {
    /// Serialized contract version supported by this module.
    pub schema_version: u32,
    /// Identifier of the work item being assigned or reviewed.
    pub work_item_id: String,
    /// Managerial recommendation for the submitted result.
    pub verdict: ManagerialVerdict,
    /// Review findings relevant to the work item.
    pub findings: Vec<String>,
    /// References supporting work item progress or completion.
    pub evidence_refs: Vec<String>,
    /// Changes required before the result can be accepted.
    pub required_changes: Vec<String>,
    /// Whether the independent security gate passed.
    pub security_gate_passed: bool,
    /// Whether the independent acceptance gate passed.
    pub acceptance_gate_passed: bool,
}

/// Proposed child work items and dependencies for a parent objective.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecompositionProposal {
    /// Serialized contract version supported by this module.
    pub schema_version: u32,
    /// Identifier of the work item being decomposed.
    pub parent_work_item_id: String,
    /// Proposed child work items.
    pub children: Vec<TeamWorkItem>,
    /// Directed child dependency pairs required for execution order.
    pub dependencies: Vec<(String, String)>,
    /// Contract for combining child results into the parent result.
    pub join_contract: String,
}

/// Read model combining the relevant result of a coordinator operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorProjection {
    /// Serialized contract version supported by this module.
    pub schema_version: u32,
    /// Coordinator operation represented by this projection.
    pub operation: String,
    /// Work item associated with the operation, if applicable.
    pub work_item: Option<TeamWorkItem>,
    /// Assignment proposal associated with the operation, if applicable.
    pub proposal: Option<DelegationProposal>,
    /// Specialist query associated with the operation, if applicable.
    pub consultation: Option<SpecialistQuery>,
    /// Review result associated with the operation, if applicable.
    pub review: Option<CoordinationReview>,
    /// Reason the operation was escalated, if applicable.
    pub escalation: Option<EscalationReason>,
    /// Number of participant candidates considered.
    pub candidate_count: usize,
}

/// Validation, compatibility, state, or gate error in team coordination.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoordinatorError {
    /// Serialized schema version is unsupported.
    #[error("unsupported coordinator schema version {0}")]
    UnsupportedVersion(u32),
    /// An identifier or text field failed validation.
    #[error("invalid coordinator identifier or text")]
    InvalidText,
    /// A collection, payload, or attempt limit was exceeded.
    #[error("coordinator collection bound exceeded")]
    Bounds,
    /// The proposal contains secret-like or authority-bearing data.
    #[error("sensitive or authority-bearing coordinator data is forbidden")]
    SensitiveData,
    /// The proposal was based on an outdated work item revision.
    #[error("work item is stale")]
    StaleRevision,
    /// The requested lifecycle transition is not allowed.
    #[error("work item transition is invalid")]
    InvalidTransition,
    /// No participant is compatible with the work item.
    #[error("no compatible participant")]
    NoCompatibleParticipant,
    /// All compatible participants are at their active assignment limit.
    #[error("all compatible participants are busy")]
    AllCandidatesBusy,
    /// The selected participant is absent from the current roster.
    #[error("participant is not in the current roster")]
    NotInRoster,
    /// The participant lacks a required capability or output contract.
    #[error("participant capability or output contract is incompatible")]
    IncompatibleParticipant,
    /// The work item has reached its reassignment limit.
    #[error("reassignment limit reached")]
    ReassignmentLimit,
    /// Managerial review cannot substitute for an independent required gate.
    #[error("managerial review cannot replace a required gate")]
    GateRequired,
    /// The proposed decomposition is invalid or cyclic.
    #[error("decomposition contains a cycle or invalid child")]
    InvalidDecomposition,
    /// A termination condition prevents new team routing.
    #[error("termination condition blocks team routing")]
    TerminationReached,
    /// The termination condition cannot be evaluated safely.
    #[error("termination condition is invalid")]
    TerminationInvalid,
}

/// Rejects routing when a supplied termination policy has fired or is invalid.
pub fn termination_allows_routing(
    policy: Option<&crate::composable_termination_conditions::TerminationPolicy>,
    state: Option<&crate::composable_termination_conditions::TerminationState>,
    event: Option<&crate::composable_termination_conditions::TerminationEvent>,
) -> Result<(), CoordinatorError> {
    match (policy, state, event) {
        (None, None, None) => Ok(()),
        (Some(policy), Some(state), Some(event)) => {
            match crate::composable_termination_conditions::evaluate_policy(policy, state, event)
                .map_err(|_| CoordinatorError::TerminationInvalid)?
            {
                Some(_) => Err(CoordinatorError::TerminationReached),
                None => Ok(()),
            }
        }
        _ => Err(CoordinatorError::TerminationInvalid),
    }
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

/// Validates work item identifiers, limits, size, and forbidden sensitive fields.
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

/// Returns whether a candidate satisfies required capabilities and output contract.
pub fn compatible(item: &TeamWorkItem, candidate: &ParticipantCandidate) -> bool {
    item.required_capabilities
        .iter()
        .all(|required| candidate.effective_capability_summary.contains(required))
        && candidate
            .supported_output_contracts
            .contains(&item.required_output_contract)
}

/// Validates bounded candidate metadata before it is used for routing.
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

/// Chooses a compatible available candidate and creates a bounded assignment proposal.
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

/// Applies termination checks before proposing an assignment.
pub fn propose_assignment_with_termination(
    item: &TeamWorkItem,
    candidates: &[ParticipantCandidate],
    policy: Option<&crate::composable_termination_conditions::TerminationPolicy>,
    state: Option<&crate::composable_termination_conditions::TerminationState>,
    event: Option<&crate::composable_termination_conditions::TerminationEvent>,
) -> Result<DelegationProposal, CoordinatorError> {
    termination_allows_routing(policy, state, event)?;
    propose_assignment(item, candidates)
}

/// Checks a proposal against the work item, roster, and current revision.
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

/// Validates the bounded specialist query and its context references.
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

/// Validates review fields without allowing managerial verdicts to bypass gates.
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

/// Checks child count, identifiers, dependency references, and cycles.
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

/// Checks retry and reassignment bounds for a work item.
pub fn validate_reassignment(item: &TeamWorkItem) -> Result<(), CoordinatorError> {
    validate_work_item(item)?;
    if item.attempt > MAX_REASSIGNMENTS {
        return Err(CoordinatorError::ReassignmentLimit);
    }
    Ok(())
}

/// Applies a valid coordinator lifecycle transition with revision checks.
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
