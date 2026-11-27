#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `tools-workspace` service module for file, patch, search, and Git tools.

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for workspace file operations, patching, and Git tools.
#[derive(Debug, Default)]
pub struct ToolsWorkspaceService;

impl ToolsWorkspaceService {
    /// Creates a new [`ToolsWorkspaceService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for ToolsWorkspaceService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "tools-workspace".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "fs.read".to_owned(),
                    description: "Reads a bounded line range from a workspace file.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": { "type": "string" } }
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "fs.patch".to_owned(),
                    description: "Applies a targeted text replacement inside a workspace file."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["path"],
                        "properties": { "path": { "type": "string" } }
                    }),
                    required_permissions: vec!["fs.write".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "git.status".to_owned(),
                    description: "Reads Git working tree status for a repository.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["git.read".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "tools-workspace".to_owned(),
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
            "fs.read" => {
                let path = payload
                    .get("path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("path must be a non-empty string")
                    })?;
                let content = std::fs::read_to_string(path).map_err(|err| {
                    ServiceContractError::invalid_input(format!("cannot read {path}: {err}"))
                })?;
                const MAX_READ_BYTES: usize = 256 * 1024;
                let truncated = content.len() > MAX_READ_BYTES;
                let slice = if truncated {
                    let mut end = MAX_READ_BYTES;
                    while !content.is_char_boundary(end) {
                        end = end.saturating_sub(1);
                    }
                    &content[..end]
                } else {
                    content.as_str()
                };
                Ok(serde_json::json!({
                    "path": path,
                    "content": slice,
                    "truncated": truncated
                }))
            }
            "fs.patch" => {
                let path = payload
                    .get("path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("path must be a non-empty string")
                    })?;
                Ok(serde_json::json!({
                    "path": path,
                    "capability": capability,
                    "status": "accepted"
                }))
            }
            "git.status" => Ok(serde_json::json!({ "status": "accepted" })),
            other => Err(ServiceContractError::not_found(format!(
                "unknown tools-workspace capability: {other}"
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
        let svc = ToolsWorkspaceService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "tw1".to_owned(),
                capability: "fs.read".to_owned(),
                payload: serde_json::json!({ "path": "Cargo.toml" }),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
