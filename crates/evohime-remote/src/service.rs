//! Service handler implementation for the isolated `remote-a2a` module.

use std::sync::Mutex;

use evohime_model_gateway::PrivacyClass;
use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};
use serde::Deserialize;

use crate::durable_remote_task_bridge as bridge;
use crate::external_agent_a2a::{binding_projection, RemoteAgentProfile, RemoteTaskState};
use crate::external_agent_a2a_runtime::A2aRuntimeRegistry;
use crate::external_coding_agent_adapter::{
    capability_snapshot, validate_acp_frame, AcpSessionState, AgentState, ExternalAgentRegistry,
    A2A_PROTOCOL_VERSION, ACP_PROTOCOL_VERSION, CONTRACT_ID, CONTRACT_VERSION,
};
use crate::remote_conversation_channels as channels;

/// Service handler for remote A2A, ACP, conversation channels, and durable task bridge.
#[derive(Debug, Default)]
pub struct RemoteA2aService {
    external_agents: Mutex<ExternalAgentRegistry>,
    a2a_runtime: Mutex<A2aRuntimeRegistry>,
}

impl RemoteA2aService {
    /// Creates a new [`RemoteA2aService`] instance with empty in-memory registries.
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteChannelPayload {
    #[serde(default)]
    connection: Option<channels::ChannelConnection>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    external_identity: Option<String>,
    #[serde(default)]
    message: Option<channels::InboundMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteTaskSubmitRequest {
    toolset: bridge::RemoteTaskToolset,
    #[serde(default)]
    request: serde_json::Value,
    #[serde(default = "default_remote_provenance")]
    provenance_ref: String,
    #[serde(default)]
    operation: String,
}

fn default_remote_provenance() -> String {
    "core".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteTaskPollRequest {
    #[serde(default = "default_remote_provenance")]
    lease_owner: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteTaskResultRequest {
    status: bridge::RemoteTaskStatus,
    #[serde(default = "default_remote_transport_status")]
    transport_status: String,
    #[serde(default)]
    result_artifact_ref: Option<String>,
}

fn default_remote_transport_status() -> String {
    "reported".into()
}

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
                ServiceCapabilityDescriptor {
                    name: "remote.channel_op".to_owned(),
                    description: "Validates and transitions remote conversation channel records."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["operation"],
                        "properties": { "operation": { "type": "string" } }
                    }),
                    required_permissions: vec!["remote.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "remote.task_bridge_op".to_owned(),
                    description: "Executes durable remote task bridge state transitions."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["operation", "remote_task_id"],
                        "properties": {
                            "operation": { "type": "string" },
                            "remote_task_id": { "type": "string" }
                        }
                    }),
                    required_permissions: vec!["remote.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "remote.external_agent_op".to_owned(),
                    description: "Handles external coding agent and A2A lifecycle operations."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["operation"],
                        "properties": { "operation": { "type": "string" } }
                    }),
                    required_permissions: vec!["remote.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "remote.acp_bridge_op".to_owned(),
                    description: "Handles Agent Client Protocol (ACP) bridge operations."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["operation"],
                        "properties": { "operation": { "type": "string" } }
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
            "remote.channel_op" => handle_channel_op(&payload),
            "remote.task_bridge_op" => handle_task_bridge_op(&payload),
            "remote.external_agent_op" => self.handle_external_agent_op(&payload),
            "remote.acp_bridge_op" => self.handle_acp_bridge_op(&payload),
            other => Err(ServiceContractError::not_found(format!(
                "unknown remote-a2a capability: {other}"
            ))),
        }
    }
}

fn handle_channel_op(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, ServiceContractError> {
    let operation = payload
        .get("operation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServiceContractError::invalid_input("operation is required"))?;

    match operation {
        "save" => {
            let connection_id = payload
                .get("connection_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ServiceContractError::invalid_input("connection_id is required"))?;
            let req_bytes = payload
                .get("payload_bytes")
                .and_then(|v| serde_json::from_value::<Vec<u8>>(v.clone()).ok())
                .ok_or_else(|| {
                    ServiceContractError::invalid_input("invalid_remote_channel_payload")
                })?;
            let request: RemoteChannelPayload =
                serde_json::from_slice(&req_bytes).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_remote_channel_payload")
                })?;
            let connection = request
                .connection
                .ok_or_else(|| ServiceContractError::invalid_input("connection_required"))?;
            if connection.connection_id != connection_id {
                return Err(ServiceContractError::invalid_input(
                    "connection_id_mismatch",
                ));
            }
            channels::validate_connection(&connection)
                .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
            let content_hash = channels::canonical_hash(&connection)
                .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
            let projection = serde_json::json!({
                "status": "saved",
                "connection_id": connection_id,
                "provider": connection.provider,
                "state": connection.state,
                "content_hash": content_hash,
                "redacted": true,
            });
            Ok(serde_json::json!({
                "connection": connection,
                "content_hash": content_hash,
                "projection": projection,
            }))
        }
        "hash_pairing" => {
            let req_bytes = payload
                .get("payload_bytes")
                .and_then(|v| serde_json::from_value::<Vec<u8>>(v.clone()).ok())
                .ok_or_else(|| {
                    ServiceContractError::invalid_input("invalid_remote_channel_payload")
                })?;
            let request: RemoteChannelPayload =
                serde_json::from_slice(&req_bytes).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_remote_channel_payload")
                })?;
            let code = request
                .code
                .as_deref()
                .ok_or_else(|| ServiceContractError::invalid_input("pairing_code_required"))?;
            let identity = request
                .external_identity
                .as_deref()
                .ok_or_else(|| ServiceContractError::invalid_input("external_identity_required"))?;
            let code_hash = channels::hash_pairing_code(code)
                .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
            Ok(serde_json::json!({
                "code_hash": code_hash,
                "external_identity": identity,
            }))
        }
        "admit" => {
            let existing: channels::ChannelConnection =
                serde_json::from_value(payload.get("existing_connection").cloned().ok_or_else(
                    || ServiceContractError::invalid_input("existing_connection is required"),
                )?)
                .map_err(|_| ServiceContractError::invalid_input("corrupt_channel"))?;
            let req_bytes = payload
                .get("payload_bytes")
                .and_then(|v| serde_json::from_value::<Vec<u8>>(v.clone()).ok())
                .ok_or_else(|| {
                    ServiceContractError::invalid_input("invalid_remote_channel_payload")
                })?;
            let request: RemoteChannelPayload =
                serde_json::from_slice(&req_bytes).map_err(|_| {
                    ServiceContractError::invalid_input("invalid_remote_channel_payload")
                })?;
            let message = request
                .message
                .ok_or_else(|| ServiceContractError::invalid_input("message_required"))?;
            let duplicate = payload
                .get("duplicate")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let now_ms = payload.get("now_ms").and_then(|v| v.as_i64()).unwrap_or(0);
            channels::admit_message(&existing, &message, 0, duplicate, now_ms)
                .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
            Ok(serde_json::json!({
                "projection": {
                    "status": "admitted",
                    "message_id": message.message_id,
                    "redacted": true,
                }
            }))
        }
        "revoke" => {
            let connection_id = payload
                .get("connection_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ServiceContractError::invalid_input("connection_id is required"))?;
            let mut connection: channels::ChannelConnection =
                serde_json::from_value(payload.get("existing_connection").cloned().ok_or_else(
                    || ServiceContractError::invalid_input("existing_connection is required"),
                )?)
                .map_err(|_| ServiceContractError::invalid_input("corrupt_channel"))?;
            connection.state = channels::ConnectionState::Revoked;
            connection.revision = connection.revision.saturating_add(1);
            let content_hash = channels::canonical_hash(&connection)
                .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
            Ok(serde_json::json!({
                "connection": connection,
                "content_hash": content_hash,
                "projection": {
                    "status": "revoked",
                    "connection_id": connection_id,
                    "redacted": true,
                }
            }))
        }
        _ => Err(ServiceContractError::invalid_input(
            "unsupported_remote_channel_operation",
        )),
    }
}

fn handle_task_bridge_op(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, ServiceContractError> {
    let operation = payload
        .get("operation")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServiceContractError::invalid_input("operation is required"))?;
    let remote_task_id = payload
        .get("remote_task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ServiceContractError::invalid_input("remote_task_id is required"))?;
    let now_ms = payload.get("now_ms").and_then(|v| v.as_i64()).unwrap_or(0);
    let policy = bridge::default_policy();

    match operation {
        "submit" => {
            let req_bytes = payload
                .get("payload_bytes")
                .and_then(|v| serde_json::from_value::<Vec<u8>>(v.clone()).ok())
                .ok_or_else(|| ServiceContractError::invalid_input("invalid_remote_task_submit"))?;
            let request: RemoteTaskSubmitRequest = serde_json::from_slice(&req_bytes)
                .map_err(|_| ServiceContractError::invalid_input("invalid_remote_task_submit"))?;
            let request_bytes = serde_json::to_vec(&request.request)
                .map_err(|_| ServiceContractError::invalid_input("invalid_remote_task_request"))?;
            let record = bridge::build_record(
                remote_task_id.to_owned(),
                &request.toolset,
                request.operation,
                &request_bytes,
                request.provenance_ref,
                now_ms,
                &policy,
            )
            .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
            let projection = bridge::status_projection(&record);
            let status_str = format!("{:?}", record.status);
            Ok(serde_json::json!({
                "record": record,
                "status_str": status_str,
                "projection": projection,
            }))
        }
        "status" => {
            let record: bridge::RemoteTaskRecord =
                serde_json::from_value(payload.get("existing_record").cloned().ok_or_else(
                    || ServiceContractError::invalid_input("existing_record is required"),
                )?)
                .map_err(|_| ServiceContractError::invalid_input("corrupt_remote_task"))?;
            Ok(serde_json::json!({
                "projection": bridge::status_projection(&record),
            }))
        }
        "cancel" | "poll" | "result" => {
            let mut record: bridge::RemoteTaskRecord =
                serde_json::from_value(payload.get("existing_record").cloned().ok_or_else(
                    || ServiceContractError::invalid_input("existing_record is required"),
                )?)
                .map_err(|_| ServiceContractError::invalid_input("corrupt_remote_task"))?;
            let req_bytes = payload
                .get("payload_bytes")
                .and_then(|v| serde_json::from_value::<Vec<u8>>(v.clone()).ok())
                .unwrap_or_default();
            match operation {
                "cancel" => {
                    let version = record.version;
                    bridge::cancel(&mut record, version, now_ms)
                        .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                }
                "poll" => {
                    let request: RemoteTaskPollRequest = serde_json::from_slice(&req_bytes)
                        .map_err(|_| {
                            ServiceContractError::invalid_input("invalid_remote_task_poll")
                        })?;
                    bridge::lease_for_poll(&mut record, &request.lease_owner, now_ms, &policy)
                        .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                }
                "result" => {
                    let request: RemoteTaskResultRequest = serde_json::from_slice(&req_bytes)
                        .map_err(|_| {
                            ServiceContractError::invalid_input("invalid_remote_task_result")
                        })?;
                    if !matches!(
                        request.status,
                        bridge::RemoteTaskStatus::InputRequired
                            | bridge::RemoteTaskStatus::Completed
                            | bridge::RemoteTaskStatus::Failed
                            | bridge::RemoteTaskStatus::Cancelled
                            | bridge::RemoteTaskStatus::Unknown
                    ) {
                        return Err(ServiceContractError::invalid_input(
                            "invalid_remote_task_transition",
                        ));
                    }
                    record.status = request.status;
                    record.transport_status = request.transport_status;
                    record.result_artifact_ref = request.result_artifact_ref;
                    record.version += 1;
                    record.updated_at_ms = now_ms;
                    bridge::validate_record(
                        &record,
                        &bridge::RemoteTaskToolset {
                            schema_version: 1,
                            id: record.toolset_id.clone(),
                            version: 1,
                            provider_kind: bridge::RemoteProviderKind::Mcp,
                            provider_ref: "trusted-adapter".into(),
                            operation_names: vec![record.operation.clone()],
                            content_hash: "trusted".into(),
                        },
                        &policy,
                    )
                    .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
                }
                _ => {}
            }
            bridge::refresh_content_hash(&mut record)
                .map_err(|e| ServiceContractError::invalid_input(e.to_string()))?;
            let projection = bridge::status_projection(&record);
            let status_str = format!("{:?}", record.status);
            Ok(serde_json::json!({
                "record": record,
                "status_str": status_str,
                "projection": projection,
            }))
        }
        _ => Err(ServiceContractError::invalid_input(
            "unsupported_remote_task_operation",
        )),
    }
}

impl RemoteA2aService {
    fn handle_external_agent_op(
        &self,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, ServiceContractError> {
        let operation = payload
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let request_id = payload
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let now_ms = payload.get("now_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let req_bytes: Vec<u8> = payload
            .get("payload_bytes")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let mut registry = self
            .external_agents
            .lock()
            .map_err(|_| ServiceContractError::unavailable("external_agents lock poisoned"))?;
        let mut projection = registry.status();
        let mut state = AgentState::Registered;
        let mut status = "ok";
        let mut error_code = "";
        let mut store_op: Option<serde_json::Value> = None;

        match operation {
            "list" | "status" => {}
            "a2a_status" => {
                let a2a = self
                    .a2a_runtime
                    .lock()
                    .map_err(|_| ServiceContractError::unavailable("a2a_runtime lock poisoned"))?;
                projection = serde_json::json!({
                    "protocol": "a2a",
                    "protocol_version": A2A_PROTOCOL_VERSION,
                    "profile_count": a2a.profiles.len(),
                    "card_count": a2a.cards.len(),
                    "binding_count": a2a.bindings.len(),
                    "raw_payload": false,
                    "credentials": false,
                });
            }
            "a2a_profile_upsert" => {
                let profile: RemoteAgentProfile = match serde_json::from_slice(&req_bytes) {
                    Ok(value) => value,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": operation,
                            "status": "rejected",
                            "state": AgentState::Unavailable,
                            "protocol": "a2a",
                            "error_code": "invalid_profile",
                            "projection_json": { "raw_payload": false },
                        }));
                    }
                };
                let mut a2a = self
                    .a2a_runtime
                    .lock()
                    .map_err(|_| ServiceContractError::unavailable("a2a_runtime lock poisoned"))?;
                match a2a.register_profile(profile.clone()) {
                    Ok(()) => {
                        store_op = Some(serde_json::json!({
                            "kind": "upsert_profile",
                            "profile_id": profile.id,
                            "revision": profile.revision,
                            "display_name": profile.display_name,
                            "protocol_kind": serde_json::to_string(&profile.protocol_kind).unwrap_or_default(),
                            "endpoint_origin": profile.endpoint.origin,
                            "allow_loopback": profile.endpoint.allow_loopback,
                            "allowed_ports_json": serde_json::to_string(&profile.endpoint.allowed_ports).unwrap_or_default(),
                            "auth_profile_ref": profile.auth_profile_ref,
                            "skill_allowlist_json": serde_json::to_string(&profile.skill_allowlist).unwrap_or_default(),
                            "capability_allowlist_json": serde_json::to_string(&profile.capability_allowlist).unwrap_or_default(),
                            "privacy_ceiling": serde_json::to_string(&profile.privacy_ceiling).unwrap_or_default(),
                            "concurrency_limit": profile.concurrency_limit,
                            "enabled": profile.enabled,
                            "content_hash": profile.content_hash,
                            "now_ms": now_ms,
                        }));
                        projection = serde_json::json!({
                            "protocol": "a2a",
                            "profile_id": profile.id,
                            "revision": profile.revision,
                            "content_hash": profile.content_hash,
                            "enabled": profile.enabled,
                            "raw_payload": false,
                            "credentials": false,
                        });
                    }
                    Err(_) => {
                        status = "rejected";
                        error_code = "a2a_profile_rejected";
                        state = AgentState::Unavailable;
                    }
                }
            }
            "a2a_refresh_card" => {
                #[derive(Deserialize)]
                struct RefreshCard {
                    profile_id: String,
                    card_json: String,
                    ttl_ms: i64,
                }
                let card_req: RefreshCard = match serde_json::from_slice(&req_bytes) {
                    Ok(value) => value,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": operation,
                            "status": "rejected",
                            "state": AgentState::Unavailable,
                            "protocol": "a2a",
                            "error_code": "invalid_card",
                            "projection_json": { "raw_payload": false },
                        }));
                    }
                };
                let mut a2a = self
                    .a2a_runtime
                    .lock()
                    .map_err(|_| ServiceContractError::unavailable("a2a_runtime lock poisoned"))?;
                match a2a.refresh_card(
                    &card_req.profile_id,
                    card_req.card_json.as_bytes(),
                    now_ms,
                    card_req.ttl_ms,
                ) {
                    Ok(card) => {
                        store_op = Some(serde_json::json!({
                            "kind": "insert_card_snapshot",
                            "profile_id": card.profile_id,
                            "profile_revision": card.profile_revision,
                            "normalized_hash": card.normalized_hash,
                            "raw_hash": card.raw_hash,
                            "origin": card.origin,
                            "protocol_version": card.protocol_version,
                            "fetched_at_ms": card.fetched_at_ms,
                            "expires_at_ms": card.expires_at_ms,
                            "supports_streaming": card.supports_streaming,
                            "skills_json": serde_json::to_string(&card.skills).unwrap_or_default(),
                            "schema_hashes_json": serde_json::to_string(&card.schema_hashes).unwrap_or_default(),
                            "auth_schemes_json": serde_json::to_string(&card.auth_schemes).unwrap_or_default(),
                            "now_ms": now_ms,
                        }));
                        projection = serde_json::json!({
                            "protocol": "a2a",
                            "profile_id": card.profile_id,
                            "profile_revision": card.profile_revision,
                            "normalized_hash": card.normalized_hash,
                            "raw_hash": card.raw_hash,
                            "expires_at_ms": card.expires_at_ms,
                            "supports_streaming": card.supports_streaming,
                            "raw_payload": false,
                            "credentials": false,
                        });
                    }
                    Err(_) => {
                        status = "rejected";
                        error_code = "a2a_card_rejected";
                        state = AgentState::Unavailable;
                    }
                }
            }
            "a2a_register_task" => {
                #[derive(Deserialize)]
                struct RegisterTask {
                    profile_id: String,
                    delegation_id: String,
                    parent_grant_ref: String,
                    capability: String,
                    objective: String,
                    required_privacy: PrivacyClass,
                    idempotency_key: String,
                    artifact_budget: u32,
                    capsule_id: String,
                    context_refs: Vec<String>,
                    parent_grant_hash: String,
                    policy_hash: String,
                }
                let task_req: RegisterTask = match serde_json::from_slice(&req_bytes) {
                    Ok(value) => value,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": operation,
                            "status": "rejected",
                            "state": AgentState::Unavailable,
                            "protocol": "a2a",
                            "error_code": "invalid_delegation",
                            "projection_json": { "raw_payload": false },
                        }));
                    }
                };
                let mut a2a = self
                    .a2a_runtime
                    .lock()
                    .map_err(|_| ServiceContractError::unavailable("a2a_runtime lock poisoned"))?;
                match a2a.register_task(
                    &task_req.profile_id,
                    task_req.delegation_id,
                    task_req.parent_grant_ref,
                    task_req.capability,
                    &task_req.objective,
                    task_req.required_privacy,
                    task_req.idempotency_key,
                    task_req.artifact_budget,
                    task_req.capsule_id,
                    task_req.context_refs,
                    task_req.parent_grant_hash,
                    task_req.policy_hash,
                    now_ms,
                ) {
                    Ok((req, capsule, binding)) => {
                        store_op = Some(serde_json::json!({
                            "kind": "upsert_task_binding",
                            "binding_id": binding.binding_id,
                            "delegation_id": binding.delegation_id,
                            "remote_task_id": binding.remote_task_id,
                            "state": serde_json::to_string(&binding.state).unwrap_or_default().trim_matches('"'),
                            "reconciliation": serde_json::to_string(&binding.reconciliation).unwrap_or_default().trim_matches('"'),
                            "attempt": binding.attempt,
                            "idempotency_key": binding.idempotency_key,
                            "request_hash": binding.request_hash,
                            "capsule_hash": binding.capsule_hash,
                            "policy_hash": binding.policy_hash,
                            "revision": binding.revision,
                            "created_at_ms": binding.created_at_ms,
                            "updated_at_ms": binding.updated_at_ms,
                            "content_hash": binding.content_hash,
                        }));
                        projection = serde_json::json!({
                            "protocol": "a2a",
                            "binding_id": binding.binding_id,
                            "delegation_id": binding.delegation_id,
                            "request_hash": req.request_hash,
                            "capsule_hash": capsule.content_hash,
                            "state": binding.state,
                            "raw_payload": false,
                            "credentials": false,
                        });
                    }
                    Err(_) => {
                        status = "rejected";
                        error_code = "a2a_delegation_rejected";
                        state = AgentState::Unavailable;
                    }
                }
            }
            "a2a_transition" | "a2a_reconcile" => {
                let val: serde_json::Value = match serde_json::from_slice(&req_bytes) {
                    Ok(value) => value,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": operation,
                            "status": "rejected",
                            "state": AgentState::Unavailable,
                            "protocol": "a2a",
                            "error_code": "invalid_payload",
                            "projection_json": { "raw_payload": false },
                        }));
                    }
                };
                let binding_id = val
                    .get("binding_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let expected_revision = val
                    .get("expected_revision")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_default();
                let mut a2a = self
                    .a2a_runtime
                    .lock()
                    .map_err(|_| ServiceContractError::unavailable("a2a_runtime lock poisoned"))?;
                let result = if operation == "a2a_reconcile" {
                    a2a.reconcile(
                        binding_id,
                        val.get("remote_task_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_owned(),
                        expected_revision,
                        now_ms,
                    )
                } else {
                    let next: RemoteTaskState = val
                        .get("state")
                        .cloned()
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or(RemoteTaskState::Unavailable);
                    a2a.transition(binding_id, next, expected_revision, now_ms)
                };
                match result {
                    Ok(binding) => {
                        store_op = Some(serde_json::json!({
                            "kind": "upsert_task_binding",
                            "binding_id": binding.binding_id,
                            "delegation_id": binding.delegation_id,
                            "remote_task_id": binding.remote_task_id,
                            "state": serde_json::to_string(&binding.state).unwrap_or_default().trim_matches('"'),
                            "reconciliation": serde_json::to_string(&binding.reconciliation).unwrap_or_default().trim_matches('"'),
                            "attempt": binding.attempt,
                            "idempotency_key": binding.idempotency_key,
                            "request_hash": binding.request_hash,
                            "capsule_hash": binding.capsule_hash,
                            "policy_hash": binding.policy_hash,
                            "revision": binding.revision,
                            "created_at_ms": binding.created_at_ms,
                            "updated_at_ms": binding.updated_at_ms,
                            "content_hash": binding.content_hash,
                        }));
                        projection = binding_projection(&binding);
                    }
                    Err(_) => {
                        status = "rejected";
                        error_code = "a2a_transition_rejected";
                        state = AgentState::Unavailable;
                    }
                }
            }
            "start_check" => {
                #[derive(Deserialize)]
                struct StartPayload {
                    #[serde(default)]
                    conversation_id: String,
                    #[serde(default)]
                    run_id: String,
                    #[serde(default)]
                    executable_ref: String,
                }
                let p: StartPayload = match serde_json::from_slice(&req_bytes) {
                    Ok(v) => v,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": "start",
                            "status": "rejected",
                            "state": AgentState::Unavailable,
                            "error_code": "invalid_payload",
                            "projection_json": { "raw_payload": false },
                        }));
                    }
                };
                if p.run_id.is_empty() || p.conversation_id.is_empty() {
                    status = "rejected";
                    error_code = "invalid_run";
                    state = AgentState::Unavailable;
                } else if registry.runs.contains_key(&p.run_id) {
                    status = "duplicate";
                    error_code = "duplicate_run";
                    state = *registry.runs.get(&p.run_id).unwrap_or(&AgentState::Unknown);
                } else {
                    status = "needs_supervisor";
                    projection = serde_json::json!({
                        "contract_id": CONTRACT_ID,
                        "contract_version": CONTRACT_VERSION,
                        "conversation_id": p.conversation_id,
                        "run_id": p.run_id,
                        "executable_ref": p.executable_ref,
                        "core_control_level": "supervised_opaque",
                        "raw_payload": false,
                    });
                }
            }
            "start_commit" => {
                let run_id = payload
                    .get("run_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let conversation_id = payload
                    .get("conversation_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let accepted = payload
                    .get("supervisor_accepted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if accepted {
                    registry.runs.insert(run_id.to_owned(), AgentState::Running);
                    state = AgentState::Running;
                    status = "accepted";
                } else {
                    registry
                        .runs
                        .insert(run_id.to_owned(), AgentState::Unavailable);
                    status = "unavailable";
                    error_code = "supervisor_unavailable";
                    state = AgentState::Unavailable;
                }
                projection = serde_json::json!({
                    "contract_id": CONTRACT_ID,
                    "contract_version": CONTRACT_VERSION,
                    "conversation_id": conversation_id,
                    "run_id": run_id,
                    "core_control_level": "supervised_opaque",
                    "raw_payload": false,
                });
            }
            "cancel" => {
                #[derive(Deserialize)]
                struct CancelPayload {
                    #[serde(default)]
                    run_id: String,
                }
                let p: CancelPayload = match serde_json::from_slice(&req_bytes) {
                    Ok(v) => v,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": "cancel",
                            "status": "rejected",
                            "state": AgentState::Unavailable,
                            "protocol": CONTRACT_ID,
                            "control_level": "supervised_opaque",
                            "error_code": "invalid_payload",
                            "projection_json": projection,
                        }));
                    }
                };
                if !p.run_id.is_empty() {
                    if registry
                        .runs
                        .insert(p.run_id, AgentState::Cancelling)
                        .is_none()
                    {
                        status = "not_found";
                        error_code = "run_not_found";
                    }
                } else {
                    status = "rejected";
                    error_code = "invalid_payload";
                }
            }
            _ => {
                status = "rejected";
                error_code = "unsupported_operation";
                state = AgentState::Unavailable;
            }
        }

        let mut response = serde_json::json!({
            "schema_version": 1,
            "request_id": request_id,
            "operation": operation,
            "status": status,
            "state": state,
            "protocol": CONTRACT_ID,
            "control_level": "supervised_opaque",
            "error_code": error_code,
            "projection_json": projection,
        });
        if let Some(op) = store_op {
            response["store_op"] = op;
        }
        Ok(response)
    }

    fn handle_acp_bridge_op(
        &self,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, ServiceContractError> {
        let operation = payload
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let request_id = payload
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let expected_revision = payload
            .get("expected_revision")
            .and_then(|v| v.as_u64())
            .unwrap_or_default();
        let req_bytes: Vec<u8> = payload
            .get("payload_bytes")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let mut registry = self
            .external_agents
            .lock()
            .map_err(|_| ServiceContractError::unavailable("external_agents lock poisoned"))?;
        let mut state = AcpSessionState::Ready;
        let mut status = "ok";
        let mut error_code = "";
        let mut projection = serde_json::json!({
            "raw_payload": false,
            "credentials": false,
            "backend_class": "external_agent_backend",
            "control_level": "supervised_opaque",
        });

        match operation {
            "status" | "list" => {
                projection["preset_count"] = serde_json::json!(registry.presets.len());
                projection["active_run_count"] = serde_json::json!(registry.runs.len());
                projection["protocol"] = serde_json::json!("acp");
            }
            "handshake" => {
                #[derive(Deserialize)]
                struct Handshake {
                    protocol_version: u32,
                    agent_identity: String,
                    agent_version: Option<String>,
                    capabilities: Vec<String>,
                    negotiated_at_ms: u64,
                }
                let h: Handshake = match serde_json::from_slice(&req_bytes) {
                    Ok(v) => v,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": "handshake",
                            "status": "rejected",
                            "state": AcpSessionState::Failed,
                            "error_code": "invalid_handshake",
                            "projection_json": { "raw_payload": false },
                        }));
                    }
                };
                let caps = h
                    .capabilities
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                match capability_snapshot(
                    h.protocol_version,
                    h.agent_identity,
                    h.agent_version,
                    &caps,
                    h.negotiated_at_ms,
                ) {
                    Ok(snapshot) => {
                        projection["protocol_version"] =
                            serde_json::json!(snapshot.protocol_version);
                        projection["agent_identity"] = serde_json::json!(snapshot.agent_identity);
                        projection["capability_hash"] = serde_json::json!(snapshot.content_hash);
                        projection["auth_state"] = serde_json::json!("ready");
                    }
                    Err(_) => {
                        status = "rejected";
                        state = AcpSessionState::Failed;
                        error_code = "capability_negotiation_failed";
                    }
                }
            }
            "start_check" => {
                #[derive(Deserialize)]
                struct Start {
                    session_id: String,
                    preset_id: String,
                    preset_revision: u64,
                }
                let s: Start = match serde_json::from_slice::<Start>(&req_bytes) {
                    Ok(v) if !v.session_id.is_empty() && !v.preset_id.is_empty() => v,
                    _ => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": "start",
                            "status": "rejected",
                            "state": AcpSessionState::Failed,
                            "error_code": "invalid_request",
                            "projection_json": { "raw_payload": false },
                        }));
                    }
                };
                if registry.runs.contains_key(&s.session_id) {
                    status = "duplicate";
                    error_code = "duplicate_session";
                    state = AcpSessionState::Interrupted;
                } else if expected_revision != 0 && expected_revision != s.preset_revision {
                    status = "conflict";
                    state = AcpSessionState::NeedsReview;
                    error_code = "stale_preset_revision";
                } else {
                    status = "needs_supervisor";
                    projection["session_id"] = serde_json::json!(s.session_id);
                    projection["preset_id"] = serde_json::json!(s.preset_id);
                    projection["preset_revision"] = serde_json::json!(s.preset_revision);
                }
            }
            "start_commit" => {
                let session_id = payload
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let preset_id = payload
                    .get("preset_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let preset_revision = payload
                    .get("preset_revision")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_default();
                let accepted = payload
                    .get("supervisor_accepted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if accepted {
                    registry
                        .runs
                        .insert(session_id.to_owned(), AgentState::Handshaking);
                    state = AcpSessionState::Initialize;
                    projection["session_id"] = serde_json::json!(session_id);
                    projection["preset_id"] = serde_json::json!(preset_id);
                    projection["preset_revision"] = serde_json::json!(preset_revision);
                } else {
                    status = "unavailable";
                    state = AcpSessionState::Interrupted;
                    error_code = "supervisor_unavailable";
                }
            }
            "cancel_check" => {
                #[derive(Deserialize)]
                struct Cancel {
                    session_id: String,
                }
                let c: Cancel = match serde_json::from_slice(&req_bytes) {
                    Ok(v) => v,
                    Err(_) => {
                        return Ok(serde_json::json!({
                            "schema_version": 1,
                            "request_id": request_id,
                            "operation": "cancel",
                            "status": "rejected",
                            "state": AcpSessionState::Failed,
                            "error_code": "invalid_request",
                            "projection_json": { "raw_payload": false },
                        }));
                    }
                };
                if registry.runs.contains_key(&c.session_id) {
                    status = "needs_supervisor";
                    projection["session_id"] = serde_json::json!(c.session_id);
                } else {
                    status = "not_found";
                    state = AcpSessionState::Interrupted;
                    error_code = "session_not_found";
                }
            }
            "cancel_commit" => {
                let session_id = payload
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let stopped = payload
                    .get("supervisor_accepted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if stopped {
                    registry.runs.remove(session_id);
                    state = AcpSessionState::Cancelled;
                    projection["session_id"] = serde_json::json!(session_id);
                } else {
                    status = "unavailable";
                    state = AcpSessionState::Interrupted;
                    error_code = "supervisor_unavailable";
                }
            }
            "frame_validate" => match validate_acp_frame(&req_bytes) {
                Ok(frame) => {
                    projection["message_kind"] = serde_json::json!(if frame.method.is_some() {
                        "request"
                    } else {
                        "response"
                    });
                }
                Err(_) => {
                    status = "rejected";
                    state = AcpSessionState::Failed;
                    error_code = "protocol_violation";
                }
            },
            _ => {
                status = "rejected";
                state = AcpSessionState::Failed;
                error_code = "unsupported_operation";
            }
        }

        Ok(serde_json::json!({
            "schema_version": 1,
            "request_id": request_id,
            "operation": operation,
            "status": status,
            "state": state,
            "protocol": "acp",
            "protocol_version": ACP_PROTOCOL_VERSION,
            "error_code": error_code,
            "projection_json": projection,
        }))
    }
}
