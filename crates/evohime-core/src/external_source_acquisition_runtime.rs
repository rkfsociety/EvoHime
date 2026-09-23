//! Versioned metadata contract for external-source acquisition runtime.
//!
//! This contract describes capability identity and scope only; its projection
//! does not fetch, download, or import a source.
//!
//! ```
//! use evohime_core::external_source_acquisition_runtime::{canonical_hash, projection, validate, ExternalSourceAcquisitionRuntimeRecord, Lifecycle, SCHEMA_VERSION};
//! let mut acquisition = ExternalSourceAcquisitionRuntimeRecord {
//!     schema_version: SCHEMA_VERSION, id: "source-acquisition-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! acquisition.content_hash = canonical_hash(&acquisition)?;
//! validate(&acquisition)?;
//! assert_eq!(projection(&acquisition)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized acquisition runtime contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state for external-source acquisition capability metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but unavailable for source acquisition.
    Draft,
    /// Valid and available within the declared scope.
    Active,
    /// Replaced by a newer capability revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for an external-source acquisition runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalSourceAcquisitionRuntimeRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable runtime capability identifier.
    pub id: String,
    /// Positive revision used to order capability updates.
    pub revision: u64,
    /// Whether the runtime capability may be selected.
    pub lifecycle: Lifecycle,
    /// Opaque workspace or project scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for external-source acquisition metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity bounds, lifecycle, or canonical digest check failed.
    #[error("invalid external_source_acquisition_runtime: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &ExternalSourceAcquisitionRuntimeRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema version, identity, bounds, lifecycle, and canonical digest.
pub fn validate(v: &ExternalSourceAcquisitionRuntimeRecord) -> Result<(), Error> {
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
/// Produces validated capability metadata without acquiring any external source.
pub fn projection(v: &ExternalSourceAcquisitionRuntimeRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"external_source_acquisition_runtime","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
