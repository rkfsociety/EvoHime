use super::*;

impl IpcBridge {
    pub fn process_once<'a, R: AsyncRead + Unpin + 'a, W: AsyncWrite + Unpin + 'a>(
        &'a self,
        reader: &'a mut R,
        writer: &'a mut W,
    ) -> Pin<Box<dyn Future<Output = Result<(), IpcBridgeError>> + 'a>> {
        Box::pin(async move {
            let payload = transport::read_frame(reader).await?;
            let command = generated::CommandEnvelope::decode(payload.as_slice())?;
            let request_id = command.request_id.clone();
            let client_id = command.client_id.clone();
            let command_hash = hex_encode(&payload);
            match command.command {
                Some(generated::command_envelope::Command::Handshake(_)) => {
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "core.ready".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: Some(generated::event_envelope::Event::Ready(generated::Ready {
                            protocol: Some(protocol()),
                            core_version: env!("CARGO_PKG_VERSION").into(),
                            core_info: Some(core_info()),
                        })),
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::GetReceiptKeyStatus(_)) => {
                    let mut status = self.receipt_status();
                    if let Ok(mut database) = self.journal.database().try_lock() {
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        ) {
                            if let Ok(counts) = runtime.counts() {
                                if let Some(object) = status.as_object_mut() {
                                    object.insert(
                                        "runtime_counts".into(),
                                        serde_json::json!({
                                            "pending": counts.pending,
                                            "pending_recovery": counts.pending_recovery,
                                            "quarantined": counts.quarantined,
                                            "approval_pending": counts.approval_pending,
                                        }),
                                    );
                                    if let Ok((rate, version)) = runtime.audit_sampling_config() {
                                        object.insert("audit_sampling".into(), serde_json::json!({"rate": rate, "policy_version": version}));
                                    }
                                    if let Ok(metrics) = runtime.metrics() {
                                        object.insert(
                                            "runtime_metrics".into(),
                                            serde_json::json!(metrics.counters),
                                        );
                                    }
                                    if let Ok(diagnostics) = runtime.diagnostic_counts() {
                                        object.insert(
                                            "runtime_diagnostics".into(),
                                            serde_json::json!(diagnostics),
                                        );
                                    }
                                    if let Ok(rotation) = runtime.storage_rotation_job() {
                                        object.insert("storage_rotation".into(), serde_json::json!(rotation.map(|job| serde_json::json!({"job_id": job.job_id, "old_key_id": job.old_key_id, "new_key_id": job.new_key_id, "cursor": job.cursor, "generation": job.generation, "state": job.state}))));
                                    }
                                }
                            }
                        }
                    }
                    self.write_response(writer, "key.status", serde_json::to_vec(&status)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ClosePendingReceiptAction(request)) => {
                    if !request.operator_confirmed
                        || request.action_id.is_empty()
                        || request.input_json.len()
                            > evohime_receipts::runtime::MAX_CALL_INPUT_BYTES
                    {
                        self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let action_id = uuid::Uuid::parse_str(&request.action_id)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let input: serde_json::Value = serde_json::from_str(&request.input_json)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let mut database = self.journal.database().lock().await;
                    let (task_id, run_id, tool_name, normalized_scope, policy_id, decision, state, approval_id, parent_approval_ref): (String,String,String,String,String,String,String,Option<String>,Option<String>) = database.connection().query_row(
                    "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,state,approval_id,parent_approval_ref FROM receipt_actions WHERE action_id=?1",
                    [action_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                    if state != "pending_recovery" {
                        self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery"}))?).await?;
                        return Ok(());
                    }
                    let policy_decision = match decision.as_str() {
                        "allow" => evohime_receipts::runtime::PolicyDecision::Allow,
                        "approval_required" => {
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired
                        }
                        "deny" => evohime_receipts::runtime::PolicyDecision::Deny,
                        _ => {
                            self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id,
                        task_id,
                        run_id,
                        tool_name,
                        policy_id,
                        normalized_scope,
                        input,
                        policy_decision,
                        approval_id: approval_id
                            .and_then(|value| uuid::Uuid::parse_str(&value).ok()),
                        parent_approval_ref,
                        preview: "unknown result closure".into(),
                    };
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let receipt_hash = runtime
                        .refuse(&receipt_request, "recovery_pending")
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":true,"action_id":request.action_id,"receipt_hash":receipt_hash,"completion_source":"reconciliation"}))?).await?;
                }
                Some(generated::command_envelope::Command::SetReceiptAuditSamplingRate(
                    request,
                )) => {
                    if request.rate > 100 {
                        self.write_response(writer, "receipt.sampling_rate", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    runtime
                        .set_audit_sampling_rate(true, request.rate as u8)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.sampling_rate", serde_json::to_vec(&serde_json::json!({"ok":true,"rate":request.rate,"policy_version":evohime_receipts::SAMPLING_POLICY_VERSION}))?).await?;
                }
                Some(generated::command_envelope::Command::ReconcilePendingReceiptAction(
                    request,
                )) => {
                    const MAX_RECONCILIATION_INPUT_BYTES: usize =
                        evohime_receipts::runtime::MAX_CALL_INPUT_BYTES;
                    let read_only = matches!(
                        request.tool_name.as_str(),
                        "filesystem.read"
                            | "filesystem.list"
                            | "git.status"
                            | "git.diff"
                            | "git.log"
                            | "git.show"
                            | "git.blame"
                            | "git.changed_files"
                            | "workspace.list"
                            | "workspace.read"
                            | "workspace.search"
                    );
                    if request.old_action_id.is_empty()
                        || request.tool_name.len() > 128
                        || !read_only
                        || request.input_json.len() > MAX_RECONCILIATION_INPUT_BYTES
                        || request.workspace_path.is_empty()
                        || request.workspace_path.len() > 32 * 1024
                        || request.workspace_path.contains('\n')
                    {
                        self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let old_action_id = match uuid::Uuid::parse_str(&request.old_action_id) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let input: serde_json::Value = match serde_json::from_str(&request.input_json) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let tools = match self.tools.as_ref() {
                        Some(value) => Arc::clone(value),
                        None => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.tool_unavailable"}))?).await?;
                            return Ok(());
                        }
                    };
                    let (task_id, old_state): (String, String) = {
                        let database = self.journal.database().lock().await;
                        database
                            .connection()
                            .query_row(
                                "SELECT task_id,state FROM receipt_actions WHERE action_id=?1",
                                [old_action_id.to_string()],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .map_err(|error| FrameError::Io(error.to_string()))?
                    };
                    if old_state != "pending_recovery" {
                        self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery"}))?).await?;
                        return Ok(());
                    }
                    let reconciliation_task_id = match task_id.parse() {
                        Ok(task_id) => task_id,
                        Err(error) => {
                            tracing::warn!(%error, task_id, "invalid reconciliation task id; generating one");
                            uuid::Uuid::now_v7()
                        }
                    };
                    let context = ToolContext {
                        workspace_root: std::path::PathBuf::from(&request.workspace_path),
                        task_id: reconciliation_task_id,
                        session_id: None,
                        progress_tx: None,
                    };
                    let (scope, preview) = match tools
                        .preflight(&context, &request.tool_name, &input)
                        .await
                    {
                        Ok(evohime_tool_runtime::ToolPreflightDecision::Allowed {
                            scope,
                            preview,
                        }) => (scope, preview),
                        Ok(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.policy_denied"}))?).await?;
                            return Ok(());
                        }
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.policy_denied"}))?).await?;
                            return Ok(());
                        }
                    };
                    let new_action_id = uuid::Uuid::now_v7();
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id: new_action_id,
                        task_id: task_id.clone(),
                        run_id: format!("reconciliation-{}", new_action_id),
                        tool_name: request.tool_name.clone(),
                        policy_id: "reconciliation:read_only".into(),
                        normalized_scope: scope,
                        input: input.clone(),
                        policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                        approval_id: None,
                        parent_approval_ref: None,
                        preview: match serde_json::to_string(&preview) {
                            Ok(preview) => preview,
                            Err(error) => {
                                tracing::warn!(%error, "reconciliation preview serialization failed");
                                "read-only reconciliation".into()
                            }
                        },
                    };
                    {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        if !matches!(
                            runtime
                                .prepare(receipt_request.clone())
                                .map_err(|error| FrameError::Io(error.to_string()))?,
                            evohime_receipts::runtime::PrepareOutcome::Prepared { .. }
                        ) {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.precondition_failed"}))?).await?;
                            return Ok(());
                        }
                        runtime
                            .mark_started(new_action_id)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                    let result = tools
                        .execute_with_cancellation(
                            &context,
                            &request.tool_name,
                            input,
                            CancellationToken::new(),
                        )
                        .await;
                    let (status, digest, error_category) = match &result {
                        Ok(value) => (
                            "succeeded",
                            evohime_receipts::sha256_hex(value.output.as_bytes()),
                            None,
                        ),
                        Err(_error) => (
                            "failed",
                            evohime_receipts::sha256_hex(b"reconciliation_tool_error"),
                            Some("tool_error"),
                        ),
                    };
                    let receipt_hash = {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        runtime
                            .mark_returned(new_action_id)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                        match runtime.complete_reconciliation(
                            &receipt_request,
                            old_action_id,
                            status,
                            &digest,
                            error_category,
                        ) {
                            Ok(hash) => hash,
                            Err(_error) => {
                                let _ = runtime
                                    .mark_pending_recovery(new_action_id, "signature_failed");
                                self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery","action_id":new_action_id.to_string()}))?).await?;
                                return Ok(());
                            }
                        }
                    };
                    self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":true,"old_action_id":old_action_id.to_string(),"action_id":new_action_id.to_string(),"status":status,"receipt_hash":receipt_hash,"completion_source":"reconciliation"}))?).await?;
                }
                Some(generated::command_envelope::Command::UnquarantineReceiptAction(request)) => {
                    if !request.operator_confirmed
                        || request.action_id.is_empty()
                        || request.input_json.len()
                            > evohime_receipts::runtime::MAX_CALL_INPUT_BYTES
                        || request.checkpoint.is_empty()
                        || request.checkpoint.len() > 256
                        || request.checkpoint.contains('\n')
                    {
                        self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let checkpoint_valid = std::fs::read(self.receipt_keys.checkpoint_path())
                        .ok()
                        .and_then(|bytes| {
                            serde_json::from_slice::<
                                evohime_receipts::key_lifecycle::KeyHistoryCheckpoint,
                            >(&bytes)
                            .ok()
                        })
                        .and_then(|checkpoint| {
                            if checkpoint.checkpoint_id != request.checkpoint {
                                return None;
                            }
                            if !self
                                .receipt_keys
                                .trusted_genesis(&checkpoint.genesis_key_id)
                                .ok()?
                            {
                                return Some(false);
                            }
                            let history = self.receipt_keys.load_history().ok()?;
                            Some(
                                evohime_receipts::key_lifecycle::verify_checkpoint(
                                    &checkpoint,
                                    &history,
                                    Some(&checkpoint.genesis_key_id),
                                )
                                .is_ok(),
                            )
                        })
                        .unwrap_or(false);
                    if !checkpoint_valid {
                        self.write_response(
                        writer,
                        "receipt.unquarantine",
                        serde_json::to_vec(
                            &serde_json::json!({"ok":false,"error_code":"receipt.key_untrusted"}),
                        )?,
                    )
                    .await?;
                        return Ok(());
                    }
                    let action_id = match uuid::Uuid::parse_str(&request.action_id) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let input: serde_json::Value = match serde_json::from_str(&request.input_json) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let mut database = self.journal.database().lock().await;
                    let (task_id, run_id, tool_name, normalized_scope, policy_id, decision, state, approval_id, parent_approval_ref): (String,String,String,String,String,String,String,Option<String>,Option<String>) = database.connection().query_row(
                    "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,state,approval_id,parent_approval_ref FROM receipt_actions WHERE action_id=?1",
                    [action_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                    if state != "quarantined" {
                        self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let policy_decision = match decision.as_str() {
                        "allow" => evohime_receipts::runtime::PolicyDecision::Allow,
                        "approval_required" => {
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired
                        }
                        "deny" => evohime_receipts::runtime::PolicyDecision::Deny,
                        _ => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id,
                        task_id,
                        run_id,
                        tool_name,
                        policy_id,
                        normalized_scope,
                        input,
                        policy_decision,
                        approval_id: approval_id
                            .and_then(|value| uuid::Uuid::parse_str(&value).ok()),
                        parent_approval_ref,
                        preview: "manual quarantine closure".into(),
                    };
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let receipt_hash = runtime
                        .unquarantine(&receipt_request, true, &request.checkpoint)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":true,"action_id":request.action_id,"receipt_hash":receipt_hash,"state":"refused","dispatch_allowed":false}))?).await?;
                }
                Some(generated::command_envelope::Command::ListReceipts(request)) => {
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.listed",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        100
                    } else {
                        request.limit as i64
                    };
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::list_receipts(
                        database.connection(),
                        &filter,
                        limit,
                    ) {
                        Ok(result) => {
                            self.write_response(
                            writer,
                            "receipts.listed",
                            serde_json::to_vec(&serde_json::json!({
                                "ok": true,
                                "snapshot_last_sequence": result.snapshot_last_sequence.to_string(),
                                "rows": result.rows,
                            }))?,
                        )
                        .await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.listed",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::VerifyReceipts(request)) => {
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.verified",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        500
                    } else {
                        request.limit as i64
                    };
                    let trust_key = if request.trust_key_id.is_empty() {
                        None
                    } else {
                        Some(request.trust_key_id.as_str())
                    };
                    let key_history = self.receipt_keys.load_history().unwrap_or_default();
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::verify_receipts(
                        database.connection(),
                        &key_history,
                        trust_key,
                        &filter,
                        limit,
                    ) {
                        Ok(result) => {
                            self.write_response(
                            writer,
                            "receipts.verified",
                            serde_json::to_vec(&serde_json::json!({
                                "ok": true,
                                "status": result.verification.status,
                                "code": result.verification.code,
                                "requested_count": result.requested_count,
                                "actual_verified_count": result.verification.actual_verified_count,
                                "chain_start_hash": result.verification.chain_start_hash,
                                "chain_end_hash": result.verification.chain_end_hash,
                                "rows": result.verification.rows,
                            }))?,
                        )
                        .await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.verified",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::ExportReceipts(request)) => {
                    if request.replace
                        || request.destination_path.is_empty()
                        || request.destination_path.len() > 4096
                    {
                        self.write_response(writer, "receipts.exported", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":if request.replace { "receipts.unsupported_operation" } else { "receipts.invalid_filter" }}))?).await?;
                        return Ok(());
                    }
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.exported",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        100_000
                    } else {
                        request.limit as i64
                    };
                    let destination = std::path::PathBuf::from(&request.destination_path);
                    let key_history = self.receipt_keys.load_history().unwrap_or_default();
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::export_receipts(
                        database.connection(),
                        &key_history,
                        &destination,
                        &filter,
                        limit,
                    ) {
                        Ok(manifest) => {
                            let manifest_sha256 = std::fs::read(destination.join("manifest.json"))
                                .ok()
                                .map(|bytes| evohime_receipts::sha256_hex(&bytes));
                            self.write_response(writer, "receipts.exported", serde_json::to_vec(&serde_json::json!({
                            "ok": true,
                            "export_id": manifest.export_id,
                            "destination_basename": destination.file_name().and_then(|value| value.to_str()),
                            "snapshot_last_sequence": manifest.snapshot_last_sequence.to_string(),
                            "requested_count": manifest.requested_count,
                            "selected_count": manifest.selected_count,
                            "actual_exported_count": manifest.actual_exported_count,
                            "manifest_sha256": manifest_sha256,
                        }))?).await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.exported",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::TrustReceiptGenesis(request)) => {
                    if !self
                        .take_receipt_approval(writer, &request.approval_id, "TrustReceiptGenesis")
                        .await?
                    {
                        return Ok(());
                    }
                    let result = self
                        .receipt_keys
                        .trust_genesis(&request.genesis_key_id, &request.source);
                    let payload = match result {
                        Ok(()) => {
                            serde_json::json!({"status": "trusted", "genesis_key_id": request.genesis_key_id})
                        }
                        Err(error) => {
                            serde_json::json!({"status": error.to_string(), "error_code": error.to_string()})
                        }
                    };
                    self.write_response(writer, "key.trust", serde_json::to_vec(&payload)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateNewReceiptGenesis(request)) => {
                    if !self
                        .take_receipt_approval(
                            writer,
                            &request.approval_id,
                            "CreateNewReceiptGenesis",
                        )
                        .await?
                    {
                        return Ok(());
                    }
                    let manager = self.receipt_keys.clone();
                    let database = self.journal.database().clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let mut database = database.blocking_lock();
                        manager.create_new_genesis_with_database(database.connection_mut(), "user")
                    })
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = match result {
                        Ok(key_id) => {
                            serde_json::json!({"status":"recovered", "key_id":key_id, "trust_required":true})
                        }
                        Err(error) => {
                            serde_json::json!({"status":"failed", "error_code":error.to_string()})
                        }
                    };
                    self.write_response(writer, "key.recovery", serde_json::to_vec(&payload)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::RotateReceiptKey(request)) => {
                    if !self
                        .take_receipt_approval(writer, &request.approval_id, "RotateReceiptKey")
                        .await?
                    {
                        return Ok(());
                    }
                    let reason = request.reason.trim().to_string();
                    if !matches!(reason.as_str(), "manual" | "compromise") {
                        self.write_response(
                            writer,
                            "key.rotation_failed",
                            br#"{"error_code":"key.rotation_failed"}"#.to_vec(),
                        )
                        .await?;
                    } else {
                        let manager = self.receipt_keys.clone();
                        let database = self.journal.database().clone();
                        let rotation_reason = reason.clone();
                        let result = tokio::task::spawn_blocking(move || -> Result<(String, Option<String>), String> {
                        let mut database = database.blocking_lock();
                        let protected_count: i64 = database.connection().query_row(
                            "SELECT COUNT(*) FROM receipt_protected_actions",
                            [],
                            |row| row.get(0),
                        ).map_err(|error| error.to_string())?;
                        let storage_key_id: Option<String> = if protected_count > 0 {
                            let signer = super::CoreReceiptSigner(Arc::clone(&manager));
                            let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                                .map_err(|error| error.to_string())?;
                            let existing_job = runtime.storage_rotation_job().map_err(|error| error.to_string())?;
                            let (job_id, old_storage_key_id, new_storage_key_id, generation) = if let Some(job) = existing_job.filter(|job| matches!(job.state.as_str(), "running" | "failed")) {
                                (job.job_id, job.old_key_id, job.new_key_id, job.generation)
                            } else {
                                let old_storage_key_id = manager.storage_key_id().map_err(|error| error.to_string())?;
                                let new_storage_key_id = manager.rotate_storage_key(true).map_err(|error| error.to_string())?;
                                (format!("storage-{}", uuid::Uuid::now_v7()), old_storage_key_id, new_storage_key_id, SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis() as i64).unwrap_or_default())
                            };
                            loop {
                                let progressed = runtime.rewrap_protected_batch(
                                    &job_id,
                                    &old_storage_key_id,
                                    &new_storage_key_id,
                                    generation,
                                    32,
                                    |envelope| manager.rewrap_storage_with_key_id(envelope, &new_storage_key_id).map_err(|_| evohime_receipts::runtime::RuntimeError::Code("storage_key_unavailable")),
                                ).map_err(|error| error.to_string())?;
                                if !progressed { break; }
                            }
                            Some(new_storage_key_id)
                        } else {
                            None
                        };
                        let signing_key_id = manager.rotate_with_database(
                            database.connection_mut(),
                            &rotation_reason,
                            "user",
                        ).map_err(|error| error.to_string())?;
                        Ok((signing_key_id, storage_key_id))
                    })
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                        let payload = match result {
                            Ok((key_id, storage_key_id)) => {
                                serde_json::json!({"status":"rotated", "key_id":key_id, "storage_key_id":storage_key_id, "reason":reason})
                            }
                            Err(error) => {
                                serde_json::json!({"status":"failed", "error_code":error.to_string()})
                            }
                        };
                        self.write_response(writer, "key.rotation", serde_json::to_vec(&payload)?)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::ResyncRequest(request)) => {
                    evohime_desktop_ipc::validate_resync_request(&request)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    if self.stale_generation(&command) {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            request.after_sequence,
                            None,
                            latest,
                            REPLAY_GAP_REASON_STALE_GENERATION,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    let limit = if request.max_events == 0 {
                        evohime_desktop_ipc::DEFAULT_RESYNC_MAX_EVENTS
                    } else {
                        request.max_events
                    } as usize;
                    let batch = self
                        .journal
                        .replay_bounded(request.after_sequence as i64, limit)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let last_sequence = batch
                        .events
                        .last()
                        .map(|record| record.sequence_id as u64)
                        .unwrap_or(request.after_sequence);
                    if batch.gap_detected {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            request.after_sequence,
                            batch.first_available_sequence.map(|value| value as u64),
                            latest,
                            REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    // Снапшот, не влезающий в кадр IPC, раньше обрывал соединение с
                    // оболочкой: она навсегда оставалась без состояния и рисовала
                    // «нет связи». Теперь превышение лимита деградирует до
                    // поштучной отправки тех же событий.
                    let snapshot = if request.include_full_snapshot {
                        let snapshot_json = serde_json::to_vec(&serde_json::json!({
                            "schema_version": 1,
                            "core_instance_id": self.core_instance_id,
                            "session_epoch": self.session_epoch,
                            "snapshot_sequence_id": last_sequence,
                            "after_sequence": request.after_sequence,
                            "actions": typed_snapshot_actions(&batch.events),
                            "events": batch.events.iter().map(|record| serde_json::json!({
                                "sequence_id": record.sequence_id,
                                "task_id": record.task_id,
                                "event_type": record.event_type,
                                "payload": record.payload,
                                "created_at": record.created_at,
                            })).collect::<Vec<_>>(),
                        }))
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        let candidate = generated::FullSnapshot {
                            sequence_id: last_sequence,
                            snapshot_json,
                        };
                        match evohime_desktop_ipc::validate_full_snapshot(&candidate) {
                            Ok(()) => Some(candidate),
                            Err(error) => {
                                tracing::warn!(
                                    event = "ipc.snapshot_oversized",
                                    error = %error,
                                    events = batch.events.len(),
                                    snapshot_bytes = candidate.snapshot_json.len(),
                                    "снапшот не влез в кадр, переходим на поштучную отправку"
                                );
                                let payload = serde_json::to_vec(&serde_json::json!({
                                    "after_sequence": request.after_sequence,
                                    "last_sequence": last_sequence,
                                    "events": batch.events.len(),
                                    "snapshot_bytes": candidate.snapshot_json.len(),
                                    "reason": "snapshot_too_large",
                                }))
                                .map_err(|error| FrameError::Io(error.to_string()))?;
                                self.write_response(writer, "replay.snapshot_skipped", payload)
                                    .await?;
                                None
                            }
                        }
                    } else {
                        None
                    };
                    if let Some(snapshot) = snapshot {
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: last_sequence,
                            task_id: String::new(),
                            event_type: "replay.full_snapshot".into(),
                            payload: Vec::new(),
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: Some(generated::event_envelope::Event::FullSnapshot(snapshot)),
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    } else {
                        for record in batch.events {
                            let event = generated::EventEnvelope {
                                protocol: Some(protocol()),
                                sequence_id: record.sequence_id as u64,
                                task_id: record.task_id,
                                event_type: record.event_type,
                                payload: record.payload,
                                core_instance_id: self.core_instance_id.clone(),
                                session_epoch: self.session_epoch,
                                event: None,
                            };
                            transport::write_frame(writer, &event.encode_to_vec()).await?;
                        }
                    }
                    // Каждый resync отдаёт не больше `limit` событий за раз. Без
                    // этого флага оболочка узнавала об оставшемся хвосте истории
                    // только по случайному разрыву sequence в живом потоке — и
                    // гонялась за ним кругами, так и не догоняя (план про «нет
                    // связи», возникавшую после больших сессий).
                    let latest_after_batch = self.latest_sequence().await;
                    let end_payload = serde_json::to_vec(&serde_json::json!({
                        "more_available": last_sequence < latest_after_batch,
                        "latest_sequence": latest_after_batch,
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let end = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "resync.end".into(),
                        payload: end_payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &end.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::ReplayEvents(replay)) => {
                    if self.stale_generation(&command) {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            replay.after_sequence,
                            None,
                            latest,
                            REPLAY_GAP_REASON_STALE_GENERATION,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    let batch = self
                        .journal
                        .replay_bounded(replay.after_sequence as i64, 1_000)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let mut last_sequence = batch.last_sequence as u64;
                    if batch.gap_detected {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            replay.after_sequence,
                            batch.first_available_sequence.map(|value| value as u64),
                            latest,
                            REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    for record in batch.events {
                        last_sequence = record.sequence_id as u64;
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: record.sequence_id as u64,
                            task_id: record.task_id,
                            event_type: record.event_type,
                            payload: record.payload,
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: None,
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    }
                    let end = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "replay.end".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &end.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::SelectModel(request)) => {
                    // Bounded: a model identifier is a short single-line token.
                    let model = request.model.trim();
                    if model.len() > 128 || model.contains(char::is_whitespace) {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(&serde_json::json!({ "reason": "invalid_model" }))
                                .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    }
                    let Some(route) = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                    else {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(
                                &serde_json::json!({ "reason": "provider_not_configured" }),
                            )
                            .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    };
                    let available = evohime_model_gateway::fetch_model_catalog(route)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    if !available.iter().any(|entry| entry.id == model) {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(
                                &serde_json::json!({ "reason": "model_not_returned_by_provider" }),
                            )
                            .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    }
                    self.selected_model.set(model);
                    let payload = match serde_json::to_vec(&self.current_model_config()) {
                        Ok(payload) => payload,
                        Err(error) => {
                            tracing::warn!(%error, "model config serialization failed");
                            b"null".to_vec()
                        }
                    };
                    self.write_response(writer, "model.config", payload).await?;
                }
                Some(generated::command_envelope::Command::ModelConfig(_)) => {
                    let payload = match serde_json::to_vec(&self.current_model_config()) {
                        Ok(payload) => payload,
                        Err(error) => {
                            tracing::warn!(%error, "model config serialization failed");
                            b"null".to_vec()
                        }
                    };
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "model.config".into(),
                        payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::ModelCatalog(request)) => {
                    let mode = if request.mode == "paid" {
                        "paid"
                    } else {
                        "free"
                    };
                    let provider = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                        .map(|route| route.provider.as_str().to_string())
                        .unwrap_or_else(|| "unknown".into());
                    let result = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                        .map(|route| async move {
                            evohime_model_gateway::fetch_model_catalog(route)
                                .await
                                .map(|entries| {
                                    entries
                                        .into_iter()
                                        .filter(|entry| {
                                            if mode == "free" {
                                                entry.id.ends_with(":free")
                                            } else {
                                                !entry.id.ends_with(":free")
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                })
                        });
                    let (entries, error) = match result {
                        Some(request) => request.await,
                        None => Err(evohime_model_gateway::providers::ProviderError::Config(
                            "provider is not configured".into(),
                        )),
                    }
                    .map_or_else(
                        |error| (Vec::new(), Some(error.to_string())),
                        |entries| (entries, None),
                    );
                    // Лимиты переживают сессию: планировщик контекста и ревью
                    // должны знать окно модели ещё до первого обновления каталога,
                    // а неудачный запрос не должен стирать то, что уже известно.
                    self.remember_model_limits(&provider, &entries).await;
                    let models = entries
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect::<Vec<_>>();
                    let limits = entries
                        .iter()
                        .map(|entry| {
                            (
                                entry.id.clone(),
                                serde_json::json!({
                                    "context": entry.context_tokens,
                                    "maxOutput": entry.max_output_tokens,
                                }),
                            )
                        })
                        .collect::<serde_json::Map<_, _>>();
                    let payload = serde_json::json!({
                        "mode": mode,
                        "models": models,
                        "limits": limits,
                        "error": error,
                    });
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "model.catalog".into(),
                        payload: serde_json::to_vec(&payload).unwrap_or_default(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
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
                Some(generated::command_envelope::Command::SchemaDrivenAgentConfiguration(
                    request,
                )) => {
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
                Some(generated::command_envelope::Command::RuntimeInterventionPipeline(
                    request,
                )) => {
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
                Some(generated::command_envelope::Command::CodeDiagnosticsFeedbackLoop(
                    request,
                )) => {
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
                Some(generated::command_envelope::Command::CoreTopicSubscriptionEventBus(
                    request,
                )) => {
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
                Some(generated::command_envelope::Command::MemoryViewsAndAdaptiveRecall(
                    request,
                )) => {
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
                Some(generated::command_envelope::Command::DeclarativeRuntimeComponents(
                    request,
                )) => {
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
                Some(generated::command_envelope::Command::MessageInterventionPolicies(
                    request,
                )) => {
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
                Some(
                    generated::command_envelope::Command::PersistentAgentOrganizationRegistry(
                        request,
                    ),
                ) => {
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
                Some(generated::command_envelope::Command::StopPlanReview(request)) => {
                    let cancelled = self
                        .review_tasks
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if let Some(ref token) = cancelled {
                        token.cancel();
                    }
                    self.write_response(
                        writer,
                        "review.stop.accepted",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "accepted": cancelled.is_some(),
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListPlanReviews(request)) => {
                    let limit = (request.limit as usize).clamp(1, 50);
                    let results = self.review_results.lock().await;
                    let mut items: Vec<_> = results.values().cloned().collect();
                    drop(results);
                    if let Ok(events) = self.journal.review_history(limit).await {
                        for event in events {
                            if let Some(result) = review_result_from_event(&event.payload) {
                                if !items.iter().any(|item| item.review_id == result.review_id) {
                                    items.push(result);
                                }
                            }
                        }
                    }
                    items.sort_by(|left, right| left.review_id.cmp(&right.review_id));
                    items.truncate(limit);
                    self.write_response(
                        writer,
                        "review.list",
                        serde_json::to_vec(&serde_json::json!({ "reviews": items }))
                            .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ClearPlanReviewHistory(_)) => {
                    // Running reviews keep their own state; only what the history
                    // lists is dropped, and the marker is what listing reads.
                    self.review_results.lock().await.clear();
                    let marker_id = format!("review-history-{}", self.latest_sequence().await);
                    // Recorded directly rather than published: the shell lists again
                    // as soon as this response arrives, and a marker still travelling
                    // through the coordinator's broadcast would not be in the journal
                    // yet, so that listing would return the reviews just cleared.
                    // Nothing subscribes to the marker, so no push is lost.
                    let _ = self
                        .journal
                        .record(&CoreEvent::ReviewHistoryCleared { marker_id })
                        .await;
                    self.write_response(
                        writer,
                        "review.historyCleared",
                        serde_json::to_vec(&serde_json::json!({ "cleared": true }))
                            .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetPlanReview(request)) => {
                    let mut result = self
                        .review_results
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) = self.journal.task_history(&request.review_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| review_result_from_event(&event.payload));
                        }
                    }
                    self.write_response(
                        writer,
                        "review.result",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "result": result,
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExportPlanReview(request)) => {
                    let mut result = self
                        .review_results
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) = self.journal.task_history(&request.review_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| review_result_from_event(&event.payload));
                        }
                    }
                    let result = result.ok_or_else(|| FrameError::Io("review not found".into()))?;
                    let destination = std::path::PathBuf::from(&request.destination_path);
                    if destination.extension().and_then(|value| value.to_str()) != Some("md") {
                        return Err(
                            FrameError::Io("review export must be a Markdown file".into()).into(),
                        );
                    }
                    let content = if request.include_reviewers {
                        serde_json::to_string_pretty(&result).unwrap_or_default()
                    } else {
                        result.final_markdown.clone()
                    };
                    tokio::fs::write(&destination, content)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(
                        writer,
                        "review.exported",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "destination_path": request.destination_path,
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::RevisePlan(request)) => {
                    self.revise_plan(request, writer).await?;
                }
                Some(generated::command_envelope::Command::StopRevision(request)) => {
                    let cancelled = self
                        .revision_tasks
                        .lock()
                        .await
                        .get(&request.revision_id)
                        .cloned();
                    if let Some(ref token) = cancelled {
                        token.cancel();
                    }
                    self.write_response(
                        writer,
                        "revision.stop.accepted",
                        serde_json::to_vec(&serde_json::json!({
                            "revision_id": request.revision_id,
                            "accepted": cancelled.is_some(),
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::SaveRevisedPlan(request)) => {
                    // Правка переживает перезапуск ядра: обновление Евы перезапускает
                    // Core, а нажать «сохранить» пользователь может и после этого.
                    let mut result = self
                        .revision_results
                        .lock()
                        .await
                        .get(&request.revision_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) =
                            self.journal.task_history(&request.revision_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| revision_result_from_event(&event.payload));
                        }
                    }
                    // Отказ отвечает событием, а не ошибкой кадра: ошибка кадра рвёт
                    // соединение с оболочкой, и опечатка в имени файла выглядела бы
                    // как падение ядра.
                    let failure = match &result {
                        None => Some("правка не найдена: запусти её заново".to_string()),
                        Some(_)
                            if std::path::Path::new(&request.destination_path)
                                .extension()
                                .and_then(|value| value.to_str())
                                != Some("md") =>
                        {
                            Some("сохранить план можно только в файл .md".to_string())
                        }
                        Some(_) => None,
                    };
                    let failure = match (failure, result) {
                        (Some(reason), _) => Some(reason),
                        (None, Some(result)) => {
                            tokio::fs::write(&request.destination_path, &result.revised_markdown)
                                .await
                                .err()
                                .map(|error| error.to_string())
                        }
                        (None, None) => Some("правка не найдена: запусти её заново".to_string()),
                    };
                    match failure {
                        Some(error) => {
                            self.write_response(
                                writer,
                                "plan.save_failed",
                                serde_json::to_vec(&serde_json::json!({
                                    "revision_id": request.revision_id,
                                    "destination_path": request.destination_path,
                                    "error": error,
                                }))
                                .unwrap_or_default(),
                            )
                            .await?;
                        }
                        None => {
                            self.write_response(
                                writer,
                                "plan.saved",
                                serde_json::to_vec(&serde_json::json!({
                                    "revision_id": request.revision_id,
                                    "destination_path": request.destination_path,
                                }))
                                .unwrap_or_default(),
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::PermissionMode(request)) => {
                    if let Some(tools) = &self.tools {
                        let mode = match request.mode.as_str() {
                            "full" => PermissionMode::Allow,
                            "read_only" => PermissionMode::Deny,
                            _ => PermissionMode::Ask,
                        };
                        tools.permissions().set_all_modes(mode).await;
                        if request.mode == "read_only" {
                            tools
                                .permissions()
                                .set_mode(Permission::FilesystemRead, PermissionMode::Allow)
                                .await;
                            tools
                                .permissions()
                                .set_mode(Permission::GitRead, PermissionMode::Allow)
                                .await;
                        }
                    }
                }
                Some(generated::command_envelope::Command::CreateProject(request)) => {
                    let result = self
                        .dispatch_create_project(client_id, request_id, command_hash, request)
                        .await?;
                    self.write_response(writer, "project.created", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateTask(request)) => {
                    let item = WorkItemRecord {
                        id: request.task_id,
                        project_id: request.project_id,
                        parent_id: (!request.parent_id.is_empty()).then_some(request.parent_id),
                        title: request.title,
                        description: request.description,
                        source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                        acceptance_criteria: request.acceptance_criteria,
                        non_goals: request.non_goals,
                        status: if request.status.is_empty() {
                            "backlog".into()
                        } else {
                            request.status
                        },
                        priority: request.priority,
                        estimate: (request.estimate != 0).then_some(request.estimate),
                        complexity: (!request.complexity.is_empty()).then_some(request.complexity),
                        attempt_count: 0,
                        version: 1,
                    };
                    let result = self
                        .dispatch_create_task(client_id, request_id, command_hash, item)
                        .await?;
                    self.write_response(writer, "task.created", result).await?;
                }
                Some(generated::command_envelope::Command::UpdateTaskStatus(request)) => {
                    let result = self
                        .dispatch_update_status(
                            client_id,
                            request_id,
                            command_hash,
                            request.task_id,
                            request.expected_version,
                            request.status,
                        )
                        .await?;
                    self.write_response(writer, "task.status_updated", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::AddTaskEdge(request)) => {
                    let result = self
                        .dispatch_add_edge(
                            client_id,
                            request_id,
                            command_hash,
                            request.from_task_id,
                            request.to_task_id,
                            request.kind,
                        )
                        .await?;
                    self.write_response(writer, "task.edge_added", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetTaskGraph(request)) => {
                    let result = self.dispatch_get_task_graph(request.project_id).await?;
                    self.write_response(writer, "task.graph", result).await?;
                }
                Some(generated::command_envelope::Command::NextReadyTask(request)) => {
                    let result = self.dispatch_next_ready_task(request.project_id).await?;
                    self.write_response(writer, "task.next_ready", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ImportPrd(request)) => {
                    let result = self
                        .dispatch_import_prd(client_id, request_id, command_hash, request)
                        .await?;
                    self.write_response(writer, "prd.imported", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskHistory(request)) => {
                    let result = self
                        .dispatch_get_task_history(request.task_id, request.limit as usize)
                        .await?;
                    self.write_response(writer, "task.history", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskContext(request)) => {
                    let result = self
                        .dispatch_get_task_context(
                            request.project_id,
                            request.task_id,
                            request.max_chars as usize,
                        )
                        .await?;
                    self.write_response(writer, "task.context", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskPlanSpec(request)) => {
                    let result = self
                        .dispatch_get_task_plan_spec(
                            request.project_id,
                            request.task_id,
                            request.max_chars as usize,
                        )
                        .await?;
                    self.write_response(writer, "task.plan_spec", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ApplyApprovedBuild(request)) => {
                    let result = self
                        .dispatch_apply_approved_build(
                            request.project_id,
                            request.run_id,
                            request.task_id,
                            request.approved_build_json,
                        )
                        .await?;
                    self.write_response(writer, "build.applied", result).await?;
                }
                Some(generated::command_envelope::Command::PrepareBuild(request)) => {
                    let result = self
                        .dispatch_prepare_build(request.project_id, request.proposal_json)
                        .await?;
                    self.write_response(writer, "build.prepared", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetTaskSnapshot(request)) => {
                    let result = self
                        .dispatch_get_task_snapshot(request.project_id, request.task_id)
                        .await?;
                    self.write_response(writer, "task.snapshot", result).await?;
                }
                Some(generated::command_envelope::Command::RestoreTaskSnapshot(request)) => {
                    let result = self
                        .dispatch_restore_task_snapshot(
                            request.project_id,
                            request.task_id,
                            request.snapshot_id,
                        )
                        .await?;
                    self.write_response(writer, "snapshot.restored", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetBuildPolicy(request)) => {
                    let result = self.dispatch_get_build_policy(request.project_id).await?;
                    self.write_response(writer, "build.policy", result).await?;
                }
                Some(generated::command_envelope::Command::SaveBuildPolicy(request)) => {
                    let result = self
                        .dispatch_save_build_policy(
                            request.project_id,
                            request.policy_json,
                            request.expected_version,
                        )
                        .await?;
                    self.write_response(writer, "build.policy.saved", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::StartTask(start)) => {
                    let has_conversation = !start.conversation_id.is_empty();
                    let has_client_message = !start.client_message_id.is_empty();
                    if has_conversation != has_client_message {
                        self.write_conversation_event_log_response(
                            writer,
                            conversation_event_log_error(
                                "accept",
                                &start.conversation_id,
                                "invalid_argument",
                            ),
                        )
                        .await?;
                        return Ok(());
                    }
                    let mut should_dispatch = true;
                    if has_conversation {
                        let workspace_id =
                            crate::task_memory::project_scope_id(&start.workspace_path);
                        let accepted = self
                            .journal
                            .accept_conversation_message(
                                &start.conversation_id,
                                &workspace_id,
                                &start.task_id,
                                &start.client_message_id,
                                &start.prompt,
                            )
                            .await;
                        let (acceptance, sequence) = match accepted {
                            Ok(value) => value,
                            Err(StorageError::ConversationEventLog(
                                evohime_local_storage::conversation_event_log_store::ConversationStoreError::IdempotencyConflict,
                            )) => {
                                self.write_conversation_event_log_response(
                                    writer,
                                    conversation_event_log_error(
                                        "accept",
                                        &start.conversation_id,
                                        "idempotency_conflict",
                                    ),
                                )
                                .await?;
                                return Ok(());
                            }
                            Err(error) => {
                                self.write_conversation_event_log_response(
                                    writer,
                                    conversation_event_log_error(
                                        "accept",
                                        &start.conversation_id,
                                        &conversation_accept_error_code(&error),
                                    ),
                                )
                                .await?;
                                return Ok(());
                            }
                        };
                        if let Some(coordinator) = &self.coordinator {
                            coordinator.notify_journalled(sequence.max(0) as u64);
                        }
                        should_dispatch = self
                            .journal
                            .claim_conversation_dispatch(
                                &start.conversation_id,
                                &start.client_message_id,
                            )
                            .await
                            .unwrap_or(false);
                        if !should_dispatch && acceptance.dispatch_state == "dispatching" {
                            self.write_conversation_event_log_response(
                                writer,
                                conversation_event_log_error(
                                    "accept",
                                    &start.conversation_id,
                                    "dispatch_unknown",
                                ),
                            )
                            .await?;
                            return Ok(());
                        }
                    }
                    if should_dispatch {
                        if let Some(coordinator) = &self.coordinator {
                            let dispatched = coordinator
                                .dispatch(CoreCommand::StartTask {
                                    task_id: start.task_id,
                                    prompt: start.prompt,
                                    workspace_root: (!start.workspace_path.is_empty())
                                        .then(|| std::path::PathBuf::from(start.workspace_path)),
                                    preferred_route_hint: match start.preferred_route_hint.as_str()
                                    {
                                        "local" | "cloud" => Some(start.preferred_route_hint),
                                        "codex_cli" if start.execution_kind == "coding" => {
                                            Some("codex_cli".into())
                                        }
                                        _ => None,
                                    },
                                })
                                .await;
                            if has_conversation {
                                self.journal
                                    .finish_conversation_dispatch(
                                        &start.conversation_id,
                                        &start.client_message_id,
                                        dispatched.is_ok(),
                                    )
                                    .await
                                    .map_err(|error| FrameError::Io(error.to_string()))?;
                            }
                            dispatched.map_err(|error| FrameError::Io(error.to_string()))?;
                        } else if has_conversation {
                            self.journal
                                .finish_conversation_dispatch(
                                    &start.conversation_id,
                                    &start.client_message_id,
                                    false,
                                )
                                .await
                                .map_err(|error| FrameError::Io(error.to_string()))?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::GetTaskCheckpoint(request)) => {
                    let projection = self.dispatch_get_task_checkpoint(request).await;
                    self.write_task_checkpoint_projection(writer, projection)
                        .await?;
                }
                Some(generated::command_envelope::Command::ResolveTaskCheckpoint(request)) => {
                    let result = self.dispatch_resolve_task_checkpoint(request).await?;
                    self.write_task_checkpoint_action_result(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListSkills(request)) => {
                    self.dispatch_list_skills(request, writer).await?;
                }
                Some(generated::command_envelope::Command::LoadSkill(request)) => {
                    self.dispatch_load_skill(request, writer).await?;
                }
                Some(generated::command_envelope::Command::LoadSkillReference(request)) => {
                    self.dispatch_load_skill_reference(request, writer).await?;
                }
                Some(generated::command_envelope::Command::CreateGoal(request)) => {
                    let result = self.dispatch_create_goal(request, &command_hash).await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::GetGoal(request)) => {
                    let projection = self.dispatch_get_goal(request).await;
                    self.write_goal_projection(writer, projection).await?;
                }
                Some(generated::command_envelope::Command::ListGoals(request)) => {
                    let projection = self.dispatch_list_goals(request).await;
                    self.write_goal_list_projection(writer, projection).await?;
                }
                Some(generated::command_envelope::Command::PauseGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Paused,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::ResumeGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Active,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::CancelGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Cancelled,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::UpdateGoal(request)) => {
                    let result = self.dispatch_update_goal(request, &command_hash).await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::VerifyGoalCriterion(request)) => {
                    let result = self
                        .dispatch_verify_goal_criterion(request, &command_hash)
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::LinkGoalReference(request)) => {
                    let result = self
                        .dispatch_link_goal_reference(request, &command_hash)
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::SaveContinuationPolicy(request)) => {
                    let result = self
                        .dispatch_save_continuation_policy(
                            request,
                            &client_id,
                            &request_id,
                            &command_hash,
                        )
                        .await;
                    self.write_response(
                        writer,
                        "continuation.policy",
                        result.unwrap_or_else(error_response_payload),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::StartContinuationRun(request)) => {
                    let result = self.dispatch_start_continuation(request).await;
                    let payload = result.unwrap_or_else(error_response_payload);
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_projection(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.run", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::GetContinuationRun(request)) => {
                    let result = self.dispatch_get_continuation(request).await;
                    let payload = result.unwrap_or_else(error_response_payload);
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_projection(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.run", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::StopContinuation(request)) => {
                    let result = self.dispatch_stop_continuation(request).await;
                    let payload = result.unwrap_or_else(error_response_payload);
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_action(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.action", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::ListRetainedChildren(request)) => {
                    let (reply, response) = oneshot::channel();
                    let parent_id = client_id.clone();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::ListRetainedChildren {
                            parent_id,
                            now_ms: crate::task_memory::now_millis(),
                            limit: request.limit,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.list", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetRetainedChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::GetRetainedChild {
                            parent_id: client_id.clone(),
                            child_id: request.child_id,
                            now_ms: crate::task_memory::now_millis(),
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::RetainChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let now_ms = crate::task_memory::now_millis();
                    let child = crate::retained_child::RetainedChildV1 {
                        version: 1,
                        child_id: request.child_id,
                        parent_id: client_id.clone(),
                        family_root_id: if request.family_root_id.is_empty() {
                            client_id.clone()
                        } else {
                            request.family_root_id
                        },
                        role: request.role,
                        stable_name: (!request.stable_name.is_empty())
                            .then_some(request.stable_name),
                        lifecycle: crate::retained_child::RetainedLifecycle::Active,
                        revision: request.revision,
                        active_session_id: None,
                        grant_snapshot_hash: request.grant_snapshot_hash,
                        context_scope_hash: request.context_scope_hash,
                        workspace_state_ref: (!request.workspace_state_ref.is_empty())
                            .then_some(request.workspace_state_ref),
                        last_report_ref: (!request.last_report_ref.is_empty())
                            .then_some(request.last_report_ref),
                        retained_until_ms: if request.retained_until_ms == 0 {
                            now_ms.saturating_add(crate::retained_child::DEFAULT_TTL_MS)
                        } else {
                            request.retained_until_ms
                        },
                        created_at_ms: if request.created_at_ms == 0 {
                            now_ms
                        } else {
                            request.created_at_ms
                        },
                        last_active_at_ms: if request.last_active_at_ms == 0 {
                            now_ms
                        } else {
                            request.last_active_at_ms
                        },
                        registry_version: request.expected_registry_version.saturating_add(1),
                    };
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::RetainChild {
                            child,
                            now_ms,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.retained", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::SendChildFollowUp(request)) => {
                    let (reply, response) = oneshot::channel();
                    let mode = match request.mode.as_str() {
                        "auto" => crate::retained_child::FollowUpMode::Auto,
                        "follow_up" | "" => crate::retained_child::FollowUpMode::FollowUp,
                        "steer" => crate::retained_child::FollowUpMode::Steer,
                        _ => {
                            self.write_response(
                                writer,
                                "retained_child.follow_up",
                                b"{\"error_code\":\"invalid_scope\"}".to_vec(),
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let follow = crate::retained_child::ChildFollowUpRequestV1 {
                        version: 1,
                        idempotency_key: request.idempotency_key,
                        parent_id: client_id.clone(),
                        child_id: request.child_id,
                        family_root_id: client_id.clone(),
                        parent_sequence: 0,
                        expected_child_revision: request.expected_child_revision,
                        instruction: request.instruction,
                        context_refs: request.context_refs,
                        requested_grants: request.requested_grants,
                        budget_json: request.budget_json,
                        mode,
                        correlation_id: request.correlation_id,
                    };
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::SendChildFollowUp {
                            request: follow,
                            now_ms: crate::task_memory::now_millis(),
                            busy: false,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.follow_up", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::DeleteRetainedChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::DeleteRetainedChild {
                            parent_id: client_id.clone(),
                            child_id: request.child_id,
                            expected_registry_version: request.expected_registry_version,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.delete", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateAnalysisKernel(request)) => {
                    let projection = self.dispatch_create_analysis_kernel(request).await;
                    write_analysis_kernel_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetAnalysisKernel(request)) => {
                    let projection = self.dispatch_get_analysis_kernel(request).await;
                    write_analysis_kernel_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExecuteAnalysisKernel(request)) => {
                    let result = self.dispatch_execute_analysis_kernel(request).await;
                    write_analysis_kernel_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ResetAnalysisKernel(request)) => {
                    let result = self.dispatch_reset_analysis_kernel(request).await;
                    write_analysis_kernel_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListRefinementCandidates(request)) => {
                    let projection = self.dispatch_list_refinement_candidates(request).await;
                    write_refinement_list_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetRefinementCandidate(request)) => {
                    let projection = self.dispatch_get_refinement_candidate(request).await;
                    write_refinement_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::RefinementAction(request)) => {
                    let result = self.dispatch_refinement_action(request).await;
                    write_refinement_action_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::PreviewWorkflowPackage(request)) => {
                    let result = crate::workflow_package::preview_from_json(
                        &request.graph_json,
                        request.name,
                        request.description,
                        request.portable_argument_keys,
                        &request.credential_slots_json,
                        request.created_at,
                    )
                    .map(|preview| serde_json::json!({
                        "status": "previewed",
                        "package_hash": preview.package_hash,
                        "stripped_fields": preview.stripped_fields,
                        "package": preview.package,
                    }))
                    .map_err(|error| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "preview", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::ExportWorkflowPackage(request)) => {
                    let result = crate::workflow_package::preview_from_json(
                        &request.graph_json,
                        request.name,
                        request.description,
                        request.portable_argument_keys,
                        &request.credential_slots_json,
                        request.created_at,
                    )
                    .and_then(|preview| {
                        crate::workflow_package::write_package(
                            std::path::Path::new(&request.destination_path),
                            &preview.package,
                        )?;
                        Ok(serde_json::json!({"status":"exported","package_hash":preview.package_hash,"stripped_fields":preview.stripped_fields}))
                    })
                    .map_err(|error: crate::workflow_package::WorkflowPackageError| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "export", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::CommitWorkflowPackage(request)) => {
                    let result = async {
                        let package = crate::workflow_package::parse_bounded(&request.package_json)?;
                        let database = self.journal.database().lock().await;
                        crate::workflow_package::commit_import(
                            &database,
                            std::path::Path::new(&request.source_path),
                            &package,
                            &request.idempotency_key,
                            now_ms(),
                        )
                    }
                    .await
                    .map(|record| serde_json::json!({"status":"committed","import_id":record.import_id,"local_workflow_id":record.local_workflow_id,"package_hash":record.package_hash}))
                    .map_err(|error: crate::workflow_package::WorkflowPackageError| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "commit", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::RebindWorkflowPackage(request)) => {
                    let result = async {
                        let package =
                            crate::workflow_package::parse_bounded(&request.package_json)?;
                        let database = self.journal.database().lock().await;
                        crate::workflow_package::persist_rebind(
                            &database,
                            &package,
                            &request.slot_id,
                            &request.local_credential_reference,
                            now_ms(),
                        )
                    }
                    .await;
                    let payload = match result {
                        Ok(value) => serde_json::to_vec(
                            &serde_json::json!({"status":"rebound","binding":value}),
                        )?,
                        Err(error) => serde_json::to_vec(
                            &serde_json::json!({"status":"rejected","error_code":error.to_string()}),
                        )?,
                    };
                    self.write_package_response(writer, "rebind", payload)
                        .await?;
                }
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
                            command.protocol,
                        )
                        .await?;
                    self.write_response(writer, "doctor.report", result).await?;
                }
                Some(generated::command_envelope::Command::CreateDiagnosticsSnapshot(request)) => {
                    let result = self
                        .dispatch_create_diagnostics_snapshot(request, command.protocol)
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
                None => {}
            }
            Ok(())
        })
    }

    // ------------------------------------------------------------------
    // Постоянное слушание (план 04.5).
    //
    // Девять команд ходят прямо через мост, а не через очередь задач: им
    // нужны журнал, разрешения и реестр состояния, и ни одна из них не
    // запускает агента. Ответ уходит JSON-полезной нагрузкой тем же
    // `write_response`, что и у чеков.
    // ------------------------------------------------------------------

}

#[path = "ipc_bridge_ambient_transport.rs"]
mod transport_domain;
#[path = "ipc_bridge_ambient_state.rs"]
mod ambient_state;
#[path = "ipc_bridge_automation.rs"]
mod automation;
#[path = "ipc_bridge_workflow.rs"]
mod workflow;
#[path = "ipc_bridge_ambient_proposals.rs"]
mod proposals;
