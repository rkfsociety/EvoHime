//! Versioned metadata contract for mobile-device automation runtime.
//!
//! The record describes a scoped capability only; the metadata API does not
//! connect to, inspect, or control a mobile device.
//!
//! ```
//! use evohime_core::mobile_device_automation_runtime::{canonical_hash, projection, validate, Lifecycle, MobileDeviceAutomationRuntimeRecord, SCHEMA_VERSION};
//! let mut runtime = MobileDeviceAutomationRuntimeRecord {
//!     schema_version: SCHEMA_VERSION, id: "mobile-automation-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "device-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! runtime.content_hash = canonical_hash(&runtime)?;
//! validate(&runtime)?;
//! assert_eq!(projection(&runtime)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized mobile-device automation contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of a mobile-device automation capability record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but unavailable for device automation.
    Draft,
    /// Valid and available within the declared device or workspace scope.
    Active,
    /// Replaced by a newer runtime capability revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity and scope for mobile-device automation runtime capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MobileDeviceAutomationRuntimeRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable runtime capability identifier.
    pub id: String,
    /// Positive revision used to order runtime metadata updates.
    pub revision: u64,
    /// Whether the capability may be selected by an independent runtime.
    pub lifecycle: Lifecycle,
    /// Opaque device, workspace, or account scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for mobile-device automation runtime metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or canonical digest validation failed.
    #[error("invalid mobile_device_automation_runtime: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding `content_hash`.
pub fn canonical_hash(v: &MobileDeviceAutomationRuntimeRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema version, runtime identity bounds, revision, lifecycle, and digest.
pub fn validate(v: &MobileDeviceAutomationRuntimeRecord) -> Result<(), Error> {
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
/// Produces validated metadata without connecting to or controlling a device.
pub fn projection(v: &MobileDeviceAutomationRuntimeRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"mobile_device_automation_runtime","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
