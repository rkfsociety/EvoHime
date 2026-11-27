#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `computer-use` service module implementing `evohime-service-contract`.

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for native GUI automation, mobile device automation, and IDE bridge.
#[derive(Debug, Default)]
pub struct ComputerUseService;

impl ComputerUseService {
    /// Creates a new [`ComputerUseService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for ComputerUseService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "computer-use".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "computer_use.screen_info".to_owned(),
                    description: "Reads active display geometry for UI grounding.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["computer_use.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "computer_use.ide_bridge".to_owned(),
                    description: "Sends a bounded command to an attached IDE companion.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["action"],
                        "properties": { "action": { "type": "string" } }
                    }),
                    required_permissions: vec!["computer_use.execute".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "computer-use".to_owned(),
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
            "computer_use.screen_info" => Ok(serde_json::json!({ "displays": [] })),
            "computer_use.ide_bridge" => {
                let action = payload
                    .get("action")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("action must be a non-empty string")
                    })?;
                Ok(serde_json::json!({ "action": action, "status": "accepted" }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown computer-use capability: {other}"
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
        let svc = ComputerUseService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "c1".to_owned(),
                capability: "computer_use.screen_info".to_owned(),
                payload: serde_json::json!({}),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
