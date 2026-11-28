//! External plan-review service exposed by `evohime-verify.exe --serve`.
//!
//! The service owns exactly the model-executing half of plan review: the
//! reviewer fan-out and the single-model revision. It reads provider
//! credentials from its own environment (the supervisor passes them to
//! Core, and Core passes the environment to spawned service processes), so
//! API keys never travel through IPC payloads, logs, or the registry.
//! Persistence, journal events, cancellation bookkeeping and UI progress
//! remain owned by the caller (Core).

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use evohime_model_gateway::{ModelGateway, ModelGatewayConfig};
use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};
use tokio_util::sync::CancellationToken;

use crate::plan_review::{
    run_review_with_progress, run_revision, ReviewError, ReviewProgress, ReviewRequest,
    ReviewResult, RevisionProgress, RevisionRequest, RevisionResult,
};

/// Bounded request to run one multi-reviewer plan review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanReviewRunRequest {
    /// The validated review request received from Core.
    pub review: ReviewRequest,
    /// Provider route Core validated the review models against. `None` uses
    /// the service's own default route.
    #[serde(default)]
    pub route: Option<String>,
}

/// Collected progress timeline plus the final review outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanReviewRunResponse {
    /// Final synthesized review result.
    pub result: ReviewResult,
    /// Progress events emitted while the review ran, in emission order.
    pub progress: Vec<ReviewProgress>,
}

/// Bounded request to run one single-model plan revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRevisionRunRequest {
    /// The validated revision request received from Core.
    pub revision: RevisionRequest,
    /// Provider route Core validated the revision model against. `None` uses
    /// the service's own default route.
    #[serde(default)]
    pub route: Option<String>,
}

/// Collected progress timeline plus the final revision outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRevisionRunResponse {
    /// Final rewritten plan.
    pub result: RevisionResult,
    /// Progress events emitted while the revision ran, in emission order.
    pub progress: Vec<RevisionProgress>,
}

/// Service handler executing plan reviews and revisions for Core.
///
/// Every invocation builds a fresh gateway from the current environment, so
/// a provider key saved by the shell applies to the next run without any
/// state to invalidate. Review runs are long-lived; Core owns cancellation
/// bookkeeping and discards results of runs it has already stopped.
#[derive(Debug, Default)]
pub struct VerifierService;

impl VerifierService {
    /// Creates a new [`VerifierService`] instance.
    pub fn new() -> Self {
        Self
    }

    fn run_review(&self, request: PlanReviewRunRequest) -> Result<Value, ServiceContractError> {
        request
            .review
            .validate()
            .map_err(|error| ServiceContractError::invalid_input(error.to_string()))?;
        let gateway = Self::gateway_from_route(request.route.as_deref())?;
        let progress: Arc<Mutex<Vec<ReviewProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let collector = Arc::clone(&progress);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                ServiceContractError::unavailable(format!("failed to build runtime: {error}"))
            })?;
        let result = runtime.block_on(run_review_with_progress(
            Arc::new(gateway),
            request.review,
            CancellationToken::new(),
            Arc::new(move |progress| {
                if let Ok(mut timeline) = collector.lock() {
                    timeline.push(progress);
                }
            }),
        ));
        let progress = progress
            .lock()
            .map(|timeline| timeline.clone())
            .unwrap_or_default();
        let result = result.map_err(Self::review_error)?;
        serde_json::to_value(PlanReviewRunResponse { result, progress })
            .map_err(|error| ServiceContractError::unavailable(error.to_string()))
    }

    fn run_revision(&self, request: PlanRevisionRunRequest) -> Result<Value, ServiceContractError> {
        request
            .revision
            .validate()
            .map_err(|error| ServiceContractError::invalid_input(error.to_string()))?;
        let gateway = Self::gateway_from_route(request.route.as_deref())?;
        let progress: Arc<Mutex<Vec<RevisionProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let collector = Arc::clone(&progress);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                ServiceContractError::unavailable(format!("failed to build runtime: {error}"))
            })?;
        let result = runtime.block_on(run_revision(
            Arc::new(gateway),
            request.revision,
            CancellationToken::new(),
            Arc::new(move |progress| {
                if let Ok(mut timeline) = collector.lock() {
                    timeline.push(progress);
                }
            }),
        ));
        let progress = progress
            .lock()
            .map(|timeline| timeline.clone())
            .unwrap_or_default();
        let result = result.map_err(Self::review_error)?;
        serde_json::to_value(PlanRevisionRunResponse { result, progress })
            .map_err(|error| ServiceContractError::unavailable(error.to_string()))
    }

    fn gateway_from_route(route: Option<&str>) -> Result<ModelGateway, ServiceContractError> {
        let config = ModelGatewayConfig::from_env()
            .map_err(|error| ServiceContractError::unavailable(error.to_string()))?;
        let Some(route_name) = route.map(str::to_owned) else {
            return ModelGateway::from_config(&config)
                .map_err(|error| ServiceContractError::unavailable(error.to_string()));
        };
        let route_config = config.routes.get(&route_name).cloned().ok_or_else(|| {
            ServiceContractError::invalid_input(format!(
                "requested provider route '{route_name}' is not configured"
            ))
        })?;
        let single = ModelGatewayConfig {
            default_route: route_name.clone(),
            routes: std::collections::HashMap::from([(route_name, route_config)]),
        };
        ModelGateway::from_config(&single)
            .map_err(|error| ServiceContractError::unavailable(error.to_string()))
    }

    fn review_error(error: ReviewError) -> ServiceContractError {
        ServiceContractError::unavailable(error.to_string())
    }
}

impl ServiceHandler for VerifierService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "verifier".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "verifier.plan_review_run".to_owned(),
                    description: "Runs a bounded multi-reviewer plan review and returns the synthesized result with its progress timeline.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["review"],
                        "properties": { "review": { "type": "object" } }
                    }),
                    required_permissions: vec!["verifier.review".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "verifier.plan_revision_run".to_owned(),
                    description: "Runs a single-model plan revision and returns the rewritten plan with its progress timeline.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["revision"],
                        "properties": { "revision": { "type": "object" } }
                    }),
                    required_permissions: vec!["verifier.review".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "verifier".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            status: ServiceHealthStatus::Ready,
            active_requests: 0,
            detail: None,
        }
    }

    fn invoke(
        &self,
        capability: &str,
        payload: Value,
        _caller_session_id: Option<&str>,
    ) -> Result<Value, ServiceContractError> {
        match capability {
            "verifier.plan_review_run" => {
                let request: PlanReviewRunRequest =
                    serde_json::from_value(payload).map_err(|error| {
                        ServiceContractError::invalid_input(format!(
                            "invalid_plan_review_run: {error}"
                        ))
                    })?;
                self.run_review(request)
            }
            "verifier.plan_revision_run" => {
                let request: PlanRevisionRunRequest =
                    serde_json::from_value(payload).map_err(|error| {
                        ServiceContractError::invalid_input(format!(
                            "invalid_plan_revision_run: {error}"
                        ))
                    })?;
                self.run_revision(request)
            }
            other => Err(ServiceContractError::not_found(format!(
                "capability '{other}' is not supported by 'verifier'"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_review() -> ReviewRequest {
        ReviewRequest {
            review_id: "review-1".to_owned(),
            file_name: "plan.md".to_owned(),
            file_names: vec!["plan.md".to_owned()],
            source_markdown: "# Plan\n\nDo the thing.".to_owned(),
            reviewer_models: vec!["model-a".to_owned(), "model-b".to_owned()],
            synthesis_model: "model-c".to_owned(),
            context_documents: Vec::new(),
        }
    }

    #[test]
    fn descriptor_advertises_review_capabilities() {
        let service = VerifierService::new();
        let descriptor = service.descriptor();
        let names: Vec<&str> = descriptor
            .capabilities
            .iter()
            .map(|capability| capability.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["verifier.plan_review_run", "verifier.plan_revision_run"]
        );
        assert_eq!(descriptor.service_id, "verifier");
    }

    #[test]
    fn invoke_rejects_unknown_capability() {
        let service = VerifierService::new();
        let error = service
            .invoke("verifier.unknown", serde_json::json!({}), None)
            .expect_err("unknown capability must be rejected");
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn invoke_rejects_invalid_review_payload() {
        let service = VerifierService::new();
        let error = service
            .invoke("verifier.plan_review_run", serde_json::json!({}), None)
            .expect_err("missing review must be rejected");
        assert!(error.to_string().contains("invalid_plan_review_run"));
    }

    #[test]
    fn invoke_rejects_review_failing_validation() {
        let service = VerifierService::new();
        let mut review = sample_review();
        review.review_id = "  ".to_owned();
        let error = service
            .invoke(
                "verifier.plan_review_run",
                serde_json::json!({ "review": review }),
                None,
            )
            .expect_err("invalid review must be rejected");
        assert!(error.to_string().contains("review id is empty"));
    }
}
