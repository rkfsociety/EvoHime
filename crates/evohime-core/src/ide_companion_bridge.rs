//! Versioned metadata contract for the IDE companion bridge capability.
//!
//! The projection identifies a scoped bridge declaration only; it does not
//! establish a connection or exchange IDE data.
//!
//! ```
//! use evohime_core::ide_companion_bridge::{canonical_hash, projection, validate, IdeCompanionBridgeRecord, Lifecycle, SCHEMA_VERSION};
//! let mut bridge = IdeCompanionBridgeRecord {
//!     schema_version: SCHEMA_VERSION, id: "ide-bridge-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! bridge.content_hash = canonical_hash(&bridge)?;
//! validate(&bridge)?;
//! assert_eq!(projection(&bridge)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized IDE companion bridge contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of an IDE companion bridge capability record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but unavailable for companion connections.
    Draft,
    /// Valid and available within the declared IDE or workspace scope.
    Active,
    /// Replaced by a newer bridge record revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for an IDE companion bridge capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdeCompanionBridgeRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable bridge record identifier.
    pub id: String,
    /// Positive revision used to order bridge capability updates.
    pub revision: u64,
    /// Whether the capability may be selected by an independent runtime.
    pub lifecycle: Lifecycle,
    /// Opaque IDE, workspace, or session scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for IDE companion bridge metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or digest validation failed.
    #[error("invalid ide_companion_bridge: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &IdeCompanionBridgeRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema, bridge identity bounds, revision, lifecycle, and digest.
pub fn validate(v: &IdeCompanionBridgeRecord) -> Result<(), Error> {
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
/// Returns metadata without connecting to an IDE or exchanging data.
pub fn projection(v: &IdeCompanionBridgeRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"ide_companion_bridge","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
