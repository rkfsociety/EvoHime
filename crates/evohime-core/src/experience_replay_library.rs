//! Bounded, untrusted episodic experience records (plan 68).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current schema version for persisted experience records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum length of a stored request or observation summary.
pub const MAX_SUMMARY: usize = 2048;
/// Maximum number of steps retained in one trajectory.
pub const MAX_STEPS: usize = 32;
/// Maximum bytes projected into a prompt from retrieved experiences.
pub const MAX_CONTEXT_BYTES: usize = 32 * 1024;
/// Scope at which an experience record may be retrieved.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExperienceScope {
    /// Available only within the originating session.
    Session,
    /// Available within one project.
    Project,
    /// Available to the current user.
    User,
    /// Associated with one role profile.
    RoleProfile,
    /// Associated with one workflow profile.
    WorkflowProfile,
}
/// Outcome observed for the recorded task.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Outcome {
    /// Task completed successfully.
    Success,
    /// Task completed with only part of its objective met.
    PartialSuccess,
    /// Task completed unsuccessfully.
    Failure,
    /// Task ended before completion.
    Aborted,
    /// Policy prevented the task from proceeding.
    PolicyBlocked,
    /// Outcome could not be established.
    UnknownOutcome,
}
/// Retrieval strategy used to find relevant experience records.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RetrievalMode {
    /// Match an exact identifier or task key.
    Exact,
    /// Match lexical terms in summaries and tags.
    Lexical,
    /// Match semantic similarity using an embedding index.
    Semantic,
    /// Combine exact, lexical, and semantic signals.
    Hybrid,
}
/// Bounded metadata describing one phase of an experience trajectory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceStep {
    /// Name of the phase represented by this step.
    pub phase: String,
    /// Optional summary of the plan used in this phase.
    pub plan_summary: Option<String>,
    /// Optional reference to the action performed.
    pub action_ref: Option<String>,
    /// Safe projection of action arguments, if retained.
    pub action_args_projection: Option<String>,
    /// Optional summary of the resulting observation.
    pub observation_summary: Option<String>,
    /// Stable result category for this phase.
    pub result_class: String,
    /// Optional score change attributed to this step.
    pub score_delta: Option<f32>,
}
/// Quality and evidence measures associated with an experience.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceScore {
    /// Overall quality score in the inclusive range from zero to one.
    pub quality: f32,
    /// Optional correctness score in the inclusive range from zero to one.
    pub correctness: Option<f32>,
    /// Optional efficiency score in the inclusive range from zero to one.
    pub efficiency: Option<f32>,
    /// Security-compliance score in the inclusive range from zero to one.
    pub security_compliance: f32,
    /// Number of independent evidence items supporting the score.
    pub evidence_count: u32,
}
/// Bounded, untrusted episode metadata used for scoped experience replay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceRecord {
    /// Stable experience identifier.
    pub id: String,
    /// Retrieval scope for this record.
    pub scope: ExperienceScope,
    /// Identifier of the session, project, user, or profile scope.
    pub scope_id: String,
    /// Redacted summary of the request that produced the experience.
    pub request_summary: String,
    /// Optional task category used for retrieval.
    pub task_class: Option<String>,
    /// Optional digest identifying relevant workspace or context state.
    pub context_fingerprint: Option<String>,
    /// Bounded sequence of phase summaries and evidence references.
    pub trajectory: Vec<ExperienceStep>,
    /// Observed task outcome.
    pub outcome: Outcome,
    /// Quality and security score supported by evidence.
    pub score: ExperienceScore,
    /// References to independent evidence; raw sensitive content is excluded.
    pub evidence_refs: Vec<String>,
    /// Bounded labels useful for lexical retrieval.
    pub tags: Vec<String>,
    /// Digest of the record with this field cleared.
    pub content_hash: String,
    /// Sensitivity classification of the retained metadata.
    pub sensitivity: String,
    /// Provenance describing how the record was created.
    pub provenance: String,
    /// Unix timestamp in milliseconds when the record was created.
    pub created_at_ms: i64,
    /// Whether the record refers to context that is no longer current.
    pub stale: bool,
    /// Whether the record is protected from ordinary eviction.
    pub pinned: bool,
}
/// Retrieval filters and output bounds for experience replay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceQuery {
    /// Scope type to search.
    pub scope: ExperienceScope,
    /// Specific scope identifier.
    pub scope_id: String,
    /// Current task summary used for relevance matching.
    pub task_summary: String,
    /// Required or preferred retrieval tags.
    pub tags: Vec<String>,
    /// Whether unsuccessful examples may be returned.
    pub include_failure_examples: bool,
    /// Retrieval algorithm to use.
    pub mode: RetrievalMode,
    /// Maximum number of records to return.
    pub max_results: usize,
    /// Maximum context bytes returned to the caller.
    pub max_context_bytes: usize,
}
/// Unsupported record, bound, scope, write-gate, or scoring failure.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExperienceError {
    /// Input uses a schema version unsupported by this implementation.
    #[error("unsupported experience schema version")]
    UnsupportedVersion,
    /// Serialized experience input exceeds a documented bound.
    #[error("experience input exceeds bounds")]
    TooLarge,
    /// Record is outside the caller's permitted scope.
    #[error("experience is outside allowed scope")]
    ScopeDenied,
    /// Record did not pass evidence and privacy write requirements.
    #[error("write gate rejected experience: {0}")]
    WriteGate(String),
    /// Unknown outcome cannot be stored as a scored success.
    #[error("unknown outcome cannot be scored as success")]
    UnknownOutcome,
    /// Record fields or integrity digest are invalid.
    #[error("invalid experience: {0}")]
    Invalid(String),
}
/// Computes the SHA-256 digest of a record with its content-hash field cleared.
pub fn content_hash(record: &ExperienceRecord) -> Result<String, ExperienceError> {
    let mut copy = record.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|e| ExperienceError::Invalid(e.to_string()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}
/// Enforces bounded identity, independent evidence, safe references, and valid scores.
pub fn validate_and_write_gate(record: &ExperienceRecord) -> Result<(), ExperienceError> {
    if record.id.is_empty()
        || record.scope_id.is_empty()
        || record.request_summary.len() > MAX_SUMMARY
        || record.trajectory.len() > MAX_STEPS
        || record.evidence_refs.is_empty()
    {
        return Err(ExperienceError::WriteGate(
            "missing bounded identity, trajectory or independent evidence".into(),
        ));
    }
    if matches!(record.outcome, Outcome::UnknownOutcome) {
        return Err(ExperienceError::UnknownOutcome);
    }
    if record
        .evidence_refs
        .iter()
        .any(|r| r.len() > 256 || r.contains("secret") || r.contains("credential"))
    {
        return Err(ExperienceError::WriteGate("unsafe evidence ref".into()));
    }
    if record.content_hash != content_hash(record)? {
        return Err(ExperienceError::Invalid("content hash".into()));
    }
    if !(0.0..=1.0).contains(&record.score.quality)
        || !(0.0..=1.0).contains(&record.score.security_compliance)
        || record.score.evidence_count == 0
    {
        return Err(ExperienceError::WriteGate("score requires evidence".into()));
    }
    Ok(())
}
/// Formats bounded experience metadata for model context without raw payloads.
pub fn project_context(
    records: &[ExperienceRecord],
    max_bytes: usize,
) -> Result<String, ExperienceError> {
    let limit = max_bytes.min(MAX_CONTEXT_BYTES);
    let mut out = String::new();
    for r in records {
        let line = format!(
            "- {}: outcome={:?}, quality={:.2}, evidence={}, stale={}\n",
            r.request_summary, r.outcome, r.score.quality, r.score.evidence_count, r.stale
        );
        if out.len() + line.len() > limit {
            break;
        }
        out.push_str(&line);
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record(outcome: Outcome) -> ExperienceRecord {
        let mut r = ExperienceRecord {
            id: "e1".into(),
            scope: ExperienceScope::Project,
            scope_id: "p1".into(),
            request_summary: "build failure".into(),
            task_class: None,
            context_fingerprint: None,
            trajectory: vec![ExperienceStep {
                phase: "result".into(),
                plan_summary: Some("fix".into()),
                action_ref: Some("test".into()),
                action_args_projection: None,
                observation_summary: Some("passed".into()),
                result_class: "ok".into(),
                score_delta: Some(1.0),
            }],
            outcome,
            score: ExperienceScore {
                quality: 0.8,
                correctness: Some(1.0),
                efficiency: None,
                security_compliance: 1.0,
                evidence_count: 1,
            },
            evidence_refs: vec!["test:42".into()],
            tags: vec!["rust".into()],
            content_hash: String::new(),
            sensitivity: "non_sensitive".into(),
            provenance: "core".into(),
            created_at_ms: 1,
            stale: false,
            pinned: false,
        };
        r.content_hash = content_hash(&r).unwrap();
        r
    }
    #[test]
    fn write_gate_rejects_unknown_and_accepts_evidence() {
        assert!(validate_and_write_gate(&record(Outcome::Success)).is_ok());
        assert_eq!(
            validate_and_write_gate(&record(Outcome::UnknownOutcome)),
            Err(ExperienceError::UnknownOutcome)
        );
    }
    #[test]
    fn context_is_bounded() {
        let r = record(Outcome::Failure);
        assert!(project_context(&[r], 8).unwrap().len() <= 8);
    }
}
