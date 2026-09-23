use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Serialized schema version for authorized security assessments.
pub const SCHEMA_VERSION: u32 = 1;
const MAX_PATHS: usize = 128;
const MAX_FINDINGS: usize = 512;

/// Lifecycle state of a scoped security assessment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentStatus {
    /// Created but not yet authorized.
    Draft,
    /// Authorization was granted; execution has not started.
    Authorized,
    /// Assessment tools are currently running.
    Running,
    /// Assessment finished and its results were recorded.
    Completed,
    /// Assessment stopped after an execution failure.
    Failed,
    /// Assessment was cancelled by an operator.
    Cancelled,
    /// Assessment cannot proceed because a prerequisite is unavailable.
    Blocked,
    /// State supplied by an older or unrecognized producer.
    Unknown,
}
/// Decision state for the authorization attached to an assessment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationState {
    /// Scope was explicitly approved and has not expired.
    Approved,
    /// Scope was explicitly rejected.
    Denied,
    /// Approval has passed its expiry time.
    Expired,
    /// Previously granted approval was withdrawn.
    Revoked,
    /// State supplied by an older or unrecognized producer.
    Unknown,
}
/// Triage state of an individual security finding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    /// Finding has not been triaged.
    Open,
    /// Evidence confirms the reported issue.
    Confirmed,
    /// Finding was reviewed and determined not to be an issue.
    FalsePositive,
    /// Finding is real, but its risk was explicitly accepted.
    AcceptedRisk,
    /// The issue has been fixed or otherwise resolved.
    Resolved,
    /// State supplied by an older or unrecognized producer.
    Unknown,
}

/// Workspace and limits authorized for a security assessment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scope {
    /// Stable reference identifying the target workspace.
    pub workspace_ref: String,
    /// Paths the assessment is permitted to inspect.
    pub allowed_paths: Vec<String>,
    /// Paths excluded even when they match an allowed path.
    pub excluded_paths: Vec<String>,
    /// Named tool policy under which the assessment may run.
    pub tool_profile: String,
    /// Maximum assessment duration in milliseconds.
    pub max_duration_ms: u64,
}
/// Signed-by-hash authorization record for an assessment scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Authorization {
    /// Stable authorization identifier referenced by the assessment.
    pub id: String,
    /// Actor that granted or denied the authorization.
    pub actor_ref: String,
    /// Current authorization decision.
    pub state: AuthorizationState,
    /// Unix timestamp in milliseconds after which approval is no longer valid.
    pub expires_at_ms: i64,
    /// Hash of the policy used to make the authorization decision.
    pub policy_hash: String,
    /// Exact workspace and tool scope approved by the actor.
    pub scope: Scope,
    /// Digest of this record with `content_hash` cleared.
    pub content_hash: String,
}
/// A security finding recorded during an assessment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    /// Stable digest used to identify the same issue across revisions.
    pub fingerprint: String,
    /// Severity label assigned to the finding.
    pub severity: String,
    /// Reference to the evidence supporting the finding.
    pub evidence_ref: String,
    /// Current triage state.
    pub state: FindingState,
}
/// Immutable assessment snapshot with scope, findings, and integrity digest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Assessment {
    /// Stable assessment identifier.
    pub id: String,
    /// One-based revision of the assessment record.
    pub revision: u64,
    /// Current execution lifecycle state.
    pub status: AssessmentStatus,
    /// Authorization required before this assessment can start.
    pub authorization_id: String,
    /// Exact scope evaluated by this assessment.
    pub scope: Scope,
    /// Security findings produced so far.
    pub findings: Vec<Finding>,
    /// Unix timestamp in milliseconds when the assessment was created.
    pub created_at_ms: i64,
    /// Digest of this record with `content_hash` cleared.
    pub content_hash: String,
}

/// Validation or authorization failure for a security assessment.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AssessmentError {
    /// A field, bound, or integrity digest is invalid.
    #[error("invalid security assessment: {0}")]
    Invalid(String),
    /// No matching approved authorization is available.
    #[error("assessment is not authorized")]
    NotAuthorized,
    /// The matching authorization has expired.
    #[error("assessment authorization expired")]
    Expired,
}
fn hash<T: Serialize>(value: &T) -> Result<String, AssessmentError> {
    serde_json::to_vec(value)
        .map(|v| hex::encode(Sha256::digest(v)))
        .map_err(|e| AssessmentError::Invalid(e.to_string()))
}
/// Checks path safety, required identifiers, and duration limits in a scope.
pub fn validate_scope(s: &Scope) -> Result<(), AssessmentError> {
    if s.workspace_ref.trim().is_empty()
        || s.tool_profile.trim().is_empty()
        || s.allowed_paths.len() > MAX_PATHS
        || s.excluded_paths.len() > MAX_PATHS
        || s.max_duration_ms == 0
        || s.max_duration_ms > 86_400_000
    {
        return Err(AssessmentError::Invalid("scope bounds".into()));
    }
    if s.allowed_paths
        .iter()
        .chain(s.excluded_paths.iter())
        .any(|p| p.trim().is_empty() || p.contains(".."))
    {
        return Err(AssessmentError::Invalid("unsafe path".into()));
    }
    Ok(())
}
/// Validates an authorization digest and confirms approval at `now_ms`.
pub fn validate_authorization(a: &Authorization, now_ms: i64) -> Result<(), AssessmentError> {
    validate_scope(&a.scope)?;
    if a.id.trim().is_empty()
        || a.actor_ref.trim().is_empty()
        || a.policy_hash.trim().is_empty()
        || a.expires_at_ms <= 0
    {
        return Err(AssessmentError::Invalid("authorization identity".into()));
    }
    let mut copy = a.clone();
    copy.content_hash.clear();
    if a.content_hash != hash(&copy)? {
        return Err(AssessmentError::Invalid("authorization hash".into()));
    }
    match a.state {
        AuthorizationState::Approved if a.expires_at_ms > now_ms => Ok(()),
        AuthorizationState::Expired if a.expires_at_ms <= now_ms => Err(AssessmentError::Expired),
        _ => Err(AssessmentError::NotAuthorized),
    }
}
/// Validates assessment identity, scope, finding bounds, and content digest.
pub fn validate_assessment(a: &Assessment) -> Result<(), AssessmentError> {
    validate_scope(&a.scope)?;
    if a.id.trim().is_empty()
        || a.authorization_id.trim().is_empty()
        || a.revision == 0
        || a.created_at_ms <= 0
        || a.findings.len() > MAX_FINDINGS
    {
        return Err(AssessmentError::Invalid("assessment identity".into()));
    }
    let mut copy = a.clone();
    copy.content_hash.clear();
    if a.content_hash != hash(&copy)? {
        return Err(AssessmentError::Invalid("assessment hash".into()));
    }
    Ok(())
}
/// Ensures the assessment references the supplied, currently valid authorization.
pub fn start_allowed(
    a: &Assessment,
    auth: &Authorization,
    now_ms: i64,
) -> Result<(), AssessmentError> {
    validate_assessment(a)?;
    if a.authorization_id != auth.id {
        return Err(AssessmentError::NotAuthorized);
    }
    validate_authorization(auth, now_ms)
}
