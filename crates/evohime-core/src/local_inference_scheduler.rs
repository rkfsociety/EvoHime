//! Typed admission boundary for local inference scheduling.
//!
//! The checkout has no versioned inference-stream adapter. Consequently this
//! module models durable intent and fail-closed admission, but never starts a
//! worker or reports measured execution.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SCHEDULER_ID_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerStatus {
    Draft,
    Active,
    Superseded,
    Invalid,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerContract {
    pub schema_version: u32,
    pub scheduler_id: String,
    pub revision: u64,
    pub priority: u8,
    pub status: SchedulerStatus,
    pub contract_hash: String,
}

impl SchedulerContract {
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

    pub const fn admission_status(&self, inference_adapter_available: bool) -> SchedulerStatus {
        if !inference_adapter_available {
            SchedulerStatus::Unavailable
        } else {
            self.status
        }
    }
}
