//! Core-owned bounded multi-reviewer reconciliation and completion contract.
//!
//! Reviewer execution remains owned by the existing model/review lanes. This
//! module only validates immutable snapshots, records independence and derives
//! conservative cross-reviewer classifications.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Serialized schema version for reviewer ensemble records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum reviewer slots in one ensemble profile.
pub const MAX_SLOTS: usize = 16;
/// Maximum finding candidates reconciled in one operation.
pub const MAX_CANDIDATES: usize = 512;
/// Maximum finding clusters emitted by reconciliation.
pub const MAX_CLUSTERS: usize = 512;
/// Maximum reasons stored in one adjudication result.
pub const MAX_REASONS: usize = 32;
/// Maximum size in bytes for bounded text fields.
pub const MAX_TEXT: usize = 512;

/// Intended task for a reviewer ensemble profile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Review code changes for defects and regressions.
    CodeReview,
    /// Assess whether implementation and required proof are complete.
    CompletionCheck,
    /// Review an architecture or system design.
    ArchitectureReview,
    /// Review security properties and potential vulnerabilities.
    SecurityReview,
    /// Critique a plan for completeness and feasibility.
    PlanCritique,
    /// Collect independent perspectives on a question.
    QuestionPanel,
    /// Purpose registered by an extension.
    CustomRegistered,
}

/// Lifecycle state of an immutable reviewer ensemble profile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Profile is being drafted.
    Draft,
    /// Profile is valid and may be used for new runs.
    Active,
    /// Profile was replaced by a newer revision.
    Superseded,
    /// Profile is invalid and cannot be used.
    Invalid,
}

/// Declared relationship used to estimate reviewer independence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndependenceClass {
    /// Reviewer shares the run that authored the reviewed changes.
    SameRunAuthor,
    /// Same model identity, but with a fresh context.
    SameModelFreshContext,
    /// Same model family, but with a fresh context.
    SameFamilyFreshContext,
    /// Different model family from the author or other reviewer.
    DistinctModelFamily,
    /// Different provider or execution service.
    IndependentProvider,
    /// Human reviewer.
    Human,
    /// Deterministic verifier rather than a reviewer model.
    Deterministic,
}

/// Outcome of one reviewer slot in an ensemble run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlotStatus {
    /// Slot is waiting to be dispatched.
    Queued,
    /// Reviewer produced a usable result.
    Completed,
    /// Reviewer execution failed.
    Failed,
    /// Reviewer capability was unavailable.
    Unavailable,
    /// Reviewer exceeded its timeout.
    TimedOut,
    /// Reviewer stopped because its budget was exhausted.
    BudgetLimited,
    /// Policy prevented reviewer execution.
    PolicyDenied,
    /// Reviewer was cancelled.
    Cancelled,
    /// Reviewer result did not satisfy the response protocol.
    ProtocolError,
}

/// Lifecycle status of a complete ensemble run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Run is queued but has not begun.
    Queued,
    /// Reviewer slots are executing.
    Running,
    /// Reviewer results are being reconciled.
    Reconciling,
    /// A disagreement is being adjudicated.
    Adjudicating,
    /// Run completed with all required outcomes.
    Completed,
    /// Run completed with degraded reviewer coverage.
    Degraded,
    /// Required results are missing or incomplete.
    Incomplete,
    /// Policy or unresolved disagreement blocks completion.
    Blocked,
    /// Run was cancelled.
    Cancelled,
    /// Run failed.
    Failed,
    /// The run outcome cannot be determined from available state.
    Unknown,
}

/// Semantic relationship assigned when comparing two finding candidates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchState {
    /// Candidates identify the same defect at the same resolved location.
    ExactSameDefect,
    /// Candidates likely identify the same defect but require confirmation.
    LikelySameDefect,
    /// Candidates are related but describe distinct issues.
    RelatedButDistinct,
    /// Available evidence cannot determine the relationship.
    Ambiguous,
    /// Candidates describe distinct defects.
    Distinct,
}

/// Reconciliation outcome for a finding cluster.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// Multiple reviewer candidates corroborate the finding.
    Corroborated,
    /// Only one reviewer supplied the finding.
    SingleSource,
    /// Reviewers disagree about the finding.
    Disagreed,
    /// Finding was removed by reconciliation policy.
    Dropped,
    /// A separate adjudication step is required.
    NeedsAdjudication,
}

/// Result of an adjudicator's review of a finding cluster.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdjudicationVerdict {
    /// Evidence confirms the finding.
    Confirmed,
    /// Evidence rejects the finding.
    Rejected,
    /// Reviewers still disagree after adjudication.
    StillDisagreed,
    /// Adjudication lacked sufficient evidence.
    InsufficientEvidence,
    /// The request fell outside the adjudication scope.
    InvalidScope,
}

/// Outcome describing whether implementation completion is established.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompletionVerdict {
    /// Completion is supported by fresh proof and required independent review.
    Done,
    /// Acceptance criteria remain unmet.
    NotDone,
    /// Verification proof is stale, missing, or failed.
    NeedsVerification,
    /// Required independent review has not occurred.
    NeedsReview,
    /// Reviewers disagree on the completion claim.
    Disagreed,
    /// Available evidence cannot establish the outcome.
    Unknown,
}

/// Freshness and success state of completion proof.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProofState {
    /// Latest required verification passed.
    FreshPassed,
    /// Existing proof no longer matches the current target.
    Stale,
    /// Verification ran and failed.
    Failed,
    /// Required verification evidence is absent.
    Missing,
    /// Proof state could not be established.
    Unknown,
}

/// One configured reviewer and its per-slot independence and budget requirements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerSlot {
    /// Stable slot identifier.
    pub id: String,
    /// Registered reviewer profile reference.
    pub reviewer_profile_ref: String,
    /// Registered model-purpose profile reference.
    pub model_purpose_ref: String,
    /// Minimum independence class required for this slot.
    pub required_independence: IndependenceClass,
    /// Optional role lens assigned to the reviewer.
    pub role_lens: Option<String>,
    /// Whether the slot must complete for the ensemble to be complete.
    pub required: bool,
    /// Scheduling priority for this slot.
    pub priority: u32,
    /// Maximum reviewer calls permitted for the slot.
    pub max_calls: u32,
    /// Digest of the canonical slot definition.
    pub content_hash: String,
}

/// Versioned policy for selecting reviewers and reconciling their results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerEnsembleProfile {
    /// Serialized schema version.
    pub schema_version: u32,
    /// Stable profile identifier.
    pub id: String,
    /// Monotonically increasing profile revision.
    pub revision: u64,
    /// Lifecycle state controlling use for new runs.
    pub lifecycle: Lifecycle,
    /// Review task performed by this ensemble.
    pub purpose: Purpose,
    /// Ordered reviewer slots configured for the ensemble.
    pub reviewer_slots: Vec<ReviewerSlot>,
    /// Minimum number of completed slots required for a usable run.
    pub minimum_completed_slots: u32,
    /// Optional minimum number of distinct model families.
    pub minimum_distinct_model_families: Option<u32>,
    /// Registered adjudication policy reference.
    pub adjudication_policy_ref: String,
    /// Registered candidate-reconciliation policy reference.
    pub reconciliation_policy_ref: String,
    /// Registered resource budget reference.
    pub budget_ref: String,
    /// Behavior when a required reviewer slot does not complete.
    pub failure_policy: String,
    /// Digest of the canonical profile with this field cleared.
    pub content_hash: String,
}

/// Immutable input context shared by reviewer slots for one ensemble run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnsembleInputSnapshot {
    /// Stable reference to the review target.
    pub target_ref: String,
    /// Digest of the reviewed diff or target content.
    pub target_diff_hash: String,
    /// Optional reference to the exact review unit.
    pub review_unit_ref: Option<String>,
    /// Digest of the context supplied for the review unit.
    pub unit_context_hash: String,
    /// Digest of applicable review rules.
    pub applicable_rule_set_hash: String,
    /// Project guidance revision captured for the run.
    pub project_guidance_revision: String,
    /// References to verification context included in the snapshot.
    pub verification_context_refs: Vec<String>,
    /// Snapshot creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Digest of the canonical input snapshot.
    pub content_hash: String,
}

/// Evidence describing how independent one reviewer run was from others.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerIndependenceSnapshot {
    /// Reviewer run whose independence is described.
    pub reviewer_run_ref: String,
    /// Provider or deterministic executor used by the reviewer.
    pub provider_or_executor_ref: String,
    /// Optional stable model identity.
    pub model_identity: Option<String>,
    /// Optional model family used for diversity accounting.
    pub model_family: Option<String>,
    /// Relationship between the reviewer and the run that authored the target.
    pub authoring_run_relation: IndependenceClass,
    /// Independence class of the evidence source.
    pub evidence_class: IndependenceClass,
    /// Relationship between the reviewer context and the authoring context.
    pub context_relation: IndependenceClass,
    /// Digest of the canonical independence snapshot.
    pub content_hash: String,
}

/// Durable state of one ensemble run and its reviewer-slot outcomes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerEnsembleRun {
    /// Serialized schema version.
    pub schema_version: u32,
    /// Stable run identifier.
    pub id: String,
    /// Monotonically increasing run revision.
    pub revision: u64,
    /// Profile reference used to configure the run.
    pub profile_ref: String,
    /// Exact profile revision captured for the run.
    pub profile_revision: u64,
    /// Immutable target and context snapshot reviewed by all slots.
    pub input_snapshot: EnsembleInputSnapshot,
    /// Reviewer run references aligned with `slot_statuses`.
    pub reviewer_run_refs: Vec<String>,
    /// Overall ensemble run status.
    pub status: RunStatus,
    /// Outcome for each reviewer slot.
    pub slot_statuses: Vec<SlotStatus>,
    /// Independence evidence for completed reviewer runs.
    pub independence: Vec<ReviewerIndependenceSnapshot>,
    /// Run start time in Unix milliseconds.
    pub started_at_ms: i64,
    /// Completion time in Unix milliseconds, if terminal.
    pub completed_at_ms: Option<i64>,
    /// Digest of the canonical run record.
    pub content_hash: String,
}

/// Normalized, content-addressed review finding emitted by one reviewer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedReviewFindingCandidate {
    /// Stable candidate reference.
    pub candidate_ref: String,
    /// Reviewer run that produced this candidate.
    pub reviewer_run_ref: String,
    /// Normalized finding category.
    pub category: String,
    /// Normalized severity label.
    pub severity: String,
    /// Resolved location within the reviewed target.
    pub resolved_position: String,
    /// Stable semantic anchor for the finding.
    pub semantic_anchor: String,
    /// Signature used to compare candidate issue identity.
    pub issue_signature: String,
    /// References to evidence supporting the finding.
    pub evidence_refs: Vec<String>,
    /// Normalized confidence label.
    pub confidence: String,
    /// Validation state assigned to the candidate.
    pub validation_state: String,
    /// Digest of the canonical candidate record.
    pub content_hash: String,
}

/// Cluster of reviewer candidates considered to describe the same finding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnsembleFindingCluster {
    /// Stable cluster identifier.
    pub id: String,
    /// Candidate references included in the cluster.
    pub member_candidate_refs: Vec<String>,
    /// Reviewer runs represented by the candidates.
    pub reviewer_refs: Vec<String>,
    /// Distinct model families represented by member reviewers.
    pub distinct_model_families: Vec<String>,
    /// Number of distinct providers represented by the cluster.
    pub distinct_provider_count: u32,
    /// State of the resolved location or code anchor.
    pub anchor_state: String,
    /// Semantic match classification for cluster members.
    pub semantic_match_state: MatchState,
    /// Dimensions on which reviewer candidates disagree.
    pub disagreement_dimensions: Vec<String>,
    /// Reconciled cross-reviewer classification.
    pub classification: Classification,
    /// Digest of the canonical cluster record.
    pub content_hash: String,
}

/// Bounded context and independence requirements for adjudicating a cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdjudicationRequest {
    /// Finding cluster being adjudicated.
    pub cluster_ref: String,
    /// Exact code or target context the adjudicator may inspect.
    pub exact_code_context_ref: String,
    /// Claims supporting the finding.
    pub supporting_claim_refs: Vec<String>,
    /// Claims contradicting the finding.
    pub contradicting_claim_refs: Vec<String>,
    /// Rules applicable to the reviewed finding.
    pub applicable_rule_refs: Vec<String>,
    /// Minimum independence required from the adjudicator.
    pub required_independence: IndependenceClass,
    /// Maximum adjudicator calls allowed.
    pub max_calls: u32,
}

/// Evidence-bound outcome of an adjudication request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdjudicationResult {
    /// Cluster adjudicated by this result.
    pub cluster_ref: String,
    /// Adjudicator's conclusion.
    pub verdict: AdjudicationVerdict,
    /// Normalized confidence label.
    pub confidence: String,
    /// Evidence references considered by the adjudicator.
    pub evidence_refs: Vec<String>,
    /// Serialized reviewer-independence snapshot.
    pub reviewer_independence_snapshot: String,
    /// Bounded reasons supporting the verdict.
    pub reasons: Vec<String>,
    /// Digest of the canonical adjudication result.
    pub content_hash: String,
}

/// Completion claim and its required independent review and verification proof.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionAssessment {
    /// Target whose completion is being assessed.
    pub target_ref: String,
    /// Optional agent or run that authored the target.
    pub authoring_agent_ref: Option<String>,
    /// Optional independent reviewer ensemble reference.
    pub reviewer_ensemble_ref: Option<String>,
    /// Verification evidence required to claim completion.
    pub required_verification_refs: Vec<String>,
    /// Acceptance criteria that remain unmet.
    pub unmet_acceptance_criteria: Vec<String>,
    /// Freshness and outcome state of the verification proof.
    pub proof_state: ProofState,
    /// Derived completion verdict.
    pub verdict: CompletionVerdict,
    /// Digest of the canonical assessment.
    pub content_hash: String,
}

/// Invalid input, exhausted bounds, or content-digest mismatch.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnsembleError {
    /// A profile, snapshot, or result violated its contract.
    #[error("invalid ensemble field: {0}")]
    Invalid(&'static str),
    /// A configured collection exceeded its maximum size.
    #[error("ensemble limit exceeded: {0}")]
    Limit(&'static str),
    /// The stored digest does not match the canonical record content.
    #[error("content hash mismatch")]
    HashMismatch,
}

fn hash<T: Serialize>(value: &T) -> Result<String, EnsembleError> {
    let bytes = serde_json::to_vec(value).map_err(|_| EnsembleError::Invalid("json"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
fn bounded(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_TEXT
}
fn with_empty_hash<T: Serialize + Clone>(
    value: &T,
    clear: impl FnOnce(&mut T),
) -> Result<String, EnsembleError> {
    let mut copy = value.clone();
    clear(&mut copy);
    hash(&copy)
}

/// Computes the profile digest with its `content_hash` field cleared.
pub fn profile_hash(profile: &ReviewerEnsembleProfile) -> Result<String, EnsembleError> {
    with_empty_hash(profile, |v| v.content_hash.clear())
}
/// Computes the immutable reviewer input snapshot digest.
pub fn snapshot_hash(snapshot: &EnsembleInputSnapshot) -> Result<String, EnsembleError> {
    with_empty_hash(snapshot, |v| v.content_hash.clear())
}
/// Computes the ensemble run digest with its `content_hash` field cleared.
pub fn run_hash(run: &ReviewerEnsembleRun) -> Result<String, EnsembleError> {
    with_empty_hash(run, |v| v.content_hash.clear())
}
/// Computes the finding candidate digest with its `content_hash` field cleared.
pub fn candidate_hash(
    candidate: &NormalizedReviewFindingCandidate,
) -> Result<String, EnsembleError> {
    with_empty_hash(candidate, |v| v.content_hash.clear())
}
/// Computes the finding cluster digest with its `content_hash` field cleared.
pub fn cluster_hash(cluster: &EnsembleFindingCluster) -> Result<String, EnsembleError> {
    with_empty_hash(cluster, |v| v.content_hash.clear())
}

/// Validates profile bounds, lifecycle, reviewer slots, and canonical digest.
pub fn validate_profile(profile: &ReviewerEnsembleProfile) -> Result<(), EnsembleError> {
    if profile.schema_version != SCHEMA_VERSION
        || profile.revision == 0
        || !bounded(&profile.id)
        || matches!(profile.lifecycle, Lifecycle::Invalid)
        || profile.reviewer_slots.is_empty()
        || profile.reviewer_slots.len() > MAX_SLOTS
        || profile.minimum_completed_slots == 0
        || profile.minimum_completed_slots as usize > profile.reviewer_slots.len()
        || !bounded(&profile.adjudication_policy_ref)
        || !bounded(&profile.reconciliation_policy_ref)
        || !bounded(&profile.budget_ref)
        || profile_hash(profile)? != profile.content_hash
    {
        return Err(EnsembleError::Invalid("profile"));
    }
    if profile.reviewer_slots.iter().any(|slot| {
        !bounded(&slot.id)
            || !bounded(&slot.reviewer_profile_ref)
            || !bounded(&slot.model_purpose_ref)
            || slot.max_calls == 0
    }) {
        return Err(EnsembleError::Invalid("slot"));
    }
    Ok(())
}
/// Validates the target snapshot's required references, timestamp, and digest.
pub fn validate_snapshot(snapshot: &EnsembleInputSnapshot) -> Result<(), EnsembleError> {
    if !bounded(&snapshot.target_ref)
        || !bounded(&snapshot.target_diff_hash)
        || !bounded(&snapshot.unit_context_hash)
        || !bounded(&snapshot.applicable_rule_set_hash)
        || !bounded(&snapshot.project_guidance_revision)
        || snapshot.created_at_ms <= 0
        || snapshot.verification_context_refs.len() > 128
        || snapshot_hash(snapshot)? != snapshot.content_hash
    {
        return Err(EnsembleError::Invalid("snapshot"));
    }
    Ok(())
}
/// Validates a run's profile binding, slot alignment, timestamp, snapshot, and digest.
pub fn validate_run(run: &ReviewerEnsembleRun) -> Result<(), EnsembleError> {
    if run.schema_version != SCHEMA_VERSION
        || !bounded(&run.id)
        || run.revision == 0
        || !bounded(&run.profile_ref)
        || run.profile_revision == 0
        || run.reviewer_run_refs.len() > MAX_SLOTS
        || run.slot_statuses.len() != run.reviewer_run_refs.len()
        || run.started_at_ms <= 0
        || run_hash(run)? != run.content_hash
    {
        return Err(EnsembleError::Invalid("run"));
    }
    validate_snapshot(&run.input_snapshot)
}

/// Reconciles exact same-location, same-signature findings into deterministic clusters.
///
/// # Example
///
/// ```
/// use evohime_core::multi_reviewer_ensemble::reconcile_candidates;
///
/// let clusters = reconcile_candidates(&[])?;
/// assert!(clusters.is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn reconcile_candidates(
    candidates: &[NormalizedReviewFindingCandidate],
) -> Result<Vec<EnsembleFindingCluster>, EnsembleError> {
    if candidates.len() > MAX_CANDIDATES {
        return Err(EnsembleError::Limit("candidates"));
    }
    for candidate in candidates {
        if !bounded(&candidate.candidate_ref)
            || !bounded(&candidate.reviewer_run_ref)
            || !bounded(&candidate.category)
            || !bounded(&candidate.issue_signature)
            || candidate_hash(candidate)? != candidate.content_hash
        {
            return Err(EnsembleError::Invalid("candidate"));
        }
    }
    let mut clusters = Vec::new();
    for candidate in candidates {
        if let Some(cluster) = clusters
            .iter_mut()
            .find(|cluster: &&mut EnsembleFindingCluster| {
                cluster.semantic_match_state == MatchState::ExactSameDefect
                    && cluster.anchor_state == candidate.resolved_position
                    && cluster.id == candidate.issue_signature
            })
        {
            cluster
                .member_candidate_refs
                .push(candidate.candidate_ref.clone());
            cluster
                .reviewer_refs
                .push(candidate.reviewer_run_ref.clone());
            continue;
        }
        let mut cluster = EnsembleFindingCluster {
            id: candidate.issue_signature.clone(),
            member_candidate_refs: vec![candidate.candidate_ref.clone()],
            reviewer_refs: vec![candidate.reviewer_run_ref.clone()],
            distinct_model_families: Vec::new(),
            distinct_provider_count: 0,
            anchor_state: candidate.resolved_position.clone(),
            semantic_match_state: MatchState::ExactSameDefect,
            disagreement_dimensions: Vec::new(),
            classification: Classification::SingleSource,
            content_hash: String::new(),
        };
        cluster.content_hash = cluster_hash(&cluster)?;
        clusters.push(cluster);
    }
    for cluster in &mut clusters {
        cluster.classification = if cluster.member_candidate_refs.len() >= 2 {
            Classification::Corroborated
        } else {
            Classification::SingleSource
        };
        cluster.content_hash = cluster_hash(cluster)?;
    }
    Ok(clusters)
}

/// Derives a completion verdict from unmet criteria, independent review, and proof freshness.
///
/// Unmet criteria always prevent completion, and missing independent review
/// cannot be compensated for by a fresh proof.
///
/// # Example
///
/// ```
/// use evohime_core::multi_reviewer_ensemble::{
///     completion_verdict, CompletionVerdict, ProofState,
/// };
///
/// assert_eq!(
///     completion_verdict(ProofState::FreshPassed, true, 0),
///     CompletionVerdict::Done,
/// );
/// assert_eq!(
///     completion_verdict(ProofState::FreshPassed, false, 0),
///     CompletionVerdict::NeedsReview,
/// );
/// ```
pub fn completion_verdict(
    proof_state: ProofState,
    independent_reviewer: bool,
    unmet: usize,
) -> CompletionVerdict {
    if unmet > 0 {
        return CompletionVerdict::NotDone;
    }
    if !independent_reviewer {
        return CompletionVerdict::NeedsReview;
    }
    match proof_state {
        ProofState::FreshPassed => CompletionVerdict::Done,
        ProofState::Stale | ProofState::Missing | ProofState::Failed => {
            CompletionVerdict::NeedsVerification
        }
        ProofState::Unknown => CompletionVerdict::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> ReviewerEnsembleProfile {
        let mut p = ReviewerEnsembleProfile {
            schema_version: SCHEMA_VERSION,
            id: "p".into(),
            revision: 1,
            lifecycle: Lifecycle::Active,
            purpose: Purpose::CodeReview,
            reviewer_slots: vec![ReviewerSlot {
                id: "a".into(),
                reviewer_profile_ref: "r".into(),
                model_purpose_ref: "review".into(),
                required_independence: IndependenceClass::DistinctModelFamily,
                role_lens: None,
                required: true,
                priority: 1,
                max_calls: 1,
                content_hash: String::new(),
            }],
            minimum_completed_slots: 1,
            minimum_distinct_model_families: None,
            adjudication_policy_ref: "adjudication".into(),
            reconciliation_policy_ref: "reconciliation".into(),
            budget_ref: "budget".into(),
            failure_policy: "fail_closed".into(),
            content_hash: String::new(),
        };
        p.content_hash = profile_hash(&p).unwrap();
        p
    }
    #[test]
    fn profile_is_bounded_and_hash_bound() {
        assert!(validate_profile(&profile()).is_ok());
    }
    #[test]
    fn fake_diversity_never_satisfies_completion() {
        assert_eq!(
            completion_verdict(ProofState::FreshPassed, false, 0),
            CompletionVerdict::NeedsReview
        );
    }
    #[test]
    fn stale_proof_never_is_done() {
        assert_eq!(
            completion_verdict(ProofState::Stale, true, 0),
            CompletionVerdict::NeedsVerification
        );
    }
}
