//! Core-owned supervised calibration sessions.
//!
//! A session stores hashes and redacted feedback metadata, never prompts or
//! model output.  Consolidation delegates candidate creation to the existing
//! Continual Refinement pipeline; this module has no activation authority.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Current serialized schema version for calibration sessions.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum length of session, actor, and reference identifiers.
pub const MAX_ID: usize = 128;
/// Maximum number of feedback iterations in one session.
pub const MAX_ITERATIONS: usize = 64;
/// Maximum size of a redacted feedback note in bytes.
pub const MAX_NOTE: usize = 2048;
/// Maximum size of proposed guidance text in bytes.
pub const MAX_GUIDANCE: usize = 8192;

/// Lifecycle state of a supervised calibration session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session accepts further calibration iterations.
    Open,
    /// Session was explicitly completed.
    Completed,
    /// Session was cancelled by its owner.
    Cancelled,
    /// Session ended after a failure.
    Failed,
}
/// Human rating attached to one observed model interaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackRating {
    /// Output was accepted without correction.
    Accept,
    /// Output was partly useful but required correction.
    Partial,
    /// Output was rejected.
    Reject,
}
/// Redacted human feedback linked to its actor and provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Feedback {
    /// Identifier of the human providing the feedback.
    pub actor_ref: String,
    /// Acceptance rating for the observed output.
    pub rating: FeedbackRating,
    /// Digest of the correction; raw corrected content is not retained here.
    pub correction_hash: String,
    /// Bounded redacted note explaining the correction.
    pub redacted_note: String,
    /// Reference identifying the independent source of this feedback.
    pub provenance_ref: String,
}
/// One task observation and optional human feedback in a calibration session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalibrationIteration {
    /// Stable iteration identifier.
    pub iteration_id: String,
    /// Reference to the task used for the observation.
    pub task_ref: String,
    /// Digest of the baseline model output or strategy.
    pub baseline_hash: String,
    /// Optional digest of the revised output.
    pub revised_hash: Option<String>,
    /// Pattern key used to group related feedback.
    pub pattern_key: String,
    /// Human feedback, when supplied.
    pub feedback: Option<Feedback>,
}
/// Refinement candidate derived from repeated independent feedback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuidanceCandidate {
    /// Stable candidate identifier.
    pub candidate_id: String,
    /// Feedback pattern addressed by the guidance.
    pub pattern_key: String,
    /// Digest of the candidate guidance text.
    pub guidance_hash: String,
    /// Iterations supporting this candidate.
    pub source_iteration_ids: Vec<String>,
    /// Identifier passed to the existing refinement pipeline.
    pub refinement_candidate_id: String,
    /// Candidate lifecycle label; candidates are proposals only.
    pub status: String,
}
/// Session-scoped calibration data with bounded history and policy provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalibrationSession {
    /// Serialized schema version.
    pub schema_version: u32,
    /// Stable session identifier.
    pub session_id: String,
    /// Workspace or owner scope restricting this session.
    pub owner_scope: String,
    /// Role, agent, or other subject being calibrated.
    pub subject_ref: String,
    /// Actor who created the session.
    pub actor_ref: String,
    /// Digest of the policy active for this session.
    pub policy_snapshot_hash: String,
    /// Current session state.
    pub status: SessionStatus,
    /// Monotonic optimistic-concurrency revision.
    pub revision: u64,
    /// Bounded observations and feedback collected so far.
    pub iterations: Vec<CalibrationIteration>,
    /// Refinement proposals derived from session evidence.
    pub candidates: Vec<GuidanceCandidate>,
    /// Digest of the serialized iteration dataset.
    pub dataset_hash: String,
}

/// Invalid calibration data, unsafe guidance, or insufficient evidence.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CalibrationError {
    /// A field or serialized value violates the contract.
    #[error("invalid calibration value: {0}")]
    Invalid(&'static str),
    /// Session uses an unsupported schema version.
    #[error("unsupported calibration schema version")]
    UnsupportedVersion,
    /// Session is no longer open for changes.
    #[error("session is not open")]
    SessionClosed,
    /// Iteration identifier conflicts with existing session data.
    #[error("duplicate or stale iteration")]
    DuplicateOrStale,
    /// Proposed guidance violates bounded or safety policy.
    #[error("guidance must remain session-scoped and redacted")]
    UnsafeGuidance,
    /// Repeated independent feedback is required before consolidation.
    #[error("consolidation requires repeated independent feedback")]
    InsufficientEvidence,
}
fn bounded(v: &str, n: usize) -> bool {
    !v.is_empty() && v.len() <= n && !v.chars().any(char::is_control)
}
/// Returns a SHA-256 digest for bounded calibration text.
pub fn hash(value: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_bytes())))
}
/// Validates actor, correction digest, note, and provenance fields.
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
/// Validates session bounds and every nested iteration and feedback record.
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
/// Appends one unique observation and refreshes the session revision and digest.
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
/// Proposes refinement guidance only after independent feedback repeats a pattern.
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
/// Computes the digest of the session's ordered calibration iterations.
pub fn dataset_hash(session: &CalibrationSession) -> Result<String, CalibrationError> {
    serde_json::to_vec(&session.iterations)
        .map(|v| format!("sha256:{}", hex::encode(Sha256::digest(v))))
        .map_err(|_| CalibrationError::Invalid("serialization"))
}
/// Creates an open session with an empty dataset and initial revision.
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
/// Projects selected session metadata into a string map for status display.
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
