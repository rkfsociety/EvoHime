//! Core-owned bounded multi-reviewer reconciliation and completion contract.
//!
//! Reviewer execution remains owned by the existing model/review lanes. This
//! module only validates immutable snapshots, records independence and derives
//! conservative cross-reviewer classifications.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SLOTS: usize = 16;
pub const MAX_CANDIDATES: usize = 512;
pub const MAX_CLUSTERS: usize = 512;
pub const MAX_REASONS: usize = 32;
pub const MAX_TEXT: usize = 512;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    CodeReview,
    CompletionCheck,
    ArchitectureReview,
    SecurityReview,
    PlanCritique,
    QuestionPanel,
    CustomRegistered,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Draft,
    Active,
    Superseded,
    Invalid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndependenceClass {
    SameRunAuthor,
    SameModelFreshContext,
    SameFamilyFreshContext,
    DistinctModelFamily,
    IndependentProvider,
    Human,
    Deterministic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlotStatus {
    Queued,
    Completed,
    Failed,
    Unavailable,
    TimedOut,
    BudgetLimited,
    PolicyDenied,
    Cancelled,
    ProtocolError,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Reconciling,
    Adjudicating,
    Completed,
    Degraded,
    Incomplete,
    Blocked,
    Cancelled,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchState {
    ExactSameDefect,
    LikelySameDefect,
    RelatedButDistinct,
    Ambiguous,
    Distinct,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    Corroborated,
    SingleSource,
    Disagreed,
    Dropped,
    NeedsAdjudication,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdjudicationVerdict {
    Confirmed,
    Rejected,
    StillDisagreed,
    InsufficientEvidence,
    InvalidScope,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompletionVerdict {
    Done,
    NotDone,
    NeedsVerification,
    NeedsReview,
    Disagreed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProofState {
    FreshPassed,
    Stale,
    Failed,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerSlot {
    pub id: String,
    pub reviewer_profile_ref: String,
    pub model_purpose_ref: String,
    pub required_independence: IndependenceClass,
    pub role_lens: Option<String>,
    pub required: bool,
    pub priority: u32,
    pub max_calls: u32,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerEnsembleProfile {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub lifecycle: Lifecycle,
    pub purpose: Purpose,
    pub reviewer_slots: Vec<ReviewerSlot>,
    pub minimum_completed_slots: u32,
    pub minimum_distinct_model_families: Option<u32>,
    pub adjudication_policy_ref: String,
    pub reconciliation_policy_ref: String,
    pub budget_ref: String,
    pub failure_policy: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnsembleInputSnapshot {
    pub target_ref: String,
    pub target_diff_hash: String,
    pub review_unit_ref: Option<String>,
    pub unit_context_hash: String,
    pub applicable_rule_set_hash: String,
    pub project_guidance_revision: String,
    pub verification_context_refs: Vec<String>,
    pub created_at_ms: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerIndependenceSnapshot {
    pub reviewer_run_ref: String,
    pub provider_or_executor_ref: String,
    pub model_identity: Option<String>,
    pub model_family: Option<String>,
    pub authoring_run_relation: IndependenceClass,
    pub evidence_class: IndependenceClass,
    pub context_relation: IndependenceClass,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerEnsembleRun {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub profile_ref: String,
    pub profile_revision: u64,
    pub input_snapshot: EnsembleInputSnapshot,
    pub reviewer_run_refs: Vec<String>,
    pub status: RunStatus,
    pub slot_statuses: Vec<SlotStatus>,
    pub independence: Vec<ReviewerIndependenceSnapshot>,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedReviewFindingCandidate {
    pub candidate_ref: String,
    pub reviewer_run_ref: String,
    pub category: String,
    pub severity: String,
    pub resolved_position: String,
    pub semantic_anchor: String,
    pub issue_signature: String,
    pub evidence_refs: Vec<String>,
    pub confidence: String,
    pub validation_state: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnsembleFindingCluster {
    pub id: String,
    pub member_candidate_refs: Vec<String>,
    pub reviewer_refs: Vec<String>,
    pub distinct_model_families: Vec<String>,
    pub distinct_provider_count: u32,
    pub anchor_state: String,
    pub semantic_match_state: MatchState,
    pub disagreement_dimensions: Vec<String>,
    pub classification: Classification,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdjudicationRequest {
    pub cluster_ref: String,
    pub exact_code_context_ref: String,
    pub supporting_claim_refs: Vec<String>,
    pub contradicting_claim_refs: Vec<String>,
    pub applicable_rule_refs: Vec<String>,
    pub required_independence: IndependenceClass,
    pub max_calls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdjudicationResult {
    pub cluster_ref: String,
    pub verdict: AdjudicationVerdict,
    pub confidence: String,
    pub evidence_refs: Vec<String>,
    pub reviewer_independence_snapshot: String,
    pub reasons: Vec<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionAssessment {
    pub target_ref: String,
    pub authoring_agent_ref: Option<String>,
    pub reviewer_ensemble_ref: Option<String>,
    pub required_verification_refs: Vec<String>,
    pub unmet_acceptance_criteria: Vec<String>,
    pub proof_state: ProofState,
    pub verdict: CompletionVerdict,
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnsembleError {
    #[error("invalid ensemble field: {0}")]
    Invalid(&'static str),
    #[error("ensemble limit exceeded: {0}")]
    Limit(&'static str),
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

pub fn profile_hash(profile: &ReviewerEnsembleProfile) -> Result<String, EnsembleError> {
    with_empty_hash(profile, |v| v.content_hash.clear())
}
pub fn snapshot_hash(snapshot: &EnsembleInputSnapshot) -> Result<String, EnsembleError> {
    with_empty_hash(snapshot, |v| v.content_hash.clear())
}
pub fn run_hash(run: &ReviewerEnsembleRun) -> Result<String, EnsembleError> {
    with_empty_hash(run, |v| v.content_hash.clear())
}
pub fn candidate_hash(
    candidate: &NormalizedReviewFindingCandidate,
) -> Result<String, EnsembleError> {
    with_empty_hash(candidate, |v| v.content_hash.clear())
}
pub fn cluster_hash(cluster: &EnsembleFindingCluster) -> Result<String, EnsembleError> {
    with_empty_hash(cluster, |v| v.content_hash.clear())
}

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
