//! Versioned metadata contract for verified Git checkpoints.
//!
//! This contract records checkpoint identity and scope; it does not run Git
//! commands or attest to repository state by itself.
//!
//! ```
//! use evohime_core::verified_git_checkpoints::{canonical_hash, projection, validate, Lifecycle, VerifiedGitCheckpointsRecord, SCHEMA_VERSION};
//! let mut checkpoint = VerifiedGitCheckpointsRecord {
//!     schema_version: SCHEMA_VERSION,
//!     id: "checkpoint-1".into(),
//!     revision: 1,
//!     lifecycle: Lifecycle::Active,
//!     scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! checkpoint.content_hash = canonical_hash(&checkpoint)?;
//! validate(&checkpoint)?;
//! assert_eq!(projection(&checkpoint)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized checkpoint contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Lifecycle of a verified Git checkpoint record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Recorded but not available for checkpoint selection.
    Draft,
    /// Valid and available as a checkpoint reference.
    Active,
    /// Replaced by a newer checkpoint revision.
    Superseded,
    /// Invalid and unavailable for use.
    Invalid,
}

/// Versioned metadata identifying a verified Git checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedGitCheckpointsRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable checkpoint record identifier.
    pub id: String,
    /// Positive monotonically increasing record revision.
    pub revision: u64,
    /// Current lifecycle state.
    pub lifecycle: Lifecycle,
    /// Opaque workspace or repository scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical checkpoint metadata.
    pub content_hash: String,
}

/// Validation error for verified Git checkpoint metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identifier, revision, lifecycle, or content hash is invalid.
    #[error("invalid verified_git_checkpoints: {0}")]
    Invalid(String),
}

/// Computes the record digest with the `content_hash` field cleared.
pub fn canonical_hash(v: &VerifiedGitCheckpointsRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates identifier bounds, revision, lifecycle, and canonical digest.
pub fn validate(v: &VerifiedGitCheckpointsRecord) -> Result<(), Error> {
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
/// Returns a validated metadata-only projection; it performs no Git operation.
pub fn projection(v: &VerifiedGitCheckpointsRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"verified_git_checkpoints","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
