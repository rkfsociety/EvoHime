//! Bounded, untrusted episodic experience records (plan 68).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SUMMARY: usize = 2048;
pub const MAX_STEPS: usize = 32;
pub const MAX_CONTEXT_BYTES: usize = 32 * 1024;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExperienceScope {
    Session,
    Project,
    User,
    RoleProfile,
    WorkflowProfile,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Outcome {
    Success,
    PartialSuccess,
    Failure,
    Aborted,
    PolicyBlocked,
    UnknownOutcome,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RetrievalMode {
    Exact,
    Lexical,
    Semantic,
    Hybrid,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceStep {
    pub phase: String,
    pub plan_summary: Option<String>,
    pub action_ref: Option<String>,
    pub action_args_projection: Option<String>,
    pub observation_summary: Option<String>,
    pub result_class: String,
    pub score_delta: Option<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceScore {
    pub quality: f32,
    pub correctness: Option<f32>,
    pub efficiency: Option<f32>,
    pub security_compliance: f32,
    pub evidence_count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceRecord {
    pub id: String,
    pub scope: ExperienceScope,
    pub scope_id: String,
    pub request_summary: String,
    pub task_class: Option<String>,
    pub context_fingerprint: Option<String>,
    pub trajectory: Vec<ExperienceStep>,
    pub outcome: Outcome,
    pub score: ExperienceScore,
    pub evidence_refs: Vec<String>,
    pub tags: Vec<String>,
    pub content_hash: String,
    pub sensitivity: String,
    pub provenance: String,
    pub created_at_ms: i64,
    pub stale: bool,
    pub pinned: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperienceQuery {
    pub scope: ExperienceScope,
    pub scope_id: String,
    pub task_summary: String,
    pub tags: Vec<String>,
    pub include_failure_examples: bool,
    pub mode: RetrievalMode,
    pub max_results: usize,
    pub max_context_bytes: usize,
}
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExperienceError {
    #[error("unsupported experience schema version")]
    UnsupportedVersion,
    #[error("experience input exceeds bounds")]
    TooLarge,
    #[error("experience is outside allowed scope")]
    ScopeDenied,
    #[error("write gate rejected experience: {0}")]
    WriteGate(String),
    #[error("unknown outcome cannot be scored as success")]
    UnknownOutcome,
    #[error("invalid experience: {0}")]
    Invalid(String),
}
pub fn content_hash(record: &ExperienceRecord) -> Result<String, ExperienceError> {
    let mut copy = record.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|e| ExperienceError::Invalid(e.to_string()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}
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
