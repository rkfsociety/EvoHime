//! Typed admission boundary for local inference scheduling.
//!
//! The checkout has no versioned inference-stream adapter. Consequently this
//! module models durable intent and fail-closed admission, but never starts a
//! worker or reports measured execution.

use serde::{Deserialize, Serialize};

/// Current serialized scheduler contract version.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum UTF-8 byte length of a scheduler identifier.
pub const MAX_SCHEDULER_ID_BYTES: usize = 128;

/// Lifecycle or admission state of a local inference scheduler contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerStatus {
    /// Contract has been created but is not enabled.
    Draft,
    /// Contract is enabled for admission when an adapter is available.
    Active,
    /// Contract was replaced by a newer revision.
    Superseded,
    /// Contract failed validation.
    Invalid,
    /// No compatible inference adapter is currently available.
    Unavailable,
}

/// Versioned scheduler intent; this contract does not represent measured execution.
///
/// ```
/// use evohime_core::local_inference_scheduler::{SchedulerContract, SchedulerStatus, SCHEMA_VERSION};
/// let contract = SchedulerContract {
///     schema_version: SCHEMA_VERSION,
///     scheduler_id: "local-default".into(),
///     revision: 1,
///     priority: 10,
///     status: SchedulerStatus::Active,
///     contract_hash: "sha256:contract".into(),
/// };
/// assert!(contract.validate().is_ok());
/// assert_eq!(contract.admission_status(false), SchedulerStatus::Unavailable);
/// assert_eq!(contract.admission_status(true), SchedulerStatus::Active);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerContract {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable scheduler identifier, bounded by [`MAX_SCHEDULER_ID_BYTES`].
    pub scheduler_id: String,
    /// Positive revision used to order scheduler contract updates.
    pub revision: u64,
    /// Relative admission priority, with the meaning defined by the scheduler.
    pub priority: u8,
    /// Desired lifecycle state.
    pub status: SchedulerStatus,
    /// Digest of the canonical scheduler contract.
    pub contract_hash: String,
}

impl SchedulerContract {
    /// Validates the schema version, identifier, revision, and contract hash.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("unsupported_schema_version");
        }
        if self.scheduler_id.is_empty() || self.scheduler_id.len() > MAX_SCHEDULER_ID_BYTES {
            return Err("invalid_scheduler_id");
        }
        if self.revision == 0 || self.contract_hash.is_empty() {
            return Err("invalid_revision_or_hash");
        }
        Ok(())
    }

    /// Fails closed when the inference adapter is unavailable.
    pub const fn admission_status(&self, inference_adapter_available: bool) -> SchedulerStatus {
        if !inference_adapter_available {
            SchedulerStatus::Unavailable
        } else {
            self.status
        }
    }
}
