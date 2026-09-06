use super::*;

impl IpcBridge {
    pub(crate) fn dispatch_benchmark_matrix(
        &self,
        request: generated::AgentBenchmarkMatrixCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
        {
            return serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "rejected",
                "error_code": "invalid_request"
            });
        }
        match request.operation.as_str() {
            "list" => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": "list",
                "status": "ok",
                "runs": [],
                "error_code": ""
            }),
            "start" | "cancel" | "approveBaseline" => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "unavailable",
                "error_code": "benchmark_runtime_not_configured"
            }),
            _ => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "rejected",
                "error_code": "unsupported_operation"
            }),
        }
    }

    pub(crate) fn dispatch_agent_middleware_pipeline(
        &self,
        request: generated::AgentMiddlewarePipelineCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        match request.operation.as_str() {
            "list" => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"ok","contract_version":crate::agent_middleware_pipeline::CONTRACT_VERSION,"contract_id":crate::agent_middleware_pipeline::CONTRACT_ID,"runs":[],"error_code":""})
            }
            "start" => {
                use crate::agent_middleware_pipeline::{
                    AgentMiddlewarePipelineService, BuiltinPolicy, FailurePolicy, HandlerMode,
                    HookPhase, MiddlewareRequest, MiddlewareSpec, PipelineDefinition,
                    PipelineRunSnapshot, StateClass,
                };
                let payload: serde_json::Value =
                    serde_json::from_slice(&request.payload).unwrap_or_default();
                let run_id = payload["runId"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("ipc-run");
                let definition = match PipelineDefinition::new(
                    "default",
                    1,
                    vec![MiddlewareSpec {
                        id: "core-observer".into(),
                        version: 1,
                        priority: 0,
                        phases: HookPhase::ALL.to_vec(),
                        state_class: StateClass::Public,
                        policy: BuiltinPolicy::Observe,
                        mode: HandlerMode::ObserveOnly,
                        failure_policy: FailurePolicy::FailOpen,
                    }],
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"rejected","error_code":"invalid_definition"})
                    }
                };
                let snapshot = PipelineRunSnapshot {
                    run_id: run_id.into(),
                    definition_id: definition.definition_id.clone(),
                    definition_revision: definition.revision,
                    contract_hash: definition.contract_hash.clone(),
                    policy_hash: "core-policy-v1".into(),
                    capability_snapshot_hash: "core-capability-snapshot".into(),
                };
                let mut service = match AgentMiddlewarePipelineService::new(
                    definition,
                    snapshot,
                    "core-capability-snapshot",
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"rejected","error_code":"invalid_snapshot"})
                    }
                };
                let middleware_request = MiddlewareRequest {
                    run_id: run_id.into(),
                    correlation_id: request.request_id.clone(),
                    idempotency_key: request.idempotency_key.clone(),
                    phase: HookPhase::BeforeAgent,
                    input_hash: "ipc-metadata".into(),
                    capability_snapshot_hash: "core-capability-snapshot".into(),
                    intervention_depth: 0,
                };
                match service.evaluate(&middleware_request) {
                    Ok((outcome, events)) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"accepted","run_id":run_id,"outcome":outcome,"events":events,"error_code":""})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"rejected","error_code":"pipeline_validation_failed"})
                    }
                }
            }
            "cancel" => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"cancel","status":"accepted","error_code":""})
            }
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"unsupported_operation"})
            }
        }
    }

    pub(crate) fn dispatch_structured_response(
        &self,
        request: generated::StructuredResponseCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        match request.operation.as_str() {
            "list" => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"ok","run_id":"","revision":0,"contract_hash":"","strategy":"","attempts":0,"error_code":"","runs":[]})
            }
            "cancel" => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"cancel","status":"unknown","run_id":"","error_code":"no_ephemeral_run"})
            }
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unsupported","error_code":"unsupported_operation"})
            }
        }
    }

    pub(crate) fn dispatch_sensitive_data_guardrails(
        &self,
        request: generated::SensitiveDataGuardrailsCommand,
    ) -> serde_json::Value {
        if request.schema_version != crate::sensitive_data_guardrails::CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > crate::sensitive_data_guardrails::MAX_INPUT_BYTES
        {
            return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        let payload: serde_json::Value = if request.payload.is_empty() {
            serde_json::json!({})
        } else {
            match serde_json::from_slice(&request.payload) {
                Ok(value) => value,
                Err(_) => {
                    return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_payload"})
                }
            }
        };
        let destination = payload["destination"].as_str().unwrap_or("provider");
        if destination.is_empty() || destination.len() > 128 {
            return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_destination"});
        }
        let snapshot = crate::sensitive_data_guardrails::default_policy(destination);
        let metadata = if request.operation == "evaluate" {
            let Some(input) = payload["input"].as_str() else {
                return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"input_required"});
            };
            match crate::sensitive_data_guardrails::redact_text(&snapshot, input) {
                Ok(result) => result.metadata,
                Err(crate::sensitive_data_guardrails::GuardrailError::Blocked(metadata)) => {
                    metadata
                }
                Err(error) => {
                    return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":error.to_string()})
                }
            }
        } else if request.operation == "status" {
            crate::sensitive_data_guardrails::RedactionMetadata {
                contract_version: 1,
                policy_hash: snapshot.policy_hash.clone(),
                destination: destination.into(),
                action: None,
                rule_ids: Vec::new(),
                match_count: 0,
                blocked: false,
                output_bytes: 0,
            }
        } else {
            return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"unsupported_operation"});
        };
        serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"ok","policy_hash":metadata.policy_hash,"destination":metadata.destination,"action":metadata.action.map(|action| format!("{action:?}").to_ascii_lowercase()),"match_count":metadata.match_count,"blocked":metadata.blocked,"error_code":""})
    }

    pub(crate) async fn write_sensitive_data_guardrails_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value = serde_json::from_slice(&payload)?;
        let result = generated::SensitiveDataGuardrailsEvent {
            schema_version: 1,
            request_id: value["request_id"].as_str().unwrap_or_default().into(),
            operation: value["operation"].as_str().unwrap_or_default().into(),
            status: value["status"].as_str().unwrap_or_default().into(),
            policy_hash: value["policy_hash"].as_str().unwrap_or_default().into(),
            destination: value["destination"].as_str().unwrap_or_default().into(),
            action: value["action"].as_str().unwrap_or_default().into(),
            match_count: value["match_count"].as_u64().unwrap_or_default() as u32,
            blocked: value["blocked"].as_bool().unwrap_or(false),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "sensitive_data_guardrails.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::SensitiveDataGuardrails(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) fn dispatch_execution_policy_profiles(
        &self,
        request: generated::ExecutionPolicyProfilesCommand,
    ) -> serde_json::Value {
        if request.schema_version
            != evohime_tool_runtime::execution_policy_profiles::CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.operation.is_empty()
        {
            return serde_json::json!({
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "rejected",
                "error_code": "invalid_request"
            });
        }
        let resolved = match evohime_tool_runtime::ExecutionPolicyProfile::resolve("shell.execute")
        {
            Ok(value) => value,
            Err(error) => {
                return serde_json::json!({
                    "request_id": request.request_id,
                    "operation": request.operation,
                    "status": "unavailable",
                    "error_code": error.to_string()
                })
            }
        };
        if request.operation != "list"
            && request.operation != "status"
            && request.operation != "resolve"
        {
            return serde_json::json!({
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "unsupported",
                "error_code": "unsupported_operation"
            });
        }
        if !request.profile_id.is_empty() && request.profile_id != resolved.profile.profile_id {
            return serde_json::json!({
                "request_id": request.request_id,
                "operation": request.operation,
                "status": "not_found",
                "error_code": "profile_not_found"
            });
        }
        serde_json::json!({
            "request_id": request.request_id,
            "operation": request.operation,
            "status": "ok",
            "profile_id": resolved.profile.profile_id,
            "version": resolved.profile.version,
            "profile_hash": resolved.profile_hash,
            "backend": resolved.backend,
            "network_policy": "deny",
            "environment_policy": "scrubbed_allowlist",
            "timeout_ms": resolved.profile.timeout_ms,
            "max_output_bytes": resolved.profile.max_output_bytes,
            "error_code": ""
        })
    }

    pub(crate) fn dispatch_model_resilience_policy(
        &self,
        request: generated::ModelResiliencePolicyCommand,
    ) -> serde_json::Value {
        if request.schema_version != crate::model_resilience_policy::CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.operation != "status"
        {
            return serde_json::json!({"request_id": request.request_id, "operation": request.operation, "status": "rejected", "error_code": "invalid_request"});
        }
        let policy = crate::model_resilience_policy::builtin_policy();
        let hash = policy.canonical_hash().unwrap_or_default();
        serde_json::json!({
            "request_id": request.request_id,
            "operation": "status",
            "status": "ok",
            "policy_id": crate::model_resilience_policy::CONTRACT_ID,
            "policy_hash": hash,
            "attempts": policy.rules.max_attempts,
            "retries": policy.rules.max_attempts.saturating_sub(1),
            "fallbacks": if policy.rules.allow_fallback { policy.rules.max_fallbacks } else { 0 },
            "terminal_outcome": "unknown_outcome_is_not_retried",
            "error_code": "",
            "projection_json": {"schema_version": 1, "ephemeral": true, "raw_payload": false, "credentials": false}
        })
    }

    pub(crate) async fn dispatch_execution_backend_registry(
        &self,
        request: generated::ExecutionBackendRegistryCommand,
    ) -> serde_json::Value {
        use crate::execution_backend_registry::{
            BackendDefinition, BackendKind, HealthState, Registry,
        };
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        let payload: ExecutionBackendPayload = if request.payload.is_empty() {
            ExecutionBackendPayload::default()
        } else {
            match serde_json::from_slice::<ExecutionBackendPayload>(&request.payload) {
                Ok(v) => v,
                Err(_) => {
                    return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_payload"})
                }
            }
        };
        let database = self.journal.database().lock().await;
        let rows = match evohime_local_storage::execution_backend_registry_store::list(
            database.connection(),
        ) {
            Ok(v) => v,
            Err(_) => {
                return serde_json::json!({"request_id":request.request_id,"operation":request.operation,"status":"unavailable","error_code":"storage_unavailable"})
            }
        };
        let mut registry = Registry::default();
        for row in rows.into_iter().filter(|row| row.id != "local.core") {
            let capabilities = serde_json::from_str(&row.capabilities_json).unwrap_or_default();
            let _ = registry.register(
                BackendDefinition {
                    id: row.id,
                    kind: if row.kind == "remote" {
                        BackendKind::Remote
                    } else {
                        BackendKind::Local
                    },
                    endpoint: row.endpoint,
                    auth_ref: row.auth_ref,
                    enabled: row.health != "disabled",
                    capabilities,
                    version: row.version as u64,
                    health: if row.health == "disabled" {
                        HealthState::Disabled
                    } else {
                        HealthState::Registered
                    },
                    health_failure: None,
                },
                registry.version(),
            );
        }
        if let Ok(Some(default_id)) =
            evohime_local_storage::execution_backend_registry_store::default_id(
                database.connection(),
            )
        {
            let _ = registry.set_default(&default_id, registry.version());
        }
        let outcome = match request.operation.as_str() {
            "list" => {
                serde_json::json!({"status":"ok","registry_version":registry.version(),"default_backend_id":registry.default_id(),"backends":registry.entries().map(|b| serde_json::json!({"id":b.id,"kind":b.kind,"enabled":b.enabled,"health":b.health,"capability_count":b.capabilities.len(),"has_auth_ref":b.auth_ref.is_some()})).collect::<Vec<_>>()})
            }
            "register" => {
                let id = payload.id.clone();
                let kind = if payload.kind == "remote" {
                    BackendKind::Remote
                } else {
                    BackendKind::Local
                };
                let backend = BackendDefinition {
                    id,
                    kind,
                    endpoint: payload.endpoint.clone(),
                    auth_ref: payload.auth_ref.clone(),
                    enabled: true,
                    capabilities: payload.capabilities.clone(),
                    version: 0,
                    health: HealthState::Registered,
                    health_failure: None,
                };
                match registry.register(backend.clone(), request.expected_version.max(1)) {
                    Ok(()) => {
                        let kind_s = if matches!(backend.kind, BackendKind::Remote) {
                            "remote"
                        } else {
                            "local"
                        };
                        let caps = match serde_json::to_string(&backend.capabilities) {
                            Ok(value) => value,
                            Err(error) => {
                                tracing::warn!(backend_id = %backend.id, %error, "failed to serialize backend capabilities");
                                "[]".into()
                            }
                        };
                        let _ = evohime_local_storage::execution_backend_registry_store::upsert(
                            database.connection(),
                            evohime_local_storage::execution_backend_registry_store::UpsertInput {
                                id: &backend.id,
                                kind: kind_s,
                                endpoint: backend.endpoint.as_deref(),
                                auth_ref: backend.auth_ref.as_deref(),
                                capabilities_json: &caps,
                                version: registry.version(),
                                health: "registered",
                                now_ms: crate::task_memory::now_millis() as i64,
                            },
                        );
                        serde_json::json!({"status":"ok","registry_version":registry.version()})
                    }
                    Err(e) => serde_json::json!({"status":"rejected","error_code":e.to_string()}),
                }
            }
            "handshake" => {
                let id = payload.backend_id.as_str();
                let hs = crate::execution_backend_registry::CapabilityHandshake {
                    protocol_major: payload.protocol_major,
                    protocol_minor: payload.protocol_minor,
                    backend_id: id.into(),
                    capabilities: payload.capabilities.clone(),
                    capability_hash: payload.capability_hash.clone(),
                };
                match registry.handshake(
                    id,
                    hs,
                    &["agent.execute".into(), "workflow.execute".into()],
                ) {
                    Ok(snapshot) => serde_json::json!({"status":"ok","snapshot":snapshot}),
                    Err(e) => {
                        serde_json::json!({"status":"unavailable","error_code":e.to_string()})
                    }
                }
            }
            "remove" => {
                let id = payload.id.as_str();
                if id.is_empty() || id == "local.core" {
                    serde_json::json!({"status":"rejected","error_code":"local_backend_required"})
                } else if registry.remove(id, registry.version()).is_err() {
                    serde_json::json!({"status":"not_found","error_code":"not_found"})
                } else {
                    let _ = evohime_local_storage::execution_backend_registry_store::remove(
                        database.connection(),
                        id,
                    );
                    serde_json::json!({"status":"ok","registry_version":registry.version()})
                }
            }
            "set_default" => {
                let id = payload.id.as_str();
                match registry.set_default(id, registry.version()) {
                    Ok(()) => {
                        let _ =
                            evohime_local_storage::execution_backend_registry_store::set_default(
                                database.connection(),
                                id,
                            );
                        serde_json::json!({"status":"ok","registry_version":registry.version(),"default_backend_id":id})
                    }
                    Err(e) => serde_json::json!({"status":"rejected","error_code":e.to_string()}),
                }
            }
            "disable" => {
                let id = payload.id.as_str();
                if id == "local.core" {
                    serde_json::json!({"status":"rejected","error_code":"local_backend_required"})
                } else if evohime_local_storage::execution_backend_registry_store::set_enabled(
                    database.connection(),
                    id,
                    false,
                )
                .unwrap_or(false)
                {
                    serde_json::json!({"status":"ok","registry_version":registry.version()})
                } else {
                    serde_json::json!({"status":"not_found","error_code":"not_found"})
                }
            }
            "snapshot" => {
                let id = if payload.backend_id.is_empty() {
                    registry.default_id()
                } else {
                    payload.backend_id.as_str()
                };
                if registry.entries().any(|b| b.id == id) {
                    serde_json::json!({"status":"ok","snapshot":{"backend_id":id,"registry_version":registry.version(),"handshake_hash":"pending","policy_hash":"core-policy-v1"}})
                } else {
                    serde_json::json!({"status":"not_found","error_code":"not_found"})
                }
            }
            _ => serde_json::json!({"status":"rejected","error_code":"unsupported_operation"}),
        };
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"registry_version":outcome["registry_version"].as_u64().unwrap_or(registry.version()),"projection_json":outcome,"error_code":outcome["error_code"].as_str().unwrap_or("")})
    }

    pub(crate) async fn dispatch_tool_simulation_runtime(
        &self,
        request: generated::ToolSimulationRuntimeCommand,
    ) -> serde_json::Value {
        if request.schema_version != crate::tool_simulation_runtime::CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request","projection_json":{"ephemeral":true,"raw_payload":false}});
        }
        let runtime = self.tool_simulation.lock().await;
        match request.operation.as_str() {
            "status" => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": "status",
                "status": "ok",
                "mode": "dry_run",
                "state": "ready",
                "provenance": "synthetic_or_fixture",
                "projection_json": {"contract_id": crate::tool_simulation_runtime::CONTRACT_ID, "contract_version": 1, "ephemeral": true, "fixture_count": runtime.fixture_count(), "completed_count": runtime.completed_count(), "real_fallback": false, "raw_payload": false},
                "error_code": ""
            }),
            "run" => serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "operation": "run",
                "status": "unavailable",
                "mode": "dry_run",
                "state": "blocked",
                "provenance": "synthetic_or_fixture",
                "projection_json": {"ephemeral": true, "raw_payload": false, "real_fallback": false},
                "error_code": "payload_not_admitted"
            }),
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"unsupported_operation","projection_json":{"real_fallback":false}})
            }
        }
    }

    pub(crate) async fn write_tool_simulation_runtime_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value = serde_json::from_slice(&payload)?;
        let projection = serde_json::to_vec(&value["projection_json"])?;
        let result = generated::ToolSimulationRuntimeEvent {
            schema_version: 1,
            request_id: value["request_id"].as_str().unwrap_or_default().into(),
            operation: value["operation"].as_str().unwrap_or_default().into(),
            status: value["status"].as_str().unwrap_or_default().into(),
            mode: value["mode"].as_str().unwrap_or_default().into(),
            state: value["state"].as_str().unwrap_or_default().into(),
            provenance: value["provenance"].as_str().unwrap_or_default().into(),
            run_id: value["run_id"].as_str().unwrap_or_default().into(),
            correlation_id: value["correlation_id"].as_str().unwrap_or_default().into(),
            contract_hash: value["contract_hash"].as_str().unwrap_or_default().into(),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
            projection_json: projection,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "tool_simulation_runtime.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::ToolSimulationRuntime(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

}
