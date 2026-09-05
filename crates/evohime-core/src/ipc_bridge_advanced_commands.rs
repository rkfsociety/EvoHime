impl IpcBridge {
    pub(crate) fn dispatch_integration_provider_sdk(
        &self,
        request: generated::IntegrationProviderSdkCommand,
    ) -> serde_json::Value {
        let operation = request.operation.as_str();
        if operation == "list_catalog" || operation == "get_provider" {
            return serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "status": "ok",
                "operation": operation,
                "providers": [crate::integration_provider_sdk::fixture_echo_manifest()],
                "error_code": "",
            });
        }
        if operation == "invoke_fixture" {
            let input = serde_json::from_slice(&request.payload).unwrap_or(serde_json::Value::Null);
            let result =
                crate::integration_provider_runtime::invoke_fixture("fixture.echo", "echo", input);
            return serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "status": "ok",
                "operation": operation,
                "result": result,
                "error_code": "",
            });
        }
        serde_json::json!({
            "schema_version": 1,
            "request_id": request.request_id,
            "status": "unavailable",
            "operation": operation,
            "error_code": "provider_adapter_unavailable",
        })
    }

    pub(crate) fn dispatch_event_trigger_runtime(
        &self,
        request: generated::EventTriggerRuntimeCommand,
    ) -> serde_json::Value {
        let operation = request.operation.as_str();
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":operation,"status":"rejected","error_code":"invalid_request"});
        }
        match operation {
            "list" | "get" => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "ok", "triggers": [], "mvp_sources": ["local_workspace_event", "system_event"],
                "provider_webhook": "unavailable", "error_code": ""
            }),
            "reconcile" | "pause" | "resume" => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "unavailable", "error_code": "no_trigger_configured"
            }),
            _ => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "unavailable", "error_code": "unsupported_operation"
            }),
        }
    }

    pub(crate) async fn dispatch_invocation_preset(
        &self,
        request: generated::InvocationPresetCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        if request.operation == "run" {
            let envelope: InvocationPresetRunPayload = match serde_json::from_slice(
                &request.payload,
            ) {
                Ok(value) => value,
                Err(_) => {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"rejected","error_code":"invalid_payload"});
                }
            };
            let preset_id = envelope.preset_id.as_str();
            let revision = envelope.revision;
            let workspace = envelope.workspace_path;
            let idempotency = if request.idempotency_key.is_empty() {
                request.request_id.clone()
            } else {
                request.idempotency_key.clone()
            };
            let mut preset = {
                let database = self.journal.database().lock().await;
                let Some((content, stored_hash, state)) =
                    evohime_local_storage::invocation_presets_store::read_revision(
                        database.connection(),
                        &request.owner_scope,
                        preset_id,
                        revision,
                    )
                    .ok()
                    .flatten()
                else {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"rejected","error_code":"unknown_preset_revision"});
                };
                if state != "ready" {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"blocked","error_code":"needs_rebinding_or_migration"});
                }
                let Ok(preset) =
                    serde_json::from_str::<crate::invocation_presets::InvocationPreset>(&content)
                else {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"rejected","error_code":"corrupt_preset"});
                };
                if preset.content_hash != stored_hash
                    || preset.canonical_content_hash() != stored_hash
                {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"rejected","error_code":"preset_hash_mismatch"});
                }
                preset
            };
            for (key, value) in envelope.temporary_overrides {
                if preset.input_values.contains_key(&key) {
                    preset.input_values.insert(key, value);
                }
            }
            return match self
                .start_invocation_preset(preset, workspace, idempotency)
                .await
            {
                Ok(run_id) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"started","run_id":run_id,"error_code":""})
                }
                Err(error) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"run","status":"blocked","error_code":error})
                }
            };
        }
        let database = self.journal.database().lock().await;
        let connection = database.connection();
        match request.operation.as_str() {
            "list" => {
                let mut statement = match connection.prepare("SELECT id, revision, content_hash, state FROM invocation_presets WHERE owner_scope=?1 ORDER BY id, revision DESC LIMIT ?2") { Ok(statement) => statement, Err(_) => return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","error_code":"storage_error"}) };
                let limit = if request.expected_revision == 0 {
                    50
                } else {
                    request.expected_revision.min(100)
                };
                let rows = statement.query_map(rusqlite::params![request.owner_scope, limit as i64], |row| Ok(serde_json::json!({"id":row.get::<_,String>(0)?,"revision":row.get::<_,i64>(1)? as u64,"content_hash":row.get::<_,String>(2)?,"state":row.get::<_,String>(3)?}))).and_then(|rows| rows.collect::<Result<Vec<_>, _>>());
                match rows {
                    Ok(presets) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"ok","presets":presets,"error_code":""})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"error","presets":[],"error_code":"storage_error"})
                    }
                }
            }
            "create" | "save" => {
                let mut preset: crate::invocation_presets::InvocationPreset =
                    match serde_json::from_slice(&request.payload) {
                        Ok(value) => value,
                        Err(_) => {
                            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":"invalid_payload"})
                        }
                    };
                if preset.owner_scope != request.owner_scope {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":"owner_scope_mismatch"});
                }
                if let Err(error) = preset.validate() {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":error.to_string()});
                }
                preset.content_hash = preset.canonical_content_hash();
                let content = serde_json::to_string(&preset).unwrap_or_default();
                let state = serde_json::to_value(preset.state)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("ready")
                    .to_string();
                match evohime_local_storage::invocation_presets_store::save_revision(
                    connection,
                    evohime_local_storage::invocation_presets_store::SaveRevisionInput {
                        owner_scope: &preset.owner_scope,
                        id: &preset.id,
                        revision: preset.revision,
                        content_json: &content,
                        content_hash: &preset.content_hash,
                        state: &state,
                        now_ms: crate::task_memory::now_millis() as i64,
                    },
                ) {
                    Ok(true) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"saved","preset_id":preset.id,"revision":preset.revision,"content_hash":preset.content_hash,"error_code":""})
                    }
                    Ok(false) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"conflict","preset_id":preset.id,"revision":preset.revision,"error_code":"duplicate_revision"})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"error","error_code":"storage_error"})
                    }
                }
            }
            "sanitize" => match crate::invocation_presets::sanitize_completed_run(
                &serde_json::from_slice(&request.payload).unwrap_or_default(),
            ) {
                Ok(preview) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"sanitize","status":"preview","preview":preview,"error_code":""})
                }
                Err(error) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"sanitize","status":"rejected","error_code":error.to_string()})
                }
            },
            "preview_migration" | "migrate" => {
                let envelope: serde_json::Value =
                    serde_json::from_slice(&request.payload).unwrap_or_default();
                let preset_id = envelope
                    .get("preset_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let source_revision = envelope
                    .get("source_revision")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let migration: crate::invocation_presets::PresetMigrationRequest =
                    match serde_json::from_value(
                        envelope.get("migration").cloned().unwrap_or_default(),
                    ) {
                        Ok(value) => value,
                        Err(_) => {
                            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_migration"})
                        }
                    };
                let Some((content, stored_hash, _state)) =
                    evohime_local_storage::invocation_presets_store::read_revision(
                        connection,
                        &request.owner_scope,
                        preset_id,
                        source_revision,
                    )
                    .ok()
                    .flatten()
                else {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"unknown_preset_revision"});
                };
                let source: crate::invocation_presets::InvocationPreset = match serde_json::from_str(
                    &content,
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"corrupt_preset"})
                    }
                };
                if source.content_hash != stored_hash
                    || source.canonical_content_hash() != stored_hash
                    || migration.source_revision != source_revision
                {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"preset_hash_mismatch"});
                }
                if request.operation == "preview_migration" {
                    return match crate::invocation_presets::preview_migration(&source, &migration) {
                        Ok(preview) => {
                            serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"preview_migration","status":"preview","preview":preview,"error_code":""})
                        }
                        Err(error) => {
                            serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"preview_migration","status":"rejected","error_code":error.to_string()})
                        }
                    };
                }
                let migrated = match crate::invocation_presets::migrate_preset(
                    &source,
                    &migration,
                    crate::task_memory::now_millis() as i64,
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"migrate","status":"rejected","error_code":error.to_string()})
                    }
                };
                let content = serde_json::to_string(&migrated).unwrap_or_default();
                let state = serde_json::to_value(migrated.state)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("ready")
                    .to_string();
                match evohime_local_storage::invocation_presets_store::save_revision(
                    connection,
                    evohime_local_storage::invocation_presets_store::SaveRevisionInput {
                        owner_scope: &migrated.owner_scope,
                        id: &migrated.id,
                        revision: migrated.revision,
                        content_json: &content,
                        content_hash: &migrated.content_hash,
                        state: &state,
                        now_ms: migrated.updated_at_ms,
                    },
                ) {
                    Ok(true) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"migrate","status":"migrated","preset_id":migrated.id,"revision":migrated.revision,"content_hash":migrated.content_hash,"error_code":""})
                    }
                    Ok(false) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"migrate","status":"conflict","error_code":"duplicate_revision"})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"migrate","status":"error","error_code":"storage_error"})
                    }
                }
            }
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unavailable","error_code":"unsupported_operation"})
            }
        }
    }

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

    pub(crate) async fn dispatch_agent_role_profiles(
        &self,
        request: generated::AgentRoleProfilesCommand,
    ) -> serde_json::Value {
        use crate::agent_role_profiles::{
            canonical_hash, AgentRoleProfile, RoleProfileError, CONTRACT_VERSION,
        };
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","state":"failed","error_code":"invalid_request","projection_json":{"raw_prompt":false,"credentials":false}});
        }
        let mut registry = self.role_profiles.lock().await;
        if request.operation == "list" && registry.profiles.is_empty() {
            if let Ok(database) = self.journal.database().try_lock() {
                if let Ok(rows) = evohime_local_storage::agent_role_profiles_store::load_all_json(
                    database.connection(),
                ) {
                    for row in rows {
                        if let Ok(profile) = serde_json::from_slice::<
                            crate::agent_role_profiles::AgentRoleProfile,
                        >(&row)
                        {
                            registry.profiles.insert(profile.id.clone(), profile);
                        }
                    }
                }
            }
        }
        let mut status = "ok";
        let mut error_code = String::new();
        let mut profile_id = String::new();
        let mut revision = 0_u64;
        let mut state = "pinned";
        let mut projection = serde_json::json!({"schema_version":1,"profile_count":registry.profiles.len(),"raw_prompt":false,"credentials":false,"executable_code":false});
        let result: Result<(), RoleProfileError> = (|| match request.operation.as_str() {
            "list" => Ok(()),
            "get" => {
                let payload: serde_json::Value =
                    serde_json::from_slice(&request.payload).unwrap_or_default();
                profile_id = payload
                    .get("profile_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned();
                if let Some(profile) = registry.profiles.get(&profile_id) {
                    revision = profile.revision;
                    projection = serde_json::json!({"schema_version":1,"profile_id":profile.id,"revision":profile.revision,"content_hash":canonical_hash(profile).unwrap_or_default(),"execution_mode":profile.execution_mode,"raw_prompt":false,"credentials":false});
                    Ok(())
                } else {
                    Err(RoleProfileError::NotFound)
                }
            }
            "create" | "revise" => {
                let profile: AgentRoleProfile = serde_json::from_slice(&request.payload)
                    .map_err(|_| RoleProfileError::Invalid("payload"))?;
                profile_id = profile.id.clone();
                revision = profile.revision;
                let saved = if request.operation == "create" {
                    registry.create(profile.clone(), &request.idempotency_key)?
                } else {
                    registry.revise(
                        profile.clone(),
                        request.expected_revision,
                        &request.idempotency_key,
                    )?
                };
                let hash = canonical_hash(&saved)?;
                if let Ok(database) = self.journal.database().try_lock() {
                    let json = serde_json::to_vec(&saved).unwrap_or_default();
                    let _ = evohime_local_storage::agent_role_profiles_store::save_revision(
                        database.connection(),
                        &saved.id,
                        saved.revision,
                        &hash,
                        &json,
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
                projection = serde_json::json!({"schema_version":1,"profile_id":saved.id,"revision":saved.revision,"content_hash":hash,"execution_mode":saved.execution_mode,"raw_prompt":false,"credentials":false});
                Ok(())
            }
            "start" => {
                let payload: AgentRoleRuntimePayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| RoleProfileError::Invalid("payload"))?;
                let run_id = payload.run_id;
                profile_id = payload.profile_id;
                revision = payload.revision;
                let grants = payload.requested_grants;
                let allowed = vec![
                    "workspace.read".to_owned(),
                    "test.execute".to_owned(),
                    "review".to_owned(),
                ];
                let run = registry.start(crate::agent_role_profiles::StartRuntimeInput {
                    run_id,
                    profile_id: &profile_id,
                    revision,
                    grants,
                    parent: &allowed,
                    policy: &allowed,
                    registry: &allowed,
                })?;
                state = "pinned";
                projection = serde_json::json!({"schema_version":1,"profile_id":run.snapshot.profile_id,"revision":run.snapshot.revision,"content_hash":run.snapshot.content_hash,"run_id":run.run_id,"effective_grants":run.effective_grants,"state":run.state,"raw_prompt":false,"credentials":false});
                Ok(())
            }
            "cancel" => {
                let payload: AgentRoleRuntimePayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| RoleProfileError::Invalid("payload"))?;
                let run = registry.cancel(&payload.run_id)?;
                state = "cancelling";
                profile_id = run.snapshot.profile_id;
                revision = run.snapshot.revision;
                projection = serde_json::json!({"schema_version":1,"run_id":run.run_id,"state":run.state,"profile_id":profile_id,"revision":revision,"raw_prompt":false,"credentials":false});
                Ok(())
            }
            _ => Err(RoleProfileError::Invalid("unsupported_operation")),
        })();
        if let Err(error) = result {
            status = "rejected";
            error_code = error.to_string();
            state = "failed";
        }
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":status,"profile_id":profile_id,"revision":revision,"state":state,"error_code":error_code,"projection_json":projection})
    }

    pub(crate) async fn write_agent_role_profiles_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: IpcResponseFields = serde_json::from_slice(&payload)?;
        let result = generated::AgentRoleProfilesEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            profile_id: value.profile_id,
            revision: value.revision,
            content_hash: value.projection_json["content_hash"]
                .as_str()
                .unwrap_or_default()
                .into(),
            state: value.state,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&value.projection_json)?,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "agent_role_profiles.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::AgentRoleProfiles(result)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn dispatch_conversation_event_log(
        &self,
        request: generated::ConversationEventLogRequest,
        operation: &str,
    ) -> generated::ConversationEventLogEvent {
        let limit = if request.limit == 0 {
            100
        } else {
            request.limit as usize
        };
        if request.schema_version != crate::conversation_event_log::CONTRACT_VERSION {
            return conversation_event_log_error(
                operation,
                &request.conversation_id,
                "event_schema_unsupported",
            );
        }
        if operation == "subscribed" {
            *self.conversation_subscription.lock().await = Some((
                request.conversation_id.clone(),
                request.kinds_filter.iter().cloned().collect(),
            ));
        }
        let invalid = request.conversation_id.is_empty()
            || request.conversation_id.len() > 128
            || limit > evohime_local_storage::conversation_event_log_store::MAX_PAGE_EVENTS
            || request.kinds_filter.len() > 16
            || request
                .kinds_filter
                .iter()
                .any(|kind| kind.is_empty() || kind.len() > 96)
            || (request.use_before_sequence && request.use_after_sequence)
            || (request.use_before_sequence && request.before_sequence == 0);
        if invalid {
            return conversation_event_log_error(
                operation,
                &request.conversation_id,
                "invalid_argument",
            );
        }
        let page = if request.use_before_sequence {
            self.journal
                .conversation_history_before(
                    &request.conversation_id,
                    request.before_sequence,
                    limit,
                )
                .await
        } else if request.use_after_sequence {
            self.journal
                .conversation_history_after(&request.conversation_id, request.after_sequence, limit)
                .await
        } else {
            self.journal
                .conversation_history_before(&request.conversation_id, u64::MAX, limit)
                .await
        };
        let page = match page {
            Ok(page) => page,
            Err(StorageError::ConversationEventLog(
                evohime_local_storage::conversation_event_log_store::ConversationStoreError::CursorExpired {
                    earliest_available_sequence,
                },
            )) => return conversation_event_log_error_with_earliest(
                operation,
                &request.conversation_id,
                "cursor_expired",
                earliest_available_sequence,
            ),
            Err(StorageError::ConversationEventLog(
                evohime_local_storage::conversation_event_log_store::ConversationStoreError::ConversationNotFound,
            )) => return conversation_event_log_error(operation, &request.conversation_id, "conversation_not_found"),
            Err(_) => return conversation_event_log_error(operation, &request.conversation_id, "history_unavailable"),
        };
        let filters = request
            .kinds_filter
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        let events = page
            .events
            .into_iter()
            .filter_map(|event| {
                if !filters.is_empty() && !filters.contains(&event.kind) {
                    return None;
                }
                crate::conversation_event_log::renderer_event(&event)
                    .ok()
                    .map(conversation_event_projection)
            })
            .collect::<Vec<_>>();
        generated::ConversationEventLogEvent {
            schema_version: crate::conversation_event_log::CONTRACT_VERSION,
            operation: operation.into(),
            conversation_id: request.conversation_id,
            oldest_sequence: page.oldest_sequence.unwrap_or(0),
            newest_sequence: page.newest_sequence.unwrap_or(0),
            has_older: page.has_older,
            has_newer: page.has_newer,
            earliest_available_sequence: page.earliest_available_sequence,
            error_code: String::new(),
            events,
        }
    }

    pub(crate) async fn write_conversation_event_log_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::ConversationEventLogEvent,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: format!("conversation.{}", result.operation),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ConversationEventLog(
                result,
            )),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn dispatch_conversation_workbench(
        &self,
        request: generated::ConversationWorkbenchRequest,
    ) -> generated::ConversationWorkbenchEvent {
        let error = |code: &'static str| generated::ConversationWorkbenchEvent {
            schema_version: crate::conversation_workbench::CONTRACT_VERSION,
            request_id: request.request_id.clone(),
            operation: "get".into(),
            conversation_id: request.conversation_id.clone(),
            event_cursor: 0,
            status: "rejected".into(),
            error_code: code.into(),
            projection_json: Vec::new(),
        };
        if request.schema_version != crate::conversation_workbench::CONTRACT_VERSION
            || request.request_id.is_empty()
            || crate::conversation_workbench::validate_scope(
                &request.conversation_id,
                &request.workspace_id,
                &request.run_id,
                &request.backend_snapshot_hash,
                &request.capability_snapshot_hash,
                request.after_sequence,
                request.limit as usize,
            )
            .is_err()
        {
            return error("invalid_request");
        }
        let limit = request.limit as usize;
        let page = if request.after_sequence == 0 {
            self.journal
                .conversation_history_before(&request.conversation_id, u64::MAX, limit)
                .await
        } else {
            self.journal
                .conversation_history_after(&request.conversation_id, request.after_sequence, limit)
                .await
        };
        let page = match page {
            Ok(page) => page,
            Err(StorageError::ConversationEventLog(
                evohime_local_storage::conversation_event_log_store::ConversationStoreError::CursorExpired { .. },
            )) => return error("cursor_expired"),
            Err(StorageError::ConversationEventLog(
                evohime_local_storage::conversation_event_log_store::ConversationStoreError::ConversationNotFound,
            )) => return error("conversation_not_found"),
            Err(_) => return error("projection_unavailable"),
        };
        let projection = crate::conversation_workbench::build_projection(
            request.conversation_id.clone(),
            request.workspace_id,
            request.run_id,
            request.backend_snapshot_hash,
            request.capability_snapshot_hash,
            page.newest_sequence.unwrap_or(request.after_sequence),
            &page.events,
        );
        let projection_json = match serde_json::to_vec(&projection) {
            Ok(value) if value.len() <= crate::conversation_workbench::MAX_PROJECTION_BYTES => {
                value
            }
            _ => return error("projection_too_large"),
        };
        generated::ConversationWorkbenchEvent {
            schema_version: crate::conversation_workbench::CONTRACT_VERSION,
            request_id: request.request_id,
            operation: "get".into(),
            conversation_id: projection.conversation_id,
            event_cursor: projection.event_cursor,
            status: "ok".into(),
            error_code: String::new(),
            projection_json,
        }
    }

    pub(crate) async fn write_conversation_workbench_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::ConversationWorkbenchEvent,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: "conversation.workbench".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ConversationWorkbench(
                result,
            )),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    pub(crate) async fn dispatch_causal_collaboration_bus(
        &self,
        request: generated::CausalCollaborationBusCommand,
    ) -> serde_json::Value {
        use crate::causal_collaboration_bus::{
            validate, Address, CollaborationMessage, DeliveryState, MessageKind, Sensitivity,
            CONTRACT_VERSION,
        };
        let base = |status: &str, code: &str, projection: serde_json::Value| serde_json::json!({"schema_version": CONTRACT_VERSION, "request_id": request.request_id, "operation": request.operation, "status": status, "error_code": code, "version": 0, "projection_json": projection});
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.correlation_id.is_empty()
            || request.payload.len() > crate::causal_collaboration_bus::MAX_PAYLOAD_BYTES
        {
            return base(
                "rejected",
                "invalid_request",
                serde_json::json!({"raw_payload":false}),
            );
        }
        if request.operation == "list" || request.operation == "reconcile" {
            let database = self.journal.database().lock().await;
            if request.operation == "reconcile" {
                let _ =
                    evohime_local_storage::collaboration_store::reconcile(database.connection());
            }
            let messages =
                evohime_local_storage::collaboration_store::list::<CollaborationMessage>(
                    database.connection(),
                    &request.owner_scope,
                    128,
                )
                .unwrap_or_default();
            return base(
                "ok",
                "",
                serde_json::json!({"session_id":request.owner_scope,"count":messages.len(),"messages":messages.iter().map(|m| serde_json::json!({"message_id":m.message_id,"kind":m.kind,"sender":m.sender,"receiver":m.receiver,"sequence":m.sequence,"payload_hash":m.payload_hash,"sensitivity":m.sensitivity,"provenance_id":m.provenance_id,"delivery":DeliveryState::Queued})).collect::<Vec<_>>(),"raw_payload":false}),
            );
        }
        if request.operation != "publish" {
            return base(
                "unavailable",
                "unsupported_operation",
                serde_json::json!({"raw_payload":false}),
            );
        }
        let mut message: CollaborationMessage = match serde_json::from_slice(&request.payload) {
            Ok(v) => v,
            Err(_) => {
                return base(
                    "rejected",
                    "invalid_payload",
                    serde_json::json!({"raw_payload":false}),
                )
            }
        };
        message.session_id = request.owner_scope.clone();
        message.idempotency_key = request.idempotency_key.clone();
        message.correlation_id = request.correlation_id.clone();
        message.sender = Address::Parent;
        if matches!(
            message.kind,
            MessageKind::Progress
                | MessageKind::Notice
                | MessageKind::ArtifactRef
                | MessageKind::Request
                | MessageKind::Response
        ) && message.sensitivity != Sensitivity::Secret
        {
        } else {
            return base(
                "rejected",
                "invalid_message",
                serde_json::json!({"raw_payload":false}),
            );
        }
        if let Err(error) = validate(&message) {
            return base(
                "rejected",
                &error.to_string(),
                serde_json::json!({"raw_payload":false}),
            );
        }
        {
            let team = self.team_sop.lock().await;
            let Some(session) = team.sessions.get(&message.session_id) else {
                return base(
                    "rejected",
                    "destination_forbidden",
                    serde_json::json!({"raw_payload":false}),
                );
            };
            if !matches!(
                session.status,
                crate::team_sop_protocols::SessionStatus::Pinned
                    | crate::team_sop_protocols::SessionStatus::Running
                    | crate::team_sop_protocols::SessionStatus::Paused
            ) || session.snapshot.content_hash != message.protocol_hash
            {
                return base(
                    "rejected",
                    "destination_forbidden",
                    serde_json::json!({"raw_payload":false}),
                );
            }
            if let Address::RoleSlot { slot_id } | Address::DirectRoleInstance { slot_id, .. } =
                &message.receiver
            {
                let Some(slot) = serde_json::from_slice::<crate::team_sop_protocols::TeamProtocol>(
                    &session.snapshot.protocol_json,
                )
                .ok()
                .and_then(|p| {
                    p.participants
                        .into_iter()
                        .find(|slot| slot.slot_id == *slot_id)
                }) else {
                    return base(
                        "rejected",
                        "destination_forbidden",
                        serde_json::json!({"raw_payload":false}),
                    );
                };
                if !slot.allowed_peer_routes.is_empty()
                    && !slot
                        .allowed_peer_routes
                        .iter()
                        .any(|route| route == "parent" || route == "*")
                {
                    return base(
                        "rejected",
                        "destination_forbidden",
                        serde_json::json!({"raw_payload":false}),
                    );
                }
            }
        }
        let mut database = self.journal.database().lock().await;
        if evohime_local_storage::collaboration_store::exists(
            database.connection(),
            &request.idempotency_key,
        )
        .unwrap_or(false)
        {
            return base(
                "ok",
                "duplicate",
                serde_json::json!({"session_id":message.session_id,"message_id":message.message_id,"deduplicated":true,"raw_payload":false}),
            );
        }
        let sequence = match evohime_local_storage::retained_child_store::RetainedChildStore::next_parent_sequence(database.connection_mut(), &message.session_id) { Ok(v)=>v, Err(_)=>return base("unavailable","storage_failed",serde_json::json!({"raw_payload":false})) };
        message.sequence = sequence;
        let sender = message.sender.clone();
        let receiver = message.receiver.clone();
        match evohime_local_storage::collaboration_store::enqueue(
            database.connection_mut(),
            evohime_local_storage::collaboration_store::EnqueueInput {
                session: &message.session_id,
                key: &message.idempotency_key,
                message_id: &message.message_id,
                sender: &sender,
                receiver: &receiver,
                envelope: &message,
                sequence,
                now: chrono::Utc::now().timestamp_millis(),
            },
        ) {
            Ok(true) => base(
                "accepted",
                "",
                serde_json::json!({"session_id":message.session_id,"message_id":message.message_id,"sequence":sequence,"delivery":"queued","raw_payload":false}),
            ),
            Ok(false) => base(
                "ok",
                "duplicate",
                serde_json::json!({"deduplicated":true,"raw_payload":false}),
            ),
            Err(_) => base(
                "rejected",
                "inbox_full",
                serde_json::json!({"raw_payload":false}),
            ),
        }
    }

    pub(crate) async fn dispatch_causal_collaboration_subscribe(
        &self,
        request: generated::CausalCollaborationBusSubscribeCommand,
    ) -> serde_json::Value {
        if request.schema_version != crate::causal_collaboration_bus::CONTRACT_VERSION
            || request.owner_scope.is_empty()
            || request.session_id != request.owner_scope
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"destination_forbidden","projection_json":{"raw_payload":false}});
        }
        let database = self.journal.database().lock().await;
        let messages = evohime_local_storage::collaboration_store::list::<
            crate::causal_collaboration_bus::CollaborationMessage,
        >(
            database.connection(),
            &request.session_id,
            request.limit.clamp(1, 128),
        )
        .unwrap_or_default();
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"ok","error_code":"","version":0,"projection_json":{"session_id":request.session_id,"after_sequence":request.after_sequence,"messages":messages.into_iter().filter(|m|m.sequence>request.after_sequence).map(|m|serde_json::json!({"message_id":m.message_id,"kind":m.kind,"sender":m.sender,"receiver":m.receiver,"sequence":m.sequence,"payload_hash":m.payload_hash,"provenance_id":m.provenance_id,"delivery":"queued","raw_payload":false})).collect::<Vec<_>>()}})
    }

    pub(crate) async fn write_causal_collaboration_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        value: serde_json::Value,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::CausalCollaborationBusEvent {
            schema_version: 1,
            request_id: value["request_id"].as_str().unwrap_or_default().into(),
            operation: value["operation"].as_str().unwrap_or_default().into(),
            status: value["status"].as_str().unwrap_or_default().into(),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
            version: value["version"].as_u64().unwrap_or_default(),
            projection_json: serde_json::to_vec(&value["projection_json"])?,
            truncated: false,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "causal_collaboration_bus.result".into(),
                payload: serde_json::to_vec(&value)?,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::CausalCollaborationBus(
                    event,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn dispatch_human_work_items(
        &self,
        request: generated::HumanWorkItemsCommand,
    ) -> serde_json::Value {
        use crate::human_work_items::{
            HumanWorkItem, HumanWorkItemError, HumanWorkItemState, CONTRACT_VERSION,
        };
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","state":"unknown","error_code":"invalid_request","projection_json":{"raw_prompt":false,"credentials":false}});
        }
        // A team-bound item is admissible only for the immutable role snapshot
        // selected by Core and only when that profile explicitly permits a human.
        if request.operation == "create" {
            if let Ok(item) = serde_json::from_slice::<HumanWorkItem>(&request.payload) {
                if let Some(slot_ref) = item.team_slot.as_ref() {
                    let team = self.team_sop.lock().await;
                    let allowed = team.sessions.get(&slot_ref.session_id).and_then(|session| {
                        if session.snapshot.content_hash != slot_ref.protocol_hash {
                            return None;
                        }
                        serde_json::from_slice::<crate::team_sop_protocols::TeamProtocol>(
                            &session.snapshot.protocol_json,
                        )
                        .ok()
                        .and_then(|protocol| {
                            protocol
                                .participants
                                .into_iter()
                                .find(|slot| slot.slot_id == slot_ref.slot_id)
                                .map(|slot| slot.role_profile_ref.id)
                        })
                    });
                    drop(team);
                    let human = if let Some(profile_id) = allowed {
                        self.role_profiles
                            .lock()
                            .await
                            .profiles
                            .get(&profile_id)
                            .is_some_and(|profile| {
                                matches!(
                                    profile.execution_mode,
                                    crate::agent_role_profiles::ExecutionMode::Human
                                )
                            })
                    } else {
                        false
                    };
                    if !human {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","state":"failed","error_code":"human_slot_denied","projection_json":{"raw_prompt":false,"credentials":false,"approval":false}});
                    }
                }
            }
        }
        let mut registry = self.human_work_items.lock().await;
        if registry.items.is_empty() {
            if let Ok(database) = self.journal.database().try_lock() {
                if let Ok(rows) = evohime_local_storage::human_work_items_store::load_all_json(
                    database.connection(),
                ) {
                    for row in rows {
                        if let Ok(item) = serde_json::from_slice::<HumanWorkItem>(&row) {
                            registry.items.insert(item.id.clone(), item);
                        }
                    }
                }
            }
        }
        let mut item_id = String::new();
        let mut revision = 0_u64;
        let mut state = "waiting_for_human".to_owned();
        let mut projection = serde_json::json!({"schema_version":1,"count":registry.items.len(),"raw_prompt":false,"credentials":false,"approval":false});
        let result: Result<(), HumanWorkItemError> = (|| match request.operation.as_str() {
            "list" => {
                projection = serde_json::json!({"schema_version":1,"count":registry.items.len(),"items":registry.list().into_iter().map(|item| serde_json::json!({"id":item.id,"revision":item.revision,"title":item.title,"state":item.state,"team_slot":item.team_slot,"expires_at_ms":item.expires_at_ms})).collect::<Vec<_>>(),"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            "get" => {
                let payload: HumanWorkItemCommandPayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| HumanWorkItemError::Invalid("payload"))?;
                item_id = payload.item_id;
                let item = registry
                    .items
                    .get(&item_id)
                    .ok_or(HumanWorkItemError::NotFound)?;
                revision = item.revision;
                state = format!("{:?}", item.state).to_lowercase();
                projection = serde_json::json!({"schema_version":1,"id":item.id,"revision":item.revision,"title":item.title,"instructions":item.instructions,"response_schema":item.response_schema,"state":item.state,"team_slot":item.team_slot,"expires_at_ms":item.expires_at_ms,"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            "create" => {
                let item: HumanWorkItem = serde_json::from_slice(&request.payload)
                    .map_err(|_| HumanWorkItemError::Invalid("payload"))?;
                item_id = item.id.clone();
                revision = item.revision;
                state = format!("{:?}", item.state).to_lowercase();
                let saved = registry.create(item, &request.idempotency_key)?;
                projection = serde_json::json!({"schema_version":1,"id":saved.id,"revision":saved.revision,"title":saved.title,"state":saved.state,"team_slot":saved.team_slot,"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            "start" | "submit" | "accept" | "revise" | "return" | "cancel" => {
                let payload: HumanWorkItemCommandPayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| HumanWorkItemError::Invalid("payload"))?;
                item_id = payload.item_id;
                let response = payload.response;
                let saved = registry.transition_idempotent(
                    crate::human_work_items::TransitionIdempotentInput {
                        id: item_id.clone(),
                        expected: request.expected_revision,
                        operation: request.operation.clone(),
                        response,
                        actor: "shell".into(),
                        now_ms: chrono::Utc::now().timestamp_millis(),
                        key: request.idempotency_key.clone(),
                    },
                )?;
                revision = saved.revision;
                state = format!("{:?}", saved.state).to_lowercase();
                projection = serde_json::json!({"schema_version":1,"id":saved.id,"revision":saved.revision,"title":saved.title,"state":saved.state,"team_slot":saved.team_slot,"response_present":saved.response.is_some(),"submitted_by":saved.submitted_by,"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            "expire_due" => {
                let changed = registry.expire_due(chrono::Utc::now().timestamp_millis());
                projection = serde_json::json!({"schema_version":1,"expired":changed.len(),"state":HumanWorkItemState::Expired,"raw_prompt":false,"credentials":false,"approval":false});
                Ok(())
            }
            _ => Err(HumanWorkItemError::Invalid("unsupported_operation")),
        })();
        let (status, error_code) = match result {
            Ok(()) => ("ok", String::new()),
            Err(error) => ("rejected", error.to_string()),
        };
        if status == "ok" && !item_id.is_empty() {
            if let Some(item) = registry.items.get(&item_id) {
                if let Ok(database) = self.journal.database().try_lock() {
                    let json = serde_json::to_vec(item).unwrap_or_default();
                    let _ = evohime_local_storage::human_work_items_store::save(
                        database.connection(),
                        &item.id,
                        item.revision,
                        &format!("{:?}", item.state).to_lowercase(),
                        &json,
                        &request.operation,
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
            }
        }
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":status,"item_id":item_id,"revision":revision,"state":state,"error_code":error_code,"projection_json":projection})
    }

    pub(crate) async fn dispatch_agentic_browser_session(
        &self,
        request: generated::AgenticBrowserSessionCommand,
    ) -> serde_json::Value {
        use crate::agentic_browser_session::{
            BrowserSession, ControlOwner, SessionState, CONTRACT_VERSION,
        };
        let rejected = |code: &str| serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":code,"projection_json":{"raw_payload":false,"credentials":false,"cdp_endpoint":false}});
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return rejected("invalid_request");
        }
        let payload_json: serde_json::Value = match serde_json::from_slice(&request.payload) {
            Ok(value) => value,
            Err(_) => return rejected("invalid_payload"),
        };
        let payload: AgenticBrowserSessionPayload =
            match serde_json::from_value(payload_json.clone()) {
                Ok(value) => value,
                Err(_) => return rejected("invalid_payload"),
            };
        if matches!(
            request.operation.as_str(),
            "legacy" | "legacy_selector" | "raw_cdp"
        ) {
            return rejected("legacy_disabled");
        }
        let database = self.journal.database();
        let now = chrono::Utc::now().timestamp_millis();
        let (mut session, is_create) = if request.operation == "create" {
            let conversation_id = payload
                .conversation_id
                .as_deref()
                .unwrap_or(request.owner_scope.as_str());
            let Ok(mut session) = BrowserSession::new(
                conversation_id,
                payload.run_id,
                payload.policy_hash.as_deref().unwrap_or("unknown"),
            ) else {
                return rejected("invalid_scope");
            };
            if session.transition(SessionState::Starting).is_err() {
                return rejected("invalid_state");
            }
            (session, true)
        } else {
            let Some(session_id) = payload.session_id.as_deref() else {
                return rejected("session_required");
            };
            let Ok(database) = database.try_lock() else {
                return rejected("storage_busy");
            };
            let Ok(Some(record)) = evohime_local_storage::browser_session_store::get(
                database.connection(),
                session_id,
            ) else {
                return rejected("session_not_found");
            };
            let Ok(session) = BrowserSession::from_metadata(
                crate::agentic_browser_session::BrowserSessionMetadata {
                    session_id: record.session_id,
                    conversation_id: record.conversation_id,
                    run_id: record.run_id,
                    state: record.state,
                    revision: record.revision,
                    control_generation: record.control_generation,
                    control_owner: record.control_owner,
                    profile_policy: record.profile_policy,
                    network_policy: record.network_policy,
                    policy_hash: record.policy_hash,
                },
            ) else {
                return rejected("invalid_session");
            };
            (session, false)
        };
        let mut operation_projection = serde_json::json!({});
        if !is_create
            && payload
                .expected_revision
                .is_some_and(|revision| revision != session.revision)
        {
            return rejected("stale_revision");
        }
        if matches!(
            request.operation.as_str(),
            "click" | "fill" | "select" | "press" | "download" | "upload"
        ) {
            let page_ref = payload.page_ref.as_deref().unwrap_or_default();
            let element_ref = payload.element_ref.as_deref().unwrap_or_default();
            if page_ref.is_empty() || page_ref.len() > 512 {
                return rejected("invalid_page_ref");
            }
            if element_ref
                .strip_prefix('e')
                .and_then(|value| value.parse::<u16>().ok())
                .is_none()
            {
                return rejected("invalid_element_ref");
            }
        }
        if !is_create && request.operation == "take_control" {
            if session.take_control().is_err() {
                return rejected("control_unavailable");
            }
        } else if !is_create && request.operation == "return_control" {
            if session.return_control().is_err() {
                return rejected("control_unavailable");
            }
        } else if !is_create && request.operation == "close" {
            let _ = session.transition(SessionState::Closing);
            let _ = session.transition(SessionState::Closed);
        } else if !is_create
            && session.control_owner == ControlOwner::Human
            && request.operation != "snapshot"
        {
            return rejected("control_taken");
        } else if is_create {
            let Ok(backend) = crate::browser_backend::BrowserBackendProcess::spawn().await else {
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unavailable","error_code":"browser_backend_unavailable","projection_json":{"raw_payload":false,"credentials":false,"cdp_endpoint":false}});
            };
            let mut backends = self.browser_backends.lock().await;
            backends.insert(session.session_id.to_string(), backend);
            let _ = session.transition(SessionState::Ready);
        } else if let Some(operation) = matches!(
            request.operation.as_str(),
            "navigate"
                | "snapshot"
                | "click"
                | "fill"
                | "select"
                | "press"
                | "scroll"
                | "wait"
                | "back"
                | "forward"
                | "reload"
                | "screenshot"
                | "download"
                | "upload"
        )
        .then_some(request.operation.as_str())
        {
            let mut backends = self.browser_backends.lock().await;
            let Some(backend) = backends.get_mut(&session.session_id.to_string()) else {
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unavailable","session_id":session.session_id,"revision":session.revision,"error_code":"browser_backend_unavailable","projection_json":{"raw_payload":false}});
            };
            let mut backend_payload = payload_json;
            if let Some(element_ref) = payload.element_ref.as_deref() {
                backend_payload["ref"] = serde_json::Value::String(element_ref.to_string());
            }
            if operation == "upload" {
                let Some(locator) = payload.artifact_ref.as_deref() else {
                    return rejected("artifact_required");
                };
                let Ok(database) = database.try_lock() else {
                    return rejected("storage_busy");
                };
                let Ok(bytes) = evohime_local_storage::artifact_store::ArtifactStore::new(
                    database.connection(),
                )
                .read_bytes(
                    locator,
                    &session.conversation_id,
                    &[],
                    "browser_upload",
                    now,
                ) else {
                    return rejected("artifact_not_readable");
                };
                if bytes.len() > 1024 * 1024 {
                    return rejected("artifact_too_large");
                }
                use base64::Engine;
                backend_payload["fileBase64"] = serde_json::Value::String(
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                );
                backend_payload["fileName"] = serde_json::Value::String("upload.bin".into());
            }
            let response = backend
                .request(crate::browser_backend::BrowserBackendProcess::command(
                    &request.request_id,
                    operation,
                    &backend_payload,
                ))
                .await;
            let Ok(response) = response else {
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unknown_outcome","session_id":session.session_id,"revision":session.revision,"error_code":"browser_backend_unknown_outcome","projection_json":{"raw_payload":false}});
            };
            if response["status"] != "ok" {
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","session_id":session.session_id,"revision":session.revision,"error_code":response["error_code"],"projection_json":{"raw_payload":false}});
            }
            if let Some(revision) = response["revision"].as_u64() {
                session.revision = revision;
            }
            if operation == "snapshot" {
                operation_projection =
                    serde_json::json!({"snapshot": response["snapshot"], "raw_dom": false});
            }
            if operation == "screenshot" || operation == "download" {
                use base64::Engine;
                let Some(encoded) = response["artifact_base64"].as_str() else {
                    return rejected("artifact_store_failed");
                };
                let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded) else {
                    return rejected("artifact_store_failed");
                };
                let Ok(database) = database.try_lock() else {
                    return rejected("storage_busy");
                };
                let Ok(offload) = evohime_local_storage::artifact_store::ArtifactStore::new(
                    database.connection(),
                )
                .offload_bytes(
                    if operation == "screenshot" {
                        "browser_screenshot"
                    } else {
                        "browser_download"
                    },
                    &session.conversation_id,
                    &session.conversation_id,
                    &bytes,
                    evohime_context_budget::item::Privacy::Workspace,
                    now,
                ) else {
                    return rejected("artifact_store_failed");
                };
                return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"ok","session_id":session.session_id,"revision":session.revision,"control_owner":"agent","control_generation":session.control_generation,"error_code":"","projection_json":{"schema_version":1,"session_id":session.session_id,"state":serde_json::to_value(session.state).unwrap_or_default(),"revision":session.revision,"artifact_ref":offload.reference.locator,"artifact_hash":offload.reference.content_hash,"raw_payload":false,"credentials":false,"cdp_endpoint":false}});
            }
        } else if !matches!(
            request.operation.as_str(),
            "create" | "take_control" | "return_control" | "close"
        ) {
            return rejected("unsupported_operation");
        }
        if is_create
            || request.operation == "take_control"
            || request.operation == "return_control"
            || request.operation == "close"
            || matches!(
                request.operation.as_str(),
                "navigate"
                    | "snapshot"
                    | "click"
                    | "fill"
                    | "select"
                    | "press"
                    | "scroll"
                    | "wait"
                    | "back"
                    | "forward"
                    | "reload"
                    | "screenshot"
                    | "download"
                    | "upload"
            )
        {
            if let Ok(database) = database.try_lock() {
                let _ = evohime_local_storage::browser_session_store::upsert(
                    database.connection(),
                    &evohime_local_storage::browser_session_store::BrowserSessionMetadata {
                        session_id: session.session_id.to_string(),
                        conversation_id: session.conversation_id.clone(),
                        run_id: session.run_id.clone(),
                        state: serde_json::to_value(session.state)
                            .unwrap_or_default()
                            .as_str()
                            .unwrap_or("failed")
                            .into(),
                        revision: session.revision,
                        control_generation: session.control_generation,
                        control_owner: match session.control_owner {
                            ControlOwner::Agent => "agent".into(),
                            ControlOwner::Human => "human".into(),
                        },
                        profile_policy: session.profile_policy.clone(),
                        network_policy: session.network_policy.clone(),
                        policy_hash: session.policy_hash.clone(),
                        updated_at_ms: now,
                    },
                );
            }
        }
        if request.operation == "close" {
            self.browser_backends
                .lock()
                .await
                .remove(&session.session_id.to_string());
        }
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"ok","session_id":session.session_id,"revision":session.revision,"control_owner":match session.control_owner { ControlOwner::Agent => "agent", ControlOwner::Human => "human" },"control_generation":session.control_generation,"error_code":"","projection_json":{"schema_version":1,"session_id":session.session_id,"state":serde_json::to_value(session.state).unwrap_or_default(),"revision":session.revision,"profile_policy":session.profile_policy,"network_policy":session.network_policy,"raw_payload":false,"credentials":false,"cdp_endpoint":false,"operation":operation_projection}})
    }

    pub(crate) async fn write_agentic_browser_session_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: AgenticBrowserSessionResponse = serde_json::from_slice(&payload)?;
        let result = generated::AgenticBrowserSessionEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            session_id: value.session_id,
            revision: value.revision,
            control_owner: value.control_owner,
            control_generation: value.control_generation,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&value.projection_json)?,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "agentic_browser_session.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::AgenticBrowserSession(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_human_work_items_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: HumanWorkItemsResponse = serde_json::from_slice(&payload)?;
        let result = generated::HumanWorkItemsEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            item_id: value.item_id,
            revision: value.revision,
            state: value.state,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&value.projection_json)?,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "human_work_items.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::HumanWorkItems(result)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn dispatch_team_sop_protocols(
        &self,
        request: generated::TeamSopProtocolsCommand,
    ) -> serde_json::Value {
        use crate::team_sop_protocols::{TeamProtocol, TeamSopError, CONTRACT_VERSION};
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","state":"unknown","error_code":"invalid_request","projection_json":{"raw_payload":false}});
        }
        let mut registry = self.team_sop.lock().await;
        let mut session_id = String::new();
        let mut state = "pinned".to_owned();
        let mut version = 0_u64;
        let mut projection = serde_json::json!({"schema_version":1,"protocol_count":registry.protocols.len(),"session_count":registry.sessions.len(),"raw_payload":false,"credentials":false});
        let result: Result<(), TeamSopError> = (|| match request.operation.as_str() {
            "list" => Ok(()),
            "create" | "revise" => {
                let payload: TeamProtocol = serde_json::from_slice(&request.payload)
                    .map_err(|_| TeamSopError::Invalid("payload"))?;
                let saved = if request.operation == "create" {
                    registry.create(payload, &request.idempotency_key)?
                } else {
                    registry.revise(payload, request.expected_version, &request.idempotency_key)?
                };
                let hash = saved.content_hash.clone();
                if let Ok(database) = self.journal.database().try_lock() {
                    let json = serde_json::to_vec(&saved).unwrap_or_default();
                    let _ = evohime_local_storage::team_sop_protocols_store::save_protocol(
                        database.connection(),
                        &saved.id,
                        saved.version,
                        &hash,
                        &json,
                        chrono::Utc::now().timestamp_millis(),
                    );
                }
                projection = serde_json::json!({"schema_version":1,"protocol_id":saved.id,"protocol_version":saved.version,"content_hash":hash,"participant_count":saved.participants.len(),"phase_count":saved.phases.len(),"handoff_count":saved.handoffs.len(),"raw_payload":false});
                Ok(())
            }
            "start" => {
                let p: TeamSopSessionPayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| TeamSopError::Invalid("payload"))?;
                session_id = p.session_id;
                let protocol_id = p
                    .protocol_id
                    .as_deref()
                    .ok_or(TeamSopError::Invalid("protocol_id"))?;
                let protocol_version = p
                    .protocol_version
                    .ok_or(TeamSopError::Invalid("protocol_version"))?;
                let s = registry.start(
                    session_id.clone(),
                    protocol_id,
                    protocol_version,
                    p.workflow_run_id,
                )?;
                version = s.version;
                state = format!("{:?}", s.status).to_lowercase();
                projection = serde_json::json!({"schema_version":1,"session_id":s.id,"protocol_id":s.snapshot.protocol_id,"protocol_version":s.snapshot.version,"content_hash":s.snapshot.content_hash,"current_phase":s.current_phase,"completed_phase_count":s.completed_phases.len(),"review_iterations":s.review_iterations,"state":state,"version":s.version,"raw_payload":false});
                Ok(())
            }
            "advance" => {
                let p: TeamSopSessionPayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| TeamSopError::Invalid("payload"))?;
                session_id = p.session_id;
                let s = registry.advance(&session_id, request.expected_version)?;
                version = s.version;
                state = format!("{:?}", s.status).to_lowercase();
                projection = serde_json::json!({"schema_version":1,"session_id":s.id,"protocol_id":s.snapshot.protocol_id,"protocol_version":s.snapshot.version,"content_hash":s.snapshot.content_hash,"current_phase":s.current_phase,"completed_phase_count":s.completed_phases.len(),"review_iterations":s.review_iterations,"state":state,"version":s.version,"raw_payload":false});
                Ok(())
            }
            "cancel" => {
                let p: TeamSopSessionPayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| TeamSopError::Invalid("payload"))?;
                session_id = p.session_id;
                let s = registry.cancel(&session_id)?;
                version = s.version;
                state = "cancelled".into();
                projection = serde_json::json!({"schema_version":1,"session_id":s.id,"state":state,"version":s.version,"raw_payload":false});
                Ok(())
            }
            "review" | "revise_session" => {
                let p: TeamSopSessionPayload = serde_json::from_slice(&request.payload)
                    .map_err(|_| TeamSopError::Invalid("payload"))?;
                session_id = p.session_id;
                let s = registry.review(
                    &session_id,
                    request.expected_version,
                    request.operation == "revise_session",
                )?;
                version = s.version;
                state = format!("{:?}", s.status);
                projection = serde_json::json!({"schema_version":1,"session_id":s.id,"current_phase":s.current_phase,"review_iterations":s.review_iterations,"state":state,"version":s.version,"raw_payload":false});
                Ok(())
            }
            _ => Err(TeamSopError::Invalid("unsupported_operation")),
        })();
        let (status, error_code) = match result {
            Ok(()) => ("ok".to_owned(), String::new()),
            Err(e) => ("rejected".to_owned(), e.to_string()),
        };
        if status == "ok" && !session_id.is_empty() {
            if let Some(session) = registry.sessions.get(&session_id) {
                if let Ok(database) = self.journal.database().try_lock() {
                    let snapshot = serde_json::to_vec(&session.snapshot).unwrap_or_default();
                    let state = format!("{:?}", session.status).to_lowercase();
                    let _ = evohime_local_storage::team_sop_protocols_store::save_session(
                        database.connection(),
                        evohime_local_storage::team_sop_protocols_store::SaveSessionInput {
                            id: &session.id,
                            protocol_id: &session.snapshot.protocol_id,
                            protocol_version: session.snapshot.version,
                            hash: &session.snapshot.content_hash,
                            snapshot: &snapshot,
                            status: &state,
                            phase: &session.current_phase,
                            version: session.version,
                            now_ms: chrono::Utc::now().timestamp_millis(),
                        },
                    );
                }
            }
        }
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":status,"session_id":session_id,"version":version,"state":state,"error_code":error_code,"projection_json":projection})
    }

    pub(crate) async fn write_team_sop_protocols_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: TeamSopProtocolsResponse = serde_json::from_slice(&payload)?;
        let result = generated::TeamSopProtocolsEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            session_id: value.session_id,
            version: value.version,
            state: value.state,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&value.projection_json)?,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "team_sop_protocols.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::TeamSopProtocols(result)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn dispatch_external_coding_agent_adapter(
        &self,
        request: generated::ExternalCodingAgentAdapterCommand,
    ) -> serde_json::Value {
        use crate::external_coding_agent_adapter::{AgentState, CONTRACT_ID, CONTRACT_VERSION};
        if request.schema_version != CONTRACT_VERSION
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.payload.len() > 64 * 1024
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","state":"unavailable","error_code":"invalid_request","projection_json":{"raw_payload":false}});
        }
        let mut registry = self.external_agents.lock().await;
        let mut projection = registry.status();
        let mut state = AgentState::Registered;
        let mut status = "ok";
        let mut error_code = "";
        match request.operation.as_str() {
            "list" | "status" => {}
            "start" => {
                let payload: ExternalAgentCommandPayload = match serde_json::from_slice(
                    &request.payload,
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"rejected","state":"unavailable","error_code":"invalid_payload","projection_json":{"raw_payload":false}});
                    }
                };
                let run_id = payload.run_id.as_str();
                let conversation_id = payload.conversation_id.as_str();
                if run_id.is_empty() || conversation_id.is_empty() {
                    status = "rejected";
                    error_code = "invalid_run";
                    state = AgentState::Unavailable;
                } else if registry.runs.contains_key(run_id) {
                    status = "duplicate";
                    error_code = "duplicate_run";
                    state = *registry.runs.get(run_id).unwrap_or(&AgentState::Unknown);
                } else {
                    let executable_ref = payload.executable_ref.as_str();
                    #[cfg(windows)]
                    let supervisor_result = crate::analysis_kernel::supervisor_command(
                        serde_json::json!({"op":"external_agent_start","run_id":run_id,"executable_ref":executable_ref}),
                    ).await;
                    #[cfg(not(windows))]
                    let supervisor_result: Result<serde_json::Value, String> =
                        Err("unsupported_platform".into());
                    if supervisor_result
                        .as_ref()
                        .ok()
                        .and_then(|v| v.get("accepted"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
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
                }
                projection = serde_json::json!({"contract_id":CONTRACT_ID,"contract_version":CONTRACT_VERSION,"conversation_id":conversation_id,"run_id":run_id,"core_control_level":"supervised_opaque","raw_payload":false});
            }
            "cancel" => {
                let payload: ExternalAgentCommandPayload = match serde_json::from_slice(
                    &request.payload,
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        status = "rejected";
                        error_code = "invalid_payload";
                        state = AgentState::Unavailable;
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":status,"state":state,"protocol":CONTRACT_ID,"control_level":"supervised_opaque","error_code":error_code,"projection_json":projection});
                    }
                };
                if !payload.run_id.is_empty() {
                    if registry
                        .runs
                        .insert(payload.run_id, AgentState::Cancelling)
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
        if request.operation == "start" || request.operation == "cancel" {
            let run_id = projection["run_id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_default();
            let conversation_id = projection["conversation_id"]
                .as_str()
                .unwrap_or("")
                .to_owned();
            if !run_id.is_empty() && !conversation_id.is_empty() {
                if let Ok(database) = self.journal.database().try_lock() {
                    let state_json = serde_json::to_string(&state).unwrap_or_else(|error| {
                        tracing::warn!(%error, "failed to serialize external agent state");
                        String::new()
                    });
                    let _ = evohime_local_storage::external_coding_agent_adapter_store::record_event(
                        database.connection(),
                        evohime_local_storage::external_coding_agent_adapter_store::RecordEventInput {
                            conversation_id: &conversation_id,
                            run_id: &run_id,
                            state: state_json.trim_matches('"'),
                            outcome: status,
                            correlation_id: &request.request_id,
                            idempotency_key: &request.idempotency_key,
                            now_ms: chrono::Utc::now().timestamp_millis(),
                        },
                    );
                }
            }
        }
        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":status,"state":state,"protocol":CONTRACT_ID,"control_level":"supervised_opaque","error_code":error_code,"projection_json":projection})
    }

    pub(crate) async fn write_external_coding_agent_adapter_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: IpcResponseFields = serde_json::from_slice(&payload)?;
        let projection = serde_json::to_vec(&value.projection_json)?;
        let result = generated::ExternalCodingAgentAdapterEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            state: value.state,
            conversation_id: value.projection_json["conversation_id"]
                .as_str()
                .unwrap_or_default()
                .into(),
            run_id: value.projection_json["run_id"]
                .as_str()
                .unwrap_or_default()
                .into(),
            protocol: value.protocol,
            control_level: value.control_level,
            snapshot_hash: String::new(),
            error_code: value.error_code,
            projection_json: projection,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "external_coding_agent_adapter.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::ExternalCodingAgentAdapter(result)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_execution_backend_registry_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: IpcResponseFields = serde_json::from_slice(&payload)?;
        let result = generated::ExecutionBackendRegistryEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.projection_json["status"]
                .as_str()
                .unwrap_or(if value.status.is_empty() {
                    "rejected"
                } else {
                    &value.status
                })
                .into(),
            registry_version: value.registry_version,
            projection_json: serde_json::to_vec(&value.projection_json)?,
            error_code: value.error_code,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "execution_backend_registry.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::ExecutionBackendRegistry(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_model_resilience_policy_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: IpcResponseFields = serde_json::from_slice(&payload)?;
        let projection = serde_json::to_vec(&value.projection_json)?;
        let result = generated::ModelResiliencePolicyEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            policy_id: value.policy_id,
            policy_hash: value.policy_hash,
            attempts: value.attempts,
            retries: value.retries,
            fallbacks: value.fallbacks,
            terminal_outcome: value.terminal_outcome,
            error_code: value.error_code,
            projection_json: projection,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "model_resilience_policy.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::ModelResiliencePolicy(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_execution_policy_profiles_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: IpcResponseFields = serde_json::from_slice(&payload)?;
        let result = generated::ExecutionPolicyProfilesEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            profile_id: value.profile_id,
            version: value.version,
            profile_hash: value.profile_hash,
            backend: value.backend,
            network_policy: value.network_policy,
            environment_policy: value.environment_policy,
            timeout_ms: value.timeout_ms,
            max_output_bytes: value.max_output_bytes,
            error_code: value.error_code,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "execution_policy_profiles.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::ExecutionPolicyProfiles(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_structured_response_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: IpcResponseFields = serde_json::from_slice(&payload)?;
        let result = generated::StructuredResponseEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            run_id: value.run_id,
            revision: value.revision,
            contract_hash: value.contract_hash,
            strategy: value.strategy,
            attempts: value.attempts,
            error_code: value.error_code,
            projection_json: payload.clone(),
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "structured_response.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::StructuredResponse(result)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_agent_middleware_pipeline_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        event_type: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: IpcResponseFields = serde_json::from_slice(&payload)?;
        let result = generated::AgentMiddlewarePipelineEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            run_id: value.run_id,
            revision: value.revision,
            contract_hash: value.contract_hash,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&value.projection_json).unwrap_or_default(),
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: event_type.into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::AgentMiddlewarePipeline(
                    result,
                )),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        event_type: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: event_type.into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: None,
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_persistent_agent_organization_registry_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        request_id: &str,
        agent_id: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: PersistentAgentOrganizationResponse = serde_json::from_slice(&payload)?;
        let encoded = generated::PersistentAgentOrganizationRegistryEvent {
            schema_version: 1,
            request_id: request_id.to_owned(),
            agent_id: agent_id.to_owned(),
            operation: value.operation,
            revision: value.revision,
            status: value.status,
            error_code: String::new(),
            projection_json: payload.clone(),
            truncated: payload.len() > 64 * 1024,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "persistent_agent_organization_registry.result".into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(
                    generated::event_envelope::Event::PersistentAgentOrganizationRegistry(encoded),
                ),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_invocation_preset_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        event_type: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: InvocationPresetResponse = serde_json::from_slice(&payload)?;
        let projection = value
            .preview
            .or(value.presets)
            .unwrap_or(serde_json::Value::Null);
        let result = generated::InvocationPresetEvent {
            schema_version: 1,
            request_id: value.request_id,
            operation: value.operation,
            status: value.status,
            preset_id: value.preset_id,
            revision: value.revision,
            content_hash: value.content_hash,
            error_code: value.error_code,
            projection_json: serde_json::to_vec(&projection).unwrap_or_default(),
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: event_type.into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::InvocationPreset(result)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn write_package_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        operation: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value = serde_json::from_slice(&payload)?;
        let result = generated::WorkflowPackageResult {
            schema_version: 1,
            operation: operation.into(),
            status: value["status"].as_str().unwrap_or("unknown").into(),
            package_hash: value["package_hash"].as_str().unwrap_or_default().into(),
            import_id: value["import_id"].as_str().unwrap_or_default().into(),
            local_workflow_id: value["local_workflow_id"]
                .as_str()
                .unwrap_or_default()
                .into(),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
        };
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: format!("workflow.package.{operation}"),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::WorkflowPackage(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }
}