#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `model-adapters` service module for tool-call parsing and model output normalization.

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for provider-specific tool-call parsing and response normalization.
#[derive(Debug, Default)]
pub struct ModelAdaptersService;

impl ModelAdaptersService {
    /// Creates a new [`ModelAdaptersService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for ModelAdaptersService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "model-adapters".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "adapter.parse_tool_calls".to_owned(),
                    description: "Extracts structured tool calls from raw model output.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["raw_text"],
                        "properties": { "raw_text": { "type": "string" } }
                    }),
                    required_permissions: vec!["model.normalize".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "adapter.normalize_edit".to_owned(),
                    description: "Normalizes model edit protocol payloads.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["payload"],
                        "properties": { "payload": { "type": "object" } }
                    }),
                    required_permissions: vec!["model.normalize".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "model-adapters".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            status: ServiceHealthStatus::Ready,
            active_requests: 0,
            detail: None,
        }
    }

    fn invoke(
        &self,
        capability: &str,
        payload: serde_json::Value,
        _caller_session_id: Option<&str>,
    ) -> Result<serde_json::Value, ServiceContractError> {
        match capability {
            "adapter.parse_tool_calls" => {
                let raw_text = payload
                    .get("raw_text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("raw_text must be a string")
                    })?;
                Ok(serde_json::json!({
                    "chars": raw_text.chars().count(),
                    "tool_calls": []
                }))
            }
            "adapter.normalize_edit" => Ok(serde_json::json!({ "status": "normalized" })),
            other => Err(ServiceContractError::not_found(format!(
                "unknown model-adapters capability: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_service_contract::{
        dispatch_request, validate_descriptor, ServiceRequest, ServiceResponse,
    };

    #[test]
    fn descriptor_and_dispatch_work() {
        let svc = ModelAdaptersService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "ma1".to_owned(),
                capability: "adapter.parse_tool_calls".to_owned(),
                payload: serde_json::json!({ "raw_text": "ok" }),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
