//! Versioned metadata contract for temporal memory facts.
//!
//! This record validates and identifies fact metadata; it does not extract,
//! rank, or mutate a memory store.
//!
//! ```
//! use evohime_core::temporal_memory_facts::{canonical_hash, projection, validate, Lifecycle, TemporalMemoryFactsRecord, SCHEMA_VERSION};
//! let mut facts = TemporalMemoryFactsRecord {
//!     schema_version: SCHEMA_VERSION, id: "facts-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "workspace-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! facts.content_hash = canonical_hash(&facts)?;
//! validate(&facts)?;
//! assert_eq!(projection(&facts)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized temporal-memory-facts contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of a temporal memory facts record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available to consumers.
    Draft,
    /// Valid and available within its declared scope.
    Active,
    /// Replaced by a newer record revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity, lifecycle, and scope for a temporal memory facts set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemporalMemoryFactsRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable identifier of the fact set.
    pub id: String,
    /// Positive revision used to order fact-set updates.
    pub revision: u64,
    /// Current availability state.
    pub lifecycle: Lifecycle,
    /// Opaque scope identifier governing which workspace may use the facts.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for a temporal memory facts record.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or canonical digest check failed.
    #[error("invalid temporal_memory_facts: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding the `content_hash` field.
pub fn canonical_hash(v: &TemporalMemoryFactsRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates the schema, fact-set identity, revision, lifecycle, and digest.
pub fn validate(v: &TemporalMemoryFactsRecord) -> Result<(), Error> {
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
/// Produces a validated metadata-only projection without changing stored facts.
pub fn projection(v: &TemporalMemoryFactsRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"temporal_memory_facts","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
