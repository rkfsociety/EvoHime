//! Versioned metadata contract for the local-model compatibility gateway.
//!
//! This record declares a scoped gateway capability; validating or projecting
//! it does not inspect a model, start a server, or route a request.
//!
//! ```
//! use evohime_core::local_model_compatibility_gateway::{canonical_hash, projection, validate, Lifecycle, LocalModelCompatibilityGatewayRecord, SCHEMA_VERSION};
//! let mut gateway = LocalModelCompatibilityGatewayRecord {
//!     schema_version: SCHEMA_VERSION, id: "local-gateway-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "host-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! gateway.content_hash = canonical_hash(&gateway)?;
//! validate(&gateway)?;
//! assert_eq!(projection(&gateway)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized local-model gateway contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of the local-model compatibility gateway capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but unavailable for request routing.
    Draft,
    /// Valid and available within the declared host or workspace scope.
    Active,
    /// Replaced by a newer gateway record revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for a local-model compatibility gateway.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalModelCompatibilityGatewayRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable gateway capability identifier.
    pub id: String,
    /// Positive revision used to order gateway metadata updates.
    pub revision: u64,
    /// Whether the capability may be selected by a request router.
    pub lifecycle: Lifecycle,
    /// Opaque host or workspace scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for local-model compatibility gateway metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or canonical digest validation failed.
    #[error("invalid local_model_compatibility_gateway: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &LocalModelCompatibilityGatewayRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema version, gateway identity bounds, revision, lifecycle, and digest.
pub fn validate(v: &LocalModelCompatibilityGatewayRecord) -> Result<(), Error> {
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
/// Produces gateway metadata without starting a server or routing a request.
pub fn projection(v: &LocalModelCompatibilityGatewayRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"local_model_compatibility_gateway","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
