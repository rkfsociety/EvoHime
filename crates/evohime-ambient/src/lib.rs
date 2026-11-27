#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `ambient` service module implementing `evohime-service-contract`.

pub mod autonomous_metric_experiment_runtime;
pub mod temporal_signal_intelligence;

use std::sync::atomic::{AtomicU64, Ordering};

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for background proactivity, temporal signals, and autonomous experiments.
#[derive(Debug, Default)]
pub struct AmbientService {
    experiments_started: AtomicU64,
}

impl AmbientService {
    /// Creates a new [`AmbientService`] instance.
    pub fn new() -> Self {
        Self {
            experiments_started: AtomicU64::new(0),
        }
    }
}

impl ServiceHandler for AmbientService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "ambient".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "ambient.status".to_owned(),
                    description: "Returns current background proactivity state.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["ambient.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "ambient.run_experiment".to_owned(),
                    description: "Starts a bounded autonomous metric experiment.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["experiment_id"],
                        "properties": { "experiment_id": { "type": "string" } }
                    }),
                    required_permissions: vec!["ambient.execute".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "ambient".to_owned(),
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
            "ambient.status" => Ok(serde_json::json!({
                "proactivity": "idle",
                "experiments_started": self.experiments_started.load(Ordering::Relaxed)
            })),
            "ambient.run_experiment" => {
                let experiment_id = payload
                    .get("experiment_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input(
                            "experiment_id must be a non-empty string",
                        )
                    })?;
                let count = self.experiments_started.fetch_add(1, Ordering::Relaxed) + 1;
                Ok(serde_json::json!({
                    "experiment_id": experiment_id,
                    "run_number": count,
                    "status": "accepted"
                }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown ambient capability: {other}"
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
        let svc = AmbientService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "a1".to_owned(),
                capability: "ambient.status".to_owned(),
                payload: serde_json::json!({}),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
