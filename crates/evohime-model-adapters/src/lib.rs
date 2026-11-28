#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `model-adapters` service module for tool-call parsing and model output normalization.

/// Legacy text-embedded tool-call parsers and visible-text projection.
pub mod legacy_parser;

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
                let calls = parse_embedded_tool_calls(raw_text);
                Ok(serde_json::json!({
                    "chars": raw_text.chars().count(),
                    "tool_calls": calls
                }))
            }
            "adapter.normalize_edit" => Ok(serde_json::json!({ "status": "normalized" })),
            other => Err(ServiceContractError::not_found(format!(
                "unknown model-adapters capability: {other}"
            ))),
        }
    }
}

/// Extracts `<tool_call>{...}</tool_call>` JSON objects from raw model output
/// and normalizes dotted tool names (`fs.read` -> `fs_read`).
pub fn parse_embedded_tool_calls(raw_text: &str) -> Vec<serde_json::Value> {
    const OPEN_TAG: &str = "<tool_call>";
    const CLOSE_TAG: &str = "</tool_call>";
    let mut results = Vec::new();
    let mut rest = raw_text;
    while let Some(start) = rest.find(OPEN_TAG) {
        let after_open = &rest[start + OPEN_TAG.len()..];
        let Some(end) = after_open.find(CLOSE_TAG) else {
            break;
        };
        let block = after_open[..end].trim();
        if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(block) {
            if let Some(name) = parsed.get("name").and_then(|v| v.as_str()) {
                let normalized = name.replace('.', "_");
                if let Some(obj) = parsed.as_object_mut() {
                    obj.insert("name".to_owned(), serde_json::Value::String(normalized));
                }
                results.push(parsed);
            }
        }
        rest = &after_open[end + CLOSE_TAG.len()..];
    }
    results
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
                payload: serde_json::json!({
                    "raw_text": "thinking... <tool_call>{\"name\":\"fs.read\",\"arguments\":{\"path\":\"Cargo.toml\"}}</tool_call>"
                }),
                caller_session_id: None,
            },
        );
        match resp {
            ServiceResponse::InvokeResult { payload, .. } => {
                let calls = payload["tool_calls"].as_array().expect("tool_calls array");
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0]["name"], "fs_read");
                assert_eq!(calls[0]["arguments"]["path"], "Cargo.toml");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
