//! Versioned metadata contract for autonomous metric experiment runtime state.
//!
//! The contract records a scoped capability declaration only; it does not
//! launch an experiment or publish measured results.
//!
//! ```
//! use evohime_core::autonomous_metric_experiment_runtime::{canonical_hash, projection, validate, AutonomousMetricExperimentRuntimeRecord, Lifecycle, SCHEMA_VERSION};
//! let mut runtime = AutonomousMetricExperimentRuntimeRecord {
//!     schema_version: SCHEMA_VERSION, id: "metric-runtime-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "project-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! runtime.content_hash = canonical_hash(&runtime)?;
//! validate(&runtime)?;
//! assert_eq!(projection(&runtime)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized metric experiment runtime contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state of an autonomous metric experiment runtime record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available for experiment admission.
    Draft,
    /// Valid and available within its declared scope.
    Active,
    /// Replaced by a newer runtime capability revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned identity, lifecycle, and scope for metric experiment runtime capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutonomousMetricExperimentRuntimeRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable runtime capability identifier.
    pub id: String,
    /// Positive revision used to order capability updates.
    pub revision: u64,
    /// Current availability state.
    pub lifecycle: Lifecycle,
    /// Opaque project or workspace scope identifier.
    pub scope: String,
    /// SHA-256 digest of canonical metadata with this field cleared.
    pub content_hash: String,
}

/// Validation failure for metric experiment runtime metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Schema, identity, bounds, lifecycle, or canonical digest check failed.
    #[error("invalid autonomous_metric_experiment_runtime: {0}")]
    Invalid(String),
}

/// Computes the canonical digest while excluding the `content_hash` field.
pub fn canonical_hash(v: &AutonomousMetricExperimentRuntimeRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates schema, identity bounds, revision, lifecycle, and canonical digest.
pub fn validate(v: &AutonomousMetricExperimentRuntimeRecord) -> Result<(), Error> {
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
/// Produces a validated metadata-only projection without running an experiment.
pub fn projection(v: &AutonomousMetricExperimentRuntimeRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"autonomous_metric_experiment_runtime","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
