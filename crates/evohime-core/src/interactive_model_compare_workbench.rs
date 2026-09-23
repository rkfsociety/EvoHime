//! Versioned metadata contract for an interactive model comparison workbench.
//!
//! The projection identifies a scoped workbench capability; it does not call
//! models or compare their outputs.
//!
//! ```
//! use evohime_core::interactive_model_compare_workbench::{canonical_hash, projection, validate, InteractiveModelCompareWorkbenchRecord, Lifecycle, SCHEMA_VERSION};
//! let mut workbench = InteractiveModelCompareWorkbenchRecord {
//!     schema_version: SCHEMA_VERSION, id: "compare-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! workbench.content_hash = canonical_hash(&workbench)?;
//! validate(&workbench)?;
//! assert_eq!(projection(&workbench)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized comparison workbench contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of the interactive comparison workbench capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but unavailable for comparison sessions.
    Draft,
    /// Valid and available within the declared scope.
    Active,
    /// Replaced by a newer capability revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for the interactive model comparison workbench.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractiveModelCompareWorkbenchRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable workbench capability identifier.
    pub id: String,
    /// Positive revision used to order workbench metadata updates.
    pub revision: u64,
    /// Whether this capability may be selected.
    pub lifecycle: Lifecycle,
    /// Opaque user or workspace scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for comparison workbench metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or digest validation failed.
    #[error("invalid interactive_model_compare_workbench: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &InteractiveModelCompareWorkbenchRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema version, workbench identity bounds, revision, lifecycle, and digest.
pub fn validate(v: &InteractiveModelCompareWorkbenchRecord) -> Result<(), Error> {
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
/// Produces validated metadata without starting a comparison session or calling a model.
pub fn projection(v: &InteractiveModelCompareWorkbenchRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"interactive_model_compare_workbench","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
