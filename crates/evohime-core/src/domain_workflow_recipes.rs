//! Versioned metadata contract for domain workflow recipes.
//!
//! The projection is metadata-only and does not perform external effects.
//!
//! ```
//! use evohime_core::domain_workflow_recipes::{canonical_hash, projection, validate, DomainWorkflowRecipesRecord, Lifecycle, SCHEMA_VERSION};
//! let mut record = DomainWorkflowRecipesRecord {
//!     schema_version: SCHEMA_VERSION,
//!     id: "recipe-1".into(),
//!     revision: 1,
//!     lifecycle: Lifecycle::Active,
//!     scope: "workspace".into(),
//!     content_hash: String::new(),
//! };
//! record.content_hash = canonical_hash(&record)?;
//! validate(&record)?;
//! assert_eq!(projection(&record)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Publication lifecycle of a domain workflow recipe record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Created but not available for selection.
    Draft,
    /// Valid and available for selection.
    Active,
    /// Replaced by a later record revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned metadata for a domain workflow recipe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainWorkflowRecipesRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable record identifier.
    pub id: String,
    /// Positive monotonically increasing revision.
    pub revision: u64,
    /// Current lifecycle state.
    pub lifecycle: Lifecycle,
    /// Scope in which the recipe is valid.
    pub scope: String,
    /// SHA-256 digest of the canonical record with this field cleared.
    pub content_hash: String,
}

/// Validation failure for a domain workflow recipe record.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The record violated schema, bounds, lifecycle, serialization, or hash constraints.
    #[error("invalid domain_workflow_recipes: {0}")]
    Invalid(String),
}

/// Computes the canonical SHA-256 digest while excluding `content_hash` itself.
pub fn canonical_hash(v: &DomainWorkflowRecipesRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates the record's version, identity, bounds, lifecycle, and content hash.
pub fn validate(v: &DomainWorkflowRecipesRecord) -> Result<(), Error> {
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
/// Produces a validated metadata-only JSON projection with no external effects.
pub fn projection(v: &DomainWorkflowRecipesRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"domain_workflow_recipes","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
