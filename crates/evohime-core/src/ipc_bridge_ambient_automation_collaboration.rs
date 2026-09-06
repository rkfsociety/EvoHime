use super::*;

impl IpcBridge {
    pub(super) async fn dispatch_automation_collaboration<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        _request_id: String,
        _client_id: String,
        _command_hash: String,
        command: Option<generated::command_envelope::Command>,
    ) -> Result<(), IpcBridgeError> {
        match command {
                Some(generated::command_envelope::Command::ListAutomationSchedules(request)) => {
                    let result = self.dispatch_list_automation_schedules(request).await;
                    self.write_response(
                        writer,
                        "automation.schedules",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::TeamSopProtocolsList(request))
                | Some(generated::command_envelope::Command::TeamSopProtocolsAction(request)) => {
                    let result = self.dispatch_team_sop_protocols(request).await;
                    self.write_team_sop_protocols_response(writer, serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::HumanWorkItems(request)) => {
                    let result = self.dispatch_human_work_items(request).await;
                    self.write_human_work_items_response(writer, serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::AgenticBrowserSession(request)) => {
                    let result = self.dispatch_agentic_browser_session(request).await;
                    self.write_agentic_browser_session_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ArtifactHandoffRegistry(request)) => {
                    let result = self.dispatch_artifact_handoff_registry(request).await;
                    self.write_artifact_handoff_registry_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetConversationEvents(request)) => {
                    let result = self
                        .dispatch_conversation_event_log(request, "history")
                        .await;
                    self.write_conversation_event_log_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubscribeConversationEvents(
                    request,
                )) => {
                    let result = self
                        .dispatch_conversation_event_log(request, "subscribed")
                        .await;
                    self.write_conversation_event_log_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetConversationWorkbench(request)) => {
                    let result = self.dispatch_conversation_workbench(request).await;
                    self.write_conversation_workbench_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CausalCollaborationBus(request)) => {
                    let result = self.dispatch_causal_collaboration_bus(request).await;
                    self.write_causal_collaboration_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CausalCollaborationBusSubscribe(
                    request,
                )) => {
                    let result = self.dispatch_causal_collaboration_subscribe(request).await;
                    self.write_causal_collaboration_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SaveAutomationSchedule(request)) => {
                    let result = self.dispatch_save_automation_schedule(request).await;
                    self.write_response(
                        writer,
                        "automation.schedule_saved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::TriggerAutomation(request)) => {
                    let result = self.dispatch_trigger_automation(request).await;
                    self.write_response(
                        writer,
                        "automation.triggered",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListAutomationRuns(request)) => {
                    let result = self.dispatch_list_automation_runs(request).await;
                    self.write_response(writer, "automation.runs", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAutomationRun(request)) => {
                    let result = self.dispatch_get_automation_run(request).await;
                    self.write_response(writer, "automation.run", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListAutomationEvents(request)) => {
                    let result = self.dispatch_list_automation_events(request).await;
                    self.write_response(writer, "automation.events", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::CancelAutomationRun(request)) => {
                    let result = self.dispatch_cancel_automation_run(request).await;
                    self.write_response(
                        writer,
                        "automation.cancelled",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::SetAutomationScheduleEnabled(
                    request,
                )) => {
                    let result = self.dispatch_set_automation_schedule_enabled(request).await;
                    self.write_response(
                        writer,
                        "automation.schedule_enabled",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }

            _ => unreachable!("command routed to the wrong domain"),
        }
        Ok(())
    }
}
