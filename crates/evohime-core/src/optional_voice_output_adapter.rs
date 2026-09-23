//! Versioned metadata contract for the optional voice-output adapter.
//!
//! This record identifies a scoped capability. It does not synthesize speech
//! or start an audio device.
//!
//! ```
//! use evohime_core::optional_voice_output_adapter::{canonical_hash, projection, validate, Lifecycle, OptionalVoiceOutputAdapterRecord, SCHEMA_VERSION};
//! let mut adapter = OptionalVoiceOutputAdapterRecord {
//!     schema_version: SCHEMA_VERSION, id: "voice-output-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "user-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! adapter.content_hash = canonical_hash(&adapter)?;
//! validate(&adapter)?;
//! assert_eq!(projection(&adapter)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized voice-output adapter contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of the optional voice-output adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but unavailable for audio output.
    Draft,
    /// Valid and available within its declared scope.
    Active,
    /// Replaced by a newer adapter record revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope of an optional voice-output adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptionalVoiceOutputAdapterRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable adapter record identifier.
    pub id: String,
    /// Positive revision used to order adapter metadata updates.
    pub revision: u64,
    /// Whether the adapter can be selected by a separate runtime.
    pub lifecycle: Lifecycle,
    /// Opaque user, workspace, or device scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for optional voice-output adapter metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or digest validation failed.
    #[error("invalid optional_voice_output_adapter: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &OptionalVoiceOutputAdapterRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema, adapter identity bounds, revision, lifecycle, and digest.
pub fn validate(v: &OptionalVoiceOutputAdapterRecord) -> Result<(), Error> {
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
/// Returns adapter metadata without synthesizing speech or opening audio devices.
pub fn projection(v: &OptionalVoiceOutputAdapterRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"optional_voice_output_adapter","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
