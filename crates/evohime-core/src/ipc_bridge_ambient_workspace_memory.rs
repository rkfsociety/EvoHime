use super::*;

impl IpcBridge {
    pub(super) async fn dispatch_workspace_memory<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        request_id: String,
        _client_id: String,
        _command_hash: String,
        command: Option<generated::command_envelope::Command>,
        protocol: Option<generated::ProtocolVersion>,
    ) -> Result<(), IpcBridgeError> {
        match command {
                Some(generated::command_envelope::Command::StopTask(stop)) => {
                    if let Some(coordinator) = &self.coordinator {
                        coordinator
                            .dispatch(CoreCommand::StopTask {
                                task_id: stop.task_id,
                            })
                            .await
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                }
                Some(generated::command_envelope::Command::ListWorkspace(request)) => {
                    let listing = crate::workspace::list_directory(
                        request.workspace_path,
                        if request.relative_path.is_empty() {
                            "."
                        } else {
                            &request.relative_path
                        },
                        if request.max_entries == 0 {
                            crate::workspace::MAX_LIST_ENTRIES
                        } else {
                            request.max_entries as usize
                        },
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = serde_json::to_vec(&listing)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "workspace.list", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::PauseContinuation(request)) => {
                    let payload = self
                        .dispatch_transition_continuation(
                            request.run_id,
                            request.idempotency_key,
                            request.expected_state,
                            "paused",
                            "pause",
                        )
                        .await
                        .unwrap_or_else(error_response_payload);
                    self.write_continuation_action(writer, payload).await?;
                }
                Some(generated::command_envelope::Command::ResumeContinuation(request)) => {
                    let run = self.dispatch_resume_continuation(request).await;
                    let payload = match run {
                        Ok(run) => {
                            if let Some(coordinator) = &self.coordinator {
                                if let (Some(prompt), Some(workspace_path)) =
                                    (run.prompt.clone(), run.workspace_path.clone())
                                {
                                    let _ = coordinator
                                        .dispatch(CoreCommand::StartTask {
                                            task_id: run.task_id.clone(),
                                            prompt,
                                            workspace_root: Some(workspace_path.into()),
                                            preferred_route_hint: None,
                                        })
                                        .await;
                                }
                            }
                            serde_json::to_vec(&serde_json::json!({
                                "run_id": run.run_id,
                                "action": "resume",
                                "applied": true,
                                "error_code": ""
                            }))
                            .unwrap_or_default()
                        }
                        Err(error) => error_response_payload(error),
                    };
                    self.write_continuation_action(writer, payload).await?;
                }
                Some(generated::command_envelope::Command::ReadWorkspaceFile(request)) => {
                    let content = crate::workspace::read_text_file(
                        request.workspace_path,
                        &request.relative_path,
                        if request.max_bytes == 0 {
                            crate::workspace::MAX_READ_BYTES
                        } else {
                            request.max_bytes as usize
                        },
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = serde_json::to_vec(&serde_json::json!({
                        "path": request.relative_path,
                        "content": content,
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "workspace.file", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::GitStatus(request)) => {
                    let payload = self
                        .dispatch_git_read(
                            request.workspace_path,
                            "git.status",
                            serde_json::Value::Null,
                            request.max_bytes,
                        )
                        .await?;
                    self.write_response(writer, "git.status", payload).await?;
                }
                Some(generated::command_envelope::Command::GitDiff(request)) => {
                    let input = if request.relative_path.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!({"path": request.relative_path})
                    };
                    let payload = self
                        .dispatch_git_read(
                            request.workspace_path,
                            "git.diff",
                            input,
                            request.max_bytes,
                        )
                        .await?;
                    self.write_response(writer, "git.diff", payload).await?;
                }
                Some(generated::command_envelope::Command::TerminalExecute(request)) => {
                    self.dispatch_terminal_execute(request, writer).await?;
                }
                Some(generated::command_envelope::Command::RunDoctor(request)) => {
                    let result = self
                        .dispatch_run_doctor(
                            request.project_id,
                            request.detail_level,
                            protocol,
                        )
                        .await?;
                    self.write_response(writer, "doctor.report", result).await?;
                }
                Some(generated::command_envelope::Command::CreateDiagnosticsSnapshot(request)) => {
                    let result = self
                        .dispatch_create_diagnostics_snapshot(request, protocol)
                        .await?;
                    self.write_response(writer, "diagnostics.snapshot", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ExportDoctorLogs(request)) => {
                    let result = self
                        .dispatch_export_doctor_logs(request.destination_path)
                        .await?;
                    self.write_response(writer, "doctor.export.completed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateDatabaseBackup(request)) => {
                    self.dispatch_create_database_backup(
                        request_id,
                        request.destination_path,
                        writer,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::PrepareDatabaseRestore(request)) => {
                    let result = self
                        .dispatch_prepare_database_restore(request_id, request.backup_path)
                        .await?;
                    self.write_response(writer, "storage.restore.preview", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RestoreDatabase(request)) => {
                    self.dispatch_restore_database(
                        request_id,
                        request.backup_path,
                        request.approval_id,
                        writer,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::CancelDatabaseOperation(request)) => {
                    let result = self
                        .dispatch_cancel_database_operation(request.operation_id)
                        .await?;
                    self.write_response(writer, "storage.cancel.requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SaveResearchEvidence(request)) => {
                    let result = self.dispatch_save_research_evidence(request).await?;
                    self.write_response(writer, "research.evidence.saved", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListResearchEvidence(request)) => {
                    let result = self
                        .dispatch_list_research_evidence(request.work_item_id)
                        .await?;
                    self.write_response(writer, "research.evidence.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RunResearchFetch(request)) => {
                    let result = self.dispatch_run_research_fetch(request).await?;
                    self.write_response(writer, "research.fetch.completed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateMemory(request)) => {
                    let result = self.dispatch_create_memory(request).await?;
                    self.write_response(writer, "memory.created", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListMemory(request)) => {
                    let result = self.dispatch_list_memory(request).await?;
                    self.write_response(writer, "memory.list", result).await?;
                }
                Some(generated::command_envelope::Command::SearchMemory(request)) => {
                    let result = self.dispatch_search_memory(request).await?;
                    self.write_response(writer, "memory.search", result).await?;
                }
                Some(generated::command_envelope::Command::ArchiveMemory(request)) => {
                    let result = self.dispatch_archive_memory(request).await?;
                    self.write_response(writer, "memory.archived", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetMemory(request)) => {
                    let result = self.dispatch_get_memory(request).await?;
                    self.write_response(writer, "memory.record", result).await?;
                }
                Some(generated::command_envelope::Command::ListMemoryPending(request)) => {
                    let result = self.dispatch_list_memory_pending(request).await?;
                    self.write_response(writer, "memory.pending", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetMemoryConflicts(request)) => {
                    let result = self.dispatch_get_memory_conflicts(request).await?;
                    self.write_response(writer, "memory.conflicts", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ConfirmMemory(request)) => {
                    let result = self.dispatch_confirm_memory(request).await?;
                    self.write_response(writer, "memory.confirmed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RejectMemory(request)) => {
                    let result = self.dispatch_reject_memory(request).await?;
                    self.write_response(writer, "memory.rejected", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReviseMemoryCandidate(request)) => {
                    let result = self.dispatch_revise_memory_candidate(request).await?;
                    self.write_response(writer, "memory.revised", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SupersedeMemory(request)) => {
                    let result = self.dispatch_supersede_memory(request).await?;
                    self.write_response(writer, "memory.superseded", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ForgetMemory(request)) => {
                    let result = self.dispatch_forget_memory(request).await?;
                    self.write_response(writer, "memory.forgotten", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::InstallCapability(request)) => {
                    let result = self.dispatch_install_capability(request).await?;
                    self.write_response(writer, "capability.installed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListCapabilities(request)) => {
                    let result = self.dispatch_list_capabilities(request).await?;
                    self.write_response(writer, "capability.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::MatchCapabilities(request)) => {
                    let result = self.dispatch_match_capabilities(request).await?;
                    self.write_response(writer, "capability.match", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RemoveCapability(request)) => {
                    let result = self.dispatch_remove_capability(request).await?;
                    self.write_response(writer, "capability.removed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListToolkits(request)) => {
                    let result = self.dispatch_list_toolkits(request).await?;
                    self.write_response(writer, "toolkit.list", result).await?;
                }
                Some(generated::command_envelope::Command::EnableToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "rollback",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.enabled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::DisableToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "disabled",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.disabled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RollbackToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "enabled",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.rolled_back", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetCapabilitySelection(request)) => {
                    let result = self.dispatch_get_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PinCapabilitySelection(request)) => {
                    let result = self.dispatch_pin_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection.pinned", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReplaceCapabilitySelection(request)) => {
                    let result = self.dispatch_replace_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection.replaced", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RequestChildHandoff(request)) => {
                    let result = self.dispatch_request_child_handoff(request).await?;
                    self.write_response(writer, "child.handoff.requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListChildHandoffs(request)) => {
                    let result = self.dispatch_list_child_handoffs(request).await?;
                    self.write_response(writer, "child.handoff.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitChildRequest(request)) => {
                    let result = self.dispatch_submit_child_request(request).await?;
                    self.write_response(writer, "child.request.submitted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitChildReport(request)) => {
                    let result = self.dispatch_submit_child_report(request).await?;
                    self.write_response(writer, "child.report.accepted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitFeedback(request)) => {
                    let result = self.dispatch_submit_feedback(request).await?;
                    self.write_response(writer, "feedback.submitted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::IndexWorkspace(request)) => {
                    let result = self.dispatch_index_workspace(request, false).await?;
                    self.write_response(writer, "workspace.indexed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RebuildIndex(request)) => {
                    let result = self.dispatch_rebuild_index(request).await?;
                    self.write_response(writer, "workspace.indexed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SearchWorkspaceKnowledge(request)) => {
                    let result = self.dispatch_search_workspace_knowledge(request).await?;
                    self.write_response(writer, "workspace.knowledge", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetIndexStatus(request)) => {
                    let result = self.dispatch_get_index_status(request).await?;
                    self.write_response(writer, "workspace.index_status", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CancelWorkspaceIndex(request)) => {
                    let result = self.dispatch_cancel_workspace_index(request).await?;
                    self.write_response(writer, "workspace.index_cancelled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetContextLedger(request)) => {
                    let result = self.dispatch_get_context_ledger(request).await?;
                    self.write_response(writer, "context.ledger", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListTaskScratchpad(request)) => {
                    let result = self.dispatch_list_task_scratchpad(request).await?;
                    self.write_response(writer, "context.scratchpad", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ClearTaskScratchpad(request)) => {
                    let result = self.dispatch_clear_task_scratchpad(request).await?;
                    self.write_response(writer, "context.scratchpad_cleared", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SummarizeContextNow(request)) => {
                    let result = self.dispatch_summarize_context_now(request).await?;
                    self.write_response(writer, "context.summarize_requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PinContextItem(request)) => {
                    let result = self.dispatch_pin_context_item(request).await?;
                    self.write_response(writer, "context.item_pinned", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReadContextArtifact(request)) => {
                    let result = self.dispatch_read_context_artifact(request).await?;
                    self.write_response(writer, "context.artifact", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListFeedback(request)) => {
                    let result = self.dispatch_list_feedback(request).await?;
                    self.write_response(writer, "feedback.list", result).await?;
                }
                Some(generated::command_envelope::Command::ResolveApproval(resolve)) => {
                    // Cancellation is a terminal rejection at the existing
                    // approval boundary; the immutable approval binding remains
                    // owned by Core and old clients keep the same semantics.
                    let granted = resolve.granted && !resolve.cancel;
                    let approval_id = uuid::Uuid::parse_str(&resolve.approval_id)
                        .map_err(|error| FrameError::Io(format!("invalid approval id: {error}")))?;
                    if let Some(tools) = &self.tools {
                        let _ = tools.permissions().resolve(approval_id, granted).await;
                    }
                    if let Some(approvals) = &self.approvals {
                        let _ = approvals.resolve(approval_id, granted).await;
                    }
                    if !granted {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        ) {
                            let _ = runtime.deny_approval(approval_id);
                        }
                    }
                    let _ = self
                        .journal
                        .record_audit(
                            &resolve.approval_id,
                            "approval.decision",
                            serde_json::to_vec(&serde_json::json!({
                                "approval_id": resolve.approval_id,
                                "granted": granted,
                                "cancelled": resolve.cancel,
                                "idempotency_key": resolve.idempotency_key,
                                "rejection_reason": resolve.rejection_reason,
                            }))
                            .unwrap_or_default()
                            .as_slice(),
                        )
                        .await;
                    self.record_ledger_approval_decision(&resolve.approval_id, granted)
                        .await;
                    // Узел workflow подтверждается той же командой, что и
                    // инструмент: отдельного пути approval у workflow нет. Если
                    // идентификатор принадлежит узлу, запуск продолжается сам —
                    // иначе он остался бы ждать уже принятого решения.
                    if self
                        .workflow_approvals
                        .resolve(&resolve.approval_id, granted)
                    {
                        if let Some(run_id) = self.workflow_approvals.run_for(&resolve.approval_id)
                        {
                            let workspace = self.journal.workflow_run_workspace(&run_id).await;
                            let _ = self.spawn_workflow_drive(run_id, workspace).await;
                        }
                    }
                }
                Some(generated::command_envelope::Command::ResolveRoutingDecision(resolve)) => {
                    let coordinator = self.coordinator.as_ref().ok_or_else(|| {
                        FrameError::Io("core command queue is not configured".into())
                    })?;
                    let (reply, response) = oneshot::channel();
                    coordinator
                        .dispatch(CoreCommand::ResolveRoutingDecision {
                            trace_id: resolve.trace_id,
                            approve: resolve.approve,
                            reply,
                        })
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let result = response
                        .await
                        .map_err(|_| FrameError::Io("routing decision response dropped".into()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "routing.decision", result)
                        .await?;
                }

            _ => unreachable!("command routed to the wrong domain"),
        }
        Ok(())
    }
}
