#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `research` service module implementing `evohime-service-contract`.

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
                Ok(serde_json::json!({ "repo": repo, "status": "accepted" }))
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
            ServiceResponse::InvokeResult { request_id, payload } => {
                assert_eq!(request_id, "q1");
                assert_eq!(payload["query"], "rust ipc");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
