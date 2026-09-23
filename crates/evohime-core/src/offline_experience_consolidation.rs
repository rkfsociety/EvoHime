use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current schema version for offline consolidation cycles.
pub const SCHEMA_VERSION: u32 = 1;
/// Lifecycle state of an offline consolidation cycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Cycle is being prepared.
    Draft,
    /// Cycle is eligible for use.
    Active,
    /// Cycle has been replaced by a newer revision.
    Superseded,
    /// Cycle failed validation or was explicitly invalidated.
    Invalid,
}
/// Bounded, content-addressed metadata for one offline consolidation cycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsolidationCycle {
    /// Schema version of this cycle.
    pub schema_version: u32,
    /// Stable cycle identifier.
    pub id: String,
    /// Monotonically increasing cycle revision.
    pub revision: u64,
    /// Lifecycle state.
    pub lifecycle: Lifecycle,
    /// Scope to which this cycle applies.
    pub scope: String,
    /// Identifiers of source records considered by consolidation.
    pub input_refs: Vec<String>,
    /// Hash identifying the resulting output.
    pub output_hash: String,
    /// Hash of the canonical cycle metadata.
    pub content_hash: String,
}
/// Validation errors for offline consolidation metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConsolidationError {
    /// The cycle failed schema, bounds, or content-hash validation.
    #[error("invalid offline consolidation cycle: {0}")]
    Invalid(String),
}
/// Computes the canonical SHA-256 hash with `content_hash` cleared.
pub fn canonical_hash(c: &ConsolidationCycle) -> Result<String, ConsolidationError> {
    let mut n = c.clone();
    n.content_hash.clear();
    let b = serde_json::to_vec(&n)
        .map_err(|_| ConsolidationError::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates the schema version, field bounds, references, and canonical hash.
pub fn validate(c: &ConsolidationCycle) -> Result<(), ConsolidationError> {
    if c.schema_version != SCHEMA_VERSION {
        return Err(ConsolidationError::Invalid(
            "unsupported_schema_version".into(),
        ));
    }
    if c.id.trim().is_empty()
        || c.id.len() > 256
        || c.scope.trim().is_empty()
        || c.scope.len() > 256
        || c.revision == 0
        || c.input_refs.is_empty()
        || c.input_refs.len() > 128
        || c.output_hash.len() > 128
    {
        return Err(ConsolidationError::Invalid("bounds".into()));
    }
    if c.input_refs
        .iter()
        .any(|x| x.trim().is_empty() || x.len() > 256)
    {
        return Err(ConsolidationError::Invalid("input_ref_bounds".into()));
    }
    if canonical_hash(c)? != c.content_hash {
        return Err(ConsolidationError::Invalid("content_hash_mismatch".into()));
    }
    Ok(())
}
/// Returns a deterministic metadata-only evaluation after validating the cycle.
pub fn evaluate(c: &ConsolidationCycle) -> Result<serde_json::Value, ConsolidationError> {
    validate(c)?;
    Ok(
        serde_json::json!({"status":"offline_metadata_only","cycle_id":c.id,"revision":c.revision,"input_count":c.input_refs.len(),"external_effect":false,"output_hash_prefix":&c.output_hash[..c.output_hash.len().min(8)]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unhashed_cycle() {
        let c = ConsolidationCycle {
            schema_version: 1,
            id: "c".into(),
            revision: 1,
            lifecycle: Lifecycle::Active,
            scope: "offline".into(),
            input_refs: vec!["ref".into()],
            output_hash: "hash".into(),
            content_hash: "bad".into(),
        };
        assert!(validate(&c).is_err())
    }
}
