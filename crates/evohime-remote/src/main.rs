#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::process::ExitCode;

use evohime_service_contract::{
    serve_stdio, ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor,
    ServiceHandler, ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

#[derive(Debug, Default)]
struct RemoteA2aService;

impl ServiceHandler for RemoteA2aService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "remote-a2a".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "remote.list_agents".to_owned(),
                    description: "Lists configured remote A2A agents.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["remote.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "remote.dispatch_task".to_owned(),
                    description: "Dispatches a task to a remote A2A agent.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["agent_id"],
                        "properties": { "agent_id": { "type": "string" } }
                    }),
                    required_permissions: vec!["remote.execute".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "remote-a2a".to_owned(),
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
            "remote.list_agents" => Ok(serde_json::json!({ "agents": [] })),
            "remote.dispatch_task" => {
                let agent_id = payload
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("agent_id must be a non-empty string")
                    })?;
                Ok(serde_json::json!({ "agent_id": agent_id, "status": "accepted" }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown remote-a2a capability: {other}"
            ))),
        }
    }
}

fn main() -> ExitCode {
    let service = RemoteA2aService;
    match serve_stdio(&service) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("evohime-remote exited with error: {err}");
            ExitCode::FAILURE
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
    fn remote_a2a_descriptor_and_dispatch_work() {
        let svc = RemoteA2aService;
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "ra1".to_owned(),
                capability: "remote.list_agents".to_owned(),
                payload: serde_json::json!({}),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
