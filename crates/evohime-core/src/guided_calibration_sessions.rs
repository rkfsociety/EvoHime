//! Core-owned supervised calibration sessions.
//!
//! A session stores hashes and redacted feedback metadata, never prompts or
//! model output.  Consolidation delegates candidate creation to the existing
//! Continual Refinement pipeline; this module has no activation authority.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID: usize = 128;
pub const MAX_ITERATIONS: usize = 64;
pub const MAX_NOTE: usize = 2048;
pub const MAX_GUIDANCE: usize = 8192;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Open,
    Completed,
    Cancelled,
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackRating {
    Accept,
    Partial,
    Reject,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Feedback {
    pub actor_ref: String,
    pub rating: FeedbackRating,
    pub correction_hash: String,
    pub redacted_note: String,
    pub provenance_ref: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalibrationIteration {
    pub iteration_id: String,
    pub task_ref: String,
    pub baseline_hash: String,
    pub revised_hash: Option<String>,
    pub pattern_key: String,
    pub feedback: Option<Feedback>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuidanceCandidate {
    pub candidate_id: String,
    pub pattern_key: String,
    pub guidance_hash: String,
    pub source_iteration_ids: Vec<String>,
    pub refinement_candidate_id: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalibrationSession {
    pub schema_version: u32,
    pub session_id: String,
    pub owner_scope: String,
    pub subject_ref: String,
    pub actor_ref: String,
    pub policy_snapshot_hash: String,
    pub status: SessionStatus,
    pub revision: u64,
    pub iterations: Vec<CalibrationIteration>,
    pub candidates: Vec<GuidanceCandidate>,
    pub dataset_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CalibrationError {
    #[error("invalid calibration value: {0}")]
    Invalid(&'static str),
    #[error("unsupported calibration schema version")]
    UnsupportedVersion,
    #[error("session is not open")]
    SessionClosed,
    #[error("duplicate or stale iteration")]
    DuplicateOrStale,
    #[error("guidance must remain session-scoped and redacted")]
    UnsafeGuidance,
    #[error("consolidation requires repeated independent feedback")]
    InsufficientEvidence,
}
fn bounded(v: &str, n: usize) -> bool {
    !v.is_empty() && v.len() <= n && !v.chars().any(char::is_control)
}
pub fn hash(value: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_bytes())))
}
pub fn validate_feedback(feedback: &Feedback) -> Result<(), CalibrationError> {
    if !bounded(&feedback.actor_ref, MAX_ID)
        || feedback.correction_hash.len() != 71
        || !bounded(&feedback.redacted_note, MAX_NOTE)
        || !bounded(&feedback.provenance_ref, MAX_ID)
    {
        return Err(CalibrationError::Invalid("feedback"));
    }
    Ok(())
}
pub fn validate_session(session: &CalibrationSession) -> Result<(), CalibrationError> {
    if session.schema_version != SCHEMA_VERSION {
        return Err(CalibrationError::UnsupportedVersion);
    }
    if !bounded(&session.session_id, MAX_ID)
        || !bounded(&session.owner_scope, MAX_ID)
        || !bounded(&session.subject_ref, MAX_ID)
        || !bounded(&session.actor_ref, MAX_ID)
        || !bounded(&session.policy_snapshot_hash, MAX_ID)
        || session.revision == 0
        || session.iterations.len() > MAX_ITERATIONS
    {
        return Err(CalibrationError::Invalid("session"));
    }
    if session.candidates.iter().any(|c| {
        c.source_iteration_ids.is_empty()
            || !bounded(&c.candidate_id, MAX_ID)
            || !bounded(&c.refinement_candidate_id, MAX_ID)
    }) {
        return Err(CalibrationError::Invalid("candidate"));
    }
    for i in &session.iterations {
        if !bounded(&i.iteration_id, MAX_ID)
            || !bounded(&i.task_ref, MAX_ID)
            || !bounded(&i.pattern_key, MAX_ID)
            || i.baseline_hash.len() != 71
            || i.revised_hash.as_ref().is_some_and(|h| h.len() != 71)
        {
            return Err(CalibrationError::Invalid("iteration"));
        }
        if let Some(f) = &i.feedback {
            validate_feedback(f)?;
        }
    }
    Ok(())
}
pub fn add_iteration(
    session: &mut CalibrationSession,
    iteration: CalibrationIteration,
) -> Result<(), CalibrationError> {
    if !matches!(session.status, SessionStatus::Open) {
        return Err(CalibrationError::SessionClosed);
    }
    if session
        .iterations
        .iter()
        .any(|i| i.iteration_id == iteration.iteration_id)
    {
        return Err(CalibrationError::DuplicateOrStale);
    }
    session.iterations.push(iteration);
    session.revision = session.revision.saturating_add(1);
    session.dataset_hash = dataset_hash(session)?;
    validate_session(session)
}
pub fn consolidate(
    session: &CalibrationSession,
    candidate_id: &str,
    pattern_key: &str,
    guidance_text: &str,
) -> Result<GuidanceCandidate, CalibrationError> {
    if !matches!(session.status, SessionStatus::Open)
        || !bounded(candidate_id, MAX_ID)
        || !bounded(pattern_key, MAX_ID)
        || guidance_text.len() > MAX_GUIDANCE
        || guidance_text
            .to_ascii_lowercase()
            .contains("approval policy")
        || guidance_text.to_ascii_lowercase().contains("grant")
    {
        return Err(CalibrationError::UnsafeGuidance);
    }
    let source: Vec<&CalibrationIteration> = session
        .iterations
        .iter()
        .filter(|i| i.pattern_key == pattern_key && i.feedback.is_some())
        .collect();
    let distinct: std::collections::BTreeSet<&str> = source
        .iter()
        .filter_map(|i| i.feedback.as_ref().map(|f| f.provenance_ref.as_str()))
        .collect();
    if source.len() < 2 || distinct.len() < 2 {
        return Err(CalibrationError::InsufficientEvidence);
    }
    Ok(GuidanceCandidate {
        candidate_id: candidate_id.into(),
        pattern_key: pattern_key.into(),
        guidance_hash: hash(guidance_text),
        source_iteration_ids: source.iter().map(|i| i.iteration_id.clone()).collect(),
        refinement_candidate_id: format!("refinement:{candidate_id}"),
        status: "proposed_for_refinement".into(),
    })
}
pub fn dataset_hash(session: &CalibrationSession) -> Result<String, CalibrationError> {
    serde_json::to_vec(&session.iterations)
        .map(|v| format!("sha256:{}", hex::encode(Sha256::digest(v))))
        .map_err(|_| CalibrationError::Invalid("serialization"))
}
pub fn new_session(
    session_id: String,
    owner_scope: String,
    subject_ref: String,
    actor_ref: String,
    policy_snapshot_hash: String,
) -> CalibrationSession {
    CalibrationSession {
        schema_version: SCHEMA_VERSION,
        session_id,
        owner_scope,
        subject_ref,
        actor_ref,
        policy_snapshot_hash,
        status: SessionStatus::Open,
        revision: 1,
        iterations: Vec::new(),
        candidates: Vec::new(),
        dataset_hash: hash(""),
    }
}
pub fn as_map(session: &CalibrationSession) -> BTreeMap<String, String> {
    [
        ("session_id".into(), session.session_id.clone()),
        (
            "status".into(),
            serde_json::to_string(&session.status)
                .unwrap_or_default()
                .trim_matches('"')
                .into(),
        ),
        ("revision".into(), session.revision.to_string()),
        ("dataset_hash".into(), session.dataset_hash.clone()),
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> CalibrationSession {
        new_session(
            "s".into(),
            "workspace".into(),
            "role".into(),
            "human:1".into(),
            "policy".into(),
        )
    }
    fn iteration(id: &str, provenance: &str) -> CalibrationIteration {
        CalibrationIteration {
            iteration_id: id.into(),
            task_ref: format!("task-{id}"),
            baseline_hash: hash("baseline"),
            revised_hash: Some(hash("revised")),
            pattern_key: "pattern".into(),
            feedback: Some(Feedback {
                actor_ref: "human:1".into(),
                rating: FeedbackRating::Partial,
                correction_hash: hash("correction"),
                redacted_note: "bounded note".into(),
                provenance_ref: provenance.into(),
            }),
        }
    }
    #[test]
    fn repeated_feedback_produces_refinement_only_candidate() {
        let mut s = session();
        add_iteration(&mut s, iteration("i1", "p1")).unwrap();
        add_iteration(&mut s, iteration("i2", "p2")).unwrap();
        let c = consolidate(&s, "c", "pattern", "keep the answer concise").unwrap();
        assert_eq!(c.status, "proposed_for_refinement");
        assert_eq!(c.source_iteration_ids.len(), 2);
    }
    #[test]
    fn unsafe_or_single_feedback_fails_closed() {
        let mut s = session();
        add_iteration(&mut s, iteration("i1", "p1")).unwrap();
        assert_eq!(
            consolidate(&s, "c", "pattern", "grant unrestricted shell"),
            Err(CalibrationError::UnsafeGuidance)
        );
        assert_eq!(
            consolidate(&s, "c", "pattern", "safe"),
            Err(CalibrationError::InsufficientEvidence)
        );
    }
}
