use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current schema version for deterministic review plans.
pub const SCHEMA_VERSION: u32 = 1;
/// Lifecycle state of a deterministic review plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Plan is being prepared.
    Draft,
    /// Plan is ready for review execution.
    Active,
    /// Plan has been replaced by a newer revision.
    Superseded,
    /// Plan is invalid and cannot be used.
    Invalid,
}
/// Content-addressed deterministic review plan and its evidence references.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewPlan {
    /// Schema version of the plan.
    pub schema_version: u32,
    /// Stable plan identifier.
    pub id: String,
    /// Monotonically increasing plan revision.
    pub revision: u64,
    /// Plan lifecycle state.
    pub lifecycle: Lifecycle,
    /// Scope to which the plan applies.
    pub scope: String,
    /// Actor responsible for the plan.
    pub actor: String,
    /// Evidence references considered by the plan.
    pub evidence_refs: Vec<String>,
    /// Hash of the canonical plan metadata.
    pub content_hash: String,
}
/// Validation errors for deterministic review plans.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReviewError {
    /// The plan failed schema, bounds, lifecycle, or content-hash validation.
    #[error("invalid deterministic review plan: {0}")]
    Invalid(String),
}
/// Computes the canonical SHA-256 hash with `content_hash` cleared.
pub fn canonical_hash(p: &ReviewPlan) -> Result<String, ReviewError> {
    let mut v = p.clone();
    v.content_hash.clear();
    let bytes =
        serde_json::to_vec(&v).map_err(|_| ReviewError::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
/// Validates plan bounds, lifecycle, evidence references, and canonical hash.
pub fn validate(p: &ReviewPlan) -> Result<(), ReviewError> {
    if p.schema_version != SCHEMA_VERSION
        || p.id.trim().is_empty()
        || p.id.len() > 256
        || p.scope.trim().is_empty()
        || p.scope.len() > 256
        || p.actor.len() > 256
        || p.revision == 0
        || p.evidence_refs.len() > 128
        || p.evidence_refs
            .iter()
            .any(|r| r.is_empty() || r.len() > 256)
    {
        return Err(ReviewError::Invalid("bounds".into()));
    }
    if canonical_hash(p)? != p.content_hash {
        return Err(ReviewError::Invalid("content_hash_mismatch".into()));
    }
    if matches!(p.lifecycle, Lifecycle::Invalid) {
        return Err(ReviewError::Invalid("invalid_lifecycle".into()));
    }
    Ok(())
}
/// Returns the deterministic metadata-only verdict after validating the plan.
pub fn verdict(p: &ReviewPlan) -> Result<serde_json::Value, ReviewError> {
    validate(p)?;
    Ok(
        serde_json::json!({"status":"deterministic_review_metadata_only","plan_id":p.id,"revision":p.revision,"scope":p.scope,"actor":p.actor,"evidence_count":p.evidence_refs.len(),"verdict":"unknown_without_evidence","external_effect":false}),
    )
}
