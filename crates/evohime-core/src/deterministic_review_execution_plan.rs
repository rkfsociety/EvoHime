use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Draft,
    Active,
    Superseded,
    Invalid,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewPlan {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub lifecycle: Lifecycle,
    pub scope: String,
    pub actor: String,
    pub evidence_refs: Vec<String>,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReviewError {
    #[error("invalid deterministic review plan: {0}")]
    Invalid(String),
}
pub fn canonical_hash(p: &ReviewPlan) -> Result<String, ReviewError> {
    let mut v = p.clone();
    v.content_hash.clear();
    let bytes =
        serde_json::to_vec(&v).map_err(|_| ReviewError::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
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
pub fn verdict(p: &ReviewPlan) -> Result<serde_json::Value, ReviewError> {
    validate(p)?;
    Ok(
        serde_json::json!({"status":"deterministic_review_metadata_only","plan_id":p.id,"revision":p.revision,"scope":p.scope,"actor":p.actor,"evidence_count":p.evidence_refs.len(),"verdict":"unknown_without_evidence","external_effect":false}),
    )
}
