use super::*;

impl IpcBridge {
    pub(super) async fn dispatch_receipts<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        _request_id: String,
        _client_id: String,
        _command_hash: String,
        command: Option<generated::command_envelope::Command>,
    ) -> Result<(), IpcBridgeError> {
        match command {
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

            _ => unreachable!("command routed to the wrong domain"),
        }
        Ok(())
    }
}
