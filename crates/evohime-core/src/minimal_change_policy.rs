//! Versioned metadata contract for the minimal-change policy capability.
//!
//! This record identifies policy metadata and its scope. It does not evaluate
//! a proposed diff or enforce a change limit.
//!
//! ```
//! use evohime_core::minimal_change_policy::{canonical_hash, projection, validate, Lifecycle, MinimalChangePolicyRecord, SCHEMA_VERSION};
//! let mut policy = MinimalChangePolicyRecord {
//!     schema_version: SCHEMA_VERSION, id: "minimal-change-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "repository-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! policy.content_hash = canonical_hash(&policy)?;
//! validate(&policy)?;
//! assert_eq!(projection(&policy)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized minimal-change policy version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of minimal-change policy metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available for use.
    Draft,
    /// Valid and available within its declared scope.
    Active,
    /// Replaced by a newer policy record revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for a minimal-change policy capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MinimalChangePolicyRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable policy record identifier.
    pub id: String,
    /// Positive revision used to order policy metadata updates.
    pub revision: u64,
    /// Whether the policy capability may be selected.
    pub lifecycle: Lifecycle,
    /// Opaque repository or workspace scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for minimal-change policy metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or digest validation failed.
    #[error("invalid minimal_change_policy: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &MinimalChangePolicyRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema version, policy identity bounds, revision, lifecycle, and digest.
pub fn validate(v: &MinimalChangePolicyRecord) -> Result<(), Error> {
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
/// Produces validated metadata without applying policy to a proposed change.
pub fn projection(v: &MinimalChangePolicyRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"minimal_change_policy","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
