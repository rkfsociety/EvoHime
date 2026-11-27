#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `workflow-engine` service module implementing `evohime-service-contract`.

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for workflows, schedules, triggers, and durable background jobs.
#[derive(Debug, Default)]
pub struct WorkflowEngineService;

impl WorkflowEngineService {
    /// Creates a new [`WorkflowEngineService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for WorkflowEngineService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "workflow-engine".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "workflow.list".to_owned(),
                    description: "Lists registered workflows and automation schedules.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["workflow.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "workflow.run".to_owned(),
                    description: "Starts a workflow execution by identifier.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["workflow_id"],
                        "properties": { "workflow_id": { "type": "string" } }
                    }),
                    required_permissions: vec!["workflow.execute".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "workflow-engine".to_owned(),
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
            "workflow.list" => {
                let dir = payload
                    .get("dir")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(".");
                let mut workflows = Vec::new();
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten().take(100) {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("json") {
                            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                workflows.push(stem.to_owned());
                            }
                        }
                    }
                }
                workflows.sort();
                Ok(serde_json::json!({ "workflows": workflows }))
            }
            "workflow.run" => {
                let workflow_id = payload
                    .get("workflow_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input(
                            "workflow_id must be a non-empty string",
                        )
                    })?;
                Ok(serde_json::json!({ "workflow_id": workflow_id, "status": "accepted" }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown workflow-engine capability: {other}"
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
        let svc = WorkflowEngineService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "w1".to_owned(),
                capability: "workflow.list".to_owned(),
                payload: serde_json::json!({}),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
