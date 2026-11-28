#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `workflow-engine` service module implementing `evohime-service-contract`.

/// Bounded automation snapshots and side-effect-free simulation primitives.
pub mod automation_simulation;
#[allow(dead_code)]
pub(crate) mod batch_invocation_runtime;
pub(crate) mod dependency_aware_task_graph;
pub mod domain_workflow_recipes;

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
                ServiceCapabilityDescriptor {
                    name: "workflow.task_graph_validate".to_owned(),
                    description:
                        "Validates a dependency-aware task graph and computes its ready set."
                            .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["graph", "grants"],
                        "properties": {
                            "graph": { "type": "object" },
                            "grants": { "type": "array" }
                        }
                    }),
                    required_permissions: vec!["workflow.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "workflow.task_graph_patch".to_owned(),
                    description:
                        "Applies validated patch operations to a dependency-aware task graph."
                            .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["current", "ops", "expected_revision", "grants"],
                        "properties": {
                            "current": { "type": "object" },
                            "ops": { "type": "array" },
                            "expected_revision": { "type": "integer" },
                            "grants": { "type": "array" }
                        }
                    }),
                    required_permissions: vec!["workflow.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "workflow.batch_create".to_owned(),
                    description: "Creates and hashes a bounded batch invocation record.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["batch_id", "request", "now_ms"],
                        "properties": {
                            "batch_id": { "type": "string" },
                            "request": { "type": "object" },
                            "now_ms": { "type": "integer" }
                        }
                    }),
                    required_permissions: vec!["workflow.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "workflow.batch_transition".to_owned(),
                    description: "Transitions or projects a stored batch invocation record."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["operation", "value", "expected_version", "now_ms"],
                        "properties": {
                            "operation": { "type": "string" },
                            "value": { "type": "object" },
                            "expected_version": { "type": "integer" },
                            "now_ms": { "type": "integer" }
                        }
                    }),
                    required_permissions: vec!["workflow.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "workflow.simulation_replay_hash".to_owned(),
                    description: "Computes a deterministic replay hash for a simulation input."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["schema_version", "definition_revision", "ordered_events", "normalized_inputs", "frozen_clock_ms", "rng_seed", "provider_fixture_ids", "capability_snapshot", "policy_snapshot"]
                    }),
                    required_permissions: vec!["workflow.read".to_owned()],
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
            "workflow.task_graph_validate" => {
                #[derive(serde::Deserialize)]
                struct ValidateReq {
                    graph: dependency_aware_task_graph::TaskGraph,
                    grants: Vec<String>,
                }
                let req: ValidateReq = serde_json::from_value(payload).map_err(|err| {
                    ServiceContractError::invalid_input(format!("invalid_graph: {err}"))
                })?;
                dependency_aware_task_graph::validate(&req.graph, &req.grants)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                Ok(serde_json::json!({
                    "status": "valid",
                    "ready": dependency_aware_task_graph::ready_set(&req.graph),
                    "revision": req.graph.revision,
                }))
            }
            "workflow.task_graph_patch" => {
                #[derive(serde::Deserialize)]
                struct PatchReq {
                    current: dependency_aware_task_graph::TaskGraph,
                    ops: Vec<dependency_aware_task_graph::PatchOp>,
                    expected_revision: u64,
                    grants: Vec<String>,
                }
                let req: PatchReq = serde_json::from_value(payload).map_err(|err| {
                    ServiceContractError::invalid_input(format!("invalid_patch: {err}"))
                })?;
                let next = dependency_aware_task_graph::apply_patch(
                    req.current,
                    &req.ops,
                    req.expected_revision,
                    &req.grants,
                )
                .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                serde_json::to_value(next)
                    .map_err(|err| ServiceContractError::unavailable(err.to_string()))
            }
            "workflow.batch_create" => {
                #[derive(serde::Deserialize)]
                struct CreateReq {
                    batch_id: String,
                    request: batch_invocation_runtime::CreateBatchRequest,
                    now_ms: i64,
                }
                let req: CreateReq = serde_json::from_value(payload)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_batch_payload"))?;
                let policy = batch_invocation_runtime::default_policy();
                let value =
                    batch_invocation_runtime::new_batch(batch_invocation_runtime::NewBatchInput {
                        id: req.batch_id,
                        definition_ref: req.request.definition_ref,
                        definition_version: req.request.definition_version,
                        inputs: req.request.inputs,
                        max_concurrency: req.request.max_concurrency,
                        failure_policy: req.request.failure_policy,
                        now_ms: req.now_ms,
                        policy,
                    })
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                let projection = batch_invocation_runtime::projection(&value);
                Ok(serde_json::json!({ "value": value, "projection": projection }))
            }
            "workflow.batch_transition" => {
                #[derive(serde::Deserialize)]
                struct TransitionReq {
                    operation: String,
                    value: batch_invocation_runtime::BatchInvocation,
                    expected_version: u64,
                    now_ms: i64,
                    #[serde(default)]
                    request: Option<batch_invocation_runtime::RecordResultRequest>,
                }
                let mut req: TransitionReq = serde_json::from_value(payload)
                    .map_err(|_| ServiceContractError::invalid_input("corrupt_batch"))?;
                let policy = batch_invocation_runtime::default_policy();
                match req.operation.as_str() {
                    "get" => {}
                    "resume" => {
                        batch_invocation_runtime::resume_pending(
                            &mut req.value,
                            req.expected_version,
                            req.now_ms,
                            &policy,
                        )
                        .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                    }
                    "start" => {
                        batch_invocation_runtime::start_batch(
                            &mut req.value,
                            req.expected_version,
                            req.now_ms,
                            &policy,
                        )
                        .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                    }
                    "result" => {
                        let res_req = req.request.ok_or_else(|| {
                            ServiceContractError::invalid_input("invalid_batch_result")
                        })?;
                        batch_invocation_runtime::record_result(
                            batch_invocation_runtime::RecordResultInput {
                                batch: &mut req.value,
                                item_id: &res_req.item_id,
                                expected_version: req.expected_version,
                                status: res_req.status,
                                result_ref: res_req.result_ref,
                                error_class: res_req.error_class,
                                now_ms: req.now_ms,
                                policy: &policy,
                            },
                        )
                        .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                    }
                    _ => {
                        return Err(ServiceContractError::invalid_input(
                            "unsupported_batch_operation",
                        ));
                    }
                }
                let projection = batch_invocation_runtime::projection(&req.value);
                Ok(serde_json::json!({ "value": req.value, "projection": projection }))
            }
            "workflow.simulation_replay_hash" => {
                let input: automation_simulation::ReplayInputV1 = serde_json::from_value(payload)
                    .map_err(|err| {
                    ServiceContractError::invalid_input(format!(
                        "invalid_simulation_replay_input: {err}"
                    ))
                })?;
                let hash = automation_simulation::replay_hash(&input)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                Ok(serde_json::json!({ "replay_hash": hash }))
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
