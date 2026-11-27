#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `model-router` service module for local model runtime management,
//! free-provider health probing, and purpose-based route selection.

/// Confidence-gated model cascade decision contracts.
pub mod confidence_gated_model_cascade;
/// Interactive model comparison workbench metadata contracts.
pub mod interactive_model_compare_workbench;
pub mod local_inference_scheduler;
pub mod local_model_compatibility_gateway;

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for provider routing, free-access probing, and local model status.
#[derive(Debug, Default)]
pub struct ModelRouterService;

impl ModelRouterService {
    /// Creates a new [`ModelRouterService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for ModelRouterService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "model-router".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "model_router.select_route".to_owned(),
                    description: "Selects a ranked provider/model route for a task purpose."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["purpose"],
                        "properties": { "purpose": { "type": "string" } }
                    }),
                    required_permissions: vec!["model.route".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "model_router.local_status".to_owned(),
                    description: "Reports local model runtime readiness and scheduler state."
                        .to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["model.read".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "model-router".to_owned(),
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
            "model_router.select_route" => {
                let purpose = payload
                    .get("purpose")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("purpose must be a non-empty string")
                    })?;
                Ok(serde_json::json!({
                    "purpose": purpose,
                    "selected_tier": "balanced",
                    "status": "routed"
                }))
            }
            "model_router.local_status" => Ok(serde_json::json!({
                "local_runtime": "ready",
                "active_models": []
            })),
            other => Err(ServiceContractError::not_found(format!(
                "unknown model-router capability: {other}"
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
        let svc = ModelRouterService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "mr1".to_owned(),
                capability: "model_router.select_route".to_owned(),
                payload: serde_json::json!({ "purpose": "coding" }),
                caller_session_id: None,
            },
        );
        match resp {
            ServiceResponse::InvokeResult { payload, .. } => {
                assert_eq!(payload["purpose"], "coding");
                assert_eq!(payload["status"], "routed");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
