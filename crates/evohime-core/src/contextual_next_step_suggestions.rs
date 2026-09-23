//! Versioned metadata contract for contextual next-step suggestions.
//!
//! The record identifies a scoped suggestion capability. It contains no
//! suggestion text and does not initiate a task or external action.
//!
//! ```
//! use evohime_core::contextual_next_step_suggestions::{canonical_hash, projection, validate, ContextualNextStepSuggestionsRecord, Lifecycle, SCHEMA_VERSION};
//! let mut capability = ContextualNextStepSuggestionsRecord {
//!     schema_version: SCHEMA_VERSION, id: "next-steps-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! capability.content_hash = canonical_hash(&capability)?;
//! validate(&capability)?;
//! assert_eq!(projection(&capability)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized suggestion capability version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of contextual next-step suggestion capability metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available to suggestion consumers.
    Draft,
    /// Valid and available within the declared scope.
    Active,
    /// Replaced by a newer capability revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for contextual next-step suggestions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextualNextStepSuggestionsRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable suggestion capability identifier.
    pub id: String,
    /// Positive revision used to order capability updates.
    pub revision: u64,
    /// Whether this capability may be selected.
    pub lifecycle: Lifecycle,
    /// Opaque workspace, task, or user scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for contextual next-step suggestion metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or digest validation failed.
    #[error("invalid contextual_next_step_suggestions: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &ContextualNextStepSuggestionsRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema version, identifier bounds, revision, lifecycle, and digest.
pub fn validate(v: &ContextualNextStepSuggestionsRecord) -> Result<(), Error> {
    if v.schema_version != SCHEMA_VERSION
        || v.id.trim().is_empty()
        || v.id.len() > 256
        || v.scope.len() > 256
        || v.revision == 0
        || matches!(v.lifecycle, Lifecycle::Invalid)
    {
        return Err(Error::Invalid("bounds_or_lifecycle".into()));
    }
    if canonical_hash(v)? != v.content_hash {
        return Err(Error::Invalid("content_hash_mismatch".into()));
    }
    Ok(())
}
/// Produces validated capability metadata without generating or executing a suggestion.
pub fn projection(v: &ContextualNextStepSuggestionsRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"contextual_next_step_suggestions","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
