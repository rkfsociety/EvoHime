use super::*;

impl IpcBridge {
    pub(super) async fn dispatch_ambient_workflow<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        _request_id: String,
        _client_id: String,
        _command_hash: String,
        command: Option<generated::command_envelope::Command>,
    ) -> Result<(), IpcBridgeError> {
        match command {
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
                    self.write_response(
                        writer,
                        "ambient.policy_saved",
                        serde_json::to_vec(&result)?,
                    )
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
                    self.write_response(
                        writer,
                        "workflow.definition",
                        serde_json::to_vec(&result)?,
                    )
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
                Some(generated::command_envelope::Command::VisualWorkflowBuilder(request)) => {
                    let result = self.dispatch_visual_workflow_builder(request).await;
                    self.write_response(
                        writer,
                        "workflow_builder.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ConversationalWorkflowComposer(
                    request,
                )) => {
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
                Some(generated::command_envelope::Command::IntegrationProviderSdkCatalog(
                    request,
                ))
                | Some(generated::command_envelope::Command::IntegrationProviderSdkAction(
                    request,
                )) => {
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
                    let result = self.dispatch_benchmark_matrix(request);
                    self.write_response(
                        writer,
                        "benchmark_matrix.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::AgentMiddlewarePipelineList(
                    request,
                ))
                | Some(generated::command_envelope::Command::AgentMiddlewarePipelineAction(
                    request,
                )) => {
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
                    self.write_sensitive_data_guardrails_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExecutionPolicyProfiles(request)) => {
                    let result = self.dispatch_execution_policy_profiles(request);
                    self.write_execution_policy_profiles_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ModelResiliencePolicy(request)) => {
                    let result = self.dispatch_model_resilience_policy(request);
                    self.write_model_resilience_policy_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
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
                    self.write_tool_simulation_runtime_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExternalCodingAgentAdapterList(
                    request,
                ))
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
