#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `rag-memory` service module implementing `evohime-service-contract`.

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for workspace RAG indexing/retrieval and memory extraction.
#[derive(Debug, Default)]
pub struct RagMemoryService;

impl RagMemoryService {
    /// Creates a new [`RagMemoryService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for RagMemoryService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "rag-memory".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "rag.search".to_owned(),
                    description: "Searches indexed workspace files and knowledge notes.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["query"],
                        "properties": { "query": { "type": "string" } }
                    }),
                    required_permissions: vec!["rag.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "memory.extract".to_owned(),
                    description: "Extracts bounded memory candidates from conversation context."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["text"],
                        "properties": { "text": { "type": "string" } }
                    }),
                    required_permissions: vec!["memory.write".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "rag-memory".to_owned(),
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
            "rag.search" => {
                let query = payload
                    .get("query")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("query must be a non-empty string")
                    })?;
                Ok(serde_json::json!({ "query": query, "matches": [] }))
            }
            "memory.extract" => {
                let text = payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("text must be a non-empty string")
                    })?;
                Ok(serde_json::json!({ "chars": text.chars().count(), "candidates": [] }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown rag-memory capability: {other}"
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
        let svc = RagMemoryService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "rm1".to_owned(),
                capability: "rag.search".to_owned(),
                payload: serde_json::json!({ "query": "service contract" }),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
