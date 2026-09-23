//! Versioned metadata contract for temporal signal intelligence.
//!
//! The stored projection identifies an active signal capability and its
//! scope; it does not infer, schedule, or execute signals.
//!
//! ```
//! use evohime_core::temporal_signal_intelligence::{canonical_hash, projection, validate, Lifecycle, TemporalSignalIntelligenceRecord, SCHEMA_VERSION};
//! let mut record = TemporalSignalIntelligenceRecord {
//!     schema_version: SCHEMA_VERSION, id: "signal-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! record.content_hash = canonical_hash(&record)?;
//! validate(&record)?;
//! assert_eq!(projection(&record)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized temporal-signal contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state for a temporal signal intelligence record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available to consumers.
    Draft,
    /// Valid and available in the declared scope.
    Active,
    /// Superseded by a newer record revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned metadata identifying a temporal-signal capability and its scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemporalSignalIntelligenceRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable temporal-signal record identifier.
    pub id: String,
    /// Positive revision used to order updates to the same record.
    pub revision: u64,
    /// Whether this capability metadata can be selected.
    pub lifecycle: Lifecycle,
    /// Opaque workspace or project scope identifier.
    pub scope: String,
    /// Digest of the canonical record with this field cleared.
    pub content_hash: String,
}

/// Validation failure for temporal signal intelligence metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The record failed schema, identity, bounds, lifecycle, or digest checks.
    #[error("invalid temporal_signal_intelligence: {0}")]
    Invalid(String),
}

/// Computes the SHA-256 digest of canonical record data, excluding its digest field.
pub fn canonical_hash(v: &TemporalSignalIntelligenceRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates the contract version, identity bounds, lifecycle, and content hash.
pub fn validate(v: &TemporalSignalIntelligenceRecord) -> Result<(), Error> {
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
/// Produces a validated metadata-only projection without evaluating any signal.
pub fn projection(v: &TemporalSignalIntelligenceRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"temporal_signal_intelligence","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
