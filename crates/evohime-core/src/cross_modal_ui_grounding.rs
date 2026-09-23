//! Versioned metadata contract for cross-modal UI grounding.
//!
//! The record identifies a scoped grounding capability. The metadata projection
//! does not observe, read, or change the UI.
//!
//! ```
//! use evohime_core::cross_modal_ui_grounding::{canonical_hash, projection, validate, CrossModalUiGroundingRecord, Lifecycle, SCHEMA_VERSION};
//! let mut grounding = CrossModalUiGroundingRecord {
//!     schema_version: SCHEMA_VERSION, id: "grounding-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "window-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! grounding.content_hash = canonical_hash(&grounding)?;
//! validate(&grounding)?;
//! assert_eq!(projection(&grounding)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized cross-modal UI grounding contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state for a cross-modal UI grounding capability record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available to consumers.
    Draft,
    /// Valid and available within its declared UI scope.
    Active,
    /// Replaced by a newer record revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for a cross-modal UI grounding capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrossModalUiGroundingRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable grounding record identifier.
    pub id: String,
    /// Positive revision used to order capability updates.
    pub revision: u64,
    /// Whether the grounding capability may be selected.
    pub lifecycle: Lifecycle,
    /// Opaque UI, workspace, or session scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for cross-modal UI grounding metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or digest validation failed.
    #[error("invalid cross_modal_ui_grounding: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding the `content_hash` field.
pub fn canonical_hash(v: &CrossModalUiGroundingRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema, identifier bounds, revision, lifecycle, and digest.
pub fn validate(v: &CrossModalUiGroundingRecord) -> Result<(), Error> {
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
/// Produces a metadata-only projection without observing or changing the UI.
pub fn projection(v: &CrossModalUiGroundingRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"cross_modal_ui_grounding","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
