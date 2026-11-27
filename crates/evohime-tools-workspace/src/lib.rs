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
                    name: "fs.list".to_owned(),
                    description: "Lists files and subdirectories inside a workspace directory."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": { "path": { "type": "string" } }
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "fs.search".to_owned(),
                    description: "Searches for a text query across workspace files.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["query"],
                        "properties": {
                            "path": { "type": "string" },
                            "query": { "type": "string" }
                        }
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "git.status".to_owned(),
                    description: "Reads Git working tree status for a repository.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["git.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "git.diff".to_owned(),
                    description: "Reads Git working tree diff summary for a repository.".to_owned(),
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
                let target = payload
                    .get("target")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("target must be a non-empty string")
                    })?;
                let replacement = payload
                    .get("replacement")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("replacement must be a string")
                    })?;
                let content = std::fs::read_to_string(path).map_err(|err| {
                    ServiceContractError::invalid_input(format!("cannot read {path}: {err}"))
                })?;
                if !content.contains(target) {
                    return Err(ServiceContractError::invalid_input(
                        "target text not found in file",
                    ));
                }
                let updated = content.replacen(target, replacement, 1);
                std::fs::write(path, &updated).map_err(|err| {
                    ServiceContractError::invalid_input(format!("cannot write {path}: {err}"))
                })?;
                Ok(serde_json::json!({
                    "path": path,
                    "replaced": true
                }))
            }
            "fs.list" => {
                let path = payload
                    .get("path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(".");
                let entries = std::fs::read_dir(path).map_err(|err| {
                    ServiceContractError::invalid_input(format!("cannot read dir {path}: {err}"))
                })?;
                let mut items = Vec::new();
                for entry in entries.flatten().take(200) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    items.push(serde_json::json!({ "name": name, "is_dir": is_dir }));
                }
                Ok(serde_json::json!({ "path": path, "entries": items }))
            }
            "fs.search" => {
                let query = payload
                    .get("query")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("query must be a non-empty string")
                    })?;
                let root = payload
                    .get("path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(".");
                let mut matches = Vec::new();
                search_dir_bounded(std::path::Path::new(root), query, &mut matches, 50);
                Ok(serde_json::json!({ "query": query, "matches": matches }))
            }
            "git.status" => {
                let cwd = payload
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(".");
                let output = std::process::Command::new("git")
                    .args(["-C", cwd, "status", "--short"])
                    .output()
                    .map_err(|err| {
                        ServiceContractError::new(
                            evohime_service_contract::ServiceErrorCode::Internal,
                            format!("failed to run git status: {err}"),
                        )
                    })?;
                Ok(serde_json::json!({
                    "exit_code": output.status.code().unwrap_or(-1),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }
            "git.diff" => {
                let cwd = payload
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(".");
                let output = std::process::Command::new("git")
                    .args(["-C", cwd, "diff", "--stat"])
                    .output()
                    .map_err(|err| {
                        ServiceContractError::new(
                            evohime_service_contract::ServiceErrorCode::Internal,
                            format!("failed to run git diff: {err}"),
                        )
                    })?;
                Ok(serde_json::json!({
                    "exit_code": output.status.code().unwrap_or(-1),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr),
                }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown tools-workspace capability: {other}"
            ))),
        }
    }
}

fn search_dir_bounded(
    dir: &std::path::Path,
    query: &str,
    out: &mut Vec<serde_json::Value>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if matches!(
            name_str.as_ref(),
            ".git" | "target" | "node_modules" | ".evohime"
        ) {
            continue;
        }
        if path.is_dir() {
            search_dir_bounded(&path, query, out, limit);
        } else if path.is_file() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.len() > 256 * 1024 {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (idx, line) in text.lines().enumerate() {
                if line.contains(query) {
                    out.push(serde_json::json!({
                        "path": path.to_string_lossy(),
                        "line": idx + 1,
                        "preview": line.chars().take(160).collect::<String>(),
                    }));
                    if out.len() >= limit {
                        break;
                    }
                }
            }
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
