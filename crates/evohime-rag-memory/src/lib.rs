#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `rag-memory` service module implementing `evohime-service-contract`.

/// Bounded episodic experience replay records and context projection.
pub mod experience_replay_library;
/// Bounded metadata contracts for offline experience consolidation.
pub mod offline_experience_consolidation;
/// Project-scoped knowledge notebook validation and pinning contracts.
pub mod project_knowledge_notebook;

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
                ServiceCapabilityDescriptor {
                    name: "rag.notebook_validate".to_owned(),
                    description: "Validates a project knowledge notebook payload.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["rag.write".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "rag.notebook_pin".to_owned(),
                    description: "Validates and creates a notebook pin projection.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["notebook", "run_id"]
                    }),
                    required_permissions: vec!["rag.write".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "memory.consolidation_validate".to_owned(),
                    description: "Validates an offline experience consolidation cycle.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["memory.write".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "memory.consolidation_evaluate".to_owned(),
                    description: "Evaluates a stored offline consolidation cycle.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["memory.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "memory.replay_validate".to_owned(),
                    description: "Validates an episodic experience replay record and scope."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["scope", "scope_id", "record"]
                    }),
                    required_permissions: vec!["memory.write".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "memory.replay_context".to_owned(),
                    description: "Projects bounded context from stored experience replay records."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["records"]
                    }),
                    required_permissions: vec!["memory.read".to_owned()],
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
                let root = payload
                    .get("root")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or(".");
                let mut hits = Vec::new();
                collect_rag_hits(std::path::Path::new(root), query, &mut hits, 20);
                hits.sort_by_key(|a| std::cmp::Reverse(a.1));
                let matches = hits
                    .into_iter()
                    .map(|(path, score)| serde_json::json!({ "path": path, "score": score }))
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({ "query": query, "matches": matches }))
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
            "rag.notebook_validate" => {
                let notebook: project_knowledge_notebook::Notebook =
                    serde_json::from_value(payload).map_err(|err| {
                        ServiceContractError::invalid_input(format!("invalid notebook: {err}"))
                    })?;
                project_knowledge_notebook::validate(&notebook)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                serde_json::to_value(&notebook).map_err(|err| {
                    ServiceContractError::new(
                        evohime_service_contract::ServiceErrorCode::Internal,
                        format!("failed to serialize notebook: {err}"),
                    )
                })
            }
            "rag.notebook_pin" => {
                let run_id = payload
                    .get("run_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ServiceContractError::invalid_input("run_id_required"))?;
                let notebook_val = payload
                    .get("notebook")
                    .cloned()
                    .ok_or_else(|| ServiceContractError::invalid_input("notebook_required"))?;
                let notebook: project_knowledge_notebook::Notebook =
                    serde_json::from_value(notebook_val).map_err(|err| {
                        ServiceContractError::invalid_input(format!("corrupt_notebook: {err}"))
                    })?;
                let pin = project_knowledge_notebook::pin(&notebook, run_id)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                serde_json::to_value(&pin).map_err(|err| {
                    ServiceContractError::new(
                        evohime_service_contract::ServiceErrorCode::Internal,
                        format!("failed to serialize pin: {err}"),
                    )
                })
            }
            "memory.consolidation_validate" => {
                let cycle: offline_experience_consolidation::ConsolidationCycle =
                    serde_json::from_value(payload).map_err(|err| {
                        ServiceContractError::invalid_input(format!(
                            "invalid_offline_consolidation: {err}"
                        ))
                    })?;
                offline_experience_consolidation::validate(&cycle)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                serde_json::to_value(&cycle).map_err(|err| {
                    ServiceContractError::new(
                        evohime_service_contract::ServiceErrorCode::Internal,
                        format!("failed to serialize consolidation cycle: {err}"),
                    )
                })
            }
            "memory.consolidation_evaluate" => {
                let cycle: offline_experience_consolidation::ConsolidationCycle =
                    serde_json::from_value(payload).map_err(|err| {
                        ServiceContractError::invalid_input(format!(
                            "corrupt_consolidation_cycle: {err}"
                        ))
                    })?;
                offline_experience_consolidation::evaluate(&cycle)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))
            }
            "memory.replay_validate" => {
                let scope = payload
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("invalid_experience_scope")
                    })?;
                let scope_id = payload
                    .get("scope_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("experience_scope_denied")
                    })?;
                let scope_kind = match scope {
                    "Session" => experience_replay_library::ExperienceScope::Session,
                    "Project" => experience_replay_library::ExperienceScope::Project,
                    "User" => experience_replay_library::ExperienceScope::User,
                    "RoleProfile" => experience_replay_library::ExperienceScope::RoleProfile,
                    "WorkflowProfile" => {
                        experience_replay_library::ExperienceScope::WorkflowProfile
                    }
                    _ => {
                        return Err(ServiceContractError::invalid_input(
                            "invalid_experience_scope",
                        ))
                    }
                };
                let record_val = payload.get("record").cloned().ok_or_else(|| {
                    ServiceContractError::invalid_input("invalid_experience_record")
                })?;
                let record: experience_replay_library::ExperienceRecord =
                    serde_json::from_value(record_val).map_err(|_| {
                        ServiceContractError::invalid_input("invalid_experience_record")
                    })?;
                if record.scope != scope_kind || record.scope_id != scope_id {
                    return Err(ServiceContractError::invalid_input(
                        "experience_scope_denied",
                    ));
                }
                experience_replay_library::validate_and_write_gate(&record)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                serde_json::to_value(&record).map_err(|err| {
                    ServiceContractError::new(
                        evohime_service_contract::ServiceErrorCode::Internal,
                        format!("failed to serialize experience record: {err}"),
                    )
                })
            }
            "memory.replay_context" => {
                let records_val = payload
                    .get("records")
                    .cloned()
                    .ok_or_else(|| ServiceContractError::invalid_input("records_required"))?;
                let records: Vec<experience_replay_library::ExperienceRecord> =
                    serde_json::from_value(records_val).map_err(|_| {
                        ServiceContractError::invalid_input("invalid_experience_record")
                    })?;
                let context = experience_replay_library::project_context(
                    &records,
                    experience_replay_library::MAX_CONTEXT_BYTES,
                )
                .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                Ok(serde_json::json!({
                    "status": "ok",
                    "context": context,
                    "max_bytes": experience_replay_library::MAX_CONTEXT_BYTES
                }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown rag-memory capability: {other}"
            ))),
        }
    }
}

fn collect_rag_hits(
    dir: &std::path::Path,
    query: &str,
    out: &mut Vec<(String, usize)>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let needle = query.to_lowercase();
    for entry in entries.flatten() {
        if out.len() >= limit {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        if matches!(
            name.to_string_lossy().as_ref(),
            ".git" | "target" | "node_modules" | ".evohime"
        ) {
            continue;
        }
        if path.is_dir() {
            collect_rag_hits(&path, query, out, limit);
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
            let score = text.to_lowercase().matches(&needle).count();
            if score > 0 {
                out.push((path.to_string_lossy().into_owned(), score));
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
