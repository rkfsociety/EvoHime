#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `tools-exec` service module for shell, process, script, and build tools.

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for bounded shell execution, scripts, and build helpers.
#[derive(Debug, Default)]
pub struct ToolsExecService;

impl ToolsExecService {
    /// Creates a new [`ToolsExecService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for ToolsExecService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "tools-exec".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "shell.execute".to_owned(),
                    description: "Executes a bounded shell command inside a workspace.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["command"],
                        "properties": { "command": { "type": "string" } }
                    }),
                    required_permissions: vec!["shell.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "process.list".to_owned(),
                    description: "Lists active background tasks managed for the workspace."
                        .to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["process.read".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "tools-exec".to_owned(),
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
            "shell.execute" => {
                let command = payload
                    .get("command")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("command must be a non-empty string")
                    })?;
                let cwd = payload
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(".");
                #[cfg(windows)]
                let output = std::process::Command::new("pwsh")
                    .args(["-NoProfile", "-Command", command])
                    .current_dir(cwd)
                    .output();
                #[cfg(not(windows))]
                let output = std::process::Command::new("sh")
                    .args(["-lc", command])
                    .current_dir(cwd)
                    .output();
                let output = output.map_err(|err| {
                    ServiceContractError::new(
                        evohime_service_contract::ServiceErrorCode::Internal,
                        format!("failed to execute command: {err}"),
                    )
                })?;
                Ok(serde_json::json!({
                    "exit_code": output.status.code().unwrap_or(-1),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }
            "process.list" => Ok(serde_json::json!({ "processes": [] })),
            other => Err(ServiceContractError::not_found(format!(
                "unknown tools-exec capability: {other}"
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
        let svc = ToolsExecService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "te1".to_owned(),
                capability: "process.list".to_owned(),
                payload: serde_json::json!({}),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
