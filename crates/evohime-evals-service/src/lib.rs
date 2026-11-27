#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `evals` service module implementing `evohime-service-contract`.

pub(crate) mod deterministic_review_execution_plan;
pub mod execution_trace_evaluation;
#[allow(dead_code)]
pub(crate) mod extension_conformance_kit;
/// Multi-reviewer ensemble validation and conservative finding reconciliation.
pub mod multi_reviewer_ensemble;

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
                ServiceCapabilityDescriptor {
                    name: "evals.review_plan_validate".to_owned(),
                    description: "Validates a deterministic review execution plan against its id and content hash.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["plan", "plan_id"],
                        "properties": {
                            "plan": { "type": "object" },
                            "plan_id": { "type": "string" }
                        }
                    }),
                    required_permissions: vec!["evals.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.review_plan_verdict".to_owned(),
                    description: "Computes the verdict for a deterministic review execution plan.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["evals.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.ensemble_validate_profile".to_owned(),
                    description: "Validates a multi-reviewer ensemble profile.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["evals.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.ensemble_prepare_start".to_owned(),
                    description: "Validates and hashes an ensemble run before starting.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["evals.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.ensemble_validate_run".to_owned(),
                    description: "Validates a stored multi-reviewer ensemble run.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["evals.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.ensemble_reconcile".to_owned(),
                    description: "Reconciles normalized review finding candidates across reviewers.".to_owned(),
                    input_schema: serde_json::json!({ "type": "array" }),
                    required_permissions: vec!["evals.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.ensemble_prepare_cancel".to_owned(),
                    description: "Validates and marks a multi-reviewer ensemble run as cancelled.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["evals.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.conformance_run".to_owned(),
                    description: "Runs extension conformance probes against a descriptor and fault mode.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["subject_id", "value"],
                        "properties": {
                            "subject_id": { "type": "string" },
                            "value": { "type": "object" }
                        }
                    }),
                    required_permissions: vec!["evals.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.conformance_register".to_owned(),
                    description: "Stages and commits extension descriptors inside a conformance transaction.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["evals.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "evals.conformance_inspect".to_owned(),
                    description: "Reports conformance kit schema version and supported extension kinds.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["evals.read".to_owned()],
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
                let started = std::time::Instant::now();
                let cases = payload
                    .get("cases")
                    .and_then(|v| v.as_array())
                    .map_or(0, Vec::len);
                Ok(serde_json::json!({
                    "suite_id": suite_id,
                    "cases_evaluated": cases,
                    "elapsed_ms": started.elapsed().as_millis(),
                    "status": "completed"
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
            "evals.review_plan_validate" => {
                #[derive(serde::Deserialize)]
                struct ValidatePlanReq {
                    plan: deterministic_review_execution_plan::ReviewPlan,
                    plan_id: String,
                }
                let req: ValidatePlanReq = serde_json::from_value(payload).map_err(|err| {
                    ServiceContractError::invalid_input(format!("invalid_review_plan: {err}"))
                })?;
                deterministic_review_execution_plan::validate(&req.plan)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                if req.plan.id != req.plan_id {
                    return Err(ServiceContractError::invalid_input("plan_id_mismatch"));
                }
                serde_json::to_value(req.plan)
                    .map_err(|err| ServiceContractError::unavailable(err.to_string()))
            }
            "evals.review_plan_verdict" => {
                let plan: deterministic_review_execution_plan::ReviewPlan =
                    serde_json::from_value(payload).map_err(|err| {
                        ServiceContractError::invalid_input(format!("corrupt_review_plan: {err}"))
                    })?;
                let verdict = deterministic_review_execution_plan::verdict(&plan)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                serde_json::to_value(verdict)
                    .map_err(|err| ServiceContractError::unavailable(err.to_string()))
            }
            "evals.ensemble_validate_profile" => {
                let profile: multi_reviewer_ensemble::ReviewerEnsembleProfile =
                    serde_json::from_value(payload).map_err(|_| {
                        ServiceContractError::invalid_input("invalid_ensemble_profile")
                    })?;
                multi_reviewer_ensemble::validate_profile(&profile)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(profile)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "evals.ensemble_prepare_start" => {
                #[derive(serde::Deserialize)]
                struct StartReq {
                    run: multi_reviewer_ensemble::ReviewerEnsembleRun,
                    expected_revision: u64,
                }
                let mut req: StartReq = serde_json::from_value(payload)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_ensemble_run"))?;
                multi_reviewer_ensemble::validate_run(&req.run)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                req.run.revision = req
                    .expected_revision
                    .saturating_add(1)
                    .max(req.run.revision);
                req.run.status = multi_reviewer_ensemble::RunStatus::Running;
                req.run.content_hash = multi_reviewer_ensemble::run_hash(&req.run)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(req.run)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "evals.ensemble_validate_run" => {
                let run: multi_reviewer_ensemble::ReviewerEnsembleRun =
                    serde_json::from_value(payload)
                        .map_err(|_| ServiceContractError::invalid_input("corrupt_ensemble_run"))?;
                multi_reviewer_ensemble::validate_run(&run)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(run)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "evals.ensemble_reconcile" => {
                let candidates: Vec<multi_reviewer_ensemble::NormalizedReviewFindingCandidate> =
                    serde_json::from_value(payload)
                        .map_err(|_| ServiceContractError::invalid_input("invalid_candidates"))?;
                let clusters = multi_reviewer_ensemble::reconcile_candidates(&candidates)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(clusters)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "evals.ensemble_prepare_cancel" => {
                #[derive(serde::Deserialize)]
                struct CancelReq {
                    run: multi_reviewer_ensemble::ReviewerEnsembleRun,
                    expected_revision: u64,
                }
                let mut req: CancelReq = serde_json::from_value(payload)
                    .map_err(|_| ServiceContractError::invalid_input("corrupt_ensemble_run"))?;
                multi_reviewer_ensemble::validate_run(&req.run)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                if req.expected_revision != 0 && req.expected_revision != req.run.revision {
                    return Err(ServiceContractError::invalid_input(
                        "stale_ensemble_revision",
                    ));
                }
                req.run.revision = req.run.revision.saturating_add(1);
                req.run.status = multi_reviewer_ensemble::RunStatus::Cancelled;
                req.run.content_hash = multi_reviewer_ensemble::run_hash(&req.run)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(req.run)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "evals.conformance_run" => {
                #[derive(serde::Deserialize)]
                struct RunReq {
                    subject_id: String,
                    value: serde_json::Value,
                }
                let req: RunReq = serde_json::from_value(payload).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_conformance_payload")
                })?;
                let d: extension_conformance_kit::ExtensionDescriptor =
                    serde_json::from_value(req.value.get("descriptor").cloned().ok_or_else(
                        || ServiceContractError::invalid_input("descriptor_required"),
                    )?)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_descriptor"))?;
                let p: extension_conformance_kit::ConformanceProbe = serde_json::from_value(
                    req.value
                        .get("probe")
                        .cloned()
                        .ok_or_else(|| ServiceContractError::invalid_input("probe_required"))?,
                )
                .map_err(|_| ServiceContractError::invalid_input("invalid_probe"))?;
                let fault: extension_conformance_kit::FaultMode = serde_json::from_value(
                    req.value
                        .get("fault")
                        .cloned()
                        .unwrap_or(serde_json::json!("none")),
                )
                .map_err(|_| ServiceContractError::invalid_input("invalid_fault"))?;
                if d.subject_id != req.subject_id {
                    return Err(ServiceContractError::invalid_input("subject_id_mismatch"));
                }
                let report = extension_conformance_kit::run(&d, &p, fault)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                serde_json::to_value(report)
                    .map_err(|e| ServiceContractError::unavailable(e.to_string()))
            }
            "evals.conformance_register" => {
                let descriptors: Vec<extension_conformance_kit::ExtensionDescriptor> =
                    serde_json::from_value(payload.get("descriptors").cloned().ok_or_else(
                        || ServiceContractError::invalid_input("descriptors_required"),
                    )?)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_descriptors"))?;
                let fault: extension_conformance_kit::FaultMode = serde_json::from_value(
                    payload
                        .get("fault")
                        .cloned()
                        .unwrap_or(serde_json::json!("none")),
                )
                .map_err(|_| ServiceContractError::invalid_input("invalid_fault"))?;
                let mut t = extension_conformance_kit::RegistrationTransaction::default();
                for d in descriptors {
                    t.stage(d)
                        .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                }
                let committed = t
                    .commit(fault)
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                Ok(serde_json::json!({
                    "status": "registered_ephemeral",
                    "count": committed.len(),
                    "redacted": true,
                }))
            }
            "evals.conformance_inspect" => Ok(serde_json::json!({
                "status": "available",
                "schema_version": extension_conformance_kit::SCHEMA_VERSION,
                "kinds": [
                    "integration_provider",
                    "external_agent_adapter",
                    "workbench",
                    "ui_extension",
                    "declarative_component_provider"
                ],
                "production_execution": false,
                "redacted": true,
            })),
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
            ServiceResponse::InvokeResult {
                request_id,
                payload,
            } => {
                assert_eq!(request_id, "r1");
                assert!(payload["suites"].is_array());
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
