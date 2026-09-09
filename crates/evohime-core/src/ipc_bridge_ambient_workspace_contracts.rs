use super::*;

impl IpcBridge {
    pub(super) async fn dispatch_workspace_contracts<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        _request_id: String,
        _client_id: String,
        _command_hash: String,
        command: Option<generated::command_envelope::Command>,
    ) -> Result<(), IpcBridgeError> {
        match command {
            Some(generated::command_envelope::Command::StartPlanReview(request)) => {
                self.start_plan_review(request, writer).await?;
            }
            Some(generated::command_envelope::Command::PlanArtifactCreate(request))
            | Some(generated::command_envelope::Command::PlanArtifactRead(request))
            | Some(generated::command_envelope::Command::PlanArtifactAction(request)) => {
                let operation = if request.operation.is_empty() {
                    "read".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self.dispatch_plan_artifact(operation, request).await?;
                self.write_response(writer, "plan_artifact.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::WorkspaceStateCheckpoint(request)) => {
                let operation = if request.operation.is_empty() {
                    "compare".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_workspace_state_checkpoint(operation, request)
                    .await?;
                self.write_response(writer, "workspace_state_checkpoint.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::IncrementalChangeProtocol(request)) => {
                let operation = if request.operation.is_empty() {
                    "status".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_incremental_change_protocol(operation, request)
                    .await?;
                self.write_response(writer, "incremental_change_protocol.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RevisionSafeWorkspaceFiles(request)) => {
                let operation = if request.operation.is_empty() {
                    "read".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_revision_safe_workspace_files(operation, request)
                    .await?;
                self.write_response(writer, "revision_safe_workspace_files.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::TaskWorktreeIsolation(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_task_worktree_isolation(operation, request)
                    .await?;
                self.write_response(writer, "task_worktree_isolation.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::TeamResourceBudget(request)) => {
                let operation = if request.operation.is_empty() {
                    "validate_policy".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_team_resource_budget(operation, request)
                    .await?;
                self.write_response(writer, "team_resource_budget.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ComposableTerminationConditions(
                request,
            )) => {
                let operation = if request.operation.is_empty() {
                    "validate_policy".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_composable_termination_conditions(operation, request)
                    .await?;
                self.write_response(writer, "composable_termination_conditions.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::WorkspaceBootstrapManifest(request)) => {
                let operation = if request.operation.is_empty() {
                    "validate".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_workspace_bootstrap_manifest(operation, request)
                    .await?;
                self.write_response(writer, "workspace_bootstrap_manifest.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::TeamCoordinationPolicies(request)) => {
                let operation = if request.operation.is_empty() {
                    "validate_policy".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_team_coordination_policies(operation, request)
                    .await?;
                self.write_response(writer, "team_coordination_policies.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::TypedAgentHandoffContract(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_typed_agent_handoff_contract(operation, request)
                    .await?;
                self.write_response(writer, "typed_agent_handoff_contract.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SchemaDrivenAgentConfiguration(request)) => {
                let operation = if request.operation.is_empty() {
                    "get_schema".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_schema_driven_agent_configuration(operation, request)
                    .await?;
                self.write_response(writer, "schema_driven_agent_configuration.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ExperienceReplayLibrary(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_experience_replay_library(operation, request)
                    .await?;
                self.write_response(writer, "experience_replay_library.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RuntimeInterventionPipeline(request)) => {
                let operation = if request.operation.is_empty() {
                    "evaluate".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_runtime_intervention_pipeline(operation, request)
                    .await?;
                self.write_response(writer, "runtime_intervention_pipeline.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CodeDiagnosticsFeedbackLoop(request)) => {
                let operation = if request.operation.is_empty() {
                    "status".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_code_diagnostics_feedback_loop(operation, request)
                    .await?;
                self.write_response(writer, "code_diagnostics_feedback_loop.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CodeReviewLane(request)) => {
                let operation = if request.operation.is_empty() { "get".to_owned() } else { request.operation.clone() };
                let result = self.dispatch_code_review_lane(request).await?;
                self.write_response(writer, &format!("code_review_lane.{operation}"), result).await?;
            }
            Some(generated::command_envelope::Command::StaticAnalysisPacks(request)) => {
                let operation = if request.operation.is_empty() { "inspect".to_owned() } else { request.operation.clone() };
                let result = self.dispatch_static_analysis_packs(request).await?;
                self.write_response(writer, &format!("static_analysis_packs.{operation}"), result).await?;
            }
            Some(generated::command_envelope::Command::ContextLoadouts(request)) => { let operation=if request.operation.is_empty(){"get".to_owned()}else{request.operation.clone()}; let result=self.dispatch_context_loadouts(request).await?; self.write_response(writer,&format!("context_loadouts.{operation}"),result).await?; }
            Some(generated::command_envelope::Command::SkillSourceLifecycle(request)) => { let operation=if request.operation.is_empty(){"get".to_owned()}else{request.operation.clone()}; let result=self.dispatch_skill_source_lifecycle(request).await?; self.write_response(writer,&format!("skill_source_lifecycle.{operation}"),result).await?; }
            Some(generated::command_envelope::Command::WorkflowOptimizationLab(request)) => {
                let operation = if request.operation.is_empty() {
                    "get_run".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_workflow_optimization_lab(operation, request)
                    .await?;
                self.write_response(writer, "workflow_optimization_lab.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CoreTopicSubscriptionEventBus(request)) => {
                let operation = if request.operation.is_empty() {
                    "subscribe".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_core_topic_subscription_event_bus(operation, request)
                    .await?;
                self.write_response(writer, "core_topic_subscription_event_bus.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::DependencyAwareTaskGraph(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_dependency_aware_task_graph(operation, request)
                    .await?;
                self.write_response(writer, "dependency_aware_task_graph.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::DeclarativeAgentComponentRegistry(
                request,
            )) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_declarative_agent_component_registry(operation, request)
                    .await?;
                self.write_response(
                    writer,
                    "declarative_agent_component_registry.result",
                    result,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::TypedContextReferences(request)) => {
                let operation = if request.operation.is_empty() {
                    "resolve".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_typed_context_references(operation, request)
                    .await?;
                self.write_response(writer, "typed_context_references.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SafeUiExtensionFramework(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_safe_ui_extension_framework(operation, request)
                    .await?;
                self.write_response(writer, "safe_ui_extension_framework.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CapabilityWorkbench(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_capability_workbench(operation, request)
                    .await?;
                self.write_response(writer, "capability_workbench.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::TeamCoordinator(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self.dispatch_team_coordinator(operation, request).await?;
                self.write_response(writer, "team_coordinator.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ProjectInstructionStack(request)) => {
                let operation = if request.operation.is_empty() {
                    "discover".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_project_instruction_stack(operation, request)
                    .await?;
                self.write_response(writer, "project_instruction_stack.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::WorkspaceSets(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self.dispatch_workspace_sets(operation, request).await?;
                self.write_response(writer, "workspace_sets.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::KnowledgeSourceRegistry(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_knowledge_source_registry(operation, request)
                    .await?;
                self.write_response(writer, "knowledge_source_registry.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::AgentGitChangeSets(request)) => {
                let operation = if request.operation.is_empty() {
                    "get_candidate".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_agent_git_change_sets(operation, request)
                    .await?;
                self.write_response(writer, "agent_git_change_sets.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ArchitectEditorPipeline(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_architect_editor_pipeline(operation, request)
                    .await?;
                self.write_response(writer, "architect_editor_pipeline.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::EventVisualizerRegistry(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_event_visualizer_registry(operation, request)
                    .await?;
                self.write_response(writer, "event_visualizer_registry.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ReasoningOperatorLibrary(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_reasoning_operator_library(operation, request)
                    .await?;
                self.write_response(writer, "reasoning_operator_library.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::OutputGuardrailPipeline(request)) => {
                let operation = if request.operation.is_empty() {
                    "evaluate".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_output_guardrail_pipeline(operation, request)
                    .await?;
                self.write_response(writer, "output_guardrail_pipeline.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CustomizationInventory(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_customization_inventory(operation, request)
                    .await?;
                self.write_response(writer, "customization_inventory.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::StandingApprovalProfiles(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_standing_approval_profiles(operation, request)
                    .await?;
                self.write_response(writer, "standing_approval_profiles.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ApprovalPolicyProfiles(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_approval_policy_profiles(operation, request)
                    .await?;
                self.write_response(writer, "approval_policy_profiles.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CheckpointForking(request)) => {
                let operation = if request.operation.is_empty() {
                    "fork".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self.dispatch_checkpoint_forking(operation, request).await?;
                self.write_response(writer, "checkpoint_forking.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::PrivacyTelemetryGovernance(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_privacy_telemetry_governance(operation, request)
                    .await?;
                self.write_response(writer, "privacy_telemetry_governance.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ConversationBridgeAdapters(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_conversation_bridge_adapters(operation, request)
                    .await?;
                self.write_response(writer, "conversation_bridge_adapters.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::MemoryViewsAndAdaptiveRecall(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_memory_views_and_adaptive_recall(operation, request)
                    .await?;
                self.write_response(writer, "memory_views_and_adaptive_recall.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ModelEditProtocolRegistry(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_model_edit_protocol_registry(operation, request)
                    .await?;
                self.write_response(writer, "model_edit_protocol_registry.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RemoteConversationChannels(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_remote_conversation_channels(operation, request)
                    .await?;
                self.write_response(writer, "remote_conversation_channels.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::PromptCachePlanner(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_prompt_cache_planner(operation, request)
                    .await?;
                self.write_response(writer, "prompt_cache_planner.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::DeclarativeRuntimeComponents(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_declarative_runtime_components(operation, request)
                    .await?;
                self.write_response(writer, "declarative_runtime_components.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GuidedCalibrationSessions(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_guided_calibration_sessions(operation, request)
                    .await?;
                self.write_response(writer, "guided_calibration_sessions.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ExtensionConformanceKit(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_extension_conformance_kit(operation, request)
                    .await?;
                self.write_response(writer, "extension_conformance_kit.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::DurableRemoteTaskBridge(request)) => {
                let operation = if request.operation.is_empty() {
                    "status".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_durable_remote_task_bridge(operation, request)
                    .await?;
                self.write_response(writer, "durable_remote_task_bridge.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::MessageInterventionPolicies(request)) => {
                let operation = if request.operation.is_empty() {
                    "evaluate".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_message_intervention_policies(operation, request)
                    .await?;
                self.write_response(writer, "message_intervention_policies.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::BatchInvocationRuntime(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_batch_invocation_runtime(operation, request)
                    .await?;
                self.write_response(writer, "batch_invocation_runtime.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::PolicyAwareToolResultCache(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_policy_aware_tool_result_cache(operation, request)
                    .await?;
                self.write_response(writer, "policy_aware_tool_result_cache.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CodeAnchoredIntentMarkers(request)) => {
                let operation = if request.operation.is_empty() {
                    "scan".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_code_anchored_intent_markers(operation, request)
                    .await?;
                self.write_response(writer, "code_anchored_intent_markers.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ModelPurposeRouting(request)) => {
                let operation = if request.operation.is_empty() {
                    "get".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_model_purpose_routing(operation, request)
                    .await?;
                self.write_response(writer, "model_purpose_routing.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::LocalModelRuntimeManager(request)) => {
                let operation = if request.operation.is_empty() {
                    "inspect".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_local_model_runtime_manager(operation, request)
                    .await?;
                self.write_response(writer, "local_model_runtime_manager.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ArchitectureSnapshot(request)) => {
                let operation = if request.operation.is_empty() {
                    "current".to_owned()
                } else {
                    request.operation.clone()
                };
                let result = self
                    .dispatch_architecture_snapshot(operation, request)
                    .await?;
                self.write_response(writer, "architecture_snapshot.result", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::PersistentAgentOrganizationRegistry(
                request,
            )) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let agent_id = request.agent_id.clone();
                let request_id = request.request_id.clone();
                let result = self
                    .dispatch_persistent_agent_organization_registry(operation, request)
                    .await?;
                self.write_persistent_agent_organization_registry_response(
                    writer,
                    &request_id,
                    &agent_id,
                    result,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ExecutionEnvironmentProfile(request)) => {
                let operation = if request.operation.is_empty() {
                    "list".to_owned()
                } else {
                    request.operation.clone()
                };
                let profile_id = request.profile_id.clone();
                let request_id = request.request_id.clone();
                let result = self
                    .dispatch_execution_environment_profile(operation, request)
                    .await?;
                self.write_execution_environment_profile_response(
                    writer,
                    &request_id,
                    &profile_id,
                    result,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ContextNamespace(request)) => {
                let operation = if request.operation.is_empty() {
                    "list_children".to_owned()
                } else {
                    request.operation.clone()
                };
                let request_id = request.request_id.clone();
                let result = self.dispatch_context_namespace(operation, request).await?;
                self.write_context_namespace_response(writer, &request_id, result)
                    .await?;
            }
            Some(generated::command_envelope::Command::BackgroundExecution(request)) => {
                let operation = if request.operation.is_empty() {
                    "list_runs".to_owned()
                } else {
                    request.operation.clone()
                };
                let request_id = request.request_id.clone();
                let run_id = request.run_id.clone();
                let result = self
                    .dispatch_durable_background_execution(operation.clone(), request)
                    .await?;
                self.write_durable_background_execution_response(
                    writer,
                    &request_id,
                    &run_id,
                    &operation,
                    result,
                )
                .await?;
            }

            _ => unreachable!("command routed to the wrong domain"),
        }
        Ok(())
    }
}
