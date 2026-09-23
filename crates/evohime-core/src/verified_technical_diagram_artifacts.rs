//! Versioned metadata contract for verified technical diagram artifacts.
//!
//! The projection records capability identity and scope only; it does not
//! render a diagram or verify artifact contents.
//!
//! ```
//! use evohime_core::verified_technical_diagram_artifacts::{canonical_hash, projection, validate, Lifecycle, VerifiedTechnicalDiagramArtifactsRecord, SCHEMA_VERSION};
//! let mut artifacts = VerifiedTechnicalDiagramArtifactsRecord {
//!     schema_version: SCHEMA_VERSION, id: "diagram-artifacts-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "project-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! artifacts.content_hash = canonical_hash(&artifacts)?;
//! validate(&artifacts)?;
//! assert_eq!(projection(&artifacts)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized technical diagram artifact contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state for verified technical diagram artifact metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available to artifact consumers.
    Draft,
    /// Valid and available within its declared scope.
    Active,
    /// Replaced by a newer artifact capability revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for a technical diagram artifact capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedTechnicalDiagramArtifactsRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable artifact capability identifier.
    pub id: String,
    /// Positive revision used to order capability metadata updates.
    pub revision: u64,
    /// Whether the capability may be selected by a separate renderer.
    pub lifecycle: Lifecycle,
    /// Opaque project or workspace scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for technical diagram artifact metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or canonical digest validation failed.
    #[error("invalid verified_technical_diagram_artifacts: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &VerifiedTechnicalDiagramArtifactsRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema, identifier bounds, revision, lifecycle, and digest.
pub fn validate(v: &VerifiedTechnicalDiagramArtifactsRecord) -> Result<(), Error> {
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
/// Produces validated capability metadata without rendering or verifying a diagram.
pub fn projection(v: &VerifiedTechnicalDiagramArtifactsRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"verified_technical_diagram_artifacts","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
