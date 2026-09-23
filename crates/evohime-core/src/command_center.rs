//! Versioned metadata contract for the command-center capability.
//!
//! The record describes availability and scope. Its projection is informational
//! only and has no external effect.
//!
//! ```
//! use evohime_core::command_center::{canonical_hash, projection, validate, CommandCenterRecord, Lifecycle, SCHEMA_VERSION};
//! let mut record = CommandCenterRecord {
//!     schema_version: SCHEMA_VERSION,
//!     id: "command-center-1".into(),
//!     revision: 1,
//!     lifecycle: Lifecycle::Active,
//!     scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! record.content_hash = canonical_hash(&record)?;
//! validate(&record)?;
//! assert_eq!(projection(&record)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized command-center contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Lifecycle state of a command-center record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Created but not available for use.
    Draft,
    /// Valid and available within its declared scope.
    Active,
    /// Replaced by a newer record revision.
    Superseded,
    /// Invalid and unavailable for use.
    Invalid,
}

/// Versioned command-center capability metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandCenterRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable record identifier.
    pub id: String,
    /// Positive monotonically increasing revision.
    pub revision: u64,
    /// Current lifecycle state.
    pub lifecycle: Lifecycle,
    /// Opaque scope identifier for the command-center record.
    pub scope: String,
    /// SHA-256 digest of the canonical record with this field cleared.
    pub content_hash: String,
}

/// Validation error for command-center capability metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The record violated schema, bounds, lifecycle, encoding, or digest rules.
    #[error("invalid command_center: {0}")]
    Invalid(String),
}

/// Computes the canonical SHA-256 digest while excluding `content_hash`.
pub fn canonical_hash(v: &CommandCenterRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Checks the schema version, record bounds, lifecycle, and canonical digest.
pub fn validate(v: &CommandCenterRecord) -> Result<(), Error> {
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
/// Returns validated metadata without performing a command-center action.
pub fn projection(v: &CommandCenterRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"command_center","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
