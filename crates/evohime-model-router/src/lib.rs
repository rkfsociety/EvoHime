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
#[allow(dead_code)]
pub(crate) mod output_guardrail_pipeline;
#[allow(dead_code)]
pub(crate) mod reasoning_operator_library;

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
                ServiceCapabilityDescriptor {
                    name: "model_router.operator_builtins".to_owned(),
                    description: "Lists built-in reasoning operator definitions.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["model.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "model_router.operator_validate".to_owned(),
                    description: "Validates a reasoning operator definition.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["model.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "model_router.operator_validate_request".to_owned(),
                    description: "Validates a reasoning operator execution request.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["operator_id", "request"],
                        "properties": {
                            "operator_id": { "type": "string" },
                            "request": { "type": "object" }
                        }
                    }),
                    required_permissions: vec!["model.route".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "model_router.guardrail_evaluate".to_owned(),
                    description: "Validates and evaluates an output guardrail pipeline.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["pipeline_id", "version", "pipeline", "raw_bytes_len"],
                        "properties": {
                            "pipeline_id": { "type": "string" },
                            "version": { "type": "integer" },
                            "pipeline": { "type": "object" }
                        }
                    }),
                    required_permissions: vec!["model.route".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "model_router.probe_accelerator".to_owned(),
                    description: "Probes host GPU/accelerator memory via platform DXGI.".to_owned(),
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
            "model_router.operator_builtins" => {
                let defs = reasoning_operator_library::builtins();
                serde_json::to_value(defs)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "model_router.operator_validate" => {
                let d: reasoning_operator_library::ReasoningOperatorDefinition =
                    serde_json::from_value(payload).map_err(|_| {
                        ServiceContractError::invalid_input("invalid_operator_definition")
                    })?;
                reasoning_operator_library::validate(&d)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(d)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "model_router.operator_validate_request" => {
                #[derive(serde::Deserialize)]
                struct ExecReq {
                    operator_id: String,
                    request: reasoning_operator_library::OperatorRequest,
                }
                let req: ExecReq = serde_json::from_value(payload)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_operator_request"))?;
                reasoning_operator_library::validate_request(&req.request)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                if req.request.operator_id != req.operator_id {
                    return Err(ServiceContractError::invalid_input("operator_id_mismatch"));
                }
                Ok(serde_json::json!({ "status": "validated" }))
            }
            "model_router.guardrail_evaluate" => {
                #[derive(serde::Deserialize)]
                struct GuardrailReq {
                    pipeline_id: String,
                    version: u64,
                    pipeline: output_guardrail_pipeline::GuardrailPipeline,
                }
                let req: GuardrailReq = serde_json::from_value(payload).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_guardrail_pipeline")
                })?;
                let bytes = serde_json::to_vec(&req.pipeline)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))?;
                let result = output_guardrail_pipeline::evaluate(&req.pipeline, &bytes)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                Ok(serde_json::json!({
                    "schema_version": 1,
                    "pipeline_id": req.pipeline_id,
                    "version": req.version,
                    "result": result,
                    "redacted": true,
                }))
            }
            "model_router.probe_accelerator" => Ok(serde_json::json!({
                "accelerator_bytes": discover_accelerator_bytes(),
            })),
            other => Err(ServiceContractError::not_found(format!(
                "unknown model-router capability: {other}"
            ))),
        }
    }
}

#[cfg(windows)]
fn discover_accelerator_bytes() -> Option<u64> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };

    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut index = 0u32;
        let mut largest = 0u64;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            if let Ok(description) = adapter.GetDesc1() {
                if description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 {
                    largest = largest.max(description.DedicatedVideoMemory as u64);
                    if description.DedicatedVideoMemory == 0 {
                        largest = largest.max((description.SharedSystemMemory / 2) as u64);
                    }
                }
            }
            index = index.saturating_add(1);
            if index >= 32 {
                break;
            }
        }
        (largest > 0).then_some(largest)
    }
}

#[cfg(not(windows))]
fn discover_accelerator_bytes() -> Option<u64> {
    None
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

    #[test]
    fn probe_accelerator_capability_returns_valid_json() {
        let svc = ModelRouterService::new();
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "mr-probe".to_owned(),
                capability: "model_router.probe_accelerator".to_owned(),
                payload: serde_json::json!({}),
                caller_session_id: None,
            },
        );
        match resp {
            ServiceResponse::InvokeResult { payload, .. } => {
                assert!(payload.get("accelerator_bytes").is_some());
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
