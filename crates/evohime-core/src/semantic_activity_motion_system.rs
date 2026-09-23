//! Versioned metadata contract for the semantic activity motion system.
//!
//! This record declares capability identity and scope. It does not synthesize,
//! schedule, or play motion output.
//!
//! ```
//! use evohime_core::semantic_activity_motion_system::{canonical_hash, projection, validate, Lifecycle, SCHEMA_VERSION, SemanticActivityMotionSystemRecord};
//! let mut activity = SemanticActivityMotionSystemRecord {
//!     schema_version: SCHEMA_VERSION, id: "motion-system-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! activity.content_hash = canonical_hash(&activity)?;
//! validate(&activity)?;
//! assert_eq!(projection(&activity)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized semantic activity motion contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state for a semantic activity motion capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available for motion consumers.
    Draft,
    /// Valid and available within the declared scope.
    Active,
    /// Replaced by a newer capability revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity, lifecycle, and scope for the motion-system capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticActivityMotionSystemRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable motion-system record identifier.
    pub id: String,
    /// Positive revision used to order capability updates.
    pub revision: u64,
    /// Whether the motion-system capability may be selected.
    pub lifecycle: Lifecycle,
    /// Opaque workspace or user scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for semantic activity motion metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or digest validation failed.
    #[error("invalid semantic_activity_motion_system: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding the `content_hash` field.
pub fn canonical_hash(v: &SemanticActivityMotionSystemRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema, identity bounds, revision, lifecycle, and digest.
pub fn validate(v: &SemanticActivityMotionSystemRecord) -> Result<(), Error> {
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
/// Produces validated metadata without creating or playing motion output.
pub fn projection(v: &SemanticActivityMotionSystemRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"semantic_activity_motion_system","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
