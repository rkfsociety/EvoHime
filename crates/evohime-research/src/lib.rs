#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `research` service module implementing `evohime-service-contract`.

pub mod external_source_acquisition_runtime;
pub(crate) mod research_fetch;
pub(crate) mod research_gate;
pub(crate) mod research_pipeline;
pub(crate) mod research_search;
pub(crate) mod research_types;

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for web search, page fetching, and repository inspection.
#[derive(Debug, Default)]
pub struct ResearchService;

impl ResearchService {
    /// Creates a new [`ResearchService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for ResearchService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "research".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "research.web_search".to_owned(),
                    description: "Executes a bounded web search query.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["query"],
                        "properties": { "query": { "type": "string" } }
                    }),
                    required_permissions: vec!["network.search".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "research.fetch_page".to_owned(),
                    description: "Fetches and normalizes a public web resource.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["url"],
                        "properties": { "url": { "type": "string" } }
                    }),
                    required_permissions: vec!["network.fetch".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "research.github_inspect".to_owned(),
                    description: "Inspects metadata of a public GitHub repository.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["repo"],
                        "properties": { "repo": { "type": "string" } }
                    }),
                    required_permissions: vec!["network.fetch".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "research.fetch_url".to_owned(),
                    description: "Executes a policy-gated, SSRF-protected HTTP GET and produces redacted evidence.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["work_item_id", "url", "title", "allowed_domains", "max_bytes", "max_latency_ms", "max_cost_micros", "ttl_ms"],
                        "properties": {
                            "work_item_id": { "type": "string" },
                            "url": { "type": "string" },
                            "title": { "type": "string" }
                        }
                    }),
                    required_permissions: vec!["network.fetch".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "research.grounded_session".to_owned(),
                    description: "Executes a bounded search-and-fetch grounded research session.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["request_id", "query", "mode", "source_policy", "budget", "endpoint", "results", "allowed_domains", "max_bytes", "max_latency_ms", "max_cost_micros", "ttl_ms"],
                        "properties": {
                            "request_id": { "type": "string" },
                            "query": { "type": "string" }
                        }
                    }),
                    required_permissions: vec!["network.search".to_owned(), "network.fetch".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "research.check_gate".to_owned(),
                    description: "Evaluates whether research evidence is required and fresh for a task.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["task_text", "evidence", "now_ms"],
                        "properties": {
                            "task_text": { "type": "string" },
                            "now_ms": { "type": "integer" }
                        }
                    }),
                    required_permissions: vec![],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "research".to_owned(),
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
            "research.web_search" => {
                let query = payload
                    .get("query")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("query must be a non-empty string")
                    })?;
                Ok(serde_json::json!({ "query": query, "status": "accepted" }))
            }
            "research.fetch_page" => {
                let url = payload
                    .get("url")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("url must be a non-empty string")
                    })?;
                if !url.starts_with("https://") && !url.starts_with("http://") {
                    return Err(ServiceContractError::invalid_input(
                        "url must use http:// or https://",
                    ));
                }
                let lower = url.to_ascii_lowercase();
                if lower.contains("://localhost")
                    || lower.contains("://127.0.0.1")
                    || lower.contains("://[::1]")
                {
                    return Err(ServiceContractError::permission_denied(
                        "loopback URLs are blocked by SSRF policy",
                    ));
                }
                Ok(serde_json::json!({ "url": url, "status": "accepted" }))
            }
            "research.github_inspect" => {
                let repo = payload
                    .get("repo")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("repo must be a non-empty string")
                    })?;
                let valid_slug = repo.split('/').count() == 2
                    && repo.split('/').all(|part| {
                        !part.is_empty()
                            && part
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                    });
                if !valid_slug {
                    return Err(ServiceContractError::invalid_input(
                        "repo must be formatted as owner/name",
                    ));
                }
                Ok(serde_json::json!({ "repo": repo, "status": "accepted" }))
            }
            "research.fetch_url" => {
                #[derive(serde::Deserialize)]
                struct FetchUrlRequest {
                    work_item_id: String,
                    url: String,
                    title: String,
                    allowed_domains: Vec<String>,
                    max_bytes: u64,
                    max_latency_ms: u64,
                    max_cost_micros: u64,
                    ttl_ms: u64,
                    #[serde(default)]
                    cancelled: bool,
                }
                let req: FetchUrlRequest = serde_json::from_value(payload).map_err(|err| {
                    ServiceContractError::invalid_input(format!("invalid fetch_url payload: {err}"))
                })?;
                if req.work_item_id.trim().is_empty() {
                    return Err(ServiceContractError::invalid_input(
                        "work_item_id must not be empty",
                    ));
                }
                let policy = research_pipeline::ResearchPolicy {
                    network_allowed: true,
                    allowed_domains: req.allowed_domains,
                    max_bytes: req.max_bytes,
                    max_latency_ms: req.max_latency_ms,
                    max_cost_micros: req.max_cost_micros,
                };
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|err| {
                        ServiceContractError::unavailable(format!(
                            "failed to build tokio runtime: {err}"
                        ))
                    })?;
                let outcome = runtime
                    .block_on(research_fetch::run_research_fetch(
                        &req.work_item_id,
                        &req.url,
                        &req.title,
                        &policy,
                        req.ttl_ms,
                        req.cancelled,
                    ))
                    .map_err(|err| {
                        let code = match err.state {
                            research_pipeline::PipelineState::Cancelled => {
                                evohime_service_contract::ServiceErrorCode::Unavailable
                            }
                            _ => evohime_service_contract::ServiceErrorCode::InvalidInput,
                        };
                        ServiceContractError::new(
                            code,
                            serde_json::json!({
                                "state": err.state,
                                "message": err.message,
                            })
                            .to_string(),
                        )
                    })?;
                serde_json::to_value(serde_json::json!({
                    "state": outcome.state,
                    "evidence": outcome.evidence,
                    "citation": outcome.citation,
                    "source_revision": outcome.revision,
                    "evidence_item": outcome.evidence_item,
                }))
                .map_err(|err| ServiceContractError::unavailable(err.to_string()))
            }
            "research.grounded_session" => {
                #[derive(serde::Deserialize)]
                struct GroundedSessionRequest {
                    request_id: String,
                    query: String,
                    mode: research_types::ResearchMode,
                    source_policy: research_types::SourcePolicy,
                    budget: research_types::ResearchBudget,
                    endpoint: String,
                    results: Vec<research_search::SearchResult>,
                    allowed_domains: Vec<String>,
                    max_bytes: u64,
                    max_latency_ms: u64,
                    max_cost_micros: u64,
                    ttl_ms: u64,
                    #[serde(default)]
                    cancelled: bool,
                }
                let req: GroundedSessionRequest =
                    serde_json::from_value(payload).map_err(|err| {
                        ServiceContractError::invalid_input(format!(
                            "invalid_research_session_run: {err}"
                        ))
                    })?;
                let network_policy =
                    evohime_tool_runtime::network_capability::NetworkCapabilityPolicy::new(
                        req.allowed_domains.clone(),
                        req.max_bytes,
                        req.max_latency_ms,
                        req.max_cost_micros,
                        false,
                        evohime_tool_runtime::network_capability::RefreshPolicy::Never,
                    )
                    .map_err(|err| {
                        ServiceContractError::invalid_input(format!(
                            "invalid_network_policy: {err:?}"
                        ))
                    })?;
                let fetch_policy = research_pipeline::ResearchPolicy {
                    network_allowed: true,
                    allowed_domains: req.allowed_domains,
                    max_bytes: req.max_bytes,
                    max_latency_ms: req.max_latency_ms,
                    max_cost_micros: req.max_cost_micros,
                };
                let provider = research_search::OfflineStubSearchProvider {
                    endpoint: req.endpoint,
                    fixed_results: req.results,
                };
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|err| {
                        ServiceContractError::unavailable(format!(
                            "failed to build tokio runtime: {err}"
                        ))
                    })?;
                let outcome = runtime
                    .block_on(research_search::run_bounded_research_session(
                        &req.request_id,
                        &req.query,
                        req.mode,
                        req.source_policy,
                        &req.budget,
                        &provider,
                        &research_search::ExtractiveSummarizer,
                        &network_policy,
                        &fetch_policy,
                        req.ttl_ms,
                        req.cancelled,
                    ))
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                Ok(serde_json::json!({
                    "schema_version": 1,
                    "state": outcome.state,
                    "coverage": outcome.coverage,
                    "fetched": outcome.fetched.iter().map(|item| serde_json::json!({
                        "revision": item.revision,
                        "evidence_item": item.evidence_item,
                    })).collect::<Vec<_>>(),
                    "summary": outcome.summary,
                    "omitted_results": outcome.omitted_results,
                    "redacted": true,
                }))
            }
            "research.check_gate" => {
                #[derive(serde::Deserialize)]
                struct CheckGateRequest {
                    task_text: String,
                    evidence: Vec<research_types::ResearchEvidence>,
                    now_ms: u64,
                }
                let req: CheckGateRequest = serde_json::from_value(payload).map_err(|err| {
                    ServiceContractError::invalid_input(format!(
                        "invalid check_gate payload: {err}"
                    ))
                })?;
                let decision =
                    research_gate::check_research_gate(&req.task_text, &req.evidence, req.now_ms);
                Ok(serde_json::json!({
                    "allowed": decision.is_allowed(),
                    "status": format!("{decision:?}"),
                }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown research capability: {other}"
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
        let svc = ResearchService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");

        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "q1".to_owned(),
                capability: "research.web_search".to_owned(),
                payload: serde_json::json!({ "query": "rust ipc" }),
                caller_session_id: None,
            },
        );
        match resp {
            ServiceResponse::InvokeResult {
                request_id,
                payload,
            } => {
                assert_eq!(request_id, "q1");
                assert_eq!(payload["query"], "rust ipc");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
