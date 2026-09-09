use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_PATHS: usize = 128;
const MAX_FINDINGS: usize = 512;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentStatus { Draft, Authorized, Running, Completed, Failed, Cancelled, Blocked, Unknown }
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationState { Approved, Denied, Expired, Revoked, Unknown }
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingState { Open, Confirmed, FalsePositive, AcceptedRisk, Resolved, Unknown }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scope { pub workspace_ref: String, pub allowed_paths: Vec<String>, pub excluded_paths: Vec<String>, pub tool_profile: String, pub max_duration_ms: u64 }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Authorization { pub id: String, pub actor_ref: String, pub state: AuthorizationState, pub expires_at_ms: i64, pub policy_hash: String, pub scope: Scope, pub content_hash: String }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding { pub fingerprint: String, pub severity: String, pub evidence_ref: String, pub state: FindingState }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Assessment { pub id: String, pub revision: u64, pub status: AssessmentStatus, pub authorization_id: String, pub scope: Scope, pub findings: Vec<Finding>, pub created_at_ms: i64, pub content_hash: String }

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AssessmentError { #[error("invalid security assessment: {0}")] Invalid(String), #[error("assessment is not authorized")] NotAuthorized, #[error("assessment authorization expired")] Expired }
fn hash<T: Serialize>(value: &T) -> Result<String, AssessmentError> { serde_json::to_vec(value).map(|v| hex::encode(Sha256::digest(v))).map_err(|e| AssessmentError::Invalid(e.to_string())) }
pub fn validate_scope(s: &Scope) -> Result<(), AssessmentError> { if s.workspace_ref.trim().is_empty() || s.tool_profile.trim().is_empty() || s.allowed_paths.len() > MAX_PATHS || s.excluded_paths.len() > MAX_PATHS || s.max_duration_ms == 0 || s.max_duration_ms > 86_400_000 { return Err(AssessmentError::Invalid("scope bounds".into())); } if s.allowed_paths.iter().chain(s.excluded_paths.iter()).any(|p| p.trim().is_empty() || p.contains("..")) { return Err(AssessmentError::Invalid("unsafe path".into())); } Ok(()) }
pub fn validate_authorization(a: &Authorization, now_ms: i64) -> Result<(), AssessmentError> { validate_scope(&a.scope)?; if a.id.trim().is_empty() || a.actor_ref.trim().is_empty() || a.policy_hash.trim().is_empty() || a.expires_at_ms <= 0 { return Err(AssessmentError::Invalid("authorization identity".into())); } let mut copy = a.clone(); copy.content_hash.clear(); if a.content_hash != hash(&copy)? { return Err(AssessmentError::Invalid("authorization hash".into())); } match a.state { AuthorizationState::Approved if a.expires_at_ms > now_ms => Ok(()), AuthorizationState::Expired if a.expires_at_ms <= now_ms => Err(AssessmentError::Expired), _ => Err(AssessmentError::NotAuthorized) } }
pub fn validate_assessment(a: &Assessment) -> Result<(), AssessmentError> { validate_scope(&a.scope)?; if a.id.trim().is_empty() || a.authorization_id.trim().is_empty() || a.revision == 0 || a.created_at_ms <= 0 || a.findings.len() > MAX_FINDINGS { return Err(AssessmentError::Invalid("assessment identity".into())); } let mut copy = a.clone(); copy.content_hash.clear(); if a.content_hash != hash(&copy)? { return Err(AssessmentError::Invalid("assessment hash".into())); } Ok(()) }
pub fn start_allowed(a: &Assessment, auth: &Authorization, now_ms: i64) -> Result<(), AssessmentError> { validate_assessment(a)?; if a.authorization_id != auth.id { return Err(AssessmentError::NotAuthorized); } validate_authorization(auth, now_ms) }
