//! Core-owned bounded contract for the optional vision/document worker.
//! Backend absence is explicit and fail-closed; visual output is never a tool
//! or host-action authority.

use serde::{Deserialize, Serialize};

pub const VISION_SCHEMA_VERSION: u32 = 1;
pub const MAX_INPUT_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_PAGES: u32 = 50;
pub const MAX_FRAMES: u32 = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisionInputV1 {
    pub schema_version: u32,
    pub request_id: String,
    pub correlation_id: String,
    pub artifact_id: String,
    pub kind: String,
    pub mime: String,
    pub byte_size: u64,
    pub page_count: u32,
    pub frame_count: u32,
    pub capability_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisionStatus {
    UnsupportedSource,
    ResourceLimit,
    Unknown,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisionOutputV1 {
    pub schema_version: u32,
    pub request_id: String,
    pub status: VisionStatus,
    pub diagnostic_code: String,
    pub evidence_refs: Vec<String>,
    pub redaction_applied: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VisionContractError {
    #[error("vision input is malformed")]
    Malformed,
    #[error("vision input exceeds a bounded limit")]
    ResourceLimit,
}

impl VisionInputV1 {
    pub fn validate(&self) -> Result<(), VisionContractError> {
        let fields = [
            &self.request_id,
            &self.correlation_id,
            &self.artifact_id,
            &self.kind,
            &self.mime,
            &self.capability_snapshot_hash,
        ];
        if self.schema_version != VISION_SCHEMA_VERSION
            || fields
                .iter()
                .any(|field| field.is_empty() || field.len() > 128)
            || self.byte_size == 0
        {
            return Err(VisionContractError::Malformed);
        }
        if self.byte_size as usize > MAX_INPUT_BYTES
            || self.page_count > MAX_PAGES
            || self.frame_count > MAX_FRAMES
        {
            return Err(VisionContractError::ResourceLimit);
        }
        Ok(())
    }

    pub fn unsupported_output(&self) -> VisionOutputV1 {
        VisionOutputV1 {
            schema_version: VISION_SCHEMA_VERSION,
            request_id: self.request_id.clone(),
            status: VisionStatus::UnsupportedSource,
            diagnostic_code: "backend_unavailable".into(),
            evidence_refs: Vec::new(),
            redaction_applied: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> VisionInputV1 {
        VisionInputV1 {
            schema_version: 1,
            request_id: "request-1".into(),
            correlation_id: "correlation-1".into(),
            artifact_id: "hash-1".into(),
            kind: "image".into(),
            mime: "image/png".into(),
            byte_size: 12,
            page_count: 1,
            frame_count: 0,
            capability_snapshot_hash: "caps-1".into(),
        }
    }

    #[test]
    fn bounded_input_has_fail_closed_output_without_backend() {
        let value = input();
        assert!(value.validate().is_ok());
        assert_eq!(
            value.unsupported_output().diagnostic_code,
            "backend_unavailable"
        );
    }

    #[test]
    fn oversized_input_is_rejected_before_worker() {
        let mut value = input();
        value.byte_size = (MAX_INPUT_BYTES + 1) as u64;
        assert_eq!(value.validate(), Err(VisionContractError::ResourceLimit));
    }
}
