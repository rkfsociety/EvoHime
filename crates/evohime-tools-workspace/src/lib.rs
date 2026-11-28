#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `tools-workspace` service module for file, patch, search, and Git tools.

#[allow(dead_code)]
pub(crate) mod architecture_snapshot;
#[allow(dead_code)]
pub(crate) mod architecture_snapshot_runtime;
#[allow(dead_code)]
pub(crate) mod code_diagnostics_feedback_loop;
#[allow(dead_code)]
pub(crate) mod static_analysis_packs;
/// Verified Git checkpoint metadata contracts.
pub mod verified_git_checkpoints;

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
                ServiceCapabilityDescriptor {
                    name: "tools.diagnostics_validate_provider".to_owned(),
                    description: "Validates a code diagnostics provider definition.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "tools.diagnostics_validate_snapshot".to_owned(),
                    description: "Validates a code diagnostics snapshot and counts active errors."
                        .to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "tools.diagnostics_delta".to_owned(),
                    description:
                        "Computes the diagnostics delta between a baseline and current snapshot."
                            .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["baseline", "current"],
                        "properties": {
                            "baseline": { "type": "object" },
                            "current": { "type": "object" }
                        }
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "tools.analysis_pack_validate".to_owned(),
                    description: "Validates a static analysis pack definition.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "tools.analysis_pack_evaluate".to_owned(),
                    description:
                        "Evaluates static analysis findings and coverage against rollout policy."
                            .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["pack_id", "expected_revision", "request"],
                        "properties": {
                            "pack_id": { "type": "string" },
                            "expected_revision": { "type": "integer" },
                            "request": { "type": "object" }
                        }
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "tools.architecture_extract".to_owned(),
                    description: "Extracts and hashes a bounded architecture snapshot for an allowed workspace root.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["root", "revision", "snapshot_id", "allowed_roots"]
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "tools.architecture_compare".to_owned(),
                    description: "Computes a deterministic delta between two architecture snapshots.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["before", "after"]
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "tools.architecture_review".to_owned(),
                    description: "Reviews an architecture delta against expected changes.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["expected", "actual"]
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "tools.architecture_fingerprint".to_owned(),
                    description: "Computes the canonical workspace source fingerprint.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["root", "revision"]
                    }),
                    required_permissions: vec!["fs.read".to_owned()],
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
            "tools.diagnostics_validate_provider" => {
                let provider: code_diagnostics_feedback_loop::Provider =
                    serde_json::from_value(payload)
                        .map_err(|_| ServiceContractError::invalid_input("invalid_provider"))?;
                code_diagnostics_feedback_loop::validate_provider(&provider)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(&provider)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "tools.diagnostics_validate_snapshot" => {
                let snapshot: code_diagnostics_feedback_loop::Snapshot =
                    serde_json::from_value(payload)
                        .map_err(|_| ServiceContractError::invalid_input("invalid_snapshot"))?;
                code_diagnostics_feedback_loop::validate_snapshot(&snapshot)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                let error_count = snapshot
                    .diagnostics
                    .iter()
                    .filter(|d| !d.stale && d.severity == "error")
                    .count();
                Ok(serde_json::json!({
                    "snapshot": snapshot,
                    "error_count": error_count,
                }))
            }
            "tools.diagnostics_delta" => {
                #[derive(serde::Deserialize)]
                struct DeltaReq {
                    baseline: code_diagnostics_feedback_loop::Snapshot,
                    current: code_diagnostics_feedback_loop::Snapshot,
                }
                let req: DeltaReq = serde_json::from_value(payload)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_snapshot"))?;
                let delta = code_diagnostics_feedback_loop::delta(&req.baseline, &req.current)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(delta)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "tools.analysis_pack_validate" => {
                let pack: static_analysis_packs::AnalysisPack = serde_json::from_value(payload)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_analysis_pack"))?;
                static_analysis_packs::validate_pack(&pack)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                Ok(serde_json::json!({
                    "pack": pack,
                    "rule_count": pack.rules.len(),
                }))
            }
            "tools.analysis_pack_evaluate" => {
                #[derive(serde::Deserialize)]
                struct EvalReq {
                    pack_id: String,
                    expected_revision: u64,
                    request: serde_json::Value,
                }
                let req: EvalReq = serde_json::from_value(payload)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_analysis_request"))?;
                let coverage: static_analysis_packs::AnalysisCoverage = serde_json::from_value(
                    req.request
                        .get("coverage")
                        .cloned()
                        .ok_or_else(|| ServiceContractError::invalid_input("coverage_required"))?,
                )
                .map_err(|_| ServiceContractError::invalid_input("invalid_coverage"))?;
                static_analysis_packs::validate_coverage(&coverage)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                let findings: Vec<static_analysis_packs::AnalysisFinding> = serde_json::from_value(
                    req.request
                        .get("findings")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([])),
                )
                .map_err(|_| ServiceContractError::invalid_input("invalid_findings"))?;
                for finding in &findings {
                    static_analysis_packs::validate_finding(finding)
                        .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                }
                let mode: static_analysis_packs::RolloutMode = serde_json::from_value(
                    req.request
                        .get("mode")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!("audit")),
                )
                .map_err(|_| ServiceContractError::invalid_input("invalid_mode"))?;
                let outcome = static_analysis_packs::safe_outcome(mode, coverage.state, &findings);
                Ok(serde_json::json!({
                    "status": outcome,
                    "pack_id": req.pack_id,
                    "revision": req.expected_revision,
                    "finding_count": findings.len(),
                    "coverage": coverage.state,
                    "adapter": "metadata_only",
                }))
            }
            "tools.architecture_extract" => {
                #[derive(serde::Deserialize)]
                struct ExtractRequest {
                    root: String,
                    revision: String,
                    snapshot_id: String,
                    allowed_roots: Vec<String>,
                }
                let req: ExtractRequest = serde_json::from_value(payload).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_architecture_extract_request")
                })?;
                let root_path = std::path::Path::new(&req.root);
                architecture_snapshot_runtime::authorize_root(root_path, &req.allowed_roots)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                let workspace_identity =
                    architecture_snapshot_runtime::source_fingerprint(root_path, &req.revision);
                let snapshot = architecture_snapshot_runtime::extract(
                    root_path,
                    &workspace_identity,
                    &req.revision,
                    &req.snapshot_id,
                )
                .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                let snapshot_hash = architecture_snapshot::snapshot_hash(&snapshot)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                Ok(serde_json::json!({
                    "workspace_identity": workspace_identity,
                    "source_revision": req.revision,
                    "snapshot_hash": snapshot_hash,
                    "snapshot": snapshot,
                }))
            }
            "tools.architecture_compare" => {
                #[derive(serde::Deserialize)]
                struct CompareRequest {
                    before: architecture_snapshot::ArchitectureSnapshot,
                    after: architecture_snapshot::ArchitectureSnapshot,
                }
                let req: CompareRequest = serde_json::from_value(payload).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_architecture_compare_request")
                })?;
                let delta = architecture_snapshot::delta(&req.before, &req.after)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                Ok(serde_json::json!({ "delta": delta }))
            }
            "tools.architecture_review" => {
                #[derive(serde::Deserialize)]
                struct ReviewRequest {
                    expected: architecture_snapshot::ExpectedArchitectureDelta,
                    actual: architecture_snapshot::ArchitectureDelta,
                }
                let req: ReviewRequest = serde_json::from_value(payload).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_architecture_review_request")
                })?;
                let review = architecture_snapshot::review(&req.expected, &req.actual)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                Ok(serde_json::json!({ "review": review }))
            }
            "tools.architecture_fingerprint" => {
                #[derive(serde::Deserialize)]
                struct FingerprintRequest {
                    root: String,
                    revision: String,
                }
                let req: FingerprintRequest = serde_json::from_value(payload).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_architecture_fingerprint_request")
                })?;
                let workspace_identity = architecture_snapshot_runtime::source_fingerprint(
                    std::path::Path::new(&req.root),
                    &req.revision,
                );
                Ok(serde_json::json!({ "workspace_identity": workspace_identity }))
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

        let fp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "tw2".to_owned(),
                capability: "tools.architecture_fingerprint".to_owned(),
                payload: serde_json::json!({ "root": ".", "revision": "working-tree" }),
                caller_session_id: None,
            },
        );
        assert!(matches!(fp, ServiceResponse::InvokeResult { .. }));
    }
}
