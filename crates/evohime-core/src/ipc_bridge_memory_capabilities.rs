impl IpcBridge {
    pub(crate) async fn dispatch_create_memory(
        &self,
        request: generated::CreateMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                title: request.title,
                content: request.content,
                provenance_kind: request.provenance_kind,
                provenance_id: request.provenance_id,
                provenance_locator: request.provenance_locator,
                privacy: request.privacy,
                ttl_ms: request.ttl_ms,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_list_memory(
        &self,
        request: generated::ListMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                include_archived: request.include_archived,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_search_memory(
        &self,
        request: generated::SearchMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SearchMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                query: request.query,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_archive_memory(
        &self,
        request: generated::ArchiveMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ArchiveMemory {
                id: request.id,
                approval_id: request.approval_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_forget_memory(
        &self,
        request: generated::ForgetMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ForgetMemory {
                id: request.id,
                approval_id: request.approval_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_memory(
        &self,
        request: generated::GetMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetMemory {
                id: request.id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_list_memory_pending(
        &self,
        request: generated::ListMemoryPending,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListMemoryPending {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                limit: request.limit,
                workspace_path: request.workspace_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_memory_conflicts(
        &self,
        request: generated::GetMemoryConflicts,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetMemoryConflicts {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                limit: request.limit,
                workspace_path: request.workspace_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_confirm_memory(
        &self,
        request: generated::ConfirmMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ConfirmMemory {
                ids: request.ids,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_reject_memory(
        &self,
        request: generated::RejectMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RejectMemory {
                ids: request.ids,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_revise_memory_candidate(
        &self,
        request: generated::ReviseMemoryCandidate,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ReviseMemoryCandidate {
                id: request.id,
                statement: request.statement,
                session_only: request.session_only,
                session_id: request.session_id,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_supersede_memory(
        &self,
        request: generated::SupersedeMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SupersedeMemory {
                old_id: request.old_id,
                new_id: request.new_id,
                reason: request.reason,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_install_capability(
        &self,
        request: generated::InstallCapability,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let toolkit_manifest = serde_json::from_str::<serde_json::Value>(&request.manifest_json)
            .ok()
            .filter(|value| {
                value.get("kind").and_then(serde_json::Value::as_str) == Some("tool/manifest/v1")
            });
        let toolkit_source = request.install_source.clone();
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::InstallCapability {
                manifest_json: request.manifest_json,
                install_source: request.install_source,
                source_path: request.source_path,
                expected_content_hash: request.expected_content_hash,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let result = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if let Some(manifest) = toolkit_manifest {
            let record = evohime_local_storage::toolkit_store::ToolkitRecord {
                toolkit_id: manifest
                    .get("tool_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                version: manifest
                    .get("version")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                manifest_hash: serde_json::from_value::<evohime_tool_runtime::ToolManifest>(
                    manifest.clone(),
                )
                .ok()
                .and_then(|value| value.canonical_hash().ok())
                .unwrap_or_default(),
                source: toolkit_source,
                package_hash: manifest
                    .get("package_hash")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                license: manifest
                    .get("license")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                status: "available".into(),
                compatible_core: manifest
                    .get("compatible_core")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                manifest_json: serde_json::to_vec(&manifest).unwrap_or_default(),
                created_at: String::new(),
                updated_at: String::new(),
            };
            if !record.toolkit_id.is_empty() && !record.version.is_empty() {
                let database = self.journal.database();
                let database = database.lock().await;
                evohime_local_storage::toolkit_store::discover(database.connection(), &record)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
            }
        }
        Ok(result)
    }

    pub(crate) async fn dispatch_list_capabilities(
        &self,
        request: generated::ListCapabilities,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListCapabilities {
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_match_capabilities(
        &self,
        request: generated::MatchCapabilities,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::MatchCapabilities {
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_remove_capability(
        &self,
        request: generated::RemoveCapability,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RemoveCapability {
                id: request.id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_list_toolkits(
        &self,
        request: generated::ListToolkits,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let database = self.journal.database();
        let database = database.lock().await;
        let records = evohime_local_storage::toolkit_store::list(
            database.connection(),
            request.limit as usize,
        )
        .map_err(|error| FrameError::Io(error.to_string()))?;
        serde_json::to_vec(&serde_json::json!({"toolkits": records})).map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_toolkit_status(
        &self,
        toolkit_id: String,
        version: String,
        reason: String,
        status: &str,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let database = self.journal.database();
        let database = database.lock().await;
        if status == "rollback" {
            evohime_local_storage::toolkit_store::rollback(
                database.connection(),
                &toolkit_id,
                &version,
                &reason,
            )
        } else {
            evohime_local_storage::toolkit_store::transition(
                database.connection(),
                &toolkit_id,
                &version,
                status,
                &reason,
            )
        }
        .map_err(|error| FrameError::Io(error.to_string()))?;
        serde_json::to_vec(&serde_json::json!({
            "toolkit_id": toolkit_id,
            "version": version,
            "status": status,
            "applied": true
        }))
        .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_get_capability_selection(
        &self,
        request: generated::GetCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetCapabilitySelection {
                task_id: request.task_id,
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_pin_capability_selection(
        &self,
        request: generated::PinCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PinCapabilitySelection {
                task_id: request.task_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_replace_capability_selection(
        &self,
        request: generated::ReplaceCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ReplaceCapabilitySelection {
                task_id: request.task_id,
                manifest_name: request.manifest_name,
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_request_child_handoff(
        &self,
        request: generated::RequestChildHandoff,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RequestChildHandoff {
                handoff_id: request.handoff_id,
                task_id: request.task_id,
                kind: request.kind,
                from_role: request.from_role,
                from_name: request.from_name,
                to_role: request.to_role,
                to_name: request.to_name,
                purpose: request.purpose,
                payload: request.payload,
                sequence: request.sequence,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_list_child_handoffs(
        &self,
        request: generated::ListChildHandoffs,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListChildHandoffs {
                task_id: request.task_id,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_submit_child_request(
        &self,
        request: generated::SubmitChildRequest,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildRequest {
                child_task_id: request.child_task_id,
                parent_task_id: request.parent_task_id,
                role: request.role,
                kind: request.kind,
                reduced_context: request.reduced_context,
                max_output_bytes: request.max_output_bytes,
                requested_capabilities: request.requested_capabilities,
                parent_is_child: request.parent_is_child,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_submit_child_report(
        &self,
        request: generated::SubmitChildReport,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildReport {
                child_task_id: request.child_task_id,
                status: request.status,
                summary: request.summary,
                findings: request.findings,
                sources: request.sources,
                confidence_percent: request.confidence_percent,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_submit_feedback(
        &self,
        request: generated::SubmitFeedback,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitFeedback {
                run_id: request.run_id,
                task_id: (!request.task_id.trim().is_empty()).then_some(request.task_id),
                subject_ref: (!request.subject_ref.trim().is_empty())
                    .then_some(request.subject_ref),
                signal: request.signal,
                correction: (!request.correction.trim().is_empty()).then_some(request.correction),
                rejection_reason: (!request.rejection_reason.trim().is_empty())
                    .then_some(request.rejection_reason),
                outcome: (!request.outcome.trim().is_empty()).then_some(request.outcome),
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    pub(crate) async fn dispatch_list_feedback(
        &self,
        request: generated::ListFeedback,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListFeedback {
                run_id: request.run_id,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    /// План 01.5: bounded projection состава контекста.
    pub(crate) async fn dispatch_index_workspace(
        &self,
        request: generated::IndexWorkspace,
        rebuild: bool,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| {
            if rebuild {
                CoreCommand::RebuildIndex {
                    workspace_path: request.workspace_path.clone(),
                    enable_embeddings: request.enable_embeddings,
                    reply,
                }
            } else {
                CoreCommand::IndexWorkspace {
                    workspace_path: request.workspace_path.clone(),
                    enable_embeddings: request.enable_embeddings,
                    reply,
                }
            }
        })
        .await
    }

    pub(crate) async fn dispatch_rebuild_index(
        &self,
        request: generated::RebuildIndex,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::RebuildIndex {
            workspace_path: request.workspace_path.clone(),
            enable_embeddings: request.enable_embeddings,
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_search_workspace_knowledge(
        &self,
        request: generated::SearchWorkspaceKnowledge,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::SearchWorkspaceKnowledge {
            workspace_path: request.workspace_path.clone(),
            query: request.query.clone(),
            path_filter: (!request.path_filter.trim().is_empty())
                .then(|| request.path_filter.clone()),
            language_filter: (!request.language_filter.trim().is_empty())
                .then(|| request.language_filter.clone()),
            hybrid: request.hybrid,
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_get_index_status(
        &self,
        request: generated::GetIndexStatus,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::GetIndexStatus {
            workspace_path: request.workspace_path.clone(),
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_cancel_workspace_index(
        &self,
        request: generated::CancelWorkspaceIndex,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::CancelWorkspaceIndex {
            workspace_path: request.workspace_path.clone(),
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_get_context_ledger(
        &self,
        request: generated::GetContextLedger,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::GetContextLedger {
            task_id: request.task_id.clone(),
            limit: request.limit,
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_list_task_scratchpad(
        &self,
        request: generated::ListTaskScratchpad,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ListTaskScratchpad {
            task_id: request.task_id.clone(),
            category: (!request.category.trim().is_empty()).then(|| request.category.clone()),
            status: (!request.status.trim().is_empty()).then(|| request.status.clone()),
            limit: request.limit,
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_clear_task_scratchpad(
        &self,
        request: generated::ClearTaskScratchpad,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ClearTaskScratchpad {
            task_id: request.task_id.clone(),
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_summarize_context_now(
        &self,
        request: generated::SummarizeContextNow,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::SummarizeContextNow {
            task_id: request.task_id.clone(),
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_pin_context_item(
        &self,
        request: generated::PinContextItem,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::PinContextItem {
            task_id: request.task_id.clone(),
            item_id: request.item_id.clone(),
            pinned: request.pinned,
            reply,
        })
        .await
    }

    pub(crate) async fn dispatch_read_context_artifact(
        &self,
        request: generated::ReadContextArtifact,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ReadContextArtifact {
            task_id: request.task_id.clone(),
            locator: request.locator.clone(),
            reply,
        })
        .await
    }

    /// Общая отправка команды контекста в очередь Core.
    pub(crate) async fn dispatch_context<F>(&self, build: F) -> Result<Vec<u8>, IpcBridgeError>
    where
        F: FnOnce(oneshot::Sender<Result<Vec<u8>, String>>) -> CoreCommand,
    {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(build(reply))
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    /// Configuration as the shell should see it: the route's own model unless
    /// the shell selected another one for the next request.
    pub(crate) fn current_model_config(&self) -> Option<ModelConfigSnapshot> {
        let config = self.model_config.as_ref()?;
        let Some(model) = self.selected_model.get() else {
            return Some(config.clone());
        };
        Some(ModelConfigSnapshot {
            model: model.to_string(),
            ..config.clone()
        })
    }

    /// Builds a Core Doctor provider probe from already-loaded, secret-free
    /// gateway configuration. Never exposes an API key value, only whether
    /// one is present.
    pub(crate) fn provider_probe(&self) -> crate::doctor::ProviderProbe {
        let (provider_id, model_id, configured) = match &self.model_config {
            Some(config) => (
                config.provider.clone(),
                config.model.clone(),
                config.configured,
            ),
            None => (String::new(), String::new(), false),
        };
        let key_present = self
            .gateway_config
            .as_ref()
            .and_then(|config| config.routes.get(&config.default_route))
            .map(|route| route.configured())
            .unwrap_or(false);
        let metadata_valid = !provider_id.is_empty() && !model_id.is_empty();
        crate::doctor::ProviderProbe {
            provider_id,
            model_id,
            configured,
            key_present,
            metadata_valid,
        }
    }
}