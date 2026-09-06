use super::*;

impl IpcBridge {
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
                let payload: ExternalAgentCommandPayload<'_> = match serde_json::from_slice(
                    &request.payload,
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"start","status":"rejected","state":"unavailable","error_code":"invalid_payload","projection_json":{"raw_payload":false}});
                    }
                };
                let run_id = payload.run_id.as_ref();
                let conversation_id = payload.conversation_id.as_ref();
                if run_id.is_empty() || conversation_id.is_empty() {
                    status = "rejected";
                    error_code = "invalid_run";
                    state = AgentState::Unavailable;
                } else if registry.runs.contains_key(run_id) {
                    status = "duplicate";
                    error_code = "duplicate_run";
                    state = *registry.runs.get(run_id).unwrap_or(&AgentState::Unknown);
                } else {
                    let executable_ref = payload.executable_ref.as_ref();
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
                let payload: ExternalAgentCommandPayload<'_> = match serde_json::from_slice(
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
                        .insert(payload.run_id.into_owned(), AgentState::Cancelling)
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
            let run_id = projection["run_id"].as_str().unwrap_or_default();
            let conversation_id = projection["conversation_id"].as_str().unwrap_or_default();
            if !run_id.is_empty() && !conversation_id.is_empty() {
                if let Ok(database) = self.journal.database().try_lock() {
                    let state_json = serde_json::to_string(&state).unwrap_or_else(|error| {
                        tracing::warn!(%error, "failed to serialize external agent state");
                        String::new()
                    });
                    let _ = evohime_local_storage::external_coding_agent_adapter_store::record_event(
                        database.connection(),
                        evohime_local_storage::external_coding_agent_adapter_store::RecordEventInput {
                            conversation_id,
                            run_id,
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

    pub(crate) async fn write_persistent_agent_organization_registry_response<
        W: AsyncWrite + Unpin,
    >(
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
