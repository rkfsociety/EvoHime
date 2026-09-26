use super::*;

impl IpcBridge {
    pub(super) async fn dispatch_ambient_workflow<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        _request_id: String,
        client_id: String,
        _command_hash: String,
        command: Option<generated::command_envelope::Command>,
    ) -> Result<(), IpcBridgeError> {
        match command {
            Some(generated::command_envelope::Command::ImageGeneration(request)) => {
                let result = self.dispatch_image_generation(&client_id, &request).await;
                self.write_image_generation_response(writer, &request, result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SetAmbientListening(request)) => {
                let result = self.dispatch_set_ambient_listening(request).await;
                self.write_response(writer, "ambient.listening", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetAmbientStatus(_)) => {
                let result = self.dispatch_get_ambient_status().await;
                self.write_response(writer, "ambient.status", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListAmbientEpisodes(request)) => {
                let result = self.dispatch_list_ambient_episodes(request).await;
                self.write_response(writer, "ambient.episodes", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetAmbientEpisode(request)) => {
                let result = self.dispatch_get_ambient_episode(request).await;
                self.write_response(writer, "ambient.episode", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::DeleteAmbientTranscripts(request)) => {
                let result = self.dispatch_delete_ambient_transcripts(request).await;
                self.write_response(writer, "ambient.deleted", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ForgetAmbientWindow(request)) => {
                let result = self.dispatch_forget_ambient_window(request).await;
                self.write_response(writer, "ambient.forgotten", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetAmbientPolicy(_)) => {
                let result = self.dispatch_get_ambient_policy().await;
                self.write_response(writer, "ambient.policy", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::SaveAmbientPolicy(request)) => {
                let result = self.dispatch_save_ambient_policy(request).await;
                self.write_response(writer, "ambient.policy_saved", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ResolveAmbientProposal(request)) => {
                let result = self.dispatch_resolve_ambient_proposal(request).await;
                // Имя ответа отличается от имени журнальной записи
                // `ambient.proposal`: renderer подписан на неё как на событие,
                // и ответ на команду не должен подменять собой список карточек.
                self.write_response(
                    writer,
                    "ambient.proposal_resolved",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ListAmbientProposals(request)) => {
                let result = self.dispatch_list_ambient_proposals(request).await;
                self.write_response(writer, "ambient.proposals", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListVoiceCommands(request)) => {
                let result = self.dispatch_list_voice_commands(request);
                self.write_response(
                    writer,
                    "ambient.voice_commands",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ResolveVoiceCommand(request)) => {
                let result = self.dispatch_resolve_voice_command(request).await;
                self.write_response(
                    writer,
                    "ambient.voice_command_resolved",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ListWorkflowTemplates(_)) => {
                let result = self.dispatch_list_workflow_templates();
                self.write_response(writer, "workflow.templates", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetWorkflowDefinition(request)) => {
                let result = self.dispatch_workflow_definition(request);
                self.write_response(writer, "workflow.definition", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::StartWorkflow(request)) => {
                let result = self.dispatch_start_workflow(request).await;
                self.write_response(writer, "workflow.started", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetWorkflowRun(request)) => {
                let result = self.dispatch_workflow_run(request).await;
                self.write_response(writer, "workflow.run", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::CancelWorkflow(request)) => {
                let result = self.dispatch_cancel_workflow(request).await;
                self.write_response(writer, "workflow.cancelled", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListWorkflowEvents(request)) => {
                let result = self.dispatch_list_workflow_events(request).await;
                self.write_response(writer, "workflow.events", serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListCapabilityRecipes(_)) => {
                let result = self.dispatch_list_capability_recipes();
                self.write_response(
                    writer,
                    "capability_recipe.catalog",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::PreflightCapabilityRecipe(request)) => {
                let result = self.dispatch_preflight_capability_recipe(request);
                self.write_response(
                    writer,
                    "capability_recipe.preflight",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::StartCapabilityRecipe(request)) => {
                let result = self.dispatch_start_capability_recipe(request).await;
                self.write_response(
                    writer,
                    "capability_recipe.started",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::GetCapabilityRecipeRun(request)) => {
                let result = self.dispatch_capability_recipe_run(request).await;
                self.write_response(
                    writer,
                    "capability_recipe.run",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ForkCapabilityRecipeRun(request)) => {
                let result = self.dispatch_fork_capability_recipe_run(request).await;
                self.write_response(
                    writer,
                    "capability_recipe.forked",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::VisualWorkflowBuilder(request)) => {
                let result = self.dispatch_visual_workflow_builder(request).await;
                self.write_response(
                    writer,
                    "workflow_builder.result",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ConversationalWorkflowComposer(request)) => {
                let result = self
                    .dispatch_conversational_workflow_composer(request)
                    .await;
                self.write_response(
                    writer,
                    "workflow_composer.result",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::IntegrationProviderSdkCatalog(request))
            | Some(generated::command_envelope::Command::IntegrationProviderSdkAction(request)) => {
                let result = self.dispatch_integration_provider_sdk(request);
                self.write_response(
                    writer,
                    "integration_provider_sdk.result",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::EventTriggerRuntimeList(request))
            | Some(generated::command_envelope::Command::EventTriggerRuntimeAction(request)) => {
                let result = self.dispatch_event_trigger_runtime(request);
                self.write_response(
                    writer,
                    "event_trigger_runtime.result",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::InvocationPresetList(request))
            | Some(generated::command_envelope::Command::InvocationPresetAction(request)) => {
                let result = self.dispatch_invocation_preset(request).await;
                self.write_invocation_preset_response(
                    writer,
                    "invocation_preset.result",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::BenchmarkMatrixList(request))
            | Some(generated::command_envelope::Command::BenchmarkMatrixAction(request)) => {
                let result = self.dispatch_benchmark_matrix(request).await;
                self.write_response(
                    writer,
                    "benchmark_matrix.result",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::AgentMiddlewarePipelineList(request))
            | Some(generated::command_envelope::Command::AgentMiddlewarePipelineAction(request)) => {
                let result = self.dispatch_agent_middleware_pipeline(request);
                self.write_agent_middleware_pipeline_response(
                    writer,
                    "agent_middleware_pipeline.result",
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::StructuredResponse(request)) => {
                let result = self.dispatch_structured_response(request);
                self.write_structured_response_response(writer, serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::SensitiveDataGuardrails(request)) => {
                let result = self.dispatch_sensitive_data_guardrails(request);
                self.write_sensitive_data_guardrails_response(writer, serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ExecutionPolicyProfiles(request)) => {
                let result = self.dispatch_execution_policy_profiles(request);
                self.write_execution_policy_profiles_response(writer, serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ModelResiliencePolicy(request)) => {
                let result = self.dispatch_model_resilience_policy(request);
                self.write_model_resilience_policy_response(writer, serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ExecutionBackendRegistry(request)) => {
                let result = self.dispatch_execution_backend_registry(request).await;
                self.write_execution_backend_registry_response(
                    writer,
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ToolSimulationRuntime(request)) => {
                let result = self.dispatch_tool_simulation_runtime(request).await;
                self.write_tool_simulation_runtime_response(writer, serde_json::to_vec(&result)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::ExternalCodingAgentAdapterList(request))
            | Some(generated::command_envelope::Command::ExternalCodingAgentAdapterAction(
                request,
            )) => {
                let result = self.dispatch_external_coding_agent_adapter(request).await;
                self.write_external_coding_agent_adapter_response(
                    writer,
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::AgentClientProtocolBridge(request)) => {
                let result = self.dispatch_agent_client_protocol_bridge(request).await;
                self.write_agent_client_protocol_bridge_response(
                    writer,
                    serde_json::to_vec(&result)?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::AgentRoleProfilesList(request))
            | Some(generated::command_envelope::Command::AgentRoleProfilesAction(request)) => {
                let result = self.dispatch_agent_role_profiles(request).await;
                self.write_agent_role_profiles_response(writer, serde_json::to_vec(&result)?)
                    .await?;
            }

            _ => unreachable!("command routed to the wrong domain"),
        }
        Ok(())
    }
}

impl IpcBridge {
    async fn dispatch_image_generation(
        &self,
        client_id: &str,
        request: &generated::ImageGenerationCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1 || request.payload.len() > 16 * 1024 {
            return serde_json::json!({"status":"rejected","error_code":"invalid_request"});
        }
        let Some(runtime) = self.image_generation_runtime() else {
            if request.operation == "capability" {
                return serde_json::to_value(
                    crate::image_generation::ImageCapabilityProjection::unavailable(
                        "unknown",
                        "model_gateway_unavailable",
                    ),
                )
                .unwrap_or_else(
                    |_| serde_json::json!({"state":"unknown","reason_code":"projection_failed"}),
                );
            }
            return serde_json::json!({"status":"unavailable","error_code":"model_gateway_unavailable"});
        };
        match request.operation.as_str() {
            "capability" => serde_json::to_value(runtime.capability()).unwrap_or_else(
                |_| serde_json::json!({"status":"unavailable","error_code":"projection_failed"}),
            ),
            "get" => match runtime.get_job(client_id, &request.job_id).await {
                Ok(Some(job)) => serde_json::to_value(job)
                    .unwrap_or_else(|_| serde_json::json!({"error_code":"projection_failed"})),
                Ok(None) => serde_json::json!({"status":"not_found","error_code":"unknown_job"}),
                Err(_) => serde_json::json!({"status":"failed","error_code":"storage_error"}),
            },
            "cancel" => match runtime.cancel_job(client_id, &request.job_id).await {
                Ok(cancelled) => {
                    serde_json::json!({"status":if cancelled {"cancelled"} else {"not_cancellable"},"job_id":request.job_id})
                }
                Err(_) => serde_json::json!({"status":"failed","error_code":"storage_error"}),
            },
            "start" => {
                let Ok(mut image_request) = serde_json::from_slice::<
                    crate::image_generation::ImageGenerationRequest,
                >(&request.payload) else {
                    return serde_json::json!({"status":"rejected","error_code":"invalid_request"});
                };
                if request.idempotency_key.trim().is_empty()
                    || request.idempotency_key.len() > 128
                    || request.job_id.trim().is_empty()
                    || request.job_id.len() > 128
                    || request.job_id != request.idempotency_key
                {
                    return serde_json::json!({"status":"rejected","error_code":"invalid_request_identity"});
                }
                image_request.idempotency_key = request.idempotency_key.clone();
                image_request.job_id = request.job_id.clone();
                if let Err(error) = runtime.preflight_request(&image_request) {
                    return serde_json::json!({"status":"rejected","error_code":error.code()});
                }
                let background_permit = match self.background_tasks.try_acquire() {
                    Some(permit) => permit,
                    None => {
                        return serde_json::json!({"status":"rejected","error_code":"image_job_capacity_reached"})
                    }
                };
                let permit = match runtime.try_reserve_slot() {
                    Ok(permit) => permit,
                    Err(error) => {
                        return serde_json::json!({"status":"rejected","error_code":error.code()})
                    }
                };
                if let Err(error) = runtime.reserve_job(&request.job_id, client_id) {
                    if matches!(
                        error,
                        crate::image_generation::ImageGenerationError::InvalidRequest(
                            "job_already_active"
                        )
                    ) {
                        return serde_json::json!({"status":"queued","job_id":request.job_id});
                    }
                    return serde_json::json!({"status":"rejected","error_code":error.code()});
                }
                let worker = runtime.clone();
                let worker_client = client_id.to_owned();
                self.background_tasks
                    .spawn_reserved(background_permit, async move {
                        let _ = worker
                            .start_with_permit(&worker_client, image_request, permit)
                            .await;
                    })
                    .await;
                serde_json::json!({"status":"accepted","job_id":request.job_id})
            }
            _ => serde_json::json!({"status":"rejected","error_code":"unsupported_operation"}),
        }
    }

    async fn write_image_generation_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        request: &generated::ImageGenerationCommand,
        result: serde_json::Value,
    ) -> Result<(), IpcBridgeError> {
        let projection_json = serde_json::to_vec(&result)?;
        let status = result
            .get("status")
            .or_else(|| result.get("state"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("completed")
            .to_owned();
        let error_code = result
            .get("error_code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let event = generated::ImageGenerationEvent {
            schema_version: 1,
            request_id: request.request_id.clone(),
            operation: request.operation.clone(),
            status,
            projection_json: projection_json.clone(),
            error_code,
        };
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: "image_generation.result".into(),
                payload: projection_json,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: Some(generated::event_envelope::Event::ImageGeneration(event)),
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod image_generation_ipc_tests {
    use super::*;
    use prost::Message;
    use tokio::io::duplex;

    #[tokio::test]
    async fn image_response_is_typed_and_contains_metadata_only() {
        let path =
            std::env::temp_dir().join(format!("evohime-image-ipc-{}.db", uuid::Uuid::now_v7()));
        let journal = EventJournal::open(&path).expect("journal opens");
        let bridge = IpcBridge::new(journal);
        let request = generated::ImageGenerationCommand {
            schema_version: 1,
            request_id: "request-1".into(),
            operation: "capability".into(),
            job_id: String::new(),
            payload: Vec::new(),
            idempotency_key: String::new(),
        };
        let projection =
            serde_json::json!({"state":"unsupported","reason_code":"no_image_output_adapter"});
        let (mut client, mut server) = duplex(4096);
        bridge
            .write_image_generation_response(&mut server, &request, projection)
            .await
            .expect("typed response writes");
        let frame = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(frame.as_slice()).expect("response decodes");
        assert_eq!(event.event_type, "image_generation.result");
        assert!(
            matches!(event.event, Some(generated::event_envelope::Event::ImageGeneration(value)) if value.request_id == "request-1" && value.status == "unsupported")
        );
        assert!(!String::from_utf8_lossy(&event.payload).contains("prompt"));
        drop(bridge);
        let _ = std::fs::remove_file(path);
    }
}
