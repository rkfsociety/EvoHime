//! Versioned metadata contract for the native computer-use capability.
//!
//! The record describes a runtime capability and its scope only. A valid
//! projection does not grant permission or perform a computer-use action.
//!
//! ```
//! use evohime_core::native_computer_use_runtime::{canonical_hash, projection, validate, Lifecycle, NativeComputerUseRuntimeRecord, SCHEMA_VERSION};
//! let mut runtime = NativeComputerUseRuntimeRecord {
//!     schema_version: SCHEMA_VERSION,
//!     id: "computer-use-1".into(),
//!     revision: 1,
//!     lifecycle: Lifecycle::Active,
//!     scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! runtime.content_hash = canonical_hash(&runtime)?;
//! validate(&runtime)?;
//! assert_eq!(projection(&runtime)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized native computer-use contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Lifecycle state of a native computer-use capability record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Created but not enabled for admission.
    Draft,
    /// Valid and available for a separate authorized runtime to consider.
    Active,
    /// Replaced by a newer record revision.
    Superseded,
    /// Invalid and unavailable for use.
    Invalid,
}

/// Versioned metadata for a native computer-use runtime capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeComputerUseRuntimeRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable capability record identifier.
    pub id: String,
    /// Positive, monotonically increasing record revision.
    pub revision: u64,
    /// Current capability lifecycle state.
    pub lifecycle: Lifecycle,
    /// Opaque workspace or device scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical record metadata.
    pub content_hash: String,
}

/// Validation error for native computer-use capability metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or digest validation failed.
    #[error("invalid native_computer_use_runtime: {0}")]
    Invalid(String),
}

/// Computes the canonical digest with the `content_hash` field cleared.
pub fn canonical_hash(v: &NativeComputerUseRuntimeRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates the record version, identity bounds, lifecycle, and digest.
pub fn validate(v: &NativeComputerUseRuntimeRecord) -> Result<(), Error> {
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
/// Produces validated metadata without performing a computer-use action.
pub fn projection(v: &NativeComputerUseRuntimeRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"native_computer_use_runtime","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
