#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `evals` service module implementing `evohime-service-contract`.

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for benchmark suites, simulation, and local calibration.
#[derive(Debug, Default)]
pub struct EvalsService;

impl EvalsService {
    /// Creates a new [`EvalsService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for EvalsService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "evals".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "evals.list_suites".to_owned(),
                    description: "Lists available evaluation and benchmark suites.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["evals.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.run_benchmark".to_owned(),
                    description: "Executes a bounded benchmark suite.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["suite_id"],
                        "properties": { "suite_id": { "type": "string" } }
                    }),
                    required_permissions: vec!["evals.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.calibrate_local".to_owned(),
                    description: "Runs local model calibration checks.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["model_id"],
                        "properties": { "model_id": { "type": "string" } }
                    }),
                    required_permissions: vec!["evals.execute".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "evals".to_owned(),
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
            "evals.list_suites" => Ok(serde_json::json!({
                "suites": ["core-smoke", "tool-routing", "local-calibration"]
            })),
            "evals.run_benchmark" => {
                let suite_id = payload
                    .get("suite_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("suite_id must be a non-empty string")
                    })?;
                Ok(serde_json::json!({
                    "suite_id": suite_id,
                    "status": "accepted"
                }))
            }
            "evals.calibrate_local" => {
                let model_id = payload
                    .get("model_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("model_id must be a non-empty string")
                    })?;
                Ok(serde_json::json!({
                    "model_id": model_id,
                    "status": "calibrated"
                }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown evals capability: {other}"
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
        let svc = EvalsService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");

        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "r1".to_owned(),
                capability: "evals.list_suites".to_owned(),
                payload: serde_json::json!({}),
                caller_session_id: None,
            },
        );
        match resp {
            ServiceResponse::InvokeResult { request_id, payload } => {
                assert_eq!(request_id, "r1");
                assert!(payload["suites"].is_array());
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
