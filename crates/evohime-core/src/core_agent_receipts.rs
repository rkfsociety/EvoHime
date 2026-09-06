use super::*;

impl ToolAgent {
    pub(super) async fn receipt_prepare_approval(
        &self,
        approval: ReceiptApprovalInput<'_>,
    ) -> Result<(), String> {
        let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) else {
            return Ok(());
        };
        let action_id = Uuid::now_v7();
        let request = ReceiptActionRequest {
            action_id,
            task_id: approval.task_id.to_owned(),
            run_id: approval.task_id.to_owned(),
            tool_name: approval.tool.to_owned(),
            policy_id: format!("permission:{}", approval.permission),
            normalized_scope: approval.scope.to_owned(),
            input: approval.input.clone(),
            policy_decision: ReceiptPolicyDecision::ApprovalRequired,
            approval_id: Some(approval.approval_id),
            parent_approval_ref: None,
            preview: serde_json::to_string(approval.preview)
                .map_err(|error| format!("approval preview serialization failed: {error}"))?,
        };
        let capability = Self::capability_snapshot_for_action(
            action_id,
            approval.task_id,
            approval.tool,
            approval.scope,
        )?;
        let mut database = journal.database().lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        let mut runtime = ReceiptRuntime::new(database.connection_mut(), &signer)
            .map_err(|error| error.to_string())?;
        let prepared = match runtime.prepare_existing_approval(request.clone()) {
            Ok(value) => value,
            Err(error) => {
                let code = error.to_string();
                let marker = if code.contains("signer_unavailable") {
                    "signer_unavailable"
                } else if code.contains("storage_key_unavailable") {
                    "storage_key_unavailable"
                } else {
                    "signer_unavailable"
                };
                let _ = runtime.store_unsigned_runtime_marker(request.action_id, marker);
                return Err(code);
            }
        };
        evohime_receipts::runtime::bind_capability_to_action(
            database.connection(),
            action_id,
            &capability,
            1,
        )
        .map_err(|e| e.to_string())?;
        let decision = evohime_receipts::capability::PolicyDecision::new(
            evohime_receipts::capability::PolicyOutcome::ApprovalRequired,
            "approval_required",
        )
        .map_err(|e| e.to_string())?;
        evohime_receipts::runtime::persist_policy_decision(
            database.connection(),
            action_id,
            Some(&capability.snapshot_hash),
            &decision,
        )
        .map_err(|e| e.to_string())?;
        match prepared {
            ReceiptPrepareOutcome::ApprovalRequired { .. } => Ok(()),
            _ => Err("receipt.approval_required".to_owned()),
        }
    }

    pub(super) async fn receipt_prepare_allowed(
        &self,
        task_id: &str,
        tool: &str,
        scope: &str,
        input: &serde_json::Value,
        preview: &evohime_permissions::ApprovalPreview,
    ) -> Result<Option<ReceiptActionRequest>, String> {
        let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) else {
            return Ok(None);
        };
        let request = ReceiptActionRequest {
            action_id: Uuid::now_v7(),
            task_id: task_id.to_owned(),
            run_id: task_id.to_owned(),
            tool_name: tool.to_owned(),
            policy_id: PERMISSION_POLICY_ID.into(),
            normalized_scope: scope.to_owned(),
            input: input.clone(),
            policy_decision: ReceiptPolicyDecision::Allow,
            approval_id: None,
            parent_approval_ref: None,
            preview: serde_json::to_string(preview)
                .map_err(|error| format!("read preview serialization failed: {error}"))?,
        };
        let capability =
            Self::capability_snapshot_for_action(request.action_id, task_id, tool, scope)?;
        let mut database = journal.database().lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        let mut runtime =
            ReceiptRuntime::new(database.connection_mut(), &signer).map_err(|e| e.to_string())?;
        let prepared = match runtime.prepare(request.clone()) {
            Ok(value) => value,
            Err(error) => {
                let code = error.to_string();
                let marker = if code.contains("signer_unavailable") {
                    "signer_unavailable"
                } else if code.contains("storage_key_unavailable") {
                    "storage_key_unavailable"
                } else {
                    "signer_unavailable"
                };
                let _ = runtime.store_unsigned_runtime_marker(request.action_id, marker);
                return Err(code);
            }
        };
        if !matches!(prepared, ReceiptPrepareOutcome::Prepared { .. }) {
            return Err("receipt.precondition_failed".into());
        }
        evohime_receipts::runtime::bind_capability_to_action(
            database.connection(),
            request.action_id,
            &capability,
            1,
        )
        .map_err(|e| e.to_string())?;
        let decision = evohime_receipts::capability::PolicyDecision::new(
            evohime_receipts::capability::PolicyOutcome::Allowed,
            "preflight_allowed",
        )
        .map_err(|e| e.to_string())?;
        evohime_receipts::runtime::persist_policy_decision(
            database.connection(),
            request.action_id,
            Some(&capability.snapshot_hash),
            &decision,
        )
        .map_err(|e| e.to_string())?;
        let runtime =
            ReceiptRuntime::new(database.connection_mut(), &signer).map_err(|e| e.to_string())?;
        runtime
            .mark_started(request.action_id)
            .map_err(|e| e.to_string())?;
        Ok(Some(request))
    }

    pub(super) async fn receipt_claim_approval(
        &self,
        approval: ReceiptClaimInput<'_>,
    ) -> Result<(Uuid, ReceiptActionRequest), String> {
        let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) else {
            return Ok((
                Uuid::nil(),
                ReceiptActionRequest {
                    action_id: Uuid::nil(),
                    task_id: approval.task_id.to_owned(),
                    run_id: approval.task_id.to_owned(),
                    tool_name: approval.tool.to_owned(),
                    policy_id: approval.permission.to_owned(),
                    normalized_scope: approval.scope.to_owned(),
                    input: approval.input.clone(),
                    policy_decision: ReceiptPolicyDecision::ApprovalRequired,
                    approval_id: Some(approval.approval_id),
                    parent_approval_ref: None,
                    preview: String::new(),
                },
            ));
        };
        let action_id = {
            let database = journal.database().lock().await;
            database
                .connection()
                .query_row(
                    "SELECT action_id FROM receipt_approval_intents WHERE approval_id=?1",
                    [approval.approval_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|error| error.to_string())?
                .parse::<Uuid>()
                .map_err(|_| "receipt.schema_violation".to_owned())?
        };
        let request = ReceiptActionRequest {
            action_id,
            task_id: approval.task_id.to_owned(),
            run_id: approval.task_id.to_owned(),
            tool_name: approval.tool.to_owned(),
            policy_id: format!("permission:{}", approval.permission),
            normalized_scope: approval.scope.to_owned(),
            input: approval.input.clone(),
            policy_decision: ReceiptPolicyDecision::ApprovalRequired,
            approval_id: Some(approval.approval_id),
            parent_approval_ref: None,
            preview: serde_json::to_string(approval.preview)
                .map_err(|error| format!("approval preview serialization failed: {error}"))?,
        };
        let capability = Self::capability_snapshot_for_action(
            action_id,
            approval.task_id,
            approval.tool,
            approval.scope,
        )?;
        // Execution-gate policy recheck: a stale approval never bypasses a
        // policy that changed after Prepare. This is a global-mode recheck
        // (scope-specific rechecks are covered separately by the exact
        // call-hash comparison inside claim_approval_checked).
        let policy_ok = matches!(
            self.tools
                .permissions()
                .check(approval.permission_value)
                .await,
            evohime_permissions::PermissionDecision::Allowed
                | evohime_permissions::PermissionDecision::NeedsApproval
        );
        let mut database = journal.database().lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        let mut runtime =
            ReceiptRuntime::new(database.connection_mut(), &signer).map_err(|e| e.to_string())?;
        runtime
            .grant_approval(approval.approval_id)
            .map_err(|e| e.to_string())?;
        runtime
            .claim_approval_checked_with_binding(
                &request,
                approval.approval_id,
                &capability.session_id,
                &capability.snapshot_hash,
                capability.policy_version,
                |_| policy_ok,
            )
            .map_err(|e| e.to_string())?;
        Ok((action_id, request))
    }

    pub(super) async fn receipt_refuse_approval(&self, refusal: ReceiptRefuseInput<'_>) {
        let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) else {
            return;
        };
        let mut database = journal.database().lock().await;
        let action_id: Result<String, _> = database.connection().query_row(
            "SELECT action_id FROM receipt_approval_intents WHERE approval_id=?1",
            [refusal.approval_id.to_string()],
            |row| row.get(0),
        );
        let Ok(action_id) = action_id else {
            return;
        };
        let Ok(action_id) = action_id.parse::<Uuid>() else {
            return;
        };
        let request = ReceiptActionRequest {
            action_id,
            task_id: refusal.task_id.to_owned(),
            run_id: refusal.task_id.to_owned(),
            tool_name: refusal.tool.to_owned(),
            policy_id: format!("permission:{}", refusal.permission),
            normalized_scope: refusal.scope.to_owned(),
            input: refusal.input.clone(),
            policy_decision: ReceiptPolicyDecision::ApprovalRequired,
            approval_id: Some(refusal.approval_id),
            parent_approval_ref: None,
            preview: match serde_json::to_string(refusal.preview) {
                Ok(preview) => preview,
                Err(error) => {
                    tracing::warn!(%error, "approval preview serialization failed during refusal");
                    "approval".to_owned()
                }
            },
        };
        let signer = CoreReceiptSigner(Arc::clone(keys));
        let Ok(mut runtime) = ReceiptRuntime::new(database.connection_mut(), &signer) else {
            return;
        };
        let _ = runtime.refuse(&request, refusal.code);
    }

    pub(super) async fn execute_tool_with_receipt(
        &self,
        context: &ToolContext,
        name: &str,
        input: serde_json::Value,
        cancellation: CancellationToken,
    ) -> Result<evohime_tool_runtime::ToolResult, evohime_tool_runtime::ToolError> {
        let preflight = self.tools.preflight(context, name, &input).await?;
        match preflight {
            evohime_tool_runtime::ToolPreflightDecision::Denied(permission) => {
                if let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) {
                    let request = ReceiptActionRequest {
                        action_id: Uuid::now_v7(),
                        task_id: context.task_id.to_string(),
                        run_id: context.task_id.to_string(),
                        tool_name: name.to_owned(),
                        policy_id: PERMISSION_POLICY_ID.into(),
                        normalized_scope: String::new(),
                        input: input.clone(),
                        policy_decision: ReceiptPolicyDecision::Deny,
                        approval_id: None,
                        parent_approval_ref: None,
                        preview: String::new(),
                    };
                    let mut database = journal.database().lock().await;
                    let signer = CoreReceiptSigner(Arc::clone(keys));
                    if let Ok(mut runtime) = ReceiptRuntime::new(database.connection_mut(), &signer)
                    {
                        if runtime.prepare(request.clone()).is_err() {
                            let _ = runtime.store_unsigned_runtime_marker(
                                request.action_id,
                                "signer_unavailable",
                            );
                        }
                    }
                }
                Err(evohime_tool_runtime::ToolError::PermissionDenied(
                    permission,
                ))
            }
            evohime_tool_runtime::ToolPreflightDecision::ApprovalRequired { .. } => {
                // A preflight approval request must never fall through to the
                // effect implementation. Re-entering the ordinary execute
                // path creates the approval intent and returns NeedsApproval.
                self.tools
                    .execute_with_cancellation(context, name, input, cancellation)
                    .await
            }
            evohime_tool_runtime::ToolPreflightDecision::Allowed { scope, preview } => {
                let scope = self
                    .tools
                    .permissions()
                    .normalize_scope(&scope)
                    .map_err(evohime_tool_runtime::ToolError::Execution)?;
                let read_only = matches!(
                    name,
                    TOOL_FILESYSTEM_READ
                        | TOOL_FILESYSTEM_LIST
                        | "git.status"
                        | "git.diff"
                        | "workspace.list"
                        | "workspace.read"
                        | "workspace.search"
                );
                if read_only {
                    let candidate_id = Uuid::now_v7();
                    if let Some((false, policy_version)) =
                        self.receipt_sampling_decision(candidate_id, name).await
                    {
                        let result = self
                            .tools
                            .execute_with_cancellation(context, name, input.clone(), cancellation)
                            .await;
                        if result.is_ok() {
                            self.receipt_unsampled_marker(
                                candidate_id,
                                name,
                                &scope,
                                &input,
                                policy_version,
                            )
                            .await;
                            return result;
                        }
                        let request = self
                            .receipt_prepare_allowed(
                                &context.task_id.to_string(),
                                name,
                                &scope,
                                &input,
                                &preview,
                            )
                            .await
                            .map_err(evohime_tool_runtime::ToolError::Execution)?;
                        if let Some(request) = request {
                            let outcome = match &result {
                                Ok(value) => recovery::ToolOutcome::success(value.clone()),
                                Err(error) => recovery::ToolOutcome::from_error(
                                    evohime_tool_runtime::ToolError::Execution(error.to_string()),
                                ),
                            };
                            self.receipt_complete(&request, &outcome).await;
                        }
                        return result;
                    }
                }
                let request = self
                    .receipt_prepare_allowed(
                        &context.task_id.to_string(),
                        name,
                        &scope,
                        &input,
                        &preview,
                    )
                    .await
                    .map_err(evohime_tool_runtime::ToolError::Execution)?;
                let result = self
                    .tools
                    .execute_with_cancellation(context, name, input, cancellation)
                    .await;
                if let Some(request) = request {
                    if matches!(
                        &result,
                        Err(evohime_tool_runtime::ToolError::NeedsApproval(_))
                    ) {
                        self.receipt_pending(&request, "unknown").await;
                        return Err(evohime_tool_runtime::ToolError::Execution(
                            "receipt.policy_changed".into(),
                        ));
                    }
                    let outcome = match &result {
                        Ok(value) => recovery::ToolOutcome::success(value.clone()),
                        Err(error) => recovery::ToolOutcome::from_error(
                            evohime_tool_runtime::ToolError::Execution(error.to_string()),
                        ),
                    };
                    self.receipt_complete(&request, &outcome).await;
                }
                result
            }
        }
    }

    pub(super) async fn receipt_complete(
        &self,
        request: &ReceiptActionRequest,
        outcome: &recovery::ToolOutcome,
    ) {
        let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) else {
            return;
        };
        let output_digest = outcome
            .structured
            .get("output_digest")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| evohime_receipts::sha256_hex(outcome.output.as_bytes()));
        let mut database = journal.database().lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        let mut runtime = match ReceiptRuntime::new(database.connection_mut(), &signer) {
            Ok(value) => value,
            Err(_) => return,
        };
        let status = if outcome.ok { "succeeded" } else { "failed" };
        runtime.mark_returned(request.action_id).ok();
        let completion = runtime.complete(
            request,
            status,
            &output_digest,
            (!outcome.ok).then_some("tool_error"),
        );
        if let Ok(terminal_receipt_hash) = completion {
            let _ = evohime_local_storage::model_provenance::ModelProvenanceRepository::new(
                database.connection(),
            )
            .link_tool_receipt(
                &request.task_id,
                &request.tool_name,
                &request.action_id.to_string(),
                &terminal_receipt_hash,
            );
        } else {
            let mut recovery_code = "signature_failed";
            let pre_hash = runtime
                .action(request.action_id)
                .ok()
                .flatten()
                .and_then(|row| row.pre_receipt_hash)
                .unwrap_or_default();
            let key_id = match keys.storage_key_id() {
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
                result_status: status.to_owned(),
                result_hash: match evohime_receipts::result_hash(&if outcome.ok {
                    serde_json::json!({"status":"succeeded","output_digest":output_digest})
                } else {
                    serde_json::json!({"status":"failed","error_category":"tool_error"})
                }) {
                    Ok(hash) => hash,
                    Err(error) => {
                        tracing::warn!(%error, "tool result hash serialization failed");
                        evohime_receipts::sha256_hex(b"tool_error")
                    }
                },
                recovery_code: recovery_code.to_owned(),
                created_at_ms: SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|value| value.as_millis() as i64)
                    .unwrap_or_default(),
                key_id,
            };
            if let Ok(plain) = serde_json::to_vec(&row) {
                match keys.protect_storage(&plain) {
                    Ok(envelope) => {
                        if runtime.store_protected_envelope(&row, envelope).is_err() {
                            recovery_code = "storage_key_unavailable";
                        }
                    }
                    Err(_) => recovery_code = "storage_key_unavailable",
                }
            } else {
                recovery_code = "storage_key_unavailable";
            }
            if recovery_code == "storage_key_unavailable" {
                let _ = runtime
                    .store_unsigned_runtime_marker(request.action_id, "storage_key_unavailable");
            }
            let _ = runtime.mark_pending_recovery(request.action_id, recovery_code);
        }
    }

    pub(super) async fn receipt_pending(&self, request: &ReceiptActionRequest, code: &str) {
        let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) else {
            return;
        };
        let mut database = journal.database().lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        if let Ok(runtime) = ReceiptRuntime::new(database.connection_mut(), &signer) {
            let _ = runtime.mark_pending_recovery(request.action_id, code);
        }
    }

    pub(super) async fn receipt_sampling_decision(
        &self,
        action_id: Uuid,
        tool: &str,
    ) -> Option<(bool, u8)> {
        let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) else {
            return None;
        };
        let mut database = journal.database().lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        let runtime = ReceiptRuntime::new(database.connection_mut(), &signer).ok()?;
        let (rate, version) = runtime.audit_sampling_config().ok()?;
        Some((
            evohime_receipts::runtime::sampled_read_only(&action_id.to_string(), tool, rate),
            version,
        ))
    }

    pub(super) async fn receipt_unsampled_marker(
        &self,
        action_id: Uuid,
        tool: &str,
        scope: &str,
        input: &serde_json::Value,
        policy_version: u8,
    ) {
        let (Some(journal), Some(keys)) = (&self.journal, &self.receipt_keys) else {
            return;
        };
        let Ok(call_hash) = evohime_receipts::runtime::canonical_call_hash(tool, scope, input)
        else {
            return;
        };
        let mut database = journal.database().lock().await;
        let signer = CoreReceiptSigner(Arc::clone(keys));
        if let Ok(runtime) = ReceiptRuntime::new(database.connection_mut(), &signer) {
            let _ = runtime.store_unsampled_read_only_marker(
                action_id,
                tool,
                &call_hash,
                policy_version,
            );
        }
    }
}
