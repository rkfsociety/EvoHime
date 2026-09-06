impl IpcBridge {
    pub(crate) async fn dispatch_git_read(
        &self,
        workspace_path: String,
        tool_name: &str,
        input: serde_json::Value,
        requested_max_bytes: u32,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        const DEFAULT_MAX_BYTES: usize = 512 * 1024;
        let max_bytes = if requested_max_bytes == 0 {
            DEFAULT_MAX_BYTES
        } else {
            (requested_max_bytes as usize).min(DEFAULT_MAX_BYTES)
        };
        let tools = self
            .tools
            .as_ref()
            .ok_or_else(|| FrameError::Io("Git tools are not configured".into()))?;
        let context = ToolContext {
            workspace_root: std::path::PathBuf::from(workspace_path),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let result = tools
            .execute(&context, tool_name, input)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let bytes = result.output.as_bytes();
        let truncated = bytes.len() > max_bytes;
        let output = if truncated {
            String::from_utf8_lossy(&bytes[..max_bytes]).into_owned()
        } else {
            result.output
        };
        serde_json::to_vec(&serde_json::json!({
            "output": output,
            "structured": result.structured,
            "truncated": truncated,
            "max_bytes": max_bytes,
        }))
        .map_err(|error| FrameError::Io(error.to_string()).into())
    }

    pub(crate) fn terminal_capability_snapshot(
        action_id: uuid::Uuid,
        context: &ToolContext,
        scope: &str,
    ) -> Result<evohime_receipts::capability::CapabilitySnapshotV1, String> {
        use evohime_receipts::capability::{CapabilityLimits, CapabilitySnapshotV1};
        let task_id = context.task_id.to_string();
        let session_id = context
            .session_id
            .map_or_else(|| "session:anonymous".to_owned(), |id| id.to_string());
        CapabilitySnapshotV1 {
            snapshot_id: format!("snapshot:{action_id}"),
            run_id: format!("run:{task_id}"),
            session_id,
            task_id: format!("task:{task_id}"),
            parent_snapshot_hash: None,
            policy_id: "policy:terminal-v1".into(),
            policy_version: 1,
            policy_hash: evohime_receipts::sha256_hex(b"policy:terminal-v1"),
            manifest_hash: evohime_receipts::sha256_hex(b"builtin:shell.execute:v1"),
            workspace_anchors: vec![context.workspace_root.to_string_lossy().into_owned()],
            operation_scopes: vec![scope.to_owned()],
            permissions: vec!["shell_execute".into()],
            tool_identities: vec!["shell.execute".into()],
            network_routes: vec![],
            adapter_scopes: vec![],
            secret_refs: vec![],
            limits: CapabilityLimits {
                timeout_ms: 30_000,
                input_bytes: 64 * 1024,
                output_bytes: 512 * 1024,
                concurrency: 1,
                tool_calls: 1,
                token_budget: 0,
                cost_micros: 0,
            },
            snapshot_hash: String::new(),
        }
        .finalize()
        .map_err(|error| error.to_string())
    }

    pub(crate) async fn execute_terminal_with_receipt(
        &self,
        context: &ToolContext,
        input: serde_json::Value,
        cancellation: CancellationToken,
    ) -> Result<evohime_tool_runtime::ToolResult, evohime_tool_runtime::ToolError> {
        match self
            .tools
            .as_ref()
            .ok_or_else(|| {
                evohime_tool_runtime::ToolError::Execution(
                    "Terminal tools are not configured".into(),
                )
            })?
            .preflight(context, "shell.execute", &input)
            .await?
        {
            evohime_tool_runtime::ToolPreflightDecision::Allowed { scope, preview } => {
                let scope = self
                    .tools
                    .as_ref()
                    .unwrap()
                    .permissions()
                    .normalize_scope(&scope)
                    .map_err(evohime_tool_runtime::ToolError::Execution)?;
                let request = evohime_receipts::runtime::ActionRequest {
                    action_id: uuid::Uuid::now_v7(),
                    task_id: context.task_id.to_string(),
                    run_id: context.task_id.to_string(),
                    tool_name: "shell.execute".into(),
                    policy_id: "permission:ShellExecute".into(),
                    normalized_scope: scope.clone(),
                    input: input.clone(),
                    policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                    approval_id: None,
                    parent_approval_ref: None,
                    preview: match serde_json::to_string(&preview) {
                        Ok(preview) => preview,
                        Err(error) => {
                            tracing::warn!(%error, "terminal preview serialization failed");
                            "terminal".into()
                        }
                    },
                };
                let capability =
                    Self::terminal_capability_snapshot(request.action_id, context, &scope)
                        .map_err(evohime_tool_runtime::ToolError::Execution)?;
                let gate = super::policy_gate::PolicyGate::new(capability.clone()).map_err(
                    |decision| evohime_tool_runtime::ToolError::Execution(decision.reason_code),
                )?;
                let binding = gate
                    .preflight(
                        &request.action_id.to_string(),
                        &request.tool_name,
                        &request.normalized_scope,
                        &request.input,
                        evohime_receipts::capability::PolicyOutcome::Allowed,
                    )
                    .map_err(|decision| {
                        evohime_tool_runtime::ToolError::Execution(decision.reason_code)
                    })?;
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                    database.connection_mut(),
                    &signer,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                let prepared = match runtime.prepare(request.clone()) {
                    Ok(value) => value,
                    Err(error) => {
                        let marker = if error.to_string().contains("signer_unavailable") {
                            "signer_unavailable"
                        } else {
                            "storage_key_unavailable"
                        };
                        let _ = runtime.store_unsigned_runtime_marker(request.action_id, marker);
                        return Err(evohime_tool_runtime::ToolError::Execution(
                            error.to_string(),
                        ));
                    }
                };
                if !matches!(
                    prepared,
                    evohime_receipts::runtime::PrepareOutcome::Prepared { .. }
                ) {
                    return Err(evohime_tool_runtime::ToolError::Execution(
                        "receipt.precondition_failed".into(),
                    ));
                }
                evohime_receipts::runtime::bind_capability_to_action(
                    database.connection(),
                    request.action_id,
                    &capability,
                    1,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                let decision = evohime_receipts::capability::PolicyDecision::new(
                    evohime_receipts::capability::PolicyOutcome::Allowed,
                    "preflight_allowed",
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                evohime_receipts::runtime::persist_policy_decision(
                    database.connection(),
                    request.action_id,
                    Some(&capability.snapshot_hash),
                    &decision,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                    database.connection_mut(),
                    &signer,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                runtime
                    .mark_started(request.action_id)
                    .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                record_ledger_tool_call(
                    &database,
                    &request,
                    context.session_id.map(|id| id.to_string()),
                );
                drop(database);
                gate.recheck_before_effect(
                    &binding,
                    &request.tool_name,
                    &request.normalized_scope,
                    &request.input,
                    evohime_receipts::capability::PolicyOutcome::Allowed,
                )
                .map_err(|decision| {
                    evohime_tool_runtime::ToolError::Execution(decision.reason_code)
                })?;
                let result = self
                    .tools
                    .as_ref()
                    .unwrap()
                    .execute_with_cancellation(context, "shell.execute", input, cancellation)
                    .await;
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                    database.connection_mut(),
                    &signer,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                match &result {
                    Ok(value) => {
                        runtime.mark_returned(request.action_id).map_err(|e| {
                            evohime_tool_runtime::ToolError::Execution(e.to_string())
                        })?;
                        let digest = evohime_receipts::sha256_hex(value.output.as_bytes());
                        let receipt_hash = runtime
                            .complete(&request, "succeeded", &digest, None)
                            .map_err(|e| {
                            evohime_tool_runtime::ToolError::Execution(e.to_string())
                        })?;
                        // План 08-4: the "observation" link of "action →
                        // tool call → observation → receipt" — a bounded
                        // content-addressed marker of the tool's output,
                        // published right before the terminal receipt so a
                        // reader sees the result was observed before it was
                        // signed. `runtime`'s borrow of `database` has to
                        // end (last use above) before this can borrow it.
                        record_ledger_tool_outcome(
                            &database,
                            &request,
                            context.session_id.map(|id| id.to_string()),
                            execution_ledger::ActionState::Running,
                            execution_ledger::ExecutionEventBody::Observation {
                                summary_digest: digest.clone(),
                                artifact_refs: Vec::new(),
                            },
                        );
                        record_ledger_tool_outcome(
                            &database,
                            &request,
                            context.session_id.map(|id| id.to_string()),
                            execution_ledger::ActionState::Succeeded,
                            execution_ledger::ExecutionEventBody::ToolReceipt {
                                receipt_action_id: request.action_id.to_string(),
                                receipt_hash,
                            },
                        );
                    }
                    Err(error) => {
                        runtime.mark_returned(request.action_id).map_err(|e| {
                            evohime_tool_runtime::ToolError::Execution(e.to_string())
                        })?;
                        let failed_digest = evohime_receipts::sha256_hex(b"tool_error");
                        if runtime
                            .complete(&request, "failed", &failed_digest, Some("tool_error"))
                            .is_ok()
                        {
                            // Ledger observability gets the specific bounded
                            // error code (e.g. "timed_out") even though the
                            // receipt's own error_category stays the coarser
                            // "tool_error" it already used — changing that
                            // category is a receipts-crate decision, out of
                            // scope here.
                            record_ledger_tool_outcome(
                                &database,
                                &request,
                                context.session_id.map(|id| id.to_string()),
                                execution_ledger::ActionState::Failed,
                                execution_ledger::ExecutionEventBody::TypedFailure {
                                    error_class: bounded_tool_error_code(error).to_string(),
                                    provider_error_id: None,
                                },
                            );
                            return result;
                        }
                        let mut recovery_code = "signature_failed";
                        let pre_hash = runtime
                            .action(request.action_id)
                            .ok()
                            .flatten()
                            .and_then(|row| row.pre_receipt_hash)
                            .unwrap_or_default();
                        let key_id = match self.receipt_keys.storage_key_id() {
                            Ok(value) => value,
                            Err(_) => {
                                recovery_code = "storage_key_unavailable";
                                "unavailable".to_owned()
                            }
                        };
                        let row = ProtectedActionRow {
                            schema_version: 1,
                            action_id: request.action_id.to_string(),
                            pre_receipt_hash: pre_hash,
                            tool_args_hash: evohime_receipts::runtime::canonical_call_hash(
                                &request.tool_name,
                                &request.normalized_scope,
                                &request.input,
                            )
                            .unwrap_or_default(),
                            result_status: "failed".into(),
                            result_hash: match evohime_receipts::result_hash(
                                &serde_json::json!({"status":"failed","error_category":"tool_error"}),
                            ) {
                                Ok(hash) => hash,
                                Err(error) => {
                                    tracing::warn!(%error, "failed to hash tool error receipt; using stable fallback");
                                    evohime_receipts::sha256_hex(b"tool_error")
                                }
                            },
                            recovery_code: recovery_code.into(),
                            created_at_ms: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|value| value.as_millis() as i64)
                                .unwrap_or_default(),
                            key_id,
                        };
                        if let Ok(plain) = serde_json::to_vec(&row) {
                            if let Ok(envelope) = self.receipt_keys.protect_storage(&plain) {
                                if runtime.store_protected_envelope(&row, envelope).is_err() {
                                    recovery_code = "storage_key_unavailable";
                                }
                            } else {
                                recovery_code = "storage_key_unavailable";
                            }
                        } else {
                            recovery_code = "storage_key_unavailable";
                        }
                        if recovery_code == "storage_key_unavailable" {
                            let _ = runtime.store_unsigned_runtime_marker(
                                request.action_id,
                                "storage_key_unavailable",
                            );
                        }
                        let _ = runtime.mark_pending_recovery(request.action_id, recovery_code);
                    }
                }
                result
            }
            evohime_tool_runtime::ToolPreflightDecision::Denied(permission) => Err(
                evohime_tool_runtime::ToolError::PermissionDenied(permission),
            ),
            evohime_tool_runtime::ToolPreflightDecision::ApprovalRequired { .. } => {
                // Preflight is a hard boundary. The ordinary execute path
                // creates the approval request and returns NeedsApproval;
                // dispatching the implementation here would bypass policy.
                self.tools
                    .as_ref()
                    .unwrap()
                    .execute_with_cancellation(context, "shell.execute", input, cancellation)
                    .await
            }
        }
    }

    pub(crate) async fn dispatch_terminal_execute<W: AsyncWrite + Unpin>(
        &self,
        request: generated::TerminalExecute,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        const DEFAULT_TIMEOUT_MS: u32 = 30_000;
        const MAX_OUTPUT_BYTES: usize = 512 * 1024;
        let tools = self
            .tools
            .as_ref()
            .ok_or_else(|| FrameError::Io("Terminal tools are not configured".into()))?;
        let task_id = uuid::Uuid::parse_str(&request.task_id)
            .map_err(|error| FrameError::Io(format!("invalid terminal task id: {error}")))?;
        let workspace_root = std::path::PathBuf::from(request.workspace_path);
        let input = serde_json::json!({
            "program": request.program,
            "args": request.args,
            "cwd": (!request.cwd.is_empty()).then_some(request.cwd),
            "timeout_ms": if request.timeout_ms == 0 { DEFAULT_TIMEOUT_MS } else { request.timeout_ms.min(DEFAULT_TIMEOUT_MS) },
        });
        let context = ToolContext {
            workspace_root,
            task_id,
            session_id: None,
            progress_tx: None,
        };
        let cancellation = tokio_util::sync::CancellationToken::new();
        let result = if request.approval_id.is_empty() {
            match self
                .execute_terminal_with_receipt(&context, input.clone(), cancellation.clone())
                .await
            {
                Ok(result) => result,
                Err(evohime_tool_runtime::ToolError::NeedsApproval(details)) => {
                    let evohime_tool_runtime::ApprovalRequired {
                        tool,
                        permission,
                        scope,
                        approval_id,
                        input,
                        preview,
                    } = *details;
                    let durable_action_id = uuid::Uuid::now_v7();
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id: durable_action_id,
                        task_id: task_id.to_string(),
                        run_id: task_id.to_string(),
                        tool_name: tool.clone(),
                        policy_id: format!("permission:{permission:?}"),
                        normalized_scope: scope.clone(),
                        input: input.clone(),
                        policy_decision:
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired,
                        approval_id: Some(approval_id),
                        parent_approval_ref: None,
                        preview: match serde_json::to_string(&preview) {
                            Ok(value) => value,
                            Err(error) => {
                                tracing::warn!(%error, "failed to serialize approval preview");
                                "approval".into()
                            }
                        },
                    };
                    let capability =
                        Self::terminal_capability_snapshot(durable_action_id, &context, &scope)
                            .map_err(|e| FrameError::Io(e.to_string()))?;
                    {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        runtime
                            .prepare_existing_approval(receipt_request)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                        evohime_receipts::runtime::bind_capability_to_action(
                            database.connection(),
                            durable_action_id,
                            &capability,
                            1,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        let decision = evohime_receipts::capability::PolicyDecision::new(
                            evohime_receipts::capability::PolicyOutcome::ApprovalRequired,
                            "approval_required",
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        evohime_receipts::runtime::persist_policy_decision(
                            database.connection(),
                            durable_action_id,
                            Some(&capability.snapshot_hash),
                            &decision,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                    self.write_response(
                        writer,
                        "approval.required",
                        serde_json::to_vec(&serde_json::json!({
                            "task_id": task_id.to_string(),
                            "approval_id": approval_id.to_string(),
                            "tool_name": tool,
                            "permission": format!("{permission:?}"),
                            "scope": scope,
                            "preview": preview,
                        }))?,
                    )
                    .await?;
                    return Ok(());
                }
                Err(error) => {
                    return self
                        .write_response(
                            writer,
                            "terminal.result",
                            serde_json::to_vec(&serde_json::json!({
                                "task_id": task_id.to_string(),
                                "ok": false,
                                "error_code": bounded_tool_error_code(&error),
                            }))?,
                        )
                        .await;
                }
            }
        } else {
            let approval_id = uuid::Uuid::parse_str(&request.approval_id).map_err(|error| {
                FrameError::Io(format!("invalid terminal approval id: {error}"))
            })?;
            let (action_id, receipt_request) = {
                let database = self.journal.database().lock().await;
                let (action_id, receipt_scope): (String, String) = database.connection().query_row(
                    "SELECT action_id,normalized_scope FROM receipt_approval_intents WHERE approval_id=?1",
                    [approval_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                let action_id = uuid::Uuid::parse_str(&action_id)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                (
                    action_id,
                    evohime_receipts::runtime::ActionRequest {
                        action_id,
                        task_id: task_id.to_string(),
                        run_id: task_id.to_string(),
                        tool_name: "shell.execute".into(),
                        policy_id: "permission:ShellExecute".into(),
                        normalized_scope: receipt_scope,
                        input: input.clone(),
                        policy_decision:
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired,
                        approval_id: Some(approval_id),
                        parent_approval_ref: None,
                        preview: "terminal approval".into(),
                    },
                )
            };
            let capability = Self::terminal_capability_snapshot(
                action_id,
                &context,
                &receipt_request.normalized_scope,
            )
            .map_err(|error| FrameError::Io(error.to_string()))?;
            {
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                    database.connection_mut(),
                    &signer,
                )
                .map_err(|error| FrameError::Io(error.to_string()))?;
                if let Err(error) = runtime.grant_approval(approval_id) {
                    // План 08-4 acceptance: the third arm of "approval
                    // approve/reject/expiry" — the approval window closed
                    // before the client claimed it. `grant_approval` is the
                    // one place `evohime-receipts` actually detects this
                    // (deadline check against the intent's own boot/wall
                    // clock), so it is the only honest place to observe it.
                    if matches!(
                        error,
                        evohime_receipts::runtime::RuntimeError::Code("approval_expired")
                    ) {
                        record_ledger_tool_outcome(
                            &database,
                            &receipt_request,
                            None,
                            execution_ledger::ActionState::TimedOut,
                            execution_ledger::ExecutionEventBody::ApprovalDecision {
                                approval_intent_id: approval_id.to_string(),
                                decision: execution_ledger::ApprovalOutcome::Expired,
                                snapshot_hash: None,
                            },
                        );
                    }
                    if matches!(
                        error,
                        evohime_receipts::runtime::RuntimeError::Code("approval_denied")
                    ) {
                        self.write_response(
                            writer,
                            "terminal.result",
                            serde_json::to_vec(&serde_json::json!({
                                "task_id": task_id.to_string(),
                                "ok": false,
                                "error_code": "approval_denied",
                            }))?,
                        )
                        .await?;
                        return Ok(());
                    }
                    return Err(FrameError::Io(error.to_string()).into());
                }
                runtime
                    .claim_approval_checked_with_binding(
                        &receipt_request,
                        approval_id,
                        &capability.session_id,
                        &capability.snapshot_hash,
                        capability.policy_version,
                        |_| true,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                runtime
                    .mark_started(action_id)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                record_ledger_tool_call(&database, &receipt_request, None);
            }
            match tools
                .execute_after_durable_approval(&context, "shell.execute", input, cancellation)
                .await
            {
                Ok(result) => {
                    let output_digest = evohime_receipts::sha256_hex(result.output.as_bytes());
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    runtime
                        .mark_returned(action_id)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let receipt_hash = runtime
                        .complete(&receipt_request, "succeeded", &output_digest, None)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    // See execute_terminal_with_receipt: the "observation"
                    // link of "action → tool call → observation → receipt".
                    record_ledger_tool_outcome(
                        &database,
                        &receipt_request,
                        None,
                        execution_ledger::ActionState::Running,
                        execution_ledger::ExecutionEventBody::Observation {
                            summary_digest: output_digest.clone(),
                            artifact_refs: Vec::new(),
                        },
                    );
                    record_ledger_tool_outcome(
                        &database,
                        &receipt_request,
                        None,
                        execution_ledger::ActionState::Succeeded,
                        execution_ledger::ExecutionEventBody::ToolReceipt {
                            receipt_action_id: action_id.to_string(),
                            receipt_hash,
                        },
                    );
                    result
                }
                Err(error) => {
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    ) {
                        let _ = runtime.mark_pending_recovery(action_id, "external_error");
                    }
                    // `mark_pending_recovery` (not a clean failure) leaves the
                    // dispatch marker open with an ambiguous outcome, so the
                    // ledger records this as `unknown_outcome`, not `failed` —
                    // the same distinction plan 08-2's startup reconciliation
                    // makes between "known failure" and "needs review".
                    record_ledger_tool_outcome(
                        &database,
                        &receipt_request,
                        None,
                        execution_ledger::ActionState::UnknownOutcome,
                        execution_ledger::ExecutionEventBody::TypedFailure {
                            error_class: "external_error".into(),
                            provider_error_id: None,
                        },
                    );
                    return self
                        .write_response(
                            writer,
                            "terminal.result",
                            serde_json::to_vec(&serde_json::json!({
                                "task_id": task_id.to_string(),
                                "ok": false,
                                "error_code": bounded_tool_error_code(&error),
                            }))?,
                        )
                        .await;
                }
            }
        };
        let bytes = result.output.as_bytes();
        let truncated = bytes.len() > MAX_OUTPUT_BYTES;
        let output = if truncated {
            String::from_utf8_lossy(&bytes[..MAX_OUTPUT_BYTES]).into_owned()
        } else {
            result.output
        };
        self.write_response(
            writer,
            "terminal.result",
            serde_json::to_vec(&serde_json::json!({
                "task_id": task_id.to_string(),
                "ok": true,
                "output": output,
                "structured": result.structured,
                "truncated": truncated,
                "max_bytes": MAX_OUTPUT_BYTES,
            }))?,
        )
        .await
    }

    /// Builds a control envelope (challenge, ready, protocol error) that the
    /// transport layer sends outside the command/response loop. Sequence 0
    /// keeps these events out of the replayable event stream.
    pub fn control_event(
        &self,
        event_type: &str,
        event: Option<generated::event_envelope::Event>,
        payload: Vec<u8>,
    ) -> generated::EventEnvelope {
        generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: event_type.into(),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event,
        }
    }

    /// The `core.ready` envelope this bridge answers a verified handshake with.
    pub fn ready_event(&self) -> generated::EventEnvelope {
        self.control_event(
            "core.ready",
            Some(generated::event_envelope::Event::Ready(generated::Ready {
                protocol: Some(protocol()),
                core_version: env!("CARGO_PKG_VERSION").into(),
                core_info: Some(core_info()),
            })),
            Vec::new(),
        )
    }

    pub(crate) async fn start_plan_review<W: AsyncWrite + Unpin>(
        &self,
        request: generated::StartPlanReview,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        if !request.file_name.to_ascii_lowercase().ends_with(".md") {
            return Err(FrameError::Io("review accepts Markdown files only".into()).into());
        }
        let context_documents =
            crate::plan_context::read_linked_plans(&request.source_paths, &request.source_markdown)
                .await;
        let review = crate::plan_review::ReviewRequest {
            review_id: request.review_id,
            file_name: request.file_name,
            file_names: request.file_names,
            source_markdown: request.source_markdown,
            reviewer_models: request.reviewer_models,
            synthesis_model: request.synthesis_model,
            context_documents,
        };
        review
            .validate()
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let gateway_config = self
            .gateway_config
            .clone()
            .ok_or_else(|| FrameError::Io("provider is not configured".into()))?;
        let gateway = evohime_model_gateway::ModelGateway::from_config(&gateway_config)
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let route = gateway_config
            .routes
            .get(&gateway_config.default_route)
            .ok_or_else(|| FrameError::Io("default provider route is missing".into()))?;
        let available = evohime_model_gateway::fetch_model_catalog(route)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        if review
            .reviewer_models
            .iter()
            .chain(std::iter::once(&review.synthesis_model))
            .any(|model| !available.iter().any(|entry| entry.id == *model))
        {
            return Err(FrameError::Io(
                "review model was not returned by the configured provider".into(),
            )
            .into());
        }
        let cancellation = CancellationToken::new();
        let background_permit = self
            .background_tasks
            .try_acquire()
            .ok_or_else(|| FrameError::Io("background task capacity is exhausted".into()))?;
        let review_id = review.review_id.clone();
        self.review_tasks
            .lock()
            .await
            .insert(review_id.clone(), cancellation.clone());
        let tasks = Arc::clone(&self.review_tasks);
        let results = Arc::clone(&self.review_results);
        let journal = self.journal.clone();
        let coordinator = self.coordinator.clone();
        let task_review_id = review_id.clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = Arc::new(move |progress: crate::plan_review::ReviewProgress| {
            let _ = progress_tx.send(progress);
        });
        tokio::spawn(async move {
            let _background_permit = background_permit;
            let progress_journal = journal.clone();
            let progress_coordinator = coordinator.clone();
            let progress_writer = tokio::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    publish_review_event(
                        &progress_coordinator,
                        &progress_journal,
                        CoreEvent::ReviewProgress {
                            review_id: progress.review_id,
                            stage: progress.stage,
                            status: progress.status,
                            model: progress.model,
                            completed: progress.completed,
                            total: progress.total,
                        },
                    )
                    .await;
                }
            });
            let event = match crate::plan_review::run_review_with_progress(
                Arc::new(gateway),
                review,
                cancellation,
                progress,
            )
            .await
            {
                Ok(result) => {
                    let payload = serde_json::to_string(&result).unwrap_or_default();
                    results
                        .lock()
                        .await
                        .insert(result.review_id.clone(), result.clone());
                    CoreEvent::TaskCompleted {
                        task_id: result.review_id,
                        final_message: payload,
                    }
                }
                Err(crate::plan_review::ReviewError::Cancelled) => CoreEvent::TaskStopped {
                    task_id: task_review_id.clone(),
                },
                Err(error) => CoreEvent::TaskFailed {
                    task_id: task_review_id.clone(),
                    error: error.to_string(),
                },
            };
            let _ = progress_writer.await;
            let terminal_progress = match &event {
                CoreEvent::TaskCompleted { .. } => Some(CoreEvent::ReviewProgress {
                    review_id: task_review_id.clone(),
                    stage: "completed".into(),
                    status: "completed".into(),
                    model: None,
                    completed: 1,
                    total: 1,
                }),
                CoreEvent::TaskFailed { .. } => Some(CoreEvent::ReviewProgress {
                    review_id: task_review_id.clone(),
                    stage: "failed".into(),
                    status: "failed".into(),
                    model: None,
                    completed: 0,
                    total: 1,
                }),
                _ => None,
            };
            if let Some(progress) = terminal_progress {
                publish_review_event(&coordinator, &journal, progress).await;
            }
            publish_review_event(&coordinator, &journal, event).await;
            tasks.lock().await.remove(&task_review_id);
        });
        self.write_response(
            writer,
            "review.started",
            serde_json::to_vec(&serde_json::json!({
                "review_id": review_id,
                "accepted": true,
            }))
            .unwrap_or_default(),
        )
        .await
    }

    /// Rewrites the plan a finished review was made for.
    ///
    /// The review text comes from Core's own cache or journal rather than from
    /// the shell: the shell may have been restarted, and a review the user did
    /// not actually run must never be passed off as one.
    pub(crate) async fn revise_plan<W: AsyncWrite + Unpin>(
        &self,
        request: generated::RevisePlan,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        if !request.file_name.to_ascii_lowercase().ends_with(".md") {
            return Err(FrameError::Io("revision accepts Markdown files only".into()).into());
        }
        let mut review = self
            .review_results
            .lock()
            .await
            .get(&request.review_id)
            .cloned();
        if review.is_none() {
            if let Ok(events) = self.journal.task_history(&request.review_id, 10).await {
                review = events
                    .iter()
                    .rev()
                    .find_map(|event| review_result_from_event(&event.payload));
            }
        }
        let review = review.ok_or_else(|| FrameError::Io("review not found".into()))?;
        let context_documents = crate::plan_context::read_linked_plans(
            std::slice::from_ref(&request.source_path),
            &request.source_markdown,
        )
        .await;
        let revision = crate::plan_review::RevisionRequest {
            revision_id: request.revision_id,
            review_id: request.review_id,
            file_name: request.file_name,
            source_markdown: request.source_markdown,
            review_markdown: strip_review_header(&review.final_markdown),
            model: request.model,
            context_documents,
        };
        revision
            .validate()
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let gateway_config = self
            .gateway_config
            .clone()
            .ok_or_else(|| FrameError::Io("provider is not configured".into()))?;
        let gateway = evohime_model_gateway::ModelGateway::from_config(&gateway_config)
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let route = gateway_config
            .routes
            .get(&gateway_config.default_route)
            .ok_or_else(|| FrameError::Io("default provider route is missing".into()))?;
        let available = evohime_model_gateway::fetch_model_catalog(route)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        if !available.iter().any(|entry| entry.id == revision.model) {
            return Err(FrameError::Io(
                "revision model was not returned by the configured provider".into(),
            )
            .into());
        }
        let cancellation = CancellationToken::new();
        let background_permit = self
            .background_tasks
            .try_acquire()
            .ok_or_else(|| FrameError::Io("background task capacity is exhausted".into()))?;
        let revision_id = revision.revision_id.clone();
        self.revision_tasks
            .lock()
            .await
            .insert(revision_id.clone(), cancellation.clone());
        let tasks = Arc::clone(&self.revision_tasks);
        let results = Arc::clone(&self.revision_results);
        let journal = self.journal.clone();
        let coordinator = self.coordinator.clone();
        let task_revision_id = revision_id.clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = Arc::new(move |progress: crate::plan_review::RevisionProgress| {
            let _ = progress_tx.send(progress);
        });
        tokio::spawn(async move {
            let _background_permit = background_permit;
            let progress_journal = journal.clone();
            let progress_coordinator = coordinator.clone();
            let progress_writer = tokio::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    publish_review_event(
                        &progress_coordinator,
                        &progress_journal,
                        CoreEvent::RevisionProgress {
                            revision_id: progress.revision_id,
                            status: progress.status,
                            model: progress.model,
                        },
                    )
                    .await;
                }
            });
            let event = match crate::plan_review::run_revision(
                Arc::new(gateway),
                revision,
                cancellation,
                progress,
            )
            .await
            {
                Ok(result) => {
                    let payload = serde_json::to_string(&result).unwrap_or_default();
                    results
                        .lock()
                        .await
                        .insert(result.revision_id.clone(), result.clone());
                    CoreEvent::TaskCompleted {
                        task_id: result.revision_id,
                        final_message: payload,
                    }
                }
                Err(crate::plan_review::ReviewError::Cancelled) => CoreEvent::TaskStopped {
                    task_id: task_revision_id.clone(),
                },
                Err(error) => CoreEvent::TaskFailed {
                    task_id: task_revision_id.clone(),
                    error: error.to_string(),
                },
            };
            let _ = progress_writer.await;
            publish_review_event(&coordinator, &journal, event).await;
            tasks.lock().await.remove(&task_revision_id);
        });
        self.write_response(
            writer,
            "revision.started",
            serde_json::to_vec(&serde_json::json!({
                "revision_id": revision_id,
                "accepted": true,
            }))
            .unwrap_or_default(),
        )
        .await
    }

    pub(crate) async fn dispatch_list_refinement_candidates(
        &self,
        request: generated::ListRefinementCandidates,
    ) -> generated::RefinementListProjection {
        let database = self.journal.database().lock().await;
        let store =
            evohime_local_storage::refinement_store::RefinementStore::new(database.connection());
        match store.list(&request.owner_scope, request.limit) {
            Ok(rows) => generated::RefinementListProjection {
                schema_version: crate::refinement::CONTRACT_VERSION,
                candidates: rows.into_iter().map(refinement_projection).collect(),
                truncated: request.limit > 0 && request.limit < 128,
                error_code: String::new(),
            },
            Err(_) => generated::RefinementListProjection {
                schema_version: crate::refinement::CONTRACT_VERSION,
                candidates: Vec::new(),
                truncated: false,
                error_code: "storage_failed".into(),
            },
        }
    }

    pub(crate) async fn dispatch_get_refinement_candidate(
        &self,
        request: generated::GetRefinementCandidate,
    ) -> generated::RefinementProjection {
        let database = self.journal.database().lock().await;
        let store =
            evohime_local_storage::refinement_store::RefinementStore::new(database.connection());
        store
            .get(&request.candidate_id, request.revision as i64)
            .ok()
            .flatten()
            .map(refinement_projection)
            .unwrap_or_else(|| refinement_projection_error("not_found"))
    }

    pub(crate) async fn dispatch_refinement_action(
        &self,
        request: generated::RefinementAction,
    ) -> generated::RefinementActionResult {
        let database = self.journal.database().lock().await;
        let store =
            evohime_local_storage::refinement_store::RefinementStore::new(database.connection());
        let Some(current) = store
            .get(&request.candidate_id, request.revision as i64)
            .ok()
            .flatten()
        else {
            return refinement_action_error(&request, "not_found");
        };
        if request.idempotency_key.trim().is_empty() {
            return refinement_action_error(&request, "missing_idempotency_key");
        }
        let request_hash = crate::refinement::content_hash(
            &serde_json::json!({
                "candidate_id": request.candidate_id,
                "revision": request.revision,
                "expected_version": request.expected_version,
                "action": request.action,
                "approval_token": request.approval_token,
            })
            .to_string(),
        );
        match store.replay_idempotency(
            &current.owner_scope,
            &request.idempotency_key,
            &request_hash,
        ) {
            Ok(Some(row)) => {
                return generated::RefinementActionResult {
                    schema_version: crate::refinement::CONTRACT_VERSION,
                    candidate_id: row.id,
                    revision: row.revision as u64,
                    action: request.action,
                    applied: true,
                    deduplicated: true,
                    version: row.version as u64,
                    status: row.status,
                    error_code: String::new(),
                };
            }
            Err(
                evohime_local_storage::refinement_store::RefinementStoreError::IdempotencyConflict,
            ) => return refinement_action_error(&request, "idempotency_conflict"),
            Ok(None) => {}
            Err(_) => return refinement_action_error(&request, "storage_failed"),
        }
        if request.action == "activate" && current.kind != "memory" {
            return refinement_action_error(&request, "unavailable");
        }
        if request.action == "activate"
            && current.owner_scope == "global"
            && request.approval_token.is_empty()
        {
            return refinement_action_error(&request, "approval_required");
        }
        let status = match request.action.as_str() {
            "approve" => "approved",
            "reject" => "rejected",
            "activate" => "active",
            "rollback" => "rolled_back",
            _ => return refinement_action_error(&request, "invalid_action"),
        };
        match store.transition_with_idempotency(
            evohime_local_storage::refinement_store::TransitionWithIdempotencyInput {
                id: &request.candidate_id,
                revision: request.revision as i64,
                expected_version: request.expected_version as i64,
                status,
                error_code: None,
                now_ms: crate::task_memory::now_millis() as i64,
                idempotency: Some((&request.idempotency_key, &request_hash)),
            },
        ) {
            Ok(row) => generated::RefinementActionResult {
                schema_version: crate::refinement::CONTRACT_VERSION,
                candidate_id: row.id,
                revision: row.revision as u64,
                action: request.action,
                applied: true,
                deduplicated: false,
                version: row.version as u64,
                status: row.status,
                error_code: String::new(),
            },
            Err(
                evohime_local_storage::refinement_store::RefinementStoreError::VersionConflict {
                    ..
                },
            ) => refinement_action_error(&request, "stale_version"),
            Err(_) => refinement_action_error(&request, "storage_failed"),
        }
    }

    pub(crate) async fn dispatch_create_analysis_kernel(
        &self,
        request: generated::CreateAnalysisKernel,
    ) -> generated::AnalysisKernelProjection {
        let limits = if request.limits_json.is_empty() {
            crate::analysis_kernel::KernelLimitsV1::default()
        } else {
            match serde_json::from_slice(&request.limits_json) {
                Ok(limits) => limits,
                Err(_) => return analysis_kernel_projection_error("invalid_limits"),
            }
        };
        let now = crate::task_memory::now_millis() as i64;
        let session = crate::analysis_kernel::AnalysisKernelSessionV1 {
            schema_version: crate::analysis_kernel::ANALYSIS_KERNEL_SCHEMA_VERSION,
            id: format!("kernel-{}", uuid::Uuid::new_v4()),
            task_id: request.task_id,
            workspace_id: request.workspace_id,
            runtime_version: request.runtime_version,
            package_manifest_hash: request.package_manifest_hash,
            policy_hash: request.policy_hash,
            status: crate::analysis_kernel::KernelStatus::Created,
            revision: 0,
            limits,
            created_at_ms: now,
            updated_at_ms: now,
        };
        if session.validate().is_err() {
            return analysis_kernel_projection_error("invalid_argument");
        }
        #[cfg(windows)]
        if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some() {
            let launch = crate::analysis_kernel::supervisor_command(serde_json::json!({
                "op": "kernel_launch",
                "kernel_id": session.id,
                "package_manifest_hash": session.package_manifest_hash,
            }))
            .await;
            if !matches!(launch, Ok(value) if value.get("accepted") == Some(&serde_json::Value::Bool(true)))
            {
                return analysis_kernel_projection_error("worker_unavailable");
            }
        }
        let database = self.journal.database().lock().await;
        let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
        if store.create_session(&session).is_err() {
            return analysis_kernel_projection_error("storage_failed");
        }
        if store
            .set_status(
                &session.id,
                session.revision,
                crate::analysis_kernel::KernelStatus::Running,
                now,
            )
            .is_err()
        {
            return analysis_kernel_projection_error("runtime_unavailable");
        }
        let mut session = session;
        session.status = crate::analysis_kernel::KernelStatus::Running;
        session.revision = session.revision.saturating_add(1);
        let mut runtime = match crate::analysis_kernel::KernelRuntime::new(session.clone()) {
            Ok(runtime) => runtime,
            Err(_) => return analysis_kernel_projection_error("invalid_argument"),
        };
        if runtime.start(std::time::Instant::now()).is_err() {
            return analysis_kernel_projection_error("runtime_unavailable");
        }
        self.analysis_kernels
            .lock()
            .await
            .insert(session.id.clone(), runtime);
        analysis_kernel_projection(&session, 0, "")
    }

    pub(crate) async fn dispatch_get_analysis_kernel(
        &self,
        request: generated::GetAnalysisKernel,
    ) -> generated::AnalysisKernelProjection {
        let database = self.journal.database().lock().await;
        let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
        let Ok(Some(session)) = store.get_session(&request.kernel_id) else {
            return analysis_kernel_projection_error("not_found");
        };
        let objects = store.list_objects(&session.id).unwrap_or_default();
        analysis_kernel_projection(&session, objects.len(), "")
    }

    pub(crate) async fn dispatch_execute_analysis_kernel(
        &self,
        request: generated::ExecuteAnalysisKernel,
    ) -> generated::AnalysisKernelResult {
        let operation_name = request.operation.clone();
        if !request.idempotency_key.is_empty() {
            let database = self.journal.database().lock().await;
            let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
            if store
                .get_idempotency(
                    &request.kernel_id,
                    &request.idempotency_key,
                    &operation_name,
                )
                .ok()
                .flatten()
                .is_some()
            {
                return analysis_kernel_result_error(&request.request_id, "duplicate_request");
            }
        }
        let operation = match serde_json::from_str(&format!("\"{}\"", request.operation)) {
            Ok(crate::analysis_kernel::KernelOperation::JsonParse) => {
                crate::analysis_kernel::KernelOperation::JsonParse
            }
            Ok(crate::analysis_kernel::KernelOperation::JsonSelect) => {
                crate::analysis_kernel::KernelOperation::JsonSelect
            }
            Ok(crate::analysis_kernel::KernelOperation::CsvSummary) => {
                crate::analysis_kernel::KernelOperation::CsvSummary
            }
            Ok(crate::analysis_kernel::KernelOperation::ObjectPut) => {
                crate::analysis_kernel::KernelOperation::ObjectPut
            }
            Ok(crate::analysis_kernel::KernelOperation::ArtifactRead) => {
                crate::analysis_kernel::KernelOperation::ArtifactRead
            }
            Ok(crate::analysis_kernel::KernelOperation::ToolRequest) => {
                crate::analysis_kernel::KernelOperation::ToolRequest
            }
            _ => return analysis_kernel_result_error(&request.request_id, "unsupported_operation"),
        };
        let host_request = crate::analysis_kernel::KernelHostRequestV1 {
            version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
            request_id: request.request_id,
            kernel_id: request.kernel_id.clone(),
            session_id: request.kernel_id.clone(),
            operation,
            args: request.args,
            requested_capability: (!request.requested_capability.is_empty())
                .then_some(request.requested_capability),
            context_refs: request.context_refs,
            correlation_id: request.correlation_id,
            idempotency_key: request.idempotency_key.clone(),
        };
        let mut kernels = self.analysis_kernels.lock().await;
        let Some(runtime) = kernels.get_mut(&request.kernel_id) else {
            return analysis_kernel_result_error(&host_request.request_id, "kernel_not_running");
        };
        let request_id = host_request.request_id.clone();
        #[cfg(windows)]
        if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some()
            && !matches!(
                &host_request.operation,
                crate::analysis_kernel::KernelOperation::ObjectPut
            )
        {
            let worker_args = match host_request.operation {
                crate::analysis_kernel::KernelOperation::CsvSummary => {
                    serde_json::Value::String(String::from_utf8_lossy(&host_request.args).into())
                }
                _ => match serde_json::from_slice(&host_request.args) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::debug!(%error, "host operation arguments are not JSON; preserving text payload");
                        serde_json::Value::String(
                            String::from_utf8_lossy(&host_request.args).into(),
                        )
                    }
                },
            };
            if let Err(error) = runtime.admit(&host_request, std::time::Instant::now()) {
                return analysis_kernel_result_error(&request_id, kernel_error_code(&error));
            }
            drop(kernels);
            let worker_response = crate::analysis_kernel::supervisor_command(serde_json::json!({
                "op": "kernel_execute",
                "kernel_id": request.kernel_id,
                "request": {
                    "request_id": host_request.request_id,
                    "operation": operation_name,
                    "args": worker_args,
                },
            }))
            .await;
            let response = match worker_response {
                Ok(value) if value.get("accepted") == Some(&serde_json::Value::Bool(true)) => value
                    .get("response")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                Ok(value) => {
                    let mut kernels = self.analysis_kernels.lock().await;
                    if let Some(runtime) = kernels.get_mut(&request.kernel_id) {
                        runtime.mark_crashed();
                    }
                    return analysis_kernel_result_error(
                        &request_id,
                        value
                            .get("reason")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("worker_unavailable"),
                    );
                }
                Err(_) => {
                    let mut kernels = self.analysis_kernels.lock().await;
                    if let Some(runtime) = kernels.get_mut(&request.kernel_id) {
                        runtime.mark_crashed();
                    }
                    return analysis_kernel_result_error(&request_id, "worker_unavailable");
                }
            };
            let status = response
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("error");
            if status != "ok" {
                return analysis_kernel_result_error(
                    &request_id,
                    response
                        .get("error_class")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("worker_error"),
                );
            }
            let inline_result =
                serde_json::to_vec(response.get("result").unwrap_or(&serde_json::Value::Null))
                    .unwrap_or_default();
            let mut kernels = self.analysis_kernels.lock().await;
            if let Some(runtime) = kernels.get_mut(&request.kernel_id) {
                if let Err(error) = runtime.accept_output(inline_result.len()) {
                    return analysis_kernel_result_error(&request_id, kernel_error_code(&error));
                }
            }
            return generated::AnalysisKernelResult {
                schema_version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
                request_id,
                status: "ok".into(),
                inline_result,
                object_ref: None,
                sensitivity: crate::analysis_kernel::KernelSensitivity::Internal
                    .as_str()
                    .into(),
                provenance: "core:analysis-kernel-worker".into(),
                error_class: String::new(),
            };
        }
        match runtime.execute(host_request, std::time::Instant::now()) {
            Ok(response) => {
                let result = generated::AnalysisKernelResult {
                    schema_version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
                    request_id: response.request_id,
                    status: "ok".into(),
                    inline_result: response.inline_result.unwrap_or_default(),
                    object_ref: response.object_ref.as_ref().map(analysis_kernel_object_ref),
                    sensitivity: response.sensitivity.as_str().into(),
                    provenance: response.provenance,
                    error_class: String::new(),
                };
                let database = self.journal.database().lock().await;
                let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
                if let Some(object) = response.object_ref.as_ref() {
                    let _ = store.put_object(object);
                }
                let _ = store.put_idempotency(
                    &request.kernel_id,
                    &request.idempotency_key,
                    &operation_name,
                    b"{\"status\":\"ok\"}",
                    crate::task_memory::now_millis() as i64,
                );
                result
            }
            Err(error) => analysis_kernel_result_error(&request_id, kernel_error_code(&error)),
        }
    }

    pub(crate) async fn dispatch_reset_analysis_kernel(
        &self,
        request: generated::ResetAnalysisKernel,
    ) -> generated::AnalysisKernelResult {
        if !request.idempotency_key.is_empty() {
            let database = self.journal.database().lock().await;
            let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
            if store
                .get_idempotency(&request.kernel_id, &request.idempotency_key, "reset")
                .ok()
                .flatten()
                .is_some()
            {
                return analysis_kernel_result_error("", "duplicate_request");
            }
        }
        if !self
            .analysis_kernels
            .lock()
            .await
            .contains_key(&request.kernel_id)
        {
            return analysis_kernel_result_error("", "not_found");
        }
        #[cfg(windows)]
        if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some() {
            let stopped = crate::analysis_kernel::supervisor_command(serde_json::json!({
                "op": "kernel_stop",
                "kernel_id": request.kernel_id,
            }))
            .await;
            if !matches!(stopped, Ok(value) if value.get("accepted") == Some(&serde_json::Value::Bool(true)))
            {
                return analysis_kernel_result_error("", "worker_unavailable");
            }
        }
        let status_result = {
            let database = self.journal.database().lock().await;
            let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
            store.set_status(
                &request.kernel_id,
                request.expected_revision,
                crate::analysis_kernel::KernelStatus::Reset,
                crate::task_memory::now_millis() as i64,
            )
        };
        let result = match status_result {
            Ok(_) => {
                self.analysis_kernels
                    .lock()
                    .await
                    .get_mut(&request.kernel_id)
                    .expect("kernel existence checked before reset")
                    .reset();
                generated::AnalysisKernelResult {
                    schema_version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
                    request_id: String::new(),
                    status: "reset".into(),
                    inline_result: Vec::new(),
                    object_ref: None,
                    sensitivity: "internal".into(),
                    provenance: "core:analysis-kernel".into(),
                    error_class: String::new(),
                }
            }
            Err(error) => analysis_kernel_result_error("", kernel_storage_error_code(&error)),
        };
        if result.status == "reset" && !request.idempotency_key.is_empty() {
            let database = self.journal.database().lock().await;
            let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
            let _ = store.put_idempotency(
                &request.kernel_id,
                &request.idempotency_key,
                "reset",
                b"{\"status\":\"reset\"}",
                crate::task_memory::now_millis() as i64,
            );
        }
        result
    }

    pub(crate) async fn dispatch_get_task_checkpoint(
        &self,
        request: generated::GetTaskCheckpoint,
    ) -> generated::TaskCheckpointProjection {
        let task_id = request.task_id;
        let max_replay_events = if request.max_replay_events == 0 {
            TASK_CHECKPOINT_IPC_MAX_REPLAY_EVENTS
        } else {
            request.max_replay_events as usize
        };
        if !valid_checkpoint_token(&task_id, 128)
            || !valid_checkpoint_workspace(&request.workspace_path)
            || max_replay_events > TASK_CHECKPOINT_IPC_MAX_REPLAY_EVENTS
        {
            return task_checkpoint_projection_error(&task_id, "invalid_argument");
        }
        let runtime = crate::task_checkpoint::TaskCheckpointRuntime::new(self.journal.clone());
        match runtime
            .recover(&task_id, std::path::Path::new(&request.workspace_path))
            .await
        {
            Ok(recovery) => task_checkpoint_projection(&task_id, recovery, max_replay_events),
            Err(error) => task_checkpoint_projection_error(&task_id, checkpoint_error_code(&error)),
        }
    }

    pub(crate) async fn dispatch_resolve_task_checkpoint(
        &self,
        request: generated::ResolveTaskCheckpoint,
    ) -> Result<generated::TaskCheckpointActionResult, IpcBridgeError> {
        let task_id = request.task_id;
        let checkpoint_id = request.checkpoint_id;
        let action = request.action;
        let idempotency_key = request.idempotency_key;
        let invalid = !valid_checkpoint_token(&task_id, 128)
            || !valid_checkpoint_workspace(&request.workspace_path)
            || !valid_checkpoint_token(&checkpoint_id, 128)
            || request.expected_source_event_seq < 0
            || !matches!(action.as_str(), "acknowledge_recovery" | "request_resume")
            || !valid_checkpoint_token(&idempotency_key, 128);
        if invalid {
            return Ok(task_checkpoint_action_result(
                task_id,
                checkpoint_id,
                action,
                false,
                false,
                "invalid_argument",
                "Запрос действия checkpoint отклонён.",
            ));
        }

        let runtime = crate::task_checkpoint::TaskCheckpointRuntime::new(self.journal.clone());
        let recovery = match runtime
            .recover(&task_id, std::path::Path::new(&request.workspace_path))
            .await
        {
            Ok(recovery) => recovery,
            Err(error) => {
                return Ok(task_checkpoint_action_result(
                    task_id,
                    checkpoint_id,
                    action,
                    false,
                    false,
                    checkpoint_error_code(&error),
                    "Состояние checkpoint недоступно.",
                ));
            }
        };
        let (applied, error_code, error_message) = match recovery.checkpoint.as_ref() {
            None => (
                false,
                "checkpoint_not_found",
                "Checkpoint для задачи не найден.",
            ),
            Some(checkpoint)
                if checkpoint.id != checkpoint_id
                    || checkpoint.source_event_seq != request.expected_source_event_seq =>
            {
                (
                    false,
                    "stale_action",
                    "Состояние checkpoint уже изменилось; обнови проекцию.",
                )
            }
            Some(_) if action == "request_resume" => {
                if recovery.disposition == crate::task_checkpoint::RecoveryDisposition::Replayable {
                    (
                        true,
                        "",
                        "Запрос reconciliation записан; внешний effect автоматически не повторяется.",
                    )
                } else {
                    (
                        false,
                        "recovery_blocked",
                        "Продолжение заблокировано до явной reconciliation.",
                    )
                }
            }
            Some(_) => (true, "", "Состояние checkpoint подтверждено пользователем."),
        };
        let request_id = format!("{task_id}:{idempotency_key}");
        let command_hash = crate::research::sha256_hex(
            format!(
                "{task_id}|{checkpoint_id}|{}|{}",
                request.expected_source_event_seq, action
            )
            .as_bytes(),
        );
        let record = TaskCheckpointActionRecord {
            task_id: task_id.clone(),
            checkpoint_id: checkpoint_id.clone(),
            action: action.clone(),
            applied,
            deduplicated: false,
            error_code: error_code.into(),
            error_message: error_message.into(),
        };
        let result_payload = serde_json::to_vec(&record)?;
        let event_payload = serde_json::to_vec(&serde_json::json!({
            "checkpoint_id": checkpoint_id,
            "action": action,
            "expected_source_event_seq": request.expected_source_event_seq,
            "applied": applied,
            "error_code": error_code,
        }))?;
        let stored = match self
            .journal
            .record_task_checkpoint_action(
                &task_id,
                &request_id,
                &command_hash,
                &event_payload,
                &result_payload,
            )
            .await
        {
            Ok(stored) => stored,
            Err(StorageError::DeduplicationConflict { .. }) => {
                return Ok(task_checkpoint_action_result(
                    task_id,
                    checkpoint_id,
                    action,
                    false,
                    false,
                    "idempotency_conflict",
                    "Ключ idempotency уже использован для другого действия.",
                ));
            }
            Err(_) => {
                return Ok(task_checkpoint_action_result(
                    task_id,
                    checkpoint_id,
                    action,
                    false,
                    false,
                    "storage_failed",
                    "Действие checkpoint не удалось записать.",
                ));
            }
        };
        let deduplicated = stored.is_some();
        let mut record = match stored {
            Some(stored) => match serde_json::from_slice::<TaskCheckpointActionRecord>(&stored) {
                Ok(record) => record,
                Err(_) => {
                    return Ok(task_checkpoint_action_result(
                        task_id,
                        checkpoint_id,
                        action,
                        false,
                        true,
                        "storage_failed",
                        "Сохранённый результат действия checkpoint повреждён.",
                    ));
                }
            },
            None => record,
        };
        record.deduplicated = deduplicated;
        Ok(task_checkpoint_action_result_from_record(record))
    }
}
