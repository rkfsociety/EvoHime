use super::*;

impl IpcBridge {
    pub(crate) async fn dispatch_run_doctor(
        &self,
        project_id: String,
        detail_level: i32,
        protocol: Option<generated::ProtocolVersion>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let approval_required = match &self.tools {
            Some(tools) => !matches!(
                tools.permissions().mode(Permission::FilesystemWrite).await,
                PermissionMode::Allow
            ),
            None => true,
        };
        let (registered_tools, expected_tools, unavailable_tools) = match &self.tools {
            Some(tools) => {
                let names = tools.list();
                (names.len() as u32, EXPECTED_TOOL_COUNT, Vec::new())
            }
            None => (0, EXPECTED_TOOL_COUNT, Vec::new()),
        };
        let detail_level = if detail_level == 1 {
            crate::doctor::DetailLevel::Detailed
        } else {
            crate::doctor::DetailLevel::Summary
        };
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RunDoctor {
                project_id,
                protocol_major: protocol.map(|version| version.major),
                expected_protocol_major: PROTOCOL_MAJOR,
                provider: self.provider_probe(),
                approval_required,
                registered_tools,
                expected_tools,
                unavailable_tools,
                detail_level,
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

    pub(crate) async fn dispatch_create_diagnostics_snapshot(
        &self,
        request: generated::CreateDiagnosticsSnapshot,
        protocol: Option<generated::ProtocolVersion>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let approval_required = match &self.tools {
            Some(tools) => !matches!(
                tools.permissions().mode(Permission::FilesystemWrite).await,
                PermissionMode::Allow
            ),
            None => true,
        };
        let (registered_tools, expected_tools, unavailable_tools) = match &self.tools {
            Some(tools) => (tools.list().len() as u32, EXPECTED_TOOL_COUNT, Vec::new()),
            None => (0, EXPECTED_TOOL_COUNT, Vec::new()),
        };
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateDiagnosticsSnapshot {
                project_id: request.project_id,
                conversation_id: request.conversation_id,
                run_id: request.run_id,
                max_event_count: request.max_event_count,
                max_log_bytes: request.max_log_bytes,
                protocol_major: protocol.map(|version| version.major),
                expected_protocol_major: PROTOCOL_MAJOR,
                provider: self.provider_probe(),
                approval_required,
                registered_tools,
                expected_tools,
                unavailable_tools,
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

    pub(crate) async fn dispatch_export_doctor_logs(
        &self,
        destination_path: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ExportDoctorLogs {
                destination_path,
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

    pub(crate) async fn dispatch_create_database_backup<W: AsyncWrite + Unpin>(
        &self,
        operation_id: String,
        destination_path: String,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        let (reply, _response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateDatabaseBackup {
                operation_id,
                destination_path,
                progress,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let payload = serde_json::to_vec(&serde_json::json!({"accepted": true}))?;
        self.write_response(writer, "storage.backup.started", payload)
            .await?;
        Ok(())
    }

    pub(crate) async fn dispatch_prepare_database_restore(
        &self,
        operation_id: String,
        backup_path: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PrepareDatabaseRestore {
                operation_id,
                backup_path,
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

    pub(crate) async fn dispatch_restore_database<W: AsyncWrite + Unpin>(
        &self,
        operation_id: String,
        backup_path: String,
        approval_id: String,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        let (reply, _response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RestoreDatabase {
                operation_id,
                backup_path,
                approval_id,
                progress,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let payload = serde_json::to_vec(&serde_json::json!({"accepted": true}))?;
        self.write_response(writer, "storage.restore.started", payload)
            .await?;
        Ok(())
    }

    pub(crate) async fn dispatch_cancel_database_operation(
        &self,
        operation_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CancelDatabaseOperation {
                operation_id,
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

    pub(crate) async fn dispatch_save_research_evidence(
        &self,
        request: generated::SaveResearchEvidence,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SaveResearchEvidence {
                work_item_id: request.work_item_id,
                source_kind: request.source_kind,
                source_ref: request.source_ref,
                title: request.title,
                publisher: request.publisher,
                content_type: request.content_type,
                raw_excerpt: request.raw_excerpt,
                retrieved_at_ms: request.retrieved_at_ms,
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

    pub(crate) async fn dispatch_list_research_evidence(
        &self,
        work_item_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListResearchEvidence {
                work_item_id,
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

    pub(crate) async fn dispatch_run_research_fetch(
        &self,
        request: generated::RunResearchFetch,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RunResearchFetch {
                work_item_id: request.work_item_id,
                url: request.url,
                title: request.title,
                allowed_domains: request.allowed_domains,
                max_bytes: request.max_bytes,
                max_latency_ms: request.max_latency_ms,
                max_cost_micros: request.max_cost_micros,
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
}

