//! Internal bounded adapter/v1 contract.
//!
//! This module is deliberately Rust-only. It is not a second IPC transport or
//! a provider catalog; descriptors are derived from Core-owned routing data.

use evohime_model_gateway::provider_contract::{CandidateHealthSnapshot, CapabilityMetadata};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ADAPTER_CONTRACT_VERSION: u16 = 1;
pub const MAX_ADAPTER_ID_BYTES: usize = 128;
pub const MAX_ADAPTER_PAYLOAD_BYTES: usize = 256 * 1024;
pub const MAX_DIAGNOSTIC_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterLimits {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub timeout_ms: u64,
}

impl AdapterLimits {
    pub fn validate(&self) -> Result<(), AdapterError> {
        if self.max_input_bytes == 0
            || self.max_input_bytes > MAX_ADAPTER_PAYLOAD_BYTES
            || self.max_output_bytes == 0
            || self.max_output_bytes > MAX_ADAPTER_PAYLOAD_BYTES
            || self.timeout_ms == 0
        {
            return Err(AdapterError::InvalidLimits);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterDescriptor {
    pub adapter_id: String,
    pub contract_version: u16,
    pub capabilities: CapabilityMetadata,
    pub health: CandidateHealthSnapshot,
    pub limits: AdapterLimits,
    pub capability_epoch: u64,
}

impl AdapterDescriptor {
    pub fn validate(&self) -> Result<(), AdapterError> {
        if self.adapter_id.is_empty() || self.adapter_id.len() > MAX_ADAPTER_ID_BYTES {
            return Err(AdapterError::InvalidDescriptor);
        }
        if self.contract_version != ADAPTER_CONTRACT_VERSION {
            return Err(AdapterError::UnsupportedVersion);
        }
        self.limits.validate()
    }

    pub fn builtin_tool() -> Self {
        Self {
            adapter_id: "core.tool-runtime".into(),
            contract_version: ADAPTER_CONTRACT_VERSION,
            capabilities: CapabilityMetadata {
                schema_version: "capability-metadata-v1".into(),
                provider_version: env!("CARGO_PKG_VERSION").into(),
                capability_epoch: 1,
                tool_calling: true,
                structured_output: true,
                context_limit: None,
                streaming: false,
                vision: false,
                execution_class: evohime_model_gateway::provider_contract::ExecutionClass::Local,
                privacy_boundary:
                    evohime_model_gateway::provider_contract::PrivacyClass::Restricted,
            },
            health: CandidateHealthSnapshot::ready(30_000),
            limits: AdapterLimits {
                max_input_bytes: MAX_ADAPTER_PAYLOAD_BYTES,
                max_output_bytes: MAX_ADAPTER_PAYLOAD_BYTES,
                timeout_ms: 30_000,
            },
            capability_epoch: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterSession {
    pub negotiated_capabilities: Vec<String>,
    pub policy_hash: String,
    pub target_generation: u64,
    pub workspace_scope: String,
    pub deadline_ms: u64,
    pub cancellation_requested: bool,
    pub secret_ref: Option<SecretRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    /// Opaque Core-issued handle; it is never the secret value.
    pub handle: String,
    pub purpose: String,
}

impl AdapterSession {
    pub fn validate(&self, descriptor: &AdapterDescriptor) -> Result<(), AdapterError> {
        descriptor.validate()?;
        if self.policy_hash.is_empty()
            || self.workspace_scope.is_empty()
            || self.deadline_ms == 0
            || self.secret_ref.as_ref().is_some_and(|reference| {
                reference.handle.is_empty() || reference.purpose.is_empty()
            })
            || self
                .negotiated_capabilities
                .iter()
                .any(|cap| !descriptor.capabilities_for(cap))
        {
            return Err(AdapterError::CapabilityMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRequest {
    pub correlation_id: String,
    pub input: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterResult {
    pub correlation_id: String,
    pub status: AdapterStatus,
    pub output: Vec<u8>,
    pub diagnostic: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterStatus {
    Success,
    Unavailable,
    Unsupported,
    Timeout,
    Cancelled,
    StaleSession,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AdapterError {
    #[error("adapter descriptor is invalid")]
    InvalidDescriptor,
    #[error("adapter contract version is unsupported")]
    UnsupportedVersion,
    #[error("adapter limits are invalid")]
    InvalidLimits,
    #[error("adapter capability or session scope mismatch")]
    CapabilityMismatch,
    #[error("adapter input or output exceeds bounded payload")]
    PayloadTooLarge,
    #[error("adapter diagnostic exceeds bounded size")]
    DiagnosticTooLarge,
}

impl AdapterDescriptor {
    fn capabilities_for(&self, name: &str) -> bool {
        match name {
            "tool_calling" => self.capabilities.tool_calling,
            "structured_output" => self.capabilities.structured_output,
            "streaming" => self.capabilities.streaming,
            "vision" => self.capabilities.vision,
            _ => false,
        }
    }
}

pub fn validate_request(request: &AdapterRequest) -> Result<(), AdapterError> {
    if request.correlation_id.is_empty() || request.input.len() > MAX_ADAPTER_PAYLOAD_BYTES {
        return Err(AdapterError::PayloadTooLarge);
    }
    Ok(())
}

pub fn validate_result(result: &AdapterResult) -> Result<(), AdapterError> {
    if result.output.len() > MAX_ADAPTER_PAYLOAD_BYTES {
        return Err(AdapterError::PayloadTooLarge);
    }
    if result.diagnostic.len() > MAX_DIAGNOSTIC_BYTES {
        return Err(AdapterError::DiagnosticTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_model_gateway::provider_contract::{ExecutionClass, PrivacyClass};

    fn descriptor() -> AdapterDescriptor {
        AdapterDescriptor {
            adapter_id: "builtin.test".into(),
            contract_version: ADAPTER_CONTRACT_VERSION,
            capabilities: CapabilityMetadata {
                schema_version: "capability-metadata-v1".into(),
                provider_version: "test".into(),
                capability_epoch: 1,
                tool_calling: true,
                structured_output: false,
                context_limit: Some(4096),
                streaming: true,
                vision: false,
                execution_class: ExecutionClass::Local,
                privacy_boundary: PrivacyClass::Restricted,
            },
            health: CandidateHealthSnapshot::ready_at(1000, 1),
            limits: AdapterLimits {
                max_input_bytes: 1024,
                max_output_bytes: 2048,
                timeout_ms: 1000,
            },
            capability_epoch: 1,
        }
    }

    #[test]
    fn bounded_descriptor_and_session_validate() {
        let descriptor = descriptor();
        let session = AdapterSession {
            negotiated_capabilities: vec!["tool_calling".into()],
            policy_hash: "policy".into(),
            target_generation: 1,
            workspace_scope: "scope".into(),
            deadline_ms: 100,
            cancellation_requested: false,
            secret_ref: None,
        };
        assert!(session.validate(&descriptor).is_ok());
        assert!(validate_request(&AdapterRequest {
            correlation_id: "id".into(),
            input: vec![0; MAX_ADAPTER_PAYLOAD_BYTES + 1]
        })
        .is_err());
    }
}
