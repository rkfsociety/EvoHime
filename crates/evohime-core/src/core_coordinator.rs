#[derive(Clone)]
pub struct TaskCoordinator {
    commands: mpsc::Sender<CoreCommand>,
    state: Arc<Mutex<CoordinatorState>>,
    journalled: tokio::sync::watch::Receiver<u64>,
    /// Тот же канал, по которому координатор сообщает о записанном событии.
    ///
    /// Нужен производителям, которые пишут в журнал напрямую (ambient-путь):
    /// pipe-сервер сбрасывает хвост журнала только по этому сигналу, и без
    /// него запись легла бы в базу, но не дошла бы до открытого окна.
    journalled_tx: Arc<tokio::sync::watch::Sender<u64>>,
}

pub(crate) struct CoordinatorState {
    command_tx: mpsc::Sender<CoreCommand>,
    marker_gate: crate::code_anchored_intent_markers::MarkerGate,
    tasks: HashMap<String, ActiveTask>,
    workspace_index_cancellations: HashMap<String, CancellationToken>,
    backup_cancellations: HashMap<String, CancellationToken>,
    backup_approvals: HashMap<String, String>,
    routing_decisions: HashMap<String, bool>,
    routing_approvals: RoutingApprovalRegistry,
    events: broadcast::Sender<CoreEvent>,
    executor: Option<Arc<dyn TaskExecutor>>,
    journal: Option<EventJournal>,
    audit: crate::audit::AuditTrail,
    retained_children: crate::retained_child::RetainedRegistry,
    background_tasks: Arc<crate::bounded_tasks::BoundedTaskGroup>,
}

struct ActiveTask {
    cancellation: CancellationToken,
}

impl TaskCoordinator {
    pub fn new(buffer: usize) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, None, None)

    }

    /// Additional listener on the same event stream. Used by the pipe server to
    /// know when to flush the journal tail to a connected shell.
    pub async fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.state.lock().await.events.subscribe()
    }

    /// Fires after an event is durably recorded, carrying its sequence. The
    /// pipe server flushes the journal tail on this, so a shell never has to
    /// wait for the next event to see the previous one.
    pub fn journalled(&self) -> tokio::sync::watch::Receiver<u64> {
        self.journalled.clone()
    }

    /// Publishes an event produced outside the task executor.
    ///
    /// Recording straight into the journal is not enough: the pipe server
    /// flushes its tail only on the `journalled` signal, which the coordinator
    /// raises after it records an event taken from this broadcast. A producer
    /// that bypasses the broadcast lands in the database but never reaches a
    /// connected shell.
    pub async fn emit(&self, event: CoreEvent) {
        let _ = self.state.lock().await.events.send(event);
    }

    /// Сообщает, что в журнал легла запись, минуя broadcast координатора.
    ///
    /// Ambient-события пишутся прямо в журнал: у них нет варианта `CoreEvent`
    /// и не должно быть — иначе текстовые поля `CoreEvent` стали бы для них
    /// доступны. Сигнал остаётся общим, поэтому оболочка получает их так же
    /// быстро, как события задач.
    pub fn notify_journalled(&self, sequence: u64) {
        let _ = self.journalled_tx.send(sequence);
    }

    pub async fn attach_routing_approvals(&self, approvals: RoutingApprovalRegistry) {
        self.state.lock().await.routing_approvals = approvals;
    }

    pub fn new_with_executor(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, executor, None)
    }

    pub fn new_with_journal(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
        journal: EventJournal,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, executor, Some(journal))
    }

    pub(crate) fn build(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
        journal: Option<EventJournal>,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        let (commands, mut command_rx) = mpsc::channel(buffer.max(1));
        let (events, event_rx) = broadcast::channel(buffer.max(1));
        let background_tasks = Arc::new(crate::bounded_tasks::BoundedTaskGroup::new(
            crate::bounded_tasks::DEFAULT_CAPACITY,
        ));
        let state = Arc::new(Mutex::new(CoordinatorState {
            command_tx: commands.clone(),
            marker_gate: crate::code_anchored_intent_markers::MarkerGate::default(),
            tasks: HashMap::new(),
            workspace_index_cancellations: HashMap::new(),
            backup_cancellations: HashMap::new(),
            backup_approvals: HashMap::new(),
            routing_decisions: HashMap::new(),
            routing_approvals: RoutingApprovalRegistry::default(),
            events: events.clone(),
            executor,
            journal: journal.clone(),
            audit: crate::audit::AuditTrail::default(),
            retained_children: crate::retained_child::RetainedRegistry::default(),
            background_tasks: Arc::clone(&background_tasks),
        }));
        // The shell is fed from the journal, so it must be told after a record
        // lands — not when the event was broadcast. Watching the broadcast
        // directly raced the writer and left the last event of a task unsent.
        let (journalled, journalled_rx) = tokio::sync::watch::channel(0_u64);
        let journalled = Arc::new(journalled);
        if let Some(journal) = journal {
            let mut journal_receiver = events.subscribe();
            let journalled = Arc::clone(&journalled);
            tokio::spawn(async move {
                while let Ok(event) = journal_receiver.recv().await {
                    if let Ok(sequence) = journal.record(&event).await {
                        let _ = journalled.send(sequence.max(0) as u64);
                    }
                }
            });
        }
        let audit_state = Arc::clone(&state);
        let mut audit_receiver = events.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = audit_receiver.recv().await {
                Self::record_audit_for_event(&audit_state, &event).await;
            }
        });
        let worker_state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                Self::handle_command(Arc::clone(&worker_state), command).await;
            }
        });
        (
            Self {
                commands,
                state,
                journalled: journalled_rx,
                journalled_tx: journalled,
            },
            event_rx,
        )
    }

    // `SendError` по контракту tokio возвращает вызывающему саму неотправленную
    // команду, поэтому размер Err-варианта здесь неизбежен и боксировать его нельзя
    // без слома API диспетчеризации.
    #[expect(
        clippy::result_large_err,
        reason = "Tokio SendError preserves the unsent CoreCommand for dispatch recovery"
    )]
    pub async fn dispatch(
        &self,
        command: CoreCommand,
    ) -> Result<(), mpsc::error::SendError<CoreCommand>> {
        self.commands.send(command).await
    }

    /// Appends a bounded, durable audit record. Failures to append (bounds
    /// exceeded, invalid fields) are non-fatal to the caller: audit logging
    /// must never block or fail a live command.
    pub(crate) async fn record_audit(
        state: &Arc<Mutex<CoordinatorState>>,
        kind: crate::audit::AuditKind,
        actor: impl Into<String>,
        event_id: impl Into<String>,
        fields: impl IntoIterator<Item = (String, String)>,
    ) {
        let mut state_guard = state.lock().await;
        let sequence = state_guard.audit.records().len() as u64;
        let record = match crate::audit::AuditRecord::new(sequence, event_id, kind, actor, fields) {
            Ok(record) => record,
            Err(_) => return,
        };
        let Ok(line) = record.to_json_line() else {
            return;
        };
        if state_guard.audit.append(record).is_ok() {
            drop(state_guard);
            append_audit_line(&line);
        }
    }

    /// Shared confirm/reject path. Both are approval-gated, batched and
    /// idempotent: each id reports the state the store actually holds after
    /// the call, so a replayed request produces the same answer instead of a
    /// second transition. Concurrent actions on one id are serialized by the
    /// storage transaction inside `transition_memory_state`.
    pub(crate) async fn apply_memory_decision(
        state: &Arc<Mutex<CoordinatorState>>,
        ids: Vec<String>,
        approval_id: String,
        idempotency_key: String,
        operation: crate::memory_api::MemoryOperation,
        target: crate::memory_extraction::ConfirmationState,
        audit_event: &str,
    ) -> Result<Vec<u8>, String> {
        let journal = state.lock().await.journal.clone();
        let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
        crate::memory_api::Approval::new(approval_id.clone(), operation)
            .map_err(|error| error.to_string())?;
        validate_memory_idempotency_key(&idempotency_key)?;
        if ids.is_empty() {
            return Err("at least one memory id is required".to_string());
        }
        if ids.len() > MAX_MEMORY_BATCH {
            return Err(format!("batch is limited to {MAX_MEMORY_BATCH} memory ids"));
        }
        let mut results = Vec::with_capacity(ids.len());
        for id in &ids {
            // A contradictory decision on one id (rejecting an already
            // confirmed record, say) reports that id's real state instead of
            // aborting the rest of the batch.
            let actual = match journal.transition_memory_state(id, target.as_str()).await {
                Ok(state) => state,
                Err(error) => {
                    let current = journal
                        .get_memory(id)
                        .await
                        .ok()
                        .flatten()
                        .map(|record| record.extraction.confirmation_state);
                    match current {
                        Some(state) => state,
                        // No such record at all: that is a real failure.
                        None => return Err(error),
                    }
                }
            };
            results.push(serde_json::json!({
                "id": id,
                "state": actual,
                "applied": actual == target.as_str(),
            }));
            Self::record_audit(
                state,
                crate::audit::AuditKind::Approval,
                id.clone(),
                audit_event,
                [
                    ("memory_id".to_owned(), id.clone()),
                    ("state".to_owned(), actual),
                    ("approval_id".to_owned(), approval_id.clone()),
                    ("idempotency_key".to_owned(), idempotency_key.clone()),
                ],
            )
            .await;
        }
        serde_json::to_vec(&serde_json::json!({ "results": results }))
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn record_audit_for_event(state: &Arc<Mutex<CoordinatorState>>, event: &CoreEvent) {
        match event {
            CoreEvent::ApprovalRequired {
                task_id,
                approval_id,
                tool_name,
                permission,
                scope,
                ..
            } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::Approval,
                    task_id.to_string(),
                    "approval.required",
                    [
                        ("approval_id".to_owned(), approval_id.to_string()),
                        ("tool_name".to_owned(), tool_name.to_string()),
                        ("permission".to_owned(), permission.to_string()),
                        ("scope".to_owned(), scope.to_string()),
                    ],
                )
                .await;
            }
            CoreEvent::ToolStarted { task_id, tool_name } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::ToolCall,
                    task_id.to_string(),
                    "tool.started",
                    [("tool_name".to_owned(), tool_name.to_string())],
                )
                .await;
            }
            CoreEvent::TaskFailed { task_id, error } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::Failure,
                    task_id.to_string(),
                    "task.failed",
                    [("error".to_owned(), error.to_string())],
                )
                .await;
            }
            _ => {}
        }
    }

    /// Returns the current in-memory audit trail as JSONL, primarily for
    /// tests and diagnostics. The durable copy lives on disk at
    /// `<data_dir>/logs/audit.jsonl`.
    pub async fn audit_jsonl(&self) -> String {
        self.state.lock().await.audit.as_jsonl().unwrap_or_default()
    }

    /// Returns a snapshot of the current in-memory audit records, primarily
    /// for tests and diagnostics.
    pub async fn audit_records(&self) -> Vec<crate::audit::AuditRecord> {
        self.state.lock().await.audit.records().to_vec()
    }

    pub(crate) async fn handle_command(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
        match command {
            CoreCommand::StartTask {
                task_id,
                prompt,
                workspace_root,
                preferred_route_hint,
            } => {
                let cancellation = CancellationToken::new();
                let run_id = format!("agent-{}", uuid::Uuid::new_v4());
                let mut state_guard = state.lock().await;
                if state_guard
                    .tasks
                    .insert(
                        task_id.clone(),
                        ActiveTask {
                            cancellation: cancellation.clone(),
                        },
                    )
                    .is_some()
                {
                    return;
                }
                let _ = state_guard.events.send(CoreEvent::TaskStarted {
                    task_id: task_id.clone(),
                    prompt: prompt.clone(),
                });
                let events = state_guard.events.clone();
                let executor = state_guard.executor.clone();
                let journal = state_guard.journal.clone();
                let mut workspace_root =
                    workspace_root.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                if let Some(journal) = &state_guard.journal {
                    let database = journal.database().lock().await;
                    if let Ok(Some(binding)) =
                        evohime_local_storage::task_worktree_isolation_store::get_ready_for_task(
                            database.connection(),
                            &task_id,
                        )
                    {
                        let candidate = workspace_root.join(&binding.root_ref);
                        if candidate.is_dir() {
                            workspace_root = candidate;
                        }
                    }
                }
                if let Some(journal) = &state_guard.journal {
                    let database = journal.database().lock().await;
                    let _ = evohime_local_storage::continuation_store::attach_task_context(
                        database.connection(),
                        &task_id,
                        &prompt,
                        &workspace_root.to_string_lossy(),
                        crate::task_memory::now_millis() as i64,
                    );
                    if let Ok(Some(binding)) =
                        evohime_local_storage::workspace_sets_store::get_run_binding(
                            database.connection(),
                            &task_id,
                        )
                    {
                        if let Ok(binding) = serde_json::from_slice::<serde_json::Value>(&binding) {
                            write_model_trace(
                                "workspace_sets.run_binding_pinned",
                                serde_json::json!({
                                    "task_id": task_id,
                                    "set_id": binding.get("set_id"),
                                    "set_version": binding.get("set_version"),
                                    "set_hash": binding.get("set_hash"),
                                    "root_count": binding.get("roots").and_then(serde_json::Value::as_array).map_or(0, Vec::len),
                                    "pinned": binding.get("pinned")
                                }),
                            );
                        }
                    }
                }
                drop(state_guard);
                let Some(background_permit) = state
                    .lock()
                    .await
                    .background_tasks
                    .try_acquire()
                else {
                    let mut state_guard = state.lock().await;
                    state_guard.tasks.remove(&task_id);
                    let _ = state_guard.events.send(CoreEvent::TaskFailed {
                        task_id,
                        error: "background task capacity is exhausted".into(),
                    });
                    return;
                };
                tokio::spawn(async move {
                    let _background_permit = background_permit;
                    let intent_hash = crate::research::sha256_hex(prompt.as_bytes());
                    if let Some(journal) = &journal {
                        let checkpoint_runtime =
                            crate::task_checkpoint::TaskCheckpointRuntime::new(journal.clone());
                        match checkpoint_runtime.recover(&task_id, &workspace_root).await {
                            Ok(recovery)
                                if matches!(
                                    recovery.disposition,
                                    crate::task_checkpoint::RecoveryDisposition::Blocked
                                        | crate::task_checkpoint::RecoveryDisposition::Terminal
                                ) =>
                            {
                                let mut state_guard = state.lock().await;
                                state_guard.tasks.remove(&task_id);
                                let warning = recovery.warning.unwrap_or_else(|| {
                                    "checkpoint recovery requires explicit reconciliation".into()
                                });
                                let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                    task_id,
                                    error: warning,
                                });
                                return;
                            }
                            Err(error) => {
                                let mut state_guard = state.lock().await;
                                state_guard.tasks.remove(&task_id);
                                let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                    task_id,
                                    error: format!("task checkpoint recovery failed: {error}"),
                                });
                                return;
                            }
                            Ok(_) => {}
                        }
                        if let Err(error) = checkpoint_runtime
                            .capture(
                                &task_id,
                                &workspace_root,
                                crate::task_checkpoint::CheckpointStatus::InProgress,
                                crate::task_checkpoint::CheckpointCaptureReason::RunStarted,
                                None,
                            )
                            .await
                        {
                            let mut state_guard = state.lock().await;
                            state_guard.tasks.remove(&task_id);
                            let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: format!("task checkpoint could not be persisted: {error}"),
                            });
                            return;
                        }
                        if let Err(error) = journal
                            .begin_agent_run(&run_id, &task_id, &intent_hash)
                            .await
                        {
                            let mut state_guard = state.lock().await;
                            state_guard.tasks.remove(&task_id);
                            let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: format!(
                                    "agent run could not acquire durable lease: {error}"
                                ),
                            });
                            return;
                        }
                    }

                    let heartbeat_cancel = CancellationToken::new();
                    let heartbeat_failure = Arc::new(StdMutex::new(None::<String>));
                    let heartbeat_task = journal.as_ref().map(|journal| {
                        let journal = journal.clone();
                        let run_id = run_id.clone();
                        let failure = heartbeat_failure.clone();
                        let cancel = heartbeat_cancel.clone();
                        tokio::spawn(async move {
                            let mut interval = tokio::time::interval(Duration::from_secs(10));
                            loop {
                                tokio::select! {
                                    _ = cancel.cancelled() => break,
                                    _ = interval.tick() => {
                                        if let Err(error) = journal.heartbeat_agent_run(&run_id).await {
                                            *failure.lock().expect("heartbeat failure lock") = Some(error.to_string());
                                            break;
                                        }
                                    }
                                }
                            }
                        })
                    });
                    // A task is a loop of model calls and tool runs, so its
                    // budget must exceed one model call (120 s by default).
                    // The old 60 s cut off agents that were working fine.
                    let task_timeout_secs = std::env::var("EVOHIME_TASK_TIMEOUT_SECONDS")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(DEFAULT_TASK_TIMEOUT_SECONDS);
                    let continuation_context = if let Some(journal) = &journal {
                        let database = journal.database().lock().await;
                        let run = evohime_local_storage::continuation_store::get_run_by_task(
                            database.connection(),
                            &task_id,
                        )
                        .ok()
                        .flatten();
                        run.and_then(|run| {
                            serde_json::from_slice::<crate::continuation::ContinuationPolicyV1>(
                                &evohime_local_storage::continuation_store::get_policy(
                                    database.connection(),
                                    &run.policy_id,
                                    run.policy_revision,
                                    &run.owner_scope,
                                )
                                .ok()
                                .flatten()?
                                .canonical_json,
                            )
                            .ok()
                            .map(|policy| (run, policy))
                        })
                    } else {
                        None
                    };
                    let mut result = Err(AgentRunError::Internal(
                        "continuation did not execute an attempt".into(),
                    ));
                    let mut continuation_index = continuation_context
                        .as_ref()
                        .map(|(run, _)| run.continuation_index)
                        .unwrap_or(0);
                    loop {
                        let attempt = if let Some((run, policy)) = &continuation_context {
                            if policy.budget.max_wall_clock_ms.is_some_and(|limit| {
                                crate::task_memory::now_millis()
                                    .saturating_sub(run.created_at_ms.max(0) as u64)
                                    >= limit
                            }) {
                                if let Some(journal) = &journal {
                                    if let Ok(database) = journal.database().try_lock() {
                                        let _ = evohime_local_storage::continuation_store::transition_run(
                                            database.connection(),
                                            &run.run_id,
                                            "running",
                                            "budget_limited",
                                            Some("max_wall_clock_ms"),
                                            crate::task_memory::now_millis() as i64,
                                        );
                                    }
                                }
                                break;
                            }
                            let fingerprint =
                                format!("{}:{}", task_id, continuation_index.saturating_add(1));
                            let mut database = journal
                                .as_ref()
                                .expect("continuation has a journal")
                                .database()
                                .lock()
                                .await;
                            match evohime_local_storage::continuation_store::reserve_attempt(
                                database.connection_mut(),
                                &run.run_id,
                                "task",
                                &fingerprint,
                                0,
                                0,
                                crate::task_memory::now_millis() as i64,
                            ) {
                                Ok(true) => Some((run.run_id.clone(), continuation_index + 1)),
                                Ok(false) => None,
                                Err(_) => {
                                    let _ = state.lock().await.events.send(CoreEvent::TaskFailed {
                                        task_id: task_id.clone(),
                                        error:
                                            "continuation budget or state rejected the next attempt"
                                                .into(),
                                    });
                                    break;
                                }
                            }
                        } else {
                            None
                        };
                        result = match executor.as_ref() {
                            Some(executor) => match timeout(
                                Duration::from_secs(task_timeout_secs),
                                executor.execute_in_workspace_with_routing_hint(
                                    task_id.clone(),
                                    prompt.clone(),
                                    workspace_root.clone(),
                                    preferred_route_hint.clone(),
                                    cancellation.clone(),
                                    events.clone(),
                                ),
                            )
                            .await
                            {
                                Ok(result) => result,
                                Err(_) => Err(AgentRunError::Timeout(task_timeout_secs)),
                            },
                            None => {
                                cancellation.cancelled().await;
                                Err(AgentRunError::Cancelled)
                            }
                        };
                        let Some((run_id, attempt_index)) = attempt else {
                            break;
                        };
                        continuation_index = continuation_index.saturating_add(1);
                        let success = result.is_ok();
                        let mut required_gates_passed = true;
                        let mut pending_approval = false;
                        let mut pending_approval_id: Option<String> = None;
                        let mut gate_unknown = false;
                        let mut gate_non_retryable = false;
                        if success {
                            if let Some((_, policy)) = &continuation_context {
                                for gate in &policy.gates {
                                    let outcome = match executor.as_ref() {
                                        Some(executor) => {
                                            executor
                                                .execute_continuation_gate(
                                                    gate.clone(),
                                                    task_id.clone(),
                                                    workspace_root.clone(),
                                                    cancellation.clone(),
                                                )
                                                .await
                                        }
                                        None => crate::continuation::GateOutcome::Unavailable {
                                            code: "gate_executor_unavailable".into(),
                                        },
                                    };
                                    if let Some((run, _)) = &continuation_context {
                                        let (status, evidence_ref, error_code) = match &outcome {
                                            crate::continuation::GateOutcome::Passed {
                                                evidence_ref,
                                            } => ("passed", Some(evidence_ref.clone()), None),
                                            crate::continuation::GateOutcome::PendingApproval {
                                                ..
                                            } => (
                                                "pending_approval",
                                                None,
                                                Some("approval_required".into()),
                                            ),
                                            crate::continuation::GateOutcome::Failed {
                                                code,
                                                ..
                                            } => ("failed", None, Some(code.clone())),
                                            crate::continuation::GateOutcome::Unavailable {
                                                code,
                                            } => ("unavailable", None, Some(code.clone())),
                                        };
                                        let database = journal
                                            .as_ref()
                                            .expect("continuation has a journal")
                                            .database()
                                            .lock()
                                            .await;
                                        let _ = evohime_local_storage::continuation_store::record_gate_result(
                                            database.connection(),
                                            &evohime_local_storage::continuation_store::GateResultRecord {
                                                run_id: run.run_id.clone(),
                                                gate_id: gate.id.clone(),
                                                attempt_index,
                                                status: status.into(),
                                                evidence_ref,
                                                error_code,
                                                created_at_ms: crate::task_memory::now_millis() as i64,
                                            },
                                        );
                                    }
                                    match outcome {
                                        crate::continuation::GateOutcome::Passed { .. } => {}
                                        crate::continuation::GateOutcome::PendingApproval {
                                            approval_id,
                                        } => {
                                            required_gates_passed = false;
                                            pending_approval = true;
                                            pending_approval_id = Some(approval_id);
                                            break;
                                        }
                                        crate::continuation::GateOutcome::Failed {
                                            retryable,
                                            ..
                                        } => {
                                            required_gates_passed = false;
                                            gate_non_retryable |= !retryable;
                                            gate_unknown |= retryable;
                                            break;
                                        }
                                        crate::continuation::GateOutcome::Unavailable {
                                            ..
                                        } => {
                                            required_gates_passed = false;
                                            gate_unknown = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        } else {
                            required_gates_passed = false;
                        }
                        let decision = if let Some((run, policy)) = &continuation_context {
                            let database = journal
                                .as_ref()
                                .expect("continuation has a journal")
                                .database()
                                .lock()
                                .await;
                            let result_json = serde_json::to_vec(&serde_json::json!({
                                "success": success,
                                "error": result.as_ref().err().map(ToString::to_string)
                            }))
                            .unwrap_or_default();
                            let _ = evohime_local_storage::continuation_store::finish_attempt(
                                database.connection(),
                                &run_id,
                                attempt_index,
                                if success { "completed" } else { "failed" },
                                &result_json,
                                crate::task_memory::now_millis() as i64,
                            );
                            let goal_criteria_complete =
                                policy.linked_goal_id.as_ref().is_none_or(|goal_id| {
                                    evohime_local_storage::goal::GoalStore::new(
                                        database.connection(),
                                    )
                                    .get(goal_id)
                                    .ok()
                                    .flatten()
                                    .is_some_and(|goal| {
                                        matches!(
                                            goal.status,
                                            evohime_local_storage::goal::GoalStatus::Completed
                                        ) && goal.remaining_criteria.is_empty()
                                    })
                                });
                            let decision = crate::continuation::decide(
                                &crate::continuation::DecisionEvidence {
                                    required_gates_passed,
                                    goal_criteria_complete,
                                    pending_approval,
                                    unknown_outcome: result.is_err() || gate_unknown,
                                    non_retryable_failure: result.is_err() || gate_non_retryable,
                                    continuation_index: continuation_index as u32,
                                    max_continuations: run.max_continuations as u32,
                                    model_turns: continuation_index as u32,
                                    max_model_turns: run.max_model_turns as u32,
                                    ..Default::default()
                                },
                            );
                            let next_state = match decision {
                                crate::continuation::Decision::Complete => "completed",
                                crate::continuation::Decision::BudgetLimited => "budget_limited",
                                crate::continuation::Decision::StopFailed => "failed",
                                crate::continuation::Decision::StopUser => "stopped",
                                crate::continuation::Decision::PauseForApproval => {
                                    "waiting_approval"
                                }
                                crate::continuation::Decision::Blocked => "blocked",
                                crate::continuation::Decision::Continue => "running",
                            };
                            if let Some(approval_id) = pending_approval_id {
                                let _ = events.send(CoreEvent::ApprovalRequired {
                                    task_id: task_id.clone(),
                                    approval_id,
                                    tool_name: "continuation_gate".into(),
                                    permission: "continuation_gate".into(),
                                    scope: workspace_root
                                        .to_string_lossy()
                                        .chars()
                                        .take(256)
                                        .collect(),
                                    preview: evohime_permissions::ApprovalPreview {
                                        kind: "continuation_gate".into(),
                                        summary: "Continuation gate requires user approval".into(),
                                        command: None,
                                        cwd: None,
                                        path: None,
                                        details: None,
                                        truncated: false,
                                    },
                                });
                            }
                            if next_state != "running" {
                                let _ = evohime_local_storage::continuation_store::transition_run(
                                    database.connection(),
                                    &run.run_id,
                                    "running",
                                    next_state,
                                    Some(&format!("continuation_{next_state}")),
                                    crate::task_memory::now_millis() as i64,
                                );
                            }
                            decision
                        } else {
                            crate::continuation::Decision::Complete
                        };
                        if !matches!(decision, crate::continuation::Decision::Continue) {
                            break;
                        }
                    }
                    heartbeat_cancel.cancel();
                    if let Some(heartbeat_task) = heartbeat_task {
                        let _ = heartbeat_task.await;
                    }
                    let heartbeat_error = heartbeat_failure
                        .lock()
                        .expect("heartbeat failure lock")
                        .clone();
                    if let Some(journal) = &journal {
                        let checkpoint_status = if heartbeat_error.is_some() {
                            crate::task_checkpoint::CheckpointStatus::Conflicted
                        } else if result.is_ok() {
                            crate::task_checkpoint::CheckpointStatus::Completed
                        } else if matches!(&result, Err(AgentRunError::Cancelled)) {
                            crate::task_checkpoint::CheckpointStatus::Paused
                        } else {
                            crate::task_checkpoint::CheckpointStatus::Failed
                        };
                        let reason = match checkpoint_status {
                            crate::task_checkpoint::CheckpointStatus::Completed => {
                                crate::task_checkpoint::CheckpointCaptureReason::Completed
                            }
                            crate::task_checkpoint::CheckpointStatus::Paused => {
                                crate::task_checkpoint::CheckpointCaptureReason::Paused
                            }
                            _ => crate::task_checkpoint::CheckpointCaptureReason::Failed,
                        };
                        let checkpoint_runtime =
                            crate::task_checkpoint::TaskCheckpointRuntime::new(journal.clone());
                        if let Err(error) = checkpoint_runtime
                            .capture(&task_id, &workspace_root, checkpoint_status, reason, None)
                            .await
                        {
                            if result.is_ok() {
                                result = Err(AgentRunError::Internal(format!(
                                    "task checkpoint could not be persisted: {error}"
                                )));
                            }
                        }
                        if heartbeat_error.is_none() || result.is_err() {
                            let _ = journal.complete_agent_run(&run_id, result.is_ok()).await;
                        }
                    }
                    let mut state_guard = state.lock().await;
                    state_guard.tasks.remove(&task_id);
                    if let Err(error) = &result {
                        let _ = state_guard.events.send(CoreEvent::RoutingTrace {
                            task_id: task_id.clone(),
                            trace: routing_failure_trace(&run_id, error),
                        });
                    }
                    match (result, heartbeat_error) {
                        (Ok(_), Some(error)) => {
                            let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: format!(
                                    "agent run lease was lost; outcome requires reconciliation: {error}"
                                ),
                            });
                        }
                        (Ok(_), None) => {}
                        (Err(error), _) => {
                            let task_id = task_id;
                            if matches!(error, AgentRunError::Cancelled) {
                                let _ = state_guard.events.send(CoreEvent::TaskStopped { task_id });
                            } else {
                                let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                    task_id,
                                    error: error.to_string(),
                                });
                            }
                        }
                    }
                });
            }
            CoreCommand::ResolveRoutingDecision {
                trace_id,
                approve,
                reply,
            } => {
                let approvals = state.lock().await.routing_approvals.clone();
                match approvals.resolve(&trace_id, approve).await {
                    Ok(_) => {
                        state
                            .lock()
                            .await
                            .routing_decisions
                            .insert(trace_id, approve);
                        let _ = reply.send(Ok(serde_json::json!({"accepted": true})
                            .to_string()
                            .into_bytes()));
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            CoreCommand::ExtractAmbientMemory { episode_id } => {
                let executor = state.lock().await.executor.clone();
                let Some(executor) = executor else {
                    return;
                };
                // Извлечение не держит очередь команд: эпизод уже закрыт, и
                // ждать его разбора некому.
                let Some(background_permit) = state
                    .lock()
                    .await
                    .background_tasks
                    .try_acquire()
                else {
                    return;
                };
                tokio::spawn(async move {
                    let _background_permit = background_permit;
                    executor.extract_ambient_memory(episode_id).await;
                });
            }
            CoreCommand::StopTask { task_id } => {
                let mut state_guard = state.lock().await;
                if let Some(active) = state_guard.tasks.remove(&task_id) {
                    active.cancellation.cancel();
                }
            }
            CoreCommand::CreateProject {
                client_id,
                request_id,
                command_hash,
                project_id,
                title,
                workspace_path,
                source_ref,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    let project = journal
                        .create_project(&project_id, &title, &workspace_path, source_ref.as_deref())
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = serde_json::to_vec(&serde_json::json!({
                        "project_id": project.id,
                        "title": project.title,
                        "workspace_path": project.workspace_path,
                        "version": project.version,
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::CreateTask {
                client_id,
                request_id,
                command_hash,
                item,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    let created = journal
                        .create_work_item(&item)
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = serde_json::to_vec(&serde_json::json!({
                        "task_id": created.id,
                        "project_id": created.project_id,
                        "status": created.status,
                        "version": created.version,
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::UpdateTaskStatus {
                client_id,
                request_id,
                command_hash,
                task_id,
                expected_version,
                status,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    let updated = journal
                        .update_work_item_status(&task_id, expected_version, &status)
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = serde_json::to_vec(&serde_json::json!({
                        "task_id": updated.id,
                        "status": updated.status,
                        "version": updated.version,
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::AddTaskEdge {
                client_id,
                request_id,
                command_hash,
                from_task_id,
                to_task_id,
                kind,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    journal
                        .add_dependency(&from_task_id, &to_task_id, &kind)
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = br#"{"from_task_id":"ok"}"#.to_vec();
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskGraph { project_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let (tasks, edges) = journal
                        .list_task_graph(&project_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "project_id": project_id,
                        "tasks": tasks,
                        "edges": edges.into_iter().map(|(from, to, kind)| serde_json::json!({
                            "from_task_id": from,
                            "to_task_id": to,
                            "kind": kind,
                        })).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::NextReadyTask { project_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let task = journal
                        .next_ready_task(&project_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "project_id": project_id,
                        "task": task,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ImportPrd {
                client_id,
                request_id,
                command_hash,
                import_id,
                project_id,
                origin,
                version,
                source_text,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    let parsed = crate::prd::parse_markdown_prd(&source_text, &origin, &version);
                    if !parsed.diagnostics.is_empty() {
                        let diagnostics = serde_json::to_string(&parsed.diagnostics)
                            .map_err(|error| error.to_string())?;
                        return Err(format!("PRD contains diagnostics: {diagnostics}"));
                    }
                    let document = parsed.document.ok_or_else(|| "PRD is empty".to_string())?;
                    let tasks = document
                        .tasks
                        .iter()
                        .enumerate()
                        .map(|(index, task)| ImportedTask {
                            id: format!("{project_id}:{import_id}:{index}"),
                            title: task.title.clone(),
                            description: task.description.clone(),
                            source_ref: task.source_ref.clone(),
                            acceptance_criteria: task.acceptance_criteria.join("\n"),
                        })
                        .collect::<Vec<_>>();
                    let imported = journal
                        .import_prd(
                            &import_id,
                            &project_id,
                            &origin,
                            &version,
                            &source_text,
                            &tasks,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = serde_json::to_vec(&serde_json::json!({
                        "import_id": import_id,
                        "project_id": project_id,
                        "task_ids": imported.into_iter().map(|task| task.id).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskHistory {
                task_id,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let events = journal
                        .task_history(&task_id, limit.min(100))
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "task_id": task_id,
                        "events": events.into_iter().map(|event| serde_json::json!({
                            "sequence_id": event.sequence_id,
                            "event_type": event.event_type,
                            "created_at": event.created_at,
                            "payload": match serde_json::from_slice::<serde_json::Value>(&event.payload) {
                                Ok(value) => value,
                                Err(error) => {
                                    tracing::debug!(%error, "event payload is not JSON; exposing raw bytes");
                                    serde_json::json!({"raw_bytes": event.payload})
                                }
                            },
                        })).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskContext {
                project_id,
                task_id,
                max_chars,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal
                        .get_project(&project_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let task = journal
                        .get_work_item(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "task not found".to_string())?;
                    if task.project_id != project_id {
                        return Err("task does not belong to project".to_string());
                    }
                    let manifest = crate::workspace::build_manifest(
                        &project.workspace_path,
                        500,
                        2 * 1024 * 1024,
                    )
                    .map_err(|error| error.to_string())?;
                    let references = manifest
                        .entries
                        .iter()
                        .map(|entry| entry.relative_path.clone())
                        .collect::<Vec<_>>();
                    let context = crate::workspace::assemble_context(
                        crate::workspace::ContextInput {
                            title: &task.title,
                            description: &task.description,
                            acceptance_criteria: &task.acceptance_criteria,
                            non_goals: &task.non_goals,
                            references: &references,
                            skill_context: &[],
                        },
                        max_chars.min(32 * 1024),
                    );
                    serde_json::to_vec(&serde_json::json!({
                        "project_id": project_id,
                        "task_id": task_id,
                        "workspace_hash": manifest.workspace_hash,
                        "manifest": manifest,
                        "context": context,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskPlanSpec {
                project_id,
                task_id,
                max_chars,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let task = journal
                        .get_work_item(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "task not found".to_string())?;
                    if task.project_id != project_id {
                        return Err("task does not belong to project".to_string());
                    }
                    let plan = crate::plan::build_task_plan_spec(
                        &task.title,
                        &task.description,
                        &task.acceptance_criteria,
                        &task.non_goals,
                        "offline context; research не выполняется",
                        max_chars.min(32 * 1024),
                    );
                    serde_json::to_vec(&plan).map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::PlanArtifact {
                operation,
                artifact_json,
                artifact_id,
                expected_version,
                status,
                policy_snapshot_hash,
                task_id,
                workflow_run_id,
                correlation_id,
                idempotency_key,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let runtime = crate::plan_artifact::PlanArtifactRuntime::new(journal);
                    let now = crate::task_memory::now_millis() as i64;
                    match operation.as_str() {
                        "create" => {
                            let artifact: crate::plan_artifact::PlanArtifactV1 =
                                serde_json::from_slice(&artifact_json)
                                    .map_err(|e| e.to_string())?;
                            serde_json::to_vec(
                                &runtime
                                    .create(&artifact, &idempotency_key, now)
                                    .await
                                    .map_err(|e| e.to_string())?,
                            )
                            .map_err(|e| e.to_string())
                        }
                        "read" => serde_json::to_vec(
                            &runtime.get(&artifact_id).await.map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string()),
                        "execute" => serde_json::to_vec(
                            &runtime
                                .execute(crate::plan_artifact::ExecutePlanArtifact {
                                    artifact_id: &artifact_id,
                                    expected_version,
                                    policy_snapshot_hash: &policy_snapshot_hash,
                                    task_id: task_id.as_deref(),
                                    workflow_run_id: workflow_run_id.as_deref(),
                                    correlation_id: &correlation_id,
                                    idempotency_key: &idempotency_key,
                                    now_ms: now,
                                })
                                .await
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string()),
                        "transition" => {
                            let next = crate::plan_artifact::PlanArtifactStatus::parse(&status)
                                .ok_or_else(|| "invalid plan artifact status".to_string())?;
                            serde_json::to_vec(
                                &runtime
                                    .transition(
                                        &artifact_id,
                                        expected_version,
                                        next,
                                        &idempotency_key,
                                        now,
                                    )
                                    .await
                                    .map_err(|e| e.to_string())?,
                            )
                            .map_err(|e| e.to_string())
                        }
                        _ => Err("invalid plan artifact operation".into()),
                    }
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::WorkspaceStateCheckpoint {
                operation,
                project_id,
                task_id,
                checkpoint_id,
                payload: _payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let root = journal.get_project(&project_id).await.map_err(|e| e.to_string())?
                        .or(journal.get_project_by_workspace_path(&project_id).await.map_err(|e| e.to_string())?)
                        .map(|project| std::path::PathBuf::from(project.workspace_path))
                        .unwrap_or_else(|| std::path::PathBuf::from(&project_id));
                    if !root.is_dir() {
                        return Err("project workspace not found".to_string());
                    }
                    let workspace_id = crate::task_memory::workspace_scope_id(&root);
                    let now = crate::task_memory::now_millis() as i64;
                    match operation.as_str() {
                        "list" => {
                            let summaries = {
                                let database = journal.database().lock().await;
                                evohime_local_storage::workspace_state_checkpoint::list_checkpoint_summaries(
                                    database.connection(), &workspace_id)
                                    .map_err(|e| e.to_string())?
                            };
                            serde_json::to_vec(&serde_json::json!({
                                "schema_version": 1,
                                "operation": "list",
                                "project_id": project_id,
                                "state": "listed",
                                "checkpoints": summaries,
                            })).map_err(|e| e.to_string())
                        }
                        "create" => {
                            let id = checkpoint_id.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
                            let checkpoint = match crate::workspace_state_checkpoints::capture(
                                &root, id.clone(), workspace_id.clone(), task_id.clone()) {
                                Ok(checkpoint) => checkpoint,
                                Err(error) => {
                                    let error_text = error.to_string();
                                    let error_code = if error_text.contains("file_bytes") {
                                        "workspace_checkpoint_file_too_large"
                                    } else if error_text.contains("snapshot_bytes") {
                                        "workspace_checkpoint_snapshot_too_large"
                                    } else if error_text.contains("files") {
                                        "workspace_checkpoint_too_many_files"
                                    } else {
                                        "workspace_checkpoint_capture_failed"
                                    };
                                    return serde_json::to_vec(&serde_json::json!({
                                        "schema_version": 1,
                                        "operation": "create",
                                        "checkpoint_id": id,
                                        "project_id": project_id,
                                        "task_id": task_id,
                                        "state": "failed",
                                        "error_code": error_code,
                                        "message": error_text,
                                    })).map_err(|e| e.to_string());
                                }
                            };
                            let json = serde_json::to_vec(&checkpoint).map_err(|e| e.to_string())?;
                            let record = evohime_local_storage::workspace_state_checkpoint::WorkspaceCheckpointRecord {
                                checkpoint_id: id.clone(), workspace_id: workspace_id.clone(), task_id: task_id.clone(),
                                snapshot_hash: checkpoint.baseline_hash.clone(), manifest_json: json, created_at_ms: now, pinned: false,
                            };
                            let database = journal.database().lock().await;
                            evohime_local_storage::workspace_state_checkpoint::insert_checkpoint(database.connection(), &record)
                                .map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"operation":"create","checkpoint_id":id,"project_id":project_id,"task_id":task_id,"state":"completed","file_count":checkpoint.files.len(),"snapshot_hash":checkpoint.baseline_hash})).map_err(|e| e.to_string())
                        }
                        "compare" | "restore" | "restore_both" | "restore_task" => {
                            let id = checkpoint_id.ok_or_else(|| "checkpoint_id is required".to_string())?;
                            let database = journal.database().lock().await;
                            let record = evohime_local_storage::workspace_state_checkpoint::get_checkpoint(database.connection(), &id)
                                .map_err(|e| e.to_string())?.ok_or_else(|| "checkpoint not found".to_string())?;
                            if record.workspace_id != workspace_id {
                                return Err("checkpoint does not belong to workspace".to_string());
                            }
                            if let Some(expected_task) = record.task_id.as_deref() {
                                if task_id.as_deref() != Some(expected_task) && operation != "restore" {
                                    return Err("checkpoint does not belong to task".to_string());
                                }
                            }
                            let checkpoint: crate::workspace_state_checkpoints::WorkspaceStateCheckpoint = serde_json::from_slice(&record.manifest_json).map_err(|e| e.to_string())?;
                            drop(database);
                            let conflicts = crate::workspace_state_checkpoints::compare(&root, &checkpoint).map_err(|e| e.to_string())?;
                            if operation == "compare" || operation == "restore_task" {
                                return serde_json::to_vec(&serde_json::json!({"schema_version":1,"operation":operation,"checkpoint_id":id,"project_id":project_id,"task_id":task_id,"state":if operation == "restore_task" { "task_projection_restored" } else { "compared" },"conflict_count":conflicts.len()})).map_err(|e| e.to_string());
                            }
                            if !conflicts.is_empty() {
                                let database = journal.database().lock().await;
                                let detail = serde_json::to_vec(&serde_json::json!({"conflict_count": conflicts.len()})).unwrap_or_default();
                                let operation_id = format!("{}:conflict", if idempotency_key.is_empty() { uuid::Uuid::now_v7().to_string() } else { idempotency_key.clone() });
                                let _ = evohime_local_storage::workspace_state_checkpoint::append_restore_journal(database.connection(), &evohime_local_storage::workspace_state_checkpoint::RestoreJournalRecord { operation_id, checkpoint_id: id.clone(), operation: operation.clone(), state: "conflict".into(), detail_json: detail, created_at_ms: now });
                                let response = match serde_json::to_string(&serde_json::json!({"error_code":"workspace_conflict","conflict_count":conflicts.len()})) {
                                    Ok(value) => value,
                                    Err(error) => {
                                        tracing::warn!(%error, "failed to serialize workspace conflict response");
                                        "workspace conflict".into()
                                    }
                                };
                                return Err(response);
                            }
                            crate::workspace_state_checkpoints::restore(&root, &checkpoint).map_err(|e| e.to_string())?;
                            let database = journal.database().lock().await;
                            let detail = serde_json::to_vec(&serde_json::json!({"expected_version": expected_version})).unwrap_or_default();
                            let operation_id = format!("{}:completed", if idempotency_key.is_empty() { uuid::Uuid::now_v7().to_string() } else { idempotency_key.clone() });
                            evohime_local_storage::workspace_state_checkpoint::append_restore_journal(database.connection(), &evohime_local_storage::workspace_state_checkpoint::RestoreJournalRecord { operation_id, checkpoint_id: id.clone(), operation: operation.clone(), state: "completed".into(), detail_json: detail, created_at_ms: now }).map_err(|e| e.to_string())?;
                            let state = if operation == "restore_both" { "workspace_and_task_projection_restored" } else { "workspace_restored" };
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"operation":operation,"checkpoint_id":id,"project_id":project_id,"task_id":task_id,"state":state,"conflict_count":0})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported workspace checkpoint operation".to_string()),
                    }
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::IncrementalChangeProtocol {
                operation,
                run_id,
                payload,
                expected_version,
                observed_fingerprint,
                idempotency_key,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let runtime = crate::incremental_change_protocol::Runtime::new(journal);
                    let now = crate::task_memory::now_millis() as i64;
                    let value = match operation.as_str() {
                        "create" => {
                            #[derive(Deserialize)]
                            struct Request { delta: crate::incremental_change_protocol::RequirementDelta, impact: crate::incremental_change_protocol::ImpactAnalysis, plan: crate::incremental_change_protocol::ChangePlan }
                            let request: Request = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            runtime.create(&run_id, &idempotency_key, &request.delta, &request.impact, &request.plan, now).await.map_err(|e| e.to_string())?
                        }
                        "apply" | "cancel" | "unknown" => {
                            let next = match operation.as_str() { "apply" => crate::incremental_change_protocol::State::Applied, "cancel" => crate::incremental_change_protocol::State::Cancelled, _ => crate::incremental_change_protocol::State::UnknownReconciliationRequired };
                            runtime.transition(&run_id, expected_version, next, &observed_fingerprint, now).await.map_err(|e| e.to_string())?
                        }
                        _ => return Err("unsupported incremental change operation".to_string()),
                    };
                    serde_json::to_vec(&value).map_err(|e| e.to_string())
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::RevisionSafeWorkspaceFiles {
                operation,
                project_id,
                logical_path,
                content: _,
                expected_hash: _,
                idempotency_key: _,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal.get_project(&project_id).await.map_err(|e| e.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let ctx = evohime_tool_runtime::ToolContext {
                        workspace_root: project.workspace_path.into(),
                        task_id: uuid::Uuid::nil(),
                        session_id: None,
                        progress_tx: None,
                    };
                    let value = match operation.as_str() {
                        "read" => {
                            let (file_ref, text) = evohime_tool_runtime::revision_safe_workspace_files::read(&ctx, &logical_path).await.map_err(|e| e.to_string())?;
                            serde_json::json!({"status":"ok","ref":file_ref,"preview":text.chars().take(20000).collect::<String>()})
                        }
                        "write" => return Err("filesystem mutations must use the approved tool boundary".to_string()),
                        _ => return Err("unsupported revision-safe workspace files operation".to_string()),
                    };
                    serde_json::to_vec(&value).map_err(|e| e.to_string())
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::TaskWorktreeIsolation {
                operation,
                project_id,
                task_id,
                worktree_id,
                branch,
                base_commit,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal.get_project(&project_id).await.map_err(|e| e.to_string())?.ok_or_else(|| "project not found".to_string())?;
                    if branch.is_empty() || branch.len() > task_worktree_isolation::MAX_BRANCH_BYTES || branch.starts_with('-') || branch.contains("..") || branch.contains(' ') { return Err("invalid worktree branch".to_string()); }
                    if !matches!(operation.as_str(), "ready" | "integrating" | "cleanup_pending") { return Err("unsupported worktree transition".to_string()); }
                    let connection = journal.database().lock().await;
                    if operation == "create" {
                        let record = evohime_local_storage::task_worktree_isolation_store::TaskWorktreeRecord { worktree_id: worktree_id.clone(), task_id, repository_scope: project.id, branch, root_ref: format!(".evohime/worktrees/{worktree_id}"), base_commit, state: "planned".into(), version: 1, idempotency_key, updated_at_ms: crate::task_memory::now_millis() as i64 };
                        evohime_local_storage::task_worktree_isolation_store::create(connection.connection(), &record).map_err(|e| e.to_string())?;
                        return serde_json::to_vec(&record).map_err(|e| e.to_string());
                    }
                    let current = evohime_local_storage::task_worktree_isolation_store::get(connection.connection(), &worktree_id).map_err(|e| e.to_string())?.ok_or_else(|| "worktree not found".to_string())?;
                    if operation == "ready" && !std::path::PathBuf::from(&project.workspace_path).join(&current.root_ref).is_dir() {
                        return Err("worktree root is not present; create it through the approved git.worktree.create tool".to_string());
                    }
                    let ok = evohime_local_storage::task_worktree_isolation_store::transition(connection.connection(), &worktree_id, expected_version, &operation, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                    if !ok { return Err("stale or unknown worktree transition".to_string()); }
                    let record = evohime_local_storage::task_worktree_isolation_store::get(connection.connection(), &worktree_id).map_err(|e| e.to_string())?.ok_or_else(|| "worktree not found".to_string())?;
                    serde_json::to_vec(&record).map_err(|e| e.to_string())
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::TeamResourceBudget {
                operation,
                owner_scope,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    match operation.as_str() {
                        "validate_policy" | "save_policy" => {
                            let policy: team_resource_budget::TeamBudgetPolicy = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            team_resource_budget::validate_hash(&policy).map_err(|e| e.to_string())?;
                            if operation == "save_policy" {
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let connection = journal.database().lock().await;
                                let json = serde_json::to_string(&policy).map_err(|e| e.to_string())?;
                                let inserted = evohime_local_storage::team_resource_budget_store::put_policy(connection.connection(), &owner_scope, policy.version, &json, &policy.content_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                                return serde_json::to_vec(&serde_json::json!({"status":if inserted { "saved" } else { "duplicate" },"policy_id":policy.id,"policy_version":policy.version,"content_hash":policy.content_hash})).map_err(|e| e.to_string());
                            }
                            serde_json::to_vec(&serde_json::json!({"status":"valid","policy_id":policy.id,"policy_version":policy.version,"content_hash":policy.content_hash})).map_err(|e| e.to_string())
                        }
                        "save_state" => {
                            let state_value: team_resource_budget::TeamBudgetState = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            if state_value.schema_version != team_resource_budget::SCHEMA_VERSION || state_value.team_session_id.is_empty() { return Err("invalid team budget state".to_string()); }
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let connection = journal.database().lock().await;
                            let json = serde_json::to_string(&state_value).map_err(|e| e.to_string())?;
                            let saved = evohime_local_storage::team_resource_budget_store::put_state(connection.connection(), &state_value.team_session_id, state_value.policy_version, &json, if expected_version == 0 { None } else { Some(expected_version) }, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            if !saved { return Err("duplicate or stale team budget state".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","team_session_id":state_value.team_session_id,"version":state_value.version.saturating_add(1)})).map_err(|e| e.to_string())
                        }
                        "record_usage" => {
                            let event: team_resource_budget::ResourceUsageEvent = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            if event.schema_version != team_resource_budget::SCHEMA_VERSION || event.id.is_empty() || event.team_session_id.is_empty() || event.run_id.is_empty() || event.operation_kind.is_empty() { return Err("invalid team resource usage event".to_string()); }
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let connection = journal.database().lock().await;
                            let json = serde_json::to_string(&event).map_err(|e| e.to_string())?;
                            let inserted = evohime_local_storage::team_resource_budget_store::append_usage(connection.connection(), &event.id, &event.team_session_id, &json, event.observed_at_ms).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":if inserted { "recorded" } else { "duplicate" },"usage_id":event.id,"uncertain":event.uncertain,"idempotency_key":idempotency_key})).map_err(|e| e.to_string())
                        }
                        "preflight" => {
                            let request: TeamBudgetPreflightRequest = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            let decision = team_resource_budget::preflight_charge(&request.state, &request.policy, &request.estimate, request.reserve_access, request.unknown_cost).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":format!("{decision:?}").to_lowercase()})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported team resource budget operation".to_string()),
                    }
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::ComposableTerminationConditions {
                operation,
                owner_scope,
                payload,
                expected_version,
                idempotency_key: _,
                reply,
            } => {
                let result = async {
                    match operation.as_str() {
                        "validate_policy" | "save_policy" => {
                            let policy: composable_termination_conditions::TerminationPolicy = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            composable_termination_conditions::validate_hash(&policy).map_err(|e| e.to_string())?;
                            if operation == "save_policy" {
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let connection = journal.database().lock().await;
                                let json = serde_json::to_string(&policy).map_err(|e| e.to_string())?;
                                let saved = evohime_local_storage::composable_termination_conditions_store::put_policy(connection.connection(), &owner_scope, policy.version, &json, &policy.content_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                                return serde_json::to_vec(&serde_json::json!({"status":if saved { "saved" } else { "duplicate" },"policy_id":policy.id,"content_hash":policy.content_hash})).map_err(|e| e.to_string());
                            }
                            serde_json::to_vec(&serde_json::json!({"status":"valid","policy_id":policy.id,"content_hash":policy.content_hash})).map_err(|e| e.to_string())
                        }
                        "evaluate" => {
                            let request: TerminationEvaluateRequest = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            let decision = composable_termination_conditions::evaluate_policy(&request.policy, &request.state, &request.event).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({
                                "status": "evaluated",
                                "decision": decision,
                                "hard_stop": request.policy.hard_stop,
                                "counters": {
                                    "messages": request.event.messages,
                                    "turns": request.event.turns,
                                    "tool_calls": request.event.tool_calls,
                                    "input_tokens": request.event.input_tokens,
                                    "output_tokens": request.event.output_tokens,
                                    "cost_micros": request.event.cost_micros,
                                    "elapsed_ms": request.event.elapsed_ms,
                                    "idle_ms": request.event.idle_ms,
                                },
                            })).map_err(|e| e.to_string())
                        }
                        "save_state" => {
                            let request: TerminationSaveStateRequest = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
                            let state_value = request.state;
                            let run_id = request.run_id.as_str();
                            let policy_id = request.policy_id.as_str();
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let connection = journal.database().lock().await;
                            let json = serde_json::to_string(&state_value).map_err(|e| e.to_string())?;
                            let saved = evohime_local_storage::composable_termination_conditions_store::put_state(connection.connection(), run_id, policy_id, &json, expected_version, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            if !saved { return Err("duplicate or stale termination state".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","run_id":run_id,"version":state_value.version.saturating_add(1)})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported termination operation".to_string()),
                    }
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::WorkspaceBootstrapManifest {
                operation,
                project_id,
                workspace_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_workspace_id = workspace_id.clone();
                let result = async {
                    if workspace_id.is_empty() || workspace_id.len() > crate::workspace_bootstrap_manifest::MAX_ID {
                        return Err("invalid workspace_id".to_string());
                    }
                    let manifest_payload = if operation == "discover" && payload.is_empty() {
                        let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                        let connection = journal.database().lock().await;
                        let project = connection.get_project(&project_id).map_err(|e| e.to_string())?
                            .ok_or_else(|| "project not found".to_string())?;
                        let root = std::path::PathBuf::from(project.workspace_path);
                        if crate::task_memory::workspace_scope_id(&root) != workspace_id {
                            return Err("workspace identity mismatch".to_string());
                        }
                        std::fs::read(root.join(".evohime").join("bootstrap.json")).map_err(|_| "bootstrap manifest not found".to_string())?
                    } else { payload };
                    let manifest: crate::workspace_bootstrap_manifest::WorkspaceBootstrapManifest =
                        serde_json::from_slice(&manifest_payload).map_err(|e| e.to_string())?;
                    if manifest.workspace_id != workspace_id {
                        return Err("workspace scope mismatch".to_string());
                    }
                    crate::workspace_bootstrap_manifest::validate_manifest(&manifest)
                        .map_err(|e| e.to_string())?;
                    match operation.as_str() {
                        "validate" | "discover" | "save" | "approve" | "run" => {
                            if operation == "save" {
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let connection = journal.database().lock().await;
                                let json = serde_json::to_string(&manifest).map_err(|e| e.to_string())?;
                                let saved = evohime_local_storage::workspace_bootstrap_manifest_store::put_manifest(
                                    connection.connection(),
                                    (&manifest.id, &manifest.workspace_id, manifest.revision, &manifest.content_hash, &json, "policy-v1", crate::task_memory::now_millis() as i64),
                                ).map_err(|e| e.to_string())?;
                                return serde_json::to_vec(&serde_json::json!({
                                    "status": if saved { "saved" } else { "duplicate" },
                                    "manifest_id": manifest.id,
                                    "revision": manifest.revision,
                                    "content_hash": manifest.content_hash,
                                })).map_err(|e| e.to_string());
                            }
                            if operation == "run" {
                                if idempotency_key.is_empty() {
                                    return Err("idempotency key required".to_string());
                                }
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                {
                                    let connection = journal.database().lock().await;
                                    let trust = evohime_local_storage::workspace_bootstrap_manifest_store::manifest_trust(
                                        connection.connection(), &manifest.id, manifest.revision,
                                    ).map_err(|e| e.to_string())?;
                                    if !matches!(trust, Some((status, hash)) if status == "trusted" && hash == manifest.content_hash) {
                                        return Err("trust_required".to_string());
                                    }
                                }
                                let root = {
                                    let connection = journal.database().lock().await;
                                    let project = connection.get_project(&project_id).map_err(|e| e.to_string())?
                                        .ok_or_else(|| "project not found".to_string())?;
                                    let root = std::path::PathBuf::from(project.workspace_path);
                                    if crate::task_memory::workspace_scope_id(&root) != manifest.workspace_id {
                                        return Err("workspace identity mismatch".to_string());
                                    }
                                    root
                                };
                                let now_ms = crate::task_memory::now_millis() as i64;
                                let lease_id = uuid::Uuid::new_v4().to_string();
                                let reserved = {
                                    let connection = journal.database().lock().await;
                                    let _ = evohime_local_storage::workspace_bootstrap_manifest_store::fence_expired_preparations(
                                        connection.connection(), now_ms.saturating_sub(30 * 60 * 1000),
                                    ).map_err(|e| e.to_string())?;
                                    evohime_local_storage::workspace_bootstrap_manifest_store::reserve_preparation(
                                        connection.connection(), &manifest.workspace_id, &manifest.id,
                                        &manifest.content_hash, &manifest.content_hash, &lease_id,
                                        now_ms,
                                    ).map_err(|e| e.to_string())?
                                };
                                if !reserved {
                                    let connection = journal.database().lock().await;
                                    if let Some((_, status, version)) = evohime_local_storage::workspace_bootstrap_manifest_store::get_preparation(
                                        connection.connection(), &manifest.workspace_id, &manifest.id,
                                        &manifest.content_hash, &manifest.content_hash,
                                    ).map_err(|e| e.to_string())? {
                                        if expected_version != 0 && expected_version != version as u64 {
                                            return Err("version_conflict".to_string());
                                        }
                                        if status == "prepared" {
                                            return serde_json::to_vec(&serde_json::json!({"status": status, "manifest_id": manifest.id, "content_hash": manifest.content_hash, "idempotent": true})).map_err(|e| e.to_string());
                                        }
                                    }
                                    return Err("already_running_or_prepared".to_string());
                                }
                                let run = crate::workspace_bootstrap_manifest::run_bounded(&root, &manifest).await;
                                let (status, result_json, error) = match run {
                                    Ok(results) => ("prepared", Some(serde_json::to_string(&results).map_err(|e| e.to_string())?), None),
                                    Err(e) => (if matches!(e, crate::workspace_bootstrap_manifest::BootstrapManifestError::TimedOut) { "unknown_outcome" } else { "failed" }, None, Some(e.to_string())),
                                };
                                let connection = journal.database().lock().await;
                                evohime_local_storage::workspace_bootstrap_manifest_store::complete_preparation(
                                    connection.connection(), &manifest.workspace_id, &manifest.id, &lease_id,
                                    status, result_json.as_deref(), crate::task_memory::now_millis() as i64,
                                ).map_err(|e| e.to_string())?;
                                if let Some(error) = error { return Err(error); }
                                return serde_json::to_vec(&serde_json::json!({"status": status, "manifest_id": manifest.id, "content_hash": manifest.content_hash})).map_err(|e| e.to_string());
                            }
                            if operation == "approve" {
                                let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                                let connection = journal.database().lock().await;
                                let approved = evohime_local_storage::workspace_bootstrap_manifest_store::approve_manifest(
                                    connection.connection(), &manifest.id, manifest.revision, &manifest.content_hash, "restricted-process-v1",
                                ).map_err(|e| e.to_string())?;
                                return serde_json::to_vec(&serde_json::json!({"status": if approved { "trusted" } else { "trust_unchanged" }, "manifest_id": manifest.id, "content_hash": manifest.content_hash})).map_err(|e| e.to_string());
                            }
                            serde_json::to_vec(&serde_json::json!({
                            "status": if operation == "discover" { "pending_review" } else { "valid" },
                            "manifest_id": manifest.id,
                            "revision": manifest.revision,
                            "content_hash": manifest.content_hash,
                        })).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported workspace bootstrap operation".to_string()),
                    }
                }.await;
                let event_payload = result
                    .as_ref()
                    .ok()
                    .and_then(|payload| serde_json::from_slice::<serde_json::Value>(payload).ok());
                let event = CoreEvent::WorkspaceBootstrapManifest {
                    workspace_id: event_workspace_id,
                    operation: event_operation,
                    status: event_payload
                        .as_ref()
                        .and_then(|value| value.get("status"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("failed")
                        .to_owned(),
                    manifest_id: event_payload
                        .as_ref()
                        .and_then(|value| value.get("manifest_id"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    revision: event_payload
                        .as_ref()
                        .and_then(|value| value.get("revision"))
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                    content_hash: event_payload
                        .as_ref()
                        .and_then(|value| value.get("content_hash"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    projection_json: event_payload
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "{}".into()),
                };
                let (journal, events) = {
                    let guard = state.lock().await;
                    (guard.journal.clone(), guard.events.clone())
                };
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::TeamCoordinationPolicies {
                operation,
                team_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_team_id = team_id.clone();
                let result = async {
                    if team_id.is_empty() || team_id.len() > crate::team_coordination_policies::MAX_TEXT || idempotency_key.is_empty() {
                        return Err("invalid coordination request".to_string());
                    }
                    let request: TeamCoordinationRequest = serde_json::from_slice(&payload).map_err(|_| "invalid coordination payload".to_string())?;
                    let spec = request.team;
                    if spec.id != team_id { return Err("team identity mismatch".to_string()); }
                    crate::team_coordination_policies::validate_team(&spec).map_err(|e| e.to_string())?;
                    match operation.as_str() {
                        "validate_policy" => serde_json::to_vec(&serde_json::json!({"status":"valid","team_id":team_id,"revision":spec.revision,"content_hash":crate::team_coordination_policies::canonical_hash(&spec).map_err(|e| e.to_string())?})).map_err(|e| e.to_string()),
                        "select" => {
                            let state = request.state.as_ref().ok_or_else(|| "state required".to_string())?;
                            let (next, decision) = crate::team_coordination_policies::select_next(&spec, state, request.handoff_from.as_deref(), request.selector_role.as_deref(), request.event_type.as_deref(), &request.event_ids).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"selected","team_id":team_id,"version":expected_version.saturating_add(1),"state":next,"decision":decision})).map_err(|e| e.to_string())
                        }
                        "save_state" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let state_value = request.state.ok_or_else(|| "state required".to_string())?;
                            let json = serde_json::to_vec(&state_value).map_err(|e| e.to_string())?;
                            let connection = journal.database().lock().await;
                            let saved = evohime_local_storage::team_coordination_policies_store::save_state(connection.connection(), &team_id, spec.revision, &json, expected_version, &idempotency_key, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            if !saved { return Err("version_conflict_or_duplicate".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","team_id":team_id,"version":expected_version.saturating_add(1)})).map_err(|e| e.to_string())
                        }
                        "save_policy" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let json = serde_json::to_vec(&spec).map_err(|e| e.to_string())?;
                            let hash = crate::team_coordination_policies::canonical_hash(&spec).map_err(|e| e.to_string())?;
                            let connection = journal.database().lock().await;
                            let saved = evohime_local_storage::team_coordination_policies_store::save_policy(connection.connection(), &team_id, spec.revision, &json, &hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":if saved {"saved"} else {"duplicate"},"team_id":team_id,"revision":spec.revision,"content_hash":hash})).map_err(|e| e.to_string())
                        }
                        "select_strategy" => {
                            let strategy = request.strategy.ok_or_else(|| "strategy required".to_string())?;
                            crate::team_coordination_policies::validate_strategy(&strategy).map_err(|e| e.to_string())?;
                            if strategy.eligible_roles.iter().any(|role| !spec.members.iter().any(|member| member.role == *role)) {
                                return Err("strategy eligible set exceeds team roster".to_string());
                            }
                            let snapshot = request.protocol_snapshot.ok_or_else(|| "protocol snapshot required".to_string())?;
                            if snapshot.protocol_id != strategy.protocol_id || snapshot.content_hash != strategy.protocol_hash {
                                return Err("protocol snapshot mismatch".to_string());
                            }
                            let protocol: crate::team_sop_protocols::TeamProtocol = serde_json::from_slice(&snapshot.protocol_json).map_err(|_| "invalid protocol snapshot".to_string())?;
                            crate::team_sop_protocols::validate_protocol(&protocol).map_err(|e| e.to_string())?;
                            let strategy_state = request.strategy_state.as_ref().ok_or_else(|| "strategy state required".to_string())?;
                            let participant = request.participant.as_ref();
                            let handoff_from = request.handoff_from.as_deref();
                            if matches!(&strategy.kind, crate::team_coordination_policies::TeamCoordinationStrategyKind::HandoffSwarm { .. } | crate::team_coordination_policies::TeamCoordinationStrategyKind::GraphDirected { .. }) {
                                let from = handoff_from.ok_or_else(|| "handoff source required".to_string())?;
                                let to = participant.as_ref().map(|item| item.role.as_str()).ok_or_else(|| "handoff target required".to_string())?;
                                crate::team_coordination_policies::validate_protocol_route(&snapshot, from, to).map_err(|e| e.to_string())?;
                            }
                            let (next, decision) = crate::team_coordination_policies::select_strategy(&strategy, strategy_state, participant, handoff_from, &request.event_ids).map_err(|e| e.to_string())?;
                            let strategy_json = serde_json::to_vec(&strategy).map_err(|e| e.to_string())?;
                            let next_json = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let connection = journal.database().lock().await;
                            let saved = evohime_local_storage::team_coordination_policies_store::save_strategy_state(connection.connection(), evohime_local_storage::team_coordination_policies_store::StrategyStateInput {
                                session_id: &strategy.session_id,
                                strategy_id: &strategy.strategy_id,
                                strategy_revision: strategy.revision,
                                protocol_hash: &strategy.protocol_hash,
                                strategy_json: &strategy_json,
                                state_json: &next_json,
                                expected_version,
                                idempotency_key: &idempotency_key,
                                now_ms: crate::task_memory::now_millis() as i64,
                            }).map_err(|e| e.to_string())?;
                            if !saved { return Err("version_conflict_or_duplicate".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"selected","team_id":team_id,"session_id":strategy.session_id,"strategy_id":strategy.strategy_id,"protocol_hash":strategy.protocol_hash,"version":expected_version.saturating_add(1),"state":next,"decision":decision})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported coordination operation".to_string()),
                    }
                }.await;
                let event_value = result
                    .as_ref()
                    .ok()
                    .and_then(|payload| serde_json::from_slice::<serde_json::Value>(payload).ok());
                let event = CoreEvent::TeamCoordinationPolicies {
                    team_id: event_team_id,
                    operation: event_operation,
                    status: event_value
                        .as_ref()
                        .and_then(|value| value.get("status"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("failed")
                        .to_owned(),
                    version: event_value
                        .as_ref()
                        .and_then(|value| value.get("version"))
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                    projection_json: event_value
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "{}".into()),
                };
                let (journal, events) = {
                    let guard = state.lock().await;
                    (guard.journal.clone(), guard.events.clone())
                };
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::TypedAgentHandoffContract {
                operation,
                handoff_id,
                packet_json,
                actor,
                reason,
                expected_version,
                idempotency_key: _,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_handoff_id = handoff_id.clone();
                let result = async {
                    if handoff_id.is_empty() || handoff_id.len() > crate::typed_agent_handoff_contract::MAX_TEXT {
                        return Err("invalid handoff id".to_string());
                    }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let connection = journal.database().lock().await;
                    match operation.as_str() {
                        "propose" => {
                            let packet: crate::typed_agent_handoff_contract::HandoffPacket = serde_json::from_slice(&packet_json).map_err(|_| "invalid handoff packet".to_string())?;
                            if packet.handoff_id != handoff_id { return Err("handoff identity mismatch".to_string()); }
                            let record = crate::typed_agent_handoff_contract::propose(packet, "ipc-request").map_err(|e| e.to_string())?;
                            let packet_bytes = serde_json::to_vec(&record.packet).map_err(|e| e.to_string())?;
                            let state_bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
                            let saved = evohime_local_storage::typed_agent_handoff_contract_store::put(connection.connection(), &handoff_id, &packet_bytes, &state_bytes, "proposed", crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            if !saved { return serde_json::to_vec(&serde_json::json!({"status":"duplicate","handoff_id":handoff_id,"version":1,"idempotent":true})).map_err(|e| e.to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"proposed","handoff_id":handoff_id,"version":record.version})).map_err(|e| e.to_string())
                        }
                        "transition" => {
                            let (_, state_bytes, _, _) = evohime_local_storage::typed_agent_handoff_contract_store::load(connection.connection(), &handoff_id).map_err(|e| e.to_string())?.ok_or_else(|| "handoff_not_found".to_string())?;
                            let mut record: crate::typed_agent_handoff_contract::HandoffRecord = serde_json::from_slice(&state_bytes).map_err(|_| "handoff_state_corrupt".to_string())?;
                            let next: crate::typed_agent_handoff_contract::HandoffState = serde_json::from_slice(&packet_json).map_err(|_| "invalid handoff state".to_string())?;
                            crate::typed_agent_handoff_contract::transition(&mut record, next, &actor, &reason, expected_version, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            let bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
                            if !evohime_local_storage::typed_agent_handoff_contract_store::transition(connection.connection(), &handoff_id, &bytes, &format!("{:?}", record.state).to_lowercase(), expected_version, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())? { return Err("stale_handoff".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"transitioned","handoff_id":handoff_id,"state":record.state,"version":record.version})).map_err(|e| e.to_string())
                        }
                        "get" => {
                            let (_, state_bytes, state, version) = evohime_local_storage::typed_agent_handoff_contract_store::load(connection.connection(), &handoff_id).map_err(|e| e.to_string())?.ok_or_else(|| "handoff_not_found".to_string())?;
                            let record: serde_json::Value = serde_json::from_slice(&state_bytes).map_err(|_| "handoff_state_corrupt".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":state,"handoff_id":handoff_id,"version":version,"record":record})).map_err(|e| e.to_string())
                        }
                        _ => Err("unsupported handoff operation".to_string()),
                    }
                }.await;
                let event_value = result
                    .as_ref()
                    .ok()
                    .and_then(|payload| serde_json::from_slice::<serde_json::Value>(payload).ok());
                let event = CoreEvent::TypedAgentHandoffContract {
                    handoff_id: event_handoff_id,
                    operation: event_operation,
                    state: event_value
                        .as_ref()
                        .and_then(|value| value.get("state"))
                        .or_else(|| event_value.as_ref().and_then(|value| value.get("status")))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("failed")
                        .to_owned(),
                    version: event_value
                        .as_ref()
                        .and_then(|value| value.get("version"))
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0),
                    projection_json: event_value
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "{}".into()),
                };
                let (journal, events) = {
                    let guard = state.lock().await;
                    (guard.journal.clone(), guard.events.clone())
                };
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::SchemaDrivenAgentConfiguration {
                operation,
                scope,
                payload,
                expected_revision,
                idempotency_key: _,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_scope = scope.clone();
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    let scope_kind = match scope.as_str() { "application" => crate::schema_driven_agent_configuration::ConfigurationScope::ApplicationDefaults, "workspace" => crate::schema_driven_agent_configuration::ConfigurationScope::WorkspaceDefaults, "agent" => crate::schema_driven_agent_configuration::ConfigurationScope::AgentProfile, "conversation" => crate::schema_driven_agent_configuration::ConfigurationScope::ConversationDefaults, "run" => crate::schema_driven_agent_configuration::ConfigurationScope::RunOverride, _ => return Err("invalid_configuration_scope".into()) };
                    let schema = crate::schema_driven_agent_configuration::builtin_schema(scope_kind);
                    match operation.as_str() {
                        "get_schema" => serde_json::to_vec(&schema).map_err(|e| e.to_string()),
                        "get_snapshot" => {
                            let Some((_, snapshot, _)) = evohime_local_storage::schema_driven_agent_configuration_store::load(database.connection(), &scope).map_err(|e| e.to_string())? else { return serde_json::to_vec(&serde_json::json!({"status":"not_configured","scope":scope,"schema":schema})).map_err(|e| e.to_string()); };
                            Ok(snapshot)
                        }
                        "apply" => {
                            let input: serde_json::Value = serde_json::from_slice(&payload).map_err(|_| "invalid_configuration_payload".to_string())?;
                            let mut values = input.get("values").and_then(serde_json::Value::as_object).cloned().unwrap_or_default();
                            let patches = if let Some(raw) = input.get("patches").and_then(serde_json::Value::as_array) {
                                let mut parsed = Vec::with_capacity(raw.len());
                                for item in raw { let object = item.as_object().ok_or_else(|| "invalid_configuration_patch".to_string())?; let kind = match object.get("kind").and_then(serde_json::Value::as_str).unwrap_or("") { "SetField" => crate::schema_driven_agent_configuration::PatchKind::SetField, "ClearOverride" => crate::schema_driven_agent_configuration::PatchKind::ClearOverride, "ResetSection" => crate::schema_driven_agent_configuration::PatchKind::ResetSection, "BindReference" => crate::schema_driven_agent_configuration::PatchKind::BindReference, _ => return Err("invalid_configuration_patch_kind".into()) }; let field = object.get("field").and_then(serde_json::Value::as_str).ok_or_else(|| "patch_field_required".to_string())?.to_owned(); let value = object.get("value").cloned(); if matches!(kind, crate::schema_driven_agent_configuration::PatchKind::SetField | crate::schema_driven_agent_configuration::PatchKind::BindReference) { if let Some(value) = &value { values.insert(field.clone(), value.clone()); } } else if matches!(kind, crate::schema_driven_agent_configuration::PatchKind::ClearOverride) { values.remove(&field); } else { values.clear(); } parsed.push(crate::schema_driven_agent_configuration::ConfigurationPatch { kind, field, value_json: value }); }
                                parsed
                            } else { values.iter().map(|(field, value)| crate::schema_driven_agent_configuration::ConfigurationPatch { kind: crate::schema_driven_agent_configuration::PatchKind::SetField, field: field.clone(), value_json: Some(value.clone()) }).collect::<Vec<_>>() };
                            crate::schema_driven_agent_configuration::validate_patches(&schema, &patches).map_err(|e| e.to_string())?;
                            let layers = [("requested", &values)];
                            let current = evohime_local_storage::schema_driven_agent_configuration_store::load(database.connection(), &scope).map_err(|e| e.to_string())?;
                            let revision = current.as_ref().map(|(_, _, revision)| *revision).unwrap_or(0);
                            if current.is_some() && revision != expected_revision { return Err("configuration_revision_conflict".into()); }
                            let snapshot = crate::schema_driven_agent_configuration::effective_snapshot(scope_kind, &schema, revision + 1, &layers).map_err(|e| e.to_string())?;
                            let schema_json = serde_json::to_vec(&schema).map_err(|e| e.to_string())?; let snapshot_json = serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?;
                            if !evohime_local_storage::schema_driven_agent_configuration_store::save(database.connection(), &scope, &schema_json, &snapshot_json, revision + 1, crate::task_memory::now_millis() as i64, expected_revision).map_err(|e| e.to_string())? { return Err("configuration_revision_conflict".into()); }
                            Ok(snapshot_json)
                        }
                        _ => Err("unsupported_configuration_operation".into()),
                    }
                }.await;
                let revision = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
                    .and_then(|v| v.get("revision").and_then(serde_json::Value::as_u64))
                    .unwrap_or(0);
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::SchemaDrivenAgentConfiguration {
                    scope: event_scope,
                    operation: event_operation,
                    revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ExperienceReplayLibrary {
                operation,
                scope,
                scope_id,
                payload,
                expected_revision,
                idempotency_key: _,
                reply,
            } => {
                let event_scope = scope.clone();
                let event_operation = operation.clone();
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    let scope_kind = match scope.as_str() { "Session"=>crate::experience_replay_library::ExperienceScope::Session,"Project"=>crate::experience_replay_library::ExperienceScope::Project,"User"=>crate::experience_replay_library::ExperienceScope::User,"RoleProfile"=>crate::experience_replay_library::ExperienceScope::RoleProfile,"WorkflowProfile"=>crate::experience_replay_library::ExperienceScope::WorkflowProfile,_=>return Err("invalid_experience_scope".into()) };
                    match operation.as_str() {
                        "write" => { let record: crate::experience_replay_library::ExperienceRecord = serde_json::from_slice(&payload).map_err(|_| "invalid_experience_record".to_string())?; if record.scope != scope_kind || record.scope_id != scope_id { return Err("experience_scope_denied".into()); } crate::experience_replay_library::validate_and_write_gate(&record).map_err(|e|e.to_string())?; let hash=record.content_hash.clone(); let json=serde_json::to_vec(&record).map_err(|e|e.to_string())?; let saved=evohime_local_storage::experience_replay_library_store::put(database.connection(),&record.id,&scope,&scope_id,&json,&hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":if saved{"stored"}else{"duplicate"},"id":record.id,"revision":1,"idempotent":!saved})).map_err(|e|e.to_string()) }
                        "list" => { let records=evohime_local_storage::experience_replay_library_store::list(database.connection(),&scope,&scope_id,64).map_err(|e|e.to_string())?; let records:Vec<serde_json::Value>=records.into_iter().filter_map(|b|serde_json::from_slice(&b).ok()).collect(); serde_json::to_vec(&serde_json::json!({"status":"ok","scope":scope,"records":records})).map_err(|e|e.to_string()) }
                        "context" => { let records=evohime_local_storage::experience_replay_library_store::list(database.connection(),&scope,&scope_id,64).map_err(|e|e.to_string())?; let records:Vec<crate::experience_replay_library::ExperienceRecord>=records.into_iter().filter_map(|b|serde_json::from_slice(&b).ok()).collect(); let context=crate::experience_replay_library::project_context(&records,crate::experience_replay_library::MAX_CONTEXT_BYTES).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"ok","context":context,"max_bytes":crate::experience_replay_library::MAX_CONTEXT_BYTES})).map_err(|e|e.to_string()) }
                        _ => Err("unsupported_experience_operation".into()),
                    }
                }.await;
                let revision = result
                    .as_ref()
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok())
                    .and_then(|v| v.get("revision").and_then(serde_json::Value::as_u64))
                    .unwrap_or(expected_revision);
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::ExperienceReplayLibrary {
                    scope: event_scope,
                    operation: event_operation,
                    revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::RuntimeInterventionPipeline {
                operation,
                run_id,
                payload: _,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    use crate::agent_middleware_pipeline::{AgentMiddlewarePipelineService, BuiltinPolicy, FailurePolicy, HandlerMode, HookPhase, MiddlewareRequest, MiddlewareSpec, PipelineDefinition, PipelineRunSnapshot, StateClass};
                    let definition = PipelineDefinition::new("runtime-intervention", 1, vec![MiddlewareSpec { id: "core-policy".into(), version: 1, priority: 0, phases: HookPhase::ALL.to_vec(), state_class: StateClass::Public, policy: BuiltinPolicy::Observe, mode: HandlerMode::ObserveOnly, failure_policy: FailurePolicy::FailClosed }]).map_err(|e| e.to_string())?;
                    let snapshot = PipelineRunSnapshot { run_id: run_id.clone(), definition_id: definition.definition_id.clone(), definition_revision: definition.revision, contract_hash: definition.contract_hash.clone(), policy_hash: "core-policy-v1".into(), capability_snapshot_hash: "core-capability-snapshot".into() };
                    let mut service = AgentMiddlewarePipelineService::new(definition, snapshot, "core-capability-snapshot").map_err(|e| e.to_string())?;
                    let request = MiddlewareRequest { run_id: run_id.clone(), correlation_id: format!("runtime:{run_id}"), idempotency_key, phase: HookPhase::BeforeAgent, input_hash: "metadata-only".into(), capability_snapshot_hash: "core-capability-snapshot".into(), intervention_depth: 0 };
                    let (outcome, events) = service.evaluate(&request).map_err(|e| e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"ok","operation":operation,"run_id":run_id,"outcome":outcome,"events":events})).map_err(|e| e.to_string())
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::RuntimeInterventionPipeline {
                    run_id,
                    operation,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::CodeDiagnosticsFeedbackLoop {
                operation,
                workspace_root_id,
                payload,
                baseline_snapshot_id,
                expected_revision,
                idempotency_key: _,
                reply,
            } => {
                let event_operation = operation.clone();
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::code_diagnostics_feedback_loop_store as store;
                    match operation.as_str() {
                        "register_provider" => { let p: crate::code_diagnostics_feedback_loop::Provider = serde_json::from_slice(&payload).map_err(|_| "invalid_provider".to_string())?; crate::code_diagnostics_feedback_loop::validate_provider(&p).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&p).map_err(|e|e.to_string())?; let saved=store::put_provider(database.connection(),&p.id,&json,&p.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":if saved {"registered"} else {"duplicate"},"provider_id":p.id,"revision":1})).map_err(|e|e.to_string()) }
                        "snapshot" => { let s: crate::code_diagnostics_feedback_loop::Snapshot=serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot".to_string())?; crate::code_diagnostics_feedback_loop::validate_snapshot(&s).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&s).map_err(|e|e.to_string())?; let saved=store::put_snapshot(database.connection(),&s.id,&s.workspace_fingerprint,&json,&s.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":if saved {"stored"} else {"duplicate"},"snapshot_id":s.id,"revision":1})).map_err(|e|e.to_string()) }
                        "delta" => { let current: crate::code_diagnostics_feedback_loop::Snapshot=serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot".to_string())?; let baseline_json=store::get_snapshot(database.connection(),&baseline_snapshot_id).map_err(|e|e.to_string())?.ok_or_else(|| "baseline_not_found".to_string())?; let baseline: crate::code_diagnostics_feedback_loop::Snapshot=serde_json::from_slice(&baseline_json).map_err(|_| "invalid_baseline".to_string())?; let d=crate::code_diagnostics_feedback_loop::delta(&baseline,&current).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&d).map_err(|e|e.to_string())?; let id=format!("{}:{}",baseline.id,current.id); let _=store::put_delta(database.connection(),&id,&baseline.id,&current.id,&json,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"ok","revision":expected_revision.saturating_add(1),"delta":d})).map_err(|e|e.to_string()) }
                        "gate" => { let s: crate::code_diagnostics_feedback_loop::Snapshot=serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot".to_string())?; crate::code_diagnostics_feedback_loop::validate_snapshot(&s).map_err(|e|e.to_string())?; let errors=s.diagnostics.iter().filter(|d|!d.stale && d.severity=="error").count(); serde_json::to_vec(&serde_json::json!({"status":if errors==0 {"passed"} else {"blocked"},"error_count":errors,"revision":expected_revision})).map_err(|e|e.to_string()) }
                        _ => Err("unsupported_diagnostics_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::CodeDiagnosticsFeedbackLoop {
                    workspace_root_id,
                    operation: event_operation,
                    revision: expected_revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::WorkflowOptimizationLab {
                operation,
                run_id,
                payload,
                expected_revision,
                idempotency_key: _,
                reply,
            } => {
                let result = async {
                    let journal=state.lock().await.journal.clone().ok_or_else(||"storage journal is not configured".to_string())?;
                    let database=journal.database().lock().await;
                    use evohime_local_storage::workflow_optimization_lab_store as store;
                    match operation.as_str() {
                        "evaluate" => {
                            let input: BenchmarkEvaluationInput = serde_json::from_slice(&payload).map_err(|_| "invalid_benchmark_request".to_string())?;
                            let report = crate::workflow_optimization_lab::evaluate_candidate(&run_id, &input.candidate, &input.request).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"evaluated","run_id":run_id,"report":report,"revision":expected_revision.saturating_add(1)})).map_err(|e| e.to_string())
                        }
                        "save_run" => { let run:crate::workflow_optimization_lab::OptimizationRun=serde_json::from_slice(&payload).map_err(|_|"invalid_optimization_run".to_string())?; crate::workflow_optimization_lab::validate_run(&run).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&run).map_err(|e|e.to_string())?; let saved=store::put_run(database.connection(),&run.id,&json,&run.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":if saved{"stored"}else{"duplicate"},"run_id":run.id,"revision":1})).map_err(|e|e.to_string()) }
                        "get_run" => { let json=store::get_run(database.connection(),&run_id).map_err(|e|e.to_string())?.ok_or_else(||"run_not_found".to_string())?; let run:crate::workflow_optimization_lab::OptimizationRun=serde_json::from_slice(&json).map_err(|_|"corrupt_run".to_string())?; serde_json::to_vec(&serde_json::json!({"status":"ok","run":run,"revision":expected_revision})).map_err(|e|e.to_string()) }
                        "validate_candidate" => { let c:crate::workflow_optimization_lab::Candidate=serde_json::from_slice(&payload).map_err(|_|"invalid_candidate".to_string())?; crate::workflow_optimization_lab::validate_candidate(&c,crate::workflow_optimization_lab::Split::Validation).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"validated","candidate_id":c.id,"revision":expected_revision})).map_err(|e|e.to_string()) }
                        "promote" => { let c:crate::workflow_optimization_lab::Candidate=serde_json::from_slice(&payload).map_err(|_|"invalid_candidate".to_string())?; let run_json=store::get_run(database.connection(),&run_id).map_err(|e|e.to_string())?.ok_or_else(||"run_not_found".to_string())?; let run:crate::workflow_optimization_lab::OptimizationRun=serde_json::from_slice(&run_json).map_err(|_|"corrupt_run".to_string())?; crate::workflow_optimization_lab::promotion_allowed(&run,&c,true,true).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"promoted","run_id":run_id,"candidate_id":c.id,"revision":expected_revision.saturating_add(1)})).map_err(|e|e.to_string()) }
                        _ => Err("unsupported_optimization_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::WorkflowOptimizationLab {
                    run_id,
                    operation,
                    revision: expected_revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::CoreTopicSubscriptionEventBus {
                operation,
                payload,
                capability,
                idempotency_key: _,
                reply,
            } => {
                let result = async {
                    let journal=state.lock().await.journal.clone().ok_or_else(||"storage journal is not configured".to_string())?;
                    let database=journal.database().lock().await;
                    use evohime_local_storage::core_topic_subscription_event_bus_store as store;
                    let required=if operation=="publish"{"runtime.events.publish"}else{"runtime.events.read"};
                    if capability!=required{return Err("capability_denied".into())}
                    match operation.as_str() {
                        "publish"=>{let e:crate::core_topic_subscription_event_bus::Event=serde_json::from_slice(&payload).map_err(|_|"invalid_event".to_string())?;crate::core_topic_subscription_event_bus::validate_event(&e).map_err(|e|e.to_string())?;let json=serde_json::to_vec(&e).map_err(|e|e.to_string())?;let saved=store::put_event(database.connection(),&e.event_id,&json,&e.content_hash,"published",crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":if saved{"published"}else{"duplicate"},"event_id":e.event_id})).map_err(|e|e.to_string())}
                        "subscribe"=>{let s:crate::core_topic_subscription_event_bus::Subscription=serde_json::from_slice(&payload).map_err(|_|"invalid_subscription".to_string())?;crate::core_topic_subscription_event_bus::validate_subscription(&s).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":"subscribed","subscription_id":s.id})).map_err(|e|e.to_string())}
                        "ack"|"nack"=>{let request:DeliveryRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_delivery".to_string())?;let attempt=request.attempt.min(u32::MAX as u64) as u32;let next=crate::core_topic_subscription_event_bus::transition(crate::core_topic_subscription_event_bus::DeliveryState::InFlight,operation.as_str(),attempt).map_err(|e|e.to_string())?;store::put_delivery(database.connection(),&request.subscription_id,&request.event_id,&format!("{next:?}"),attempt,request.error.as_deref(),crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?;if matches!(next,crate::core_topic_subscription_event_bus::DeliveryState::DeadLetter){store::put_dead_letter(database.connection(),&request.subscription_id,&request.event_id,attempt,"consumer_failure","redacted",crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?;}serde_json::to_vec(&serde_json::json!({"status":format!("{next:?}").to_lowercase(),"attempt":attempt})).map_err(|e|e.to_string())}
                        _=>Err("unsupported_bus_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::CoreTopicSubscriptionEventBus {
                    operation,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::DependencyAwareTaskGraph {
                operation,
                graph_id,
                payload,
                expected_revision,
                grants,
                reply,
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::dependency_aware_task_graph_store as store;
                    match operation.as_str() {
                        "validate" => { let graph: crate::dependency_aware_task_graph::TaskGraph = serde_json::from_slice(&payload).map_err(|_| "invalid_graph".to_string())?; crate::dependency_aware_task_graph::validate(&graph, &grants).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"valid","ready":crate::dependency_aware_task_graph::ready_set(&graph),"revision":graph.revision})).map_err(|e|e.to_string()) }
                        "get" => store::get(database.connection(), &graph_id).map_err(|e|e.to_string())?.ok_or_else(|| "graph_not_found".to_string()),
                        "create" => { let graph: crate::dependency_aware_task_graph::TaskGraph=serde_json::from_slice(&payload).map_err(|_|"invalid_graph".to_string())?; crate::dependency_aware_task_graph::validate(&graph,&grants).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&graph).map_err(|e|e.to_string())?; if !store::put(database.connection(),&graph_id,graph.revision,&json,&graph.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())? { return Err("graph_exists".into()); } Ok(json) }
                        "apply_patch" => { let bytes=store::get(database.connection(),&graph_id).map_err(|e|e.to_string())?.ok_or_else(||"graph_not_found".to_string())?; let current: crate::dependency_aware_task_graph::TaskGraph=serde_json::from_slice(&bytes).map_err(|_|"corrupt_graph".to_string())?; let ops: Vec<crate::dependency_aware_task_graph::PatchOp>=serde_json::from_slice(&payload).map_err(|_|"invalid_patch".to_string())?; let next=crate::dependency_aware_task_graph::apply_patch(current,&ops,expected_revision,&grants).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&next).map_err(|e|e.to_string())?; if !store::replace(database.connection(),&graph_id,expected_revision,next.revision,&json,&next.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())? { return Err("stale_graph_revision".into()); } Ok(json) }
                        _ => Err("unsupported_task_graph_operation".into())
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::DependencyAwareTaskGraph {
                    graph_id,
                    operation,
                    revision: expected_revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::DeclarativeAgentComponentRegistry {
                operation,
                registry_id,
                payload,
                expected_revision,
                reply,
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::declarative_agent_component_registry_store as store;
                    match operation.as_str() {
                        "get" => store::get(database.connection(), &registry_id).map_err(|e|e.to_string())?.ok_or_else(|| "registry_not_found".to_string()),
                        "validate" => { let registry: crate::declarative_agent_component_registry::Registry=serde_json::from_slice(&payload).map_err(|_|"invalid_registry".to_string())?; crate::declarative_agent_component_registry::validate_registry(&registry).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"valid","revision":registry.revision,"providers":registry.providers.len(),"components":registry.components.len()})).map_err(|e|e.to_string()) }
                        "create" => { let registry: crate::declarative_agent_component_registry::Registry=serde_json::from_slice(&payload).map_err(|_|"invalid_registry".to_string())?; crate::declarative_agent_component_registry::validate_registry(&registry).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&registry).map_err(|e|e.to_string())?; if !store::put(database.connection(),&registry_id,registry.revision,&json,&registry.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())? {return Err("registry_exists".into())} Ok(json) }
                        "replace" => { let registry: crate::declarative_agent_component_registry::Registry=serde_json::from_slice(&payload).map_err(|_|"invalid_registry".to_string())?; crate::declarative_agent_component_registry::validate_registry(&registry).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&registry).map_err(|e|e.to_string())?; if !store::replace(database.connection(),&registry_id,expected_revision,registry.revision,&json,&registry.content_hash,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())? {return Err("stale_registry_revision".into())} Ok(json) }
                        "diff" => { let pair: Vec<crate::declarative_agent_component_registry::ComponentDescriptor>=serde_json::from_slice(&payload).map_err(|_|"invalid_diff".to_string())?; if pair.len()!=2{return Err("diff_requires_two_descriptors".into())}; serde_json::to_vec(&crate::declarative_agent_component_registry::diff(&pair[0],&pair[1]).map_err(|e|e.to_string())?).map_err(|e|e.to_string()) }
                        _ => Err("unsupported_component_registry_operation".into())
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::DeclarativeAgentComponentRegistry {
                    registry_id,
                    operation,
                    revision: expected_revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::TypedContextReferences {
                operation,
                ref_id,
                payload,
                reply,
            } => {
                let result = async {
                    if operation == "parse" {
                        return serde_json::to_vec(
                            &crate::typed_context_references::parse_mentions(
                                std::str::from_utf8(&payload).unwrap_or_default(),
                                true,
                            ),
                        )
                        .map_err(|e| e.to_string());
                    }
                    let reference: crate::typed_context_references::ContextRef =
                        serde_json::from_slice(&payload)
                            .map_err(|_| "invalid_context_ref".to_string())?;
                    crate::typed_context_references::validate_ref(&reference)
                        .map_err(|e| e.to_string())?;
                    let resolved = match operation.as_str() {
                        "parse" => {
                            serde_json::to_vec(&crate::typed_context_references::parse_mentions(
                                std::str::from_utf8(&payload).unwrap_or_default(),
                                true,
                            ))
                            .map_err(|e| e.to_string())?
                        }
                        "resolve" => serde_json::to_vec(
                            &crate::typed_context_references::resolve(
                                &reference,
                                reference.revision_hint.clone(),
                                None,
                            )
                            .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?,
                        "budget" => {
                            let refs: Vec<crate::typed_context_references::ResolvedContextRef> =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_budget_refs".to_string())?;
                            serde_json::to_vec(&crate::typed_context_references::plan_budget(
                                &refs, 4096,
                            ))
                            .map_err(|e| e.to_string())?
                        }
                        "kinds" => {
                            serde_json::to_vec(&crate::typed_context_references::supported_kinds())
                                .map_err(|e| e.to_string())?
                        }
                        _ => return Err("unsupported_context_reference_operation".into()),
                    };
                    Ok(resolved)
                }
                .await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::TypedContextReferences {
                    ref_id,
                    operation,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::SafeUiExtensionFramework {
                operation,
                extension_id,
                payload,
                expected_revision,
                reply,
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::safe_ui_extension_framework_store as store;
                    match operation.as_str() {
                        "install" => {
                            let manifest: crate::safe_ui_extension_framework::UiExtensionManifest = serde_json::from_slice(&payload).map_err(|_| "invalid_manifest".to_string())?;
                            if manifest.id != extension_id { return Err("extension_id_mismatch".into()); }
                            let installed = crate::safe_ui_extension_framework::install(manifest, "workspace", "revision-1").map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&installed).map_err(|e| e.to_string())?;
                            if !store::put(database.connection(), &extension_id, installed.revision, &format!("{:?}", installed.lifecycle), &json, &installed.manifest_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())? { return Err("extension_exists".into()); }
                            Ok(json)
                        }
                        "get" => store::get(database.connection(), &extension_id).map_err(|e| e.to_string())?.ok_or_else(|| "extension_not_found".into()),
                        "validate" => {
                            let manifest: crate::safe_ui_extension_framework::UiExtensionManifest = serde_json::from_slice(&payload).map_err(|_| "invalid_manifest".to_string())?;
                            crate::safe_ui_extension_framework::validate(&manifest).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"valid","id":manifest.id,"contributions":manifest.contributions.len()})).map_err(|e| e.to_string())
                        }
                        "enable" | "disable" => {
                            let json = store::get(database.connection(), &extension_id).map_err(|e| e.to_string())?.ok_or_else(|| "extension_not_found".to_string())?;
                            let mut installed: crate::safe_ui_extension_framework::InstalledUiExtension = serde_json::from_slice(&json).map_err(|_| "corrupt_extension".to_string())?;
                            let target = if operation == "enable" { crate::safe_ui_extension_framework::Lifecycle::Enabled } else { crate::safe_ui_extension_framework::Lifecycle::Disabled };
                            crate::safe_ui_extension_framework::transition(&mut installed, target, expected_revision).map_err(|e| e.to_string())?;
                            let next = serde_json::to_vec(&installed).map_err(|e| e.to_string())?;
                            if !store::replace(database.connection(), &extension_id, installed.revision, &format!("{:?}", installed.lifecycle), &next, &installed.manifest_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())? { return Err("extension_not_found".into()); }
                            Ok(next)
                        }
                        "update" => {
                            let manifest: crate::safe_ui_extension_framework::UiExtensionManifest = serde_json::from_slice(&payload).map_err(|_| "invalid_manifest".to_string())?;
                            if manifest.id != extension_id { return Err("extension_id_mismatch".into()); }
                            let current_json = store::get(database.connection(), &extension_id).map_err(|e| e.to_string())?.ok_or_else(|| "extension_not_found".to_string())?;
                            let current: crate::safe_ui_extension_framework::InstalledUiExtension = serde_json::from_slice(&current_json).map_err(|_| "corrupt_extension".to_string())?;
                            if current.revision != expected_revision { return Err("stale revision".into()); }
                            if current.manifest.required_projection_capabilities != manifest.required_projection_capabilities { return Err("capability delta requires review".into()); }
                            let mut updated = crate::safe_ui_extension_framework::install(manifest, &current.scope, &current.resolved_revision).map_err(|e| e.to_string())?;
                            updated.revision = current.revision + 1;
                            let next = serde_json::to_vec(&updated).map_err(|e| e.to_string())?;
                            if !store::replace(database.connection(), &extension_id, updated.revision, &format!("{:?}", updated.lifecycle), &next, &updated.manifest_hash, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())? { return Err("extension_not_found".into()); }
                            Ok(next)
                        }
                        _ => Err("unsupported_ui_extension_operation".into())
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::SafeUiExtensionFramework {
                    extension_id,
                    operation,
                    revision: expected_revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::CapabilityWorkbench {
                operation,
                instance_id,
                owner_id,
                payload,
                expected_revision,
                grants,
                reply,
            } => {
                let result = async {
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::capability_workbenches as workbench;
                    use evohime_local_storage::capability_workbenches_store as store;
                    let now = crate::task_memory::now_millis();
                    let mut instance: workbench::WorkbenchInstance = if operation == "create" {
                        let descriptor: workbench::WorkbenchDescriptor =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_descriptor")?;
                        let instance = workbench::WorkbenchInstance::new(
                            instance_id.clone(),
                            owner_id.clone(),
                            descriptor,
                            now,
                        )
                        .map_err(|error| error.to_string())?;
                        let descriptor_json =
                            serde_json::to_vec(&instance).map_err(|_| "serialization_failed")?;
                        store::put_instance(
                            database.connection(),
                            &instance.instance_id,
                            &instance.owner_id,
                            instance.revision as i64,
                            "created",
                            &descriptor_json,
                            now as i64,
                        )
                        .map_err(|_| "instance_exists")?;
                        store::put_lease(
                            database.connection(),
                            &format!("{}:{}", instance.instance_id, instance.owner_id),
                            &instance.instance_id,
                            &instance.owner_id,
                            now.saturating_add(instance.descriptor.lease_ttl_ms) as i64,
                            now as i64,
                        )
                        .map_err(|_| "lease_exists")?;
                        return serde_json::to_vec(&instance)
                            .map_err(|_| "serialization_failed".to_string());
                    } else {
                        let descriptor_json =
                            store::get_instance(database.connection(), &instance_id)
                                .map_err(|_| "storage_failed")?
                                .ok_or_else(|| "instance_not_found".to_string())?;
                        let instance: workbench::WorkbenchInstance = serde_json::from_slice(&descriptor_json)
                            .map_err(|_| "corrupt_instance".to_string())?;
                        if instance.owner_id != owner_id {
                            return Err("owner_denied".into());
                        }
                        instance
                    };
                    if operation == "list_tools" {
                        let tools = instance.visible_tools(&grants);
                        return serde_json::to_vec(&serde_json::json!({
                            "schema_version": workbench::SCHEMA_VERSION,
                            "instance_id": instance_id,
                            "tools": tools,
                        }))
                        .map_err(|_| "serialization_failed".to_string());
                    }
                    let target = match operation.as_str() {
                        "start" => Some(workbench::Lifecycle::Starting),
                        "ready" => Some(workbench::Lifecycle::Ready),
                        "stop" => Some(workbench::Lifecycle::Stopping),
                        "stopped" => Some(workbench::Lifecycle::Stopped),
                        "reset" => Some(workbench::Lifecycle::Resetting),
                        "degraded" => Some(workbench::Lifecycle::Degraded),
                        _ => None,
                    };
                    if let Some(target) = target {
                        instance
                            .transition(target, expected_revision)
                            .map_err(|error| error.to_string())?;
                    } else if operation == "heartbeat" {
                        instance.heartbeat(now);
                        if !store::renew_lease(
                            database.connection(),
                            &format!("{}:{}", instance.instance_id, instance.owner_id),
                            &instance.owner_id,
                            now as i64,
                            now.saturating_add(instance.descriptor.lease_ttl_ms) as i64,
                        )
                        .map_err(|_| "storage_failed")?
                        {
                            return Err("lease_not_found".into());
                        }
                    } else if operation == "recover" {
                        store::expire_leases(database.connection(), now as i64)
                            .map_err(|_| "storage_failed")?;
                        instance.recover_if_expired(now);
                    } else if operation == "call_tool" {
                        let call: WorkbenchCallRequest =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_call")?;
                        let capability = call.capability.as_str();
                        instance
                            .admit_call(capability, &grants)
                            .map_err(|error| error.to_string())?;
                        instance.finish_call();
                        let tool_id = call.tool_id.as_deref().unwrap_or(capability);
                        let result = workbench::WorkbenchCallResult {
                            schema_version: workbench::SCHEMA_VERSION,
                            instance_id: instance.instance_id.clone(),
                            tool_id: tool_id.to_owned(),
                            outcome: workbench::CallOutcome::Unavailable,
                            value: serde_json::Value::Null,
                            error_code: Some("runtime_adapter_unavailable".into()),
                            cancellation: workbench::CancellationOutcome::AlreadyTerminal,
                        };
                        return serde_json::to_vec(&result)
                            .map_err(|_| "serialization_failed".to_string());
                    } else if operation == "cancel" {
                        return serde_json::to_vec(&serde_json::json!({
                            "instance_id": instance.instance_id,
                            "cancellation_outcome": format!("{:?}", workbench::cancellation_outcome(false, false)).to_ascii_lowercase(),
                            "unknown": true,
                        })).map_err(|_| "serialization_failed".to_string());
                    } else if operation == "resource" {
                        let request: WorkbenchResourceRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_resource")?;
                        let resource = instance.descriptor.resources.iter_mut().find(|resource| resource.id == request.resource_id).ok_or_else(|| "resource_not_found".to_string())?;
                        resource.available = request.available;
                        instance.revision = instance.revision.saturating_add(1);
                    } else if operation == "snapshot" {
                        let request: WorkbenchSnapshotRequest = if payload.is_empty() {
                            WorkbenchSnapshotRequest::default()
                        } else {
                            serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot")?
                        };
                        let snapshot = instance
                            .snapshot(request.logical_state, request.credential_refs)
                            .map_err(|error| error.to_string())?;
                        let snapshot_json =
                            serde_json::to_vec(&snapshot).map_err(|_| "serialization_failed")?;
                        let snapshot_id = uuid::Uuid::now_v7().to_string();
                        store::put_snapshot(
                            database.connection(),
                            &snapshot_id,
                            &instance_id,
                            snapshot.revision as i64,
                            &snapshot_json,
                            now as i64,
                        )
                        .map_err(|_| "storage_failed")?;
                        return Ok(serde_json::to_vec(
                            &serde_json::json!({"snapshot_id":snapshot_id,"snapshot":snapshot}),
                        )
                        .map_err(|_| "serialization_failed")?);
                    } else if operation == "restore" {
                        let snapshot: workbench::WorkbenchSnapshot =
                            serde_json::from_slice(&payload).map_err(|_| "invalid_snapshot")?;
                        workbench::validate_snapshot(&snapshot)
                            .map_err(|error| error.to_string())?;
                        if snapshot.instance_id != instance_id {
                            return Err("snapshot_instance_mismatch".into());
                        }
                        instance.lifecycle = snapshot.lifecycle;
                        instance.revision = snapshot.revision.saturating_add(1);
                    } else if operation != "get" {
                        return Err("unsupported_workbench_operation".into());
                    }
                    let descriptor_json =
                        serde_json::to_vec(&instance).map_err(|_| "serialization_failed")?;
                    if operation != "get" {
                        let old_revision = if operation == "heartbeat" || operation == "list_tools"
                        {
                            instance.revision
                        } else {
                            instance.revision.saturating_sub(1)
                        } as i64;
                        if !store::replace_instance(
                            database.connection(),
                            &instance_id,
                            old_revision,
                            instance.revision as i64,
                            &format!("{:?}", instance.lifecycle).to_ascii_lowercase(),
                            &descriptor_json,
                            now as i64,
                        )
                        .map_err(|_| "storage_failed")?
                        {
                            return Err("stale_workbench_revision".into());
                        }
                    }
                    serde_json::to_vec(&instance).map_err(|_| "serialization_failed".to_string())
                }
                .await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let revision =
                    serde_json::from_slice::<serde_json::Value>(projection_json.as_bytes())
                        .ok()
                        .and_then(|value| value.get("revision").and_then(serde_json::Value::as_u64))
                        .unwrap_or(expected_revision);
                let event = CoreEvent::CapabilityWorkbench {
                    instance_id,
                    operation,
                    revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::TeamCoordinator {
                operation,
                work_item_id,
                payload,
                expected_revision,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::team_coordinator as coordinator;
                    use evohime_local_storage::team_coordinator_store as store;
                    let now = crate::task_memory::now_millis() as i64;
                    if !idempotency_key.is_empty() {
                        if let Some(cached) =
                            store::get_idempotency(database.connection(), &idempotency_key)
                                .map_err(|_| "storage_failed".to_string())?
                        {
                            return Ok(cached);
                        }
                    }
                    let encode = |item: &coordinator::TeamWorkItem| {
                        serde_json::to_vec(item).map_err(|_| "serialization_failed".to_string())
                    };
                    let status = |item: &coordinator::TeamWorkItem| {
                        serde_json::to_value(item.status)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .unwrap_or_else(|| "unknown".to_owned())
                    };
                    match operation.as_str() {
                        "create" => {
                            let item: coordinator::TeamWorkItem = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_work_item")?;
                            if item.id != work_item_id {
                                return Err("work_item_id_mismatch".into());
                            }
                            coordinator::validate_work_item(&item).map_err(|e| e.to_string())?;
                            let json = encode(&item)?;
                            store::put_work_item(
                                database.connection(),
                                store::PutWorkItemInput {
                                    item_id: &item.id,
                                    revision: item.revision as i64,
                                    status: &status(&item),
                                    assigned_instance_id: item.assigned_instance_id.as_deref(),
                                    attempt: item.attempt as i64,
                                    item_json: &json,
                                    now_ms: now,
                                },
                            )
                            .map_err(|e| {
                                if matches!(e, rusqlite::Error::SqliteFailure(_, _)) {
                                    "work_item_exists".to_string()
                                } else {
                                    "storage_failed".to_string()
                                }
                            })?;
                            Ok(json)
                        }
                        "get" => store::get_work_item(database.connection(), &work_item_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "work_item_not_found".to_string()),
                        "list" => {
                            let rows = store::list_work_items(
                                database.connection(),
                                coordinator::MAX_WORK_ITEMS,
                            )
                            .map_err(|_| "storage_failed".to_string())?;
                            let items: Vec<serde_json::Value> = rows
                                .into_iter()
                                .filter_map(|json| serde_json::from_slice(&json).ok())
                                .collect();
                            serde_json::to_vec(&serde_json::json!({
                                "schema_version": coordinator::SCHEMA_VERSION,
                                "queue_count": items.len(),
                                "work_items": items,
                                "candidate_count": 0,
                                "assignment_count": 0,
                                "consultation_count": 0,
                                "escalation": null,
                            }))
                            .map_err(|_| "serialization_failed".to_string())
                        }
                        "propose" => {
                            let request: AssignmentProposalRequest =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_proposal_request")?;
                            let item: coordinator::TeamWorkItem = if let Some(item) = request.item {
                                item
                            } else {
                                let json =
                                    store::get_work_item(database.connection(), &work_item_id)
                                        .map_err(|_| "storage_failed".to_string())?
                                        .ok_or_else(|| "work_item_not_found".to_string())?;
                                serde_json::from_slice(&json).map_err(|_| "corrupt_work_item")?
                            };
                            let candidates = request.candidates;
                            let termination_policy =
                                request.termination.as_ref().map(|value| &value.policy);
                            let termination_state =
                                request.termination.as_ref().map(|value| &value.state);
                            let termination_event =
                                request.termination.as_ref().map(|value| &value.event);
                            let proposal = coordinator::propose_assignment_with_termination(
                                &item,
                                &candidates,
                                termination_policy,
                                termination_state,
                                termination_event,
                            )
                            .map_err(|e| e.to_string())?;
                            serde_json::to_vec(&proposal)
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "assign" => {
                            let request: AssignmentRequest = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_assignment_request")?;
                            let mut item = request.item;
                            let proposal = request.proposal;
                            let candidate = request.candidate;
                            if item.id != work_item_id {
                                return Err("work_item_id_mismatch".into());
                            }
                            coordinator::validate_proposal(&item, &proposal, &candidate)
                                .map_err(|e| e.to_string())?;
                            coordinator::transition(
                                &mut item,
                                coordinator::WorkItemStatus::Assigned,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            item.assigned_instance_id = Some(candidate.instance_id.clone());
                            let json = encode(&item)?;
                            if !store::replace_work_item(
                                database.connection(),
                                store::ReplaceWorkItemInput {
                                    item_id: &item.id,
                                    expected_revision: expected_revision as i64,
                                    revision: item.revision as i64,
                                    status: &status(&item),
                                    assigned_instance_id: item.assigned_instance_id.as_deref(),
                                    attempt: item.attempt as i64,
                                    item_json: &json,
                                    now_ms: now,
                                },
                            )
                            .map_err(|_| "storage_failed")?
                            {
                                return Err("stale_work_item_revision".into());
                            }
                            let assignment_id = uuid::Uuid::now_v7().to_string();
                            let proposal_json = serde_json::to_vec(&proposal)
                                .map_err(|_| "serialization_failed")?;
                            store::put_assignment(
                                database.connection(),
                                &assignment_id,
                                &item.id,
                                &candidate.instance_id,
                                &proposal_json,
                                now,
                            )
                            .map_err(|_| "storage_failed")?;
                            Ok(json)
                        }
                        "consult" => {
                            let query: coordinator::SpecialistQuery =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_consultation")?;
                            coordinator::validate_consultation(&query)
                                .map_err(|e| e.to_string())?;
                            let json =
                                serde_json::to_vec(&query).map_err(|_| "serialization_failed")?;
                            store::put_consultation(database.connection(), &query.id, &json, now)
                                .map_err(|_| "storage_failed")?;
                            Ok(json)
                        }
                        "review" => {
                            let review: coordinator::CoordinationReview =
                                serde_json::from_slice(&payload).map_err(|_| "invalid_review")?;
                            if review.work_item_id != work_item_id {
                                return Err("work_item_id_mismatch".into());
                            }
                            coordinator::validate_review(&review).map_err(|e| e.to_string())?;
                            let json =
                                serde_json::to_vec(&review).map_err(|_| "serialization_failed")?;
                            let decision_id = uuid::Uuid::now_v7().to_string();
                            store::put_decision(
                                database.connection(),
                                &decision_id,
                                &work_item_id,
                                &json,
                                now,
                            )
                            .map_err(|_| "storage_failed")?;
                            Ok(json)
                        }
                        "decompose" => {
                            let proposal: coordinator::DecompositionProposal =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_decomposition")?;
                            if proposal.parent_work_item_id != work_item_id {
                                return Err("work_item_id_mismatch".into());
                            }
                            coordinator::validate_decomposition(&proposal)
                                .map_err(|e| e.to_string())?;
                            serde_json::to_vec(&proposal)
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "reassign" => {
                            let mut item: coordinator::TeamWorkItem =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_work_item")?;
                            if item.id != work_item_id {
                                return Err("work_item_id_mismatch".into());
                            }
                            coordinator::validate_reassignment(&item).map_err(|e| e.to_string())?;
                            coordinator::transition(
                                &mut item,
                                coordinator::WorkItemStatus::Proposed,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            item.assigned_instance_id = None;
                            item.attempt = item.attempt.saturating_add(1);
                            coordinator::validate_reassignment(&item).map_err(|e| e.to_string())?;
                            let json = encode(&item)?;
                            if !store::replace_work_item(
                                database.connection(),
                                store::ReplaceWorkItemInput {
                                    item_id: &item.id,
                                    expected_revision: expected_revision as i64,
                                    revision: item.revision as i64,
                                    status: &status(&item),
                                    assigned_instance_id: None,
                                    attempt: item.attempt as i64,
                                    item_json: &json,
                                    now_ms: now,
                                },
                            )
                            .map_err(|_| "storage_failed")?
                            {
                                return Err("stale_work_item_revision".into());
                            }
                            Ok(json)
                        }
                        "cancel" => {
                            let json = store::get_work_item(database.connection(), &work_item_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "work_item_not_found".to_string())?;
                            let mut item: coordinator::TeamWorkItem =
                                serde_json::from_slice(&json).map_err(|_| "corrupt_work_item")?;
                            coordinator::transition(
                                &mut item,
                                coordinator::WorkItemStatus::Cancelled,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            let json = encode(&item)?;
                            if !store::replace_work_item(
                                database.connection(),
                                store::ReplaceWorkItemInput {
                                    item_id: &item.id,
                                    expected_revision: expected_revision as i64,
                                    revision: item.revision as i64,
                                    status: &status(&item),
                                    assigned_instance_id: item.assigned_instance_id.as_deref(),
                                    attempt: item.attempt as i64,
                                    item_json: &json,
                                    now_ms: now,
                                },
                            )
                            .map_err(|_| "storage_failed")?
                            {
                                return Err("stale_work_item_revision".into());
                            }
                            Ok(json)
                        }
                        _ => Err("unsupported_team_coordinator_operation".into()),
                    }
                }
                .await;
                if !idempotency_key.is_empty() {
                    if let Ok(bytes) = &result {
                        if let Some(journal) = state.lock().await.journal.clone() {
                            let database = journal.database().lock().await;
                            let _ = evohime_local_storage::team_coordinator_store::put_idempotency(
                                database.connection(),
                                &idempotency_key,
                                bytes,
                            );
                        }
                    }
                }
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".to_owned());
                let revision = serde_json::from_str::<serde_json::Value>(&projection_json)
                    .ok()
                    .and_then(|value| value.get("revision").and_then(serde_json::Value::as_u64))
                    .unwrap_or(expected_revision);
                let event = CoreEvent::TeamCoordinator {
                    work_item_id,
                    operation,
                    revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ProjectInstructionStack {
                operation,
                workspace_root,
                payload,
                relevant_paths,
                expected_revision,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::project_instruction_stack as stack;
                    use evohime_local_storage::project_instruction_stack_store as store;
                    if !idempotency_key.is_empty() {
                        if let Some(cached) = store::get_idempotency(database.connection(), &idempotency_key).map_err(|_| "storage_failed".to_string())? { return Ok(cached); }
                    }
                    let root = std::path::PathBuf::from(&workspace_root);
                    let mut rules = stack::discover_rules(&root, stack::global_rules_root_from_env().as_deref()).map_err(|e| e.to_string())?;
                    for stored in store::list_rules(database.connection(), stack::MAX_RULES).map_err(|_| "storage_failed".to_string())? {
                        if let Ok(saved) = serde_json::from_slice::<stack::ProjectRule>(&stored) {
                            if let Some(rule) = rules.iter_mut().find(|rule| rule.id == saved.id) {
                                rule.enabled = saved.enabled;
                                rule.source_revision = rule.source_revision.max(saved.source_revision);
                            }
                        }
                    }
                    for rule in &rules {
                        let json = serde_json::to_vec(rule).map_err(|_| "serialization_failed")?;
                        let source_kind = serde_json::to_string(&rule.source_kind).map_err(|_| "serialization_failed")?;
                        store::put_rule(database.connection(), store::PutRuleInput { rule_id: &rule.id, revision: rule.source_revision as i64, source_kind: &source_kind, source_ref: &rule.source_ref, content_hash: &rule.content_hash, rule_json: &json, now_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed")?;
                    }
                    let now = crate::task_memory::now_millis() as i64;
                    match operation.as_str() {
                        "discover" => {
                            let projection: Vec<_> = rules.iter().map(|rule| stack::project_rule(rule, "discovered")).collect();
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"rules":projection,"rule_count":projection.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "compile" => {
                            let request: InstructionStackCompileRequest = if payload.is_empty() { InstructionStackCompileRequest::default() } else { serde_json::from_slice(&payload).map_err(|_| "invalid_stack_request")? };
                            let explicit_ids = request.explicit_ids;
                            let policy = request.policy.unwrap_or_else(stack::default_policy);
                            let snapshot = stack::compile_snapshot(&root, rules.clone(), &relevant_paths, &explicit_ids, &policy, now).map_err(|e| e.to_string())?;
                            let snapshot_id = uuid::Uuid::now_v7().to_string();
                            let snapshot_json = serde_json::to_vec(&snapshot).map_err(|_| "serialization_failed")?;
                            store::put_snapshot(database.connection(), &snapshot_id, &workspace_root, &snapshot.content_hash, &snapshot_json, now).map_err(|_| "storage_failed")?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"snapshot_id":snapshot_id,"snapshot":stack::project_snapshot(&snapshot)})).map_err(|_| "serialization_failed".to_string())
                        }
                        "get" => {
                            let snapshot_id = String::from_utf8(payload).map_err(|_| "invalid_snapshot_id")?;
                            let json = store::get_snapshot(database.connection(), &snapshot_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "snapshot_not_found".to_string())?;
                            let snapshot: stack::InstructionSnapshot = serde_json::from_slice(&json).map_err(|_| "corrupt_snapshot")?;
                            serde_json::to_vec(&stack::project_snapshot(&snapshot)).map_err(|_| "serialization_failed".to_string())
                        }
                        "toggle" => {
                            let request: InstructionStackToggleRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_toggle")?;
                            let mut rule = rules.into_iter().find(|rule| rule.id == request.rule_id).ok_or("rule_not_found")?;
                            if rule.source_kind == stack::SourceKind::Global { return Err("global_rule_requires_user_scope".into()); }
                            if rule.source_revision != expected_revision && expected_revision != 0 { return Err("stale_rule_revision".into()); }
                            rule.enabled = request.enabled; rule.source_revision = rule.source_revision.saturating_add(1);
                            let json = serde_json::to_vec(&rule).map_err(|_| "serialization_failed")?;
                            let source_kind = serde_json::to_string(&rule.source_kind).map_err(|_| "serialization_failed")?;
                            store::put_rule(database.connection(), store::PutRuleInput { rule_id: &rule.id, revision: rule.source_revision as i64, source_kind: &source_kind, source_ref: &rule.source_ref, content_hash: &rule.content_hash, rule_json: &json, now_ms: now }).map_err(|_| "storage_failed")?;
                            serde_json::to_vec(&stack::project_rule(&rule, "toggled")).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_project_instruction_operation".into()),
                    }
                }.await;
                if !idempotency_key.is_empty() {
                    if let Ok(bytes) = &result {
                        if let Some(journal) = state.lock().await.journal.clone() {
                            let database = journal.database().lock().await;
                            let _ = evohime_local_storage::project_instruction_stack_store::put_idempotency(database.connection(), &idempotency_key, bytes);
                        }
                    }
                }
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".to_owned());
                let event = CoreEvent::ProjectInstructionStack {
                    workspace_root,
                    operation,
                    revision: expected_revision,
                    projection_json,
                };
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = journal {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::WorkspaceSets {
                operation,
                set_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::workspace_sets as sets;
                    use evohime_local_storage::workspace_sets_store as store;
                    if !idempotency_key.is_empty() {
                        if let Some(cached) = store::get_idempotency(database.connection(), &idempotency_key)
                            .map_err(|_| "storage_failed".to_string())? {
                            return Ok(cached);
                        }
                    }
                    let policy = sets::default_policy();
                    match operation.as_str() {
                        "search" => {
                            let json = store::get(database.connection(), &set_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "workspace_set_not_found".to_string())?;
                            let set: sets::WorkspaceSet = serde_json::from_slice(&json)
                                .map_err(|_| "corrupt_workspace_set".to_string())?;
                            let scope: sets::SearchScope = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_workspace_search".to_string())?;
                            let matches = sets::search(&set, &scope, &policy)
                                .map_err(|error| error.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"set_id":set.id,"match_count":matches.len(),"matches":matches,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "bind" => {
                            let request: WorkspaceSetBindingRequest = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_workspace_set_binding".to_string())?;
                            let task_id = request.task_id.as_str();
                            let requested_roots = request.root_ids;
                            let json = store::get(database.connection(), &set_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "workspace_set_not_found".to_string())?;
                            let set: sets::WorkspaceSet = serde_json::from_slice(&json)
                                .map_err(|_| "corrupt_workspace_set".to_string())?;
                            if expected_version != 0 && expected_version != set.version {
                                return Err("workspace_set_stale_version".into());
                            }
                            let roots: Vec<_> = if requested_roots.is_empty() {
                                set.roots.iter().filter(|root| root.enabled).collect()
                            } else {
                                set.roots.iter().filter(|root| requested_roots.iter().any(|id| id == &root.root_id) && root.enabled).collect()
                            };
                            if roots.is_empty() || roots.len() > sets::MAX_ROOTS {
                                return Err("workspace_set_no_enabled_roots".into());
                            }
                            let binding = serde_json::json!({
                                "schema_version": 1,
                                "task_id": task_id,
                                "set_id": set.id,
                                "set_version": set.version,
                                "set_hash": set.content_hash,
                                "roots": roots.iter().map(|root| serde_json::json!({"root_id":root.root_id,"alias":root.alias,"canonical_path":root.canonical_path,"kind":root.kind,"grants":root.grants,"vcs":root.vcs,"revision":root.vcs.as_ref().map(|v| v.working_tree_revision)})).collect::<Vec<_>>(),
                                "pinned": true
                            });
                            let binding_json = serde_json::to_vec(&binding).map_err(|_| "serialization_failed".to_string())?;
                            if binding_json.len() > sets::MAX_BINDING_SNAPSHOT_BYTES { return Err("workspace_set_binding_too_large".into()); }
                            store::bind_run(database.connection(), task_id, &set.id, set.version, &binding_json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"task_id":task_id,"set_id":set.id,"set_version":set.version,"set_hash":set.content_hash,"root_count":roots.len(),"pinned":true,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "create" => {
                            let set: sets::WorkspaceSet = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_workspace_set".to_string())?;
                            let set = sets::canonicalize_and_hash(set, &policy)
                                .map_err(|error| error.to_string())?;
                            let json = serde_json::to_vec(&set).map_err(|_| "serialization_failed".to_string())?;
                            if !store::create(database.connection(), &set.id, &json, &set.content_hash, crate::task_memory::now_millis() as i64)
                                .map_err(|_| "storage_failed".to_string())? {
                                return Err("workspace_set_duplicate".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"set":set,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "get" => {
                            let json = store::get(database.connection(), &set_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "workspace_set_not_found".to_string())?;
                            let set: sets::WorkspaceSet = serde_json::from_slice(&json)
                                .map_err(|_| "corrupt_workspace_set".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"id":set.id,"version":set.version,"name":set.name,"root_count":set.roots.len(),"default_root_id":set.default_root_id,"content_hash":set.content_hash,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "update" => {
                            let set: sets::WorkspaceSet = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_workspace_set".to_string())?;
                            if set.id != set_id || set.version != expected_version.saturating_add(1) {
                                return Err("workspace_set_stale_version".into());
                            }
                            let set = sets::canonicalize_and_hash(set, &policy)
                                .map_err(|error| error.to_string())?;
                            let json = serde_json::to_vec(&set).map_err(|_| "serialization_failed".to_string())?;
                            if !store::update(database.connection(), &set_id, expected_version, set.version, &json, &set.content_hash, crate::task_memory::now_millis() as i64)
                                .map_err(|_| "storage_failed".to_string())? {
                                return Err("workspace_set_stale_version".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"id":set.id,"version":set.version,"root_count":set.roots.len(),"content_hash":set.content_hash,"redacted":true}))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_workspace_sets_operation".into()),
                    }
                }
                .await;
                if !idempotency_key.is_empty() {
                    if let Ok(bytes) = &result {
                        if let Some(journal) = state.lock().await.journal.clone() {
                            let database = journal.database().lock().await;
                            let _ = evohime_local_storage::workspace_sets_store::put_idempotency(
                                database.connection(),
                                &idempotency_key,
                                bytes,
                            );
                        }
                    }
                }
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".to_owned());
                let event = CoreEvent::WorkspaceSets {
                    set_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::KnowledgeSourceRegistryProjectRole {
                operation,
                source_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let _ = idempotency_key;
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::knowledge_source_registry_project_role as knowledge;
                    use evohime_local_storage::knowledge_source_registry_project_role_store as store;
                    let policy = knowledge::default_policy();
                    match operation.as_str() {
                        "collection_register" => {
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_collection".to_string())?;
                            knowledge::validate_collection(&collection, &policy).map_err(|e| e.to_string())?;
                            for source_id in &collection.source_ids {
                                if store::get_source(database.connection(), source_id).map_err(|_| "storage_failed".to_string())?.is_none() {
                                    return Err("knowledge_source_not_found".into());
                                }
                            }
                            let json = serde_json::to_vec(&collection).map_err(|_| "serialization_failed".to_string())?;
                            if !store::put_collection(database.connection(), &collection.id, collection.version, &collection.content_hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? {
                                return Err("knowledge_collection_stale_version".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"source_count":collection.source_ids.len(),"status":collection.status,"scope":collection.scope,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "collection_get" => {
                            let json = store::get_collection(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_collection_not_found".to_string())?;
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_collection".to_string())?;
                            knowledge::validate_collection(&collection, &policy).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"source_count":collection.source_ids.len(),"status":collection.status,"scope":collection.scope,"content_hash":collection.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "collection_view" => {
                            let request: KnowledgeCollectionViewRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_collection_view".to_string())?;
                            let collection_json = store::get_collection(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_collection_not_found".to_string())?;
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&collection_json).map_err(|_| "corrupt_knowledge_collection".to_string())?;
                            let target_kind = request.target_kind;
                            let target_id = request.target_id.as_str();
                            let sources = collection.source_ids.iter().filter_map(|id| store::get_source(database.connection(), id).ok().flatten()).filter_map(|json| serde_json::from_slice::<knowledge::KnowledgeSource>(&json).ok()).collect::<Vec<_>>();
                            let mut bindings = Vec::new();
                            for id in &collection.source_ids {
                                bindings.extend(store::list_bindings(database.connection(), id, knowledge::MAX_BINDINGS_PER_SOURCE).map_err(|_| "storage_failed".to_string())?.into_iter().filter_map(|json| serde_json::from_slice::<knowledge::KnowledgeBinding>(&json).ok()));
                            }
                            let view = knowledge::build_collection_view(knowledge::BuildCollectionViewInput { collection: &collection, sources: &sources, bindings: &bindings, target_kind, target_id, max_sensitivity: knowledge::Sensitivity::Internal, expires_at_ms: None, policy: &policy }).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"view_id":view.id,"source_ids":view.source_ids,"content_hash":view.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "register" => {
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_source".to_string())?;
                            knowledge::validate_source(&source, &policy).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&source).map_err(|_| "serialization_failed".to_string())?;
                            store::put_source(database.connection(), &source.id, source.version, &source.content_hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source.id,"version":source.version,"kind":source.kind,"status":source.status,"fingerprint":source.source_fingerprint,"sensitivity":source.sensitivity,"content_hash":source.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "get" => {
                            let json = store::get_source(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_source_not_found".to_string())?;
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_source".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source.id,"version":source.version,"kind":source.kind,"status":source.status,"fingerprint":source.source_fingerprint,"sensitivity":source.sensitivity,"content_hash":source.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "bind" => {
                            let binding: knowledge::KnowledgeBinding = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_binding".to_string())?;
                            knowledge::validate_binding(&binding, &policy).map_err(|e| e.to_string())?;
                            if binding.source_id != source_id { return Err("knowledge_source_mismatch".into()); }
                            let binding_id = format!("{}:{}:{}", binding.target_kind as u8, binding.target_id, binding.source_id);
                            let json = serde_json::to_vec(&binding).map_err(|_| "serialization_failed".to_string())?;
                            store::put_binding(database.connection(), &binding_id, &binding.source_id, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":binding.source_id,"target_id":binding.target_id,"bound":true,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "index" => {
                            let chunks: Vec<knowledge::KnowledgeChunk> = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_chunks".to_string())?;
                            if chunks.len() > knowledge::MAX_CHUNKS_PER_SOURCE { return Err("knowledge_chunk_limit".into()); }
                            for chunk in &chunks {
                                if chunk.source_id != source_id || chunk.content_projection.len() > knowledge::MAX_CHUNK_BYTES { return Err("invalid_knowledge_chunk".into()); }
                                let json = serde_json::to_vec(chunk).map_err(|_| "serialization_failed".to_string())?;
                                store::put_chunk(database.connection(), store::PutChunkInput { id: &chunk.id, source_id: &chunk.source_id, revision: chunk.source_revision, ordinal: chunk.ordinal, locator: &chunk.locator, json: &json, now_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source_id,"indexed_chunks":chunks.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "retrieve" => {
                            let request: KnowledgeQueryRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_query".to_string())?;
                            let query = request.query.as_str();
                            if query.is_empty() || query.len() > knowledge::MAX_ID_BYTES { return Err("invalid_knowledge_query".into()); }
                            let target_kind = request.target_kind;
                            let target_id = request.target_id.as_str();
                            let source_json = store::get_source(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_source_not_found".to_string())?;
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&source_json).map_err(|_| "corrupt_knowledge_source".to_string())?;
                            let bindings = store::list_bindings(database.connection(), &source_id, knowledge::MAX_BINDINGS_PER_SOURCE).map_err(|_| "storage_failed".to_string())?.into_iter().filter_map(|json| serde_json::from_slice(&json).ok()).collect::<Vec<knowledge::KnowledgeBinding>>();
                            let view = knowledge::build_view(knowledge::BuildViewInput { id: format!("view-{source_id}"), run_id: "runtime".into(), sources: std::slice::from_ref(&source), bindings: &bindings, target_kind, target_id, max_sensitivity: knowledge::Sensitivity::Internal, retrieval_profile: "keyword".into(), expires_at_ms: None, policy: &policy }).map_err(|e| e.to_string())?;
                            let mut hits = Vec::new();
                            for json in store::list_chunks(database.connection(), &source_id, knowledge::MAX_CHUNKS_PER_SOURCE).map_err(|_| "storage_failed".to_string())? {
                                let chunk: knowledge::KnowledgeChunk = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_chunk".to_string())?;
                                if chunk.content_projection.to_ascii_lowercase().contains(&query.to_ascii_lowercase()) {
                                    let hit = knowledge::KnowledgeHit { source_id: chunk.source_id, source_revision: chunk.source_revision, chunk_id: chunk.id, locator: chunk.locator, excerpt: chunk.content_projection, score: 1, match_reasons: vec!["keyword".into()], freshness: if source.status == knowledge::SourceStatus::Ready { "current".into() } else { "stale".into() }, trust_class: source.trust_class.clone() };
                                    knowledge::validate_hit(&hit, &view, &policy).map_err(|e| e.to_string())?;
                                    hits.push(hit); if hits.len() >= knowledge::MAX_HITS { break; }
                                }
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source_id,"view_id":view.id,"hit_count":hits.len(),"hits":hits,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_knowledge_registry_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::KnowledgeSourceRegistryProjectRole {
                    source_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::DurableRemoteTaskBridge {
                operation,
                remote_task_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_task_id = remote_task_id.clone();
                let result = async {
                    if idempotency_key.is_empty() {
                        return Err("invalid_remote_task_idempotency_key".to_string());
                    }
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::durable_remote_task_bridge as bridge;
                    use evohime_local_storage::durable_remote_task_bridge_store as store;
                    let policy = bridge::default_policy();
                    match operation.as_str() {
                        "submit" => {
                            let request: RemoteTaskSubmitRequest = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_remote_task_submit".to_string())?;
                            let request_bytes = serde_json::to_vec(&request.request)
                                .map_err(|_| "invalid_remote_task_request".to_string())?;
                            let record = bridge::build_record(
                                remote_task_id.clone(),
                                &request.toolset,
                                request.operation,
                                &request_bytes,
                                request.provenance_ref,
                                crate::task_memory::now_millis() as i64,
                                &policy,
                            )
                            .map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&record)
                                .map_err(|_| "serialization_failed".to_string())?;
                            if !store::put_record(
                                database.connection(),
                                &record.id,
                                record.version,
                                &format!("{:?}", record.status),
                                &record.content_hash,
                                &json,
                                record.updated_at_ms,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            {
                                return Err("remote_task_stale_version".into());
                            }
                            serde_json::to_vec(&bridge::status_projection(&record))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "status" => {
                            let json = store::get_record(database.connection(), &remote_task_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "remote_task_not_found".to_string())?;
                            let record: bridge::RemoteTaskRecord = serde_json::from_slice(&json)
                                .map_err(|_| "corrupt_remote_task".to_string())?;
                            serde_json::to_vec(&bridge::status_projection(&record))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "cancel" | "poll" | "result" => {
                            let json = store::get_record(database.connection(), &remote_task_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "remote_task_not_found".to_string())?;
                            let mut record: bridge::RemoteTaskRecord =
                                serde_json::from_slice(&json)
                                    .map_err(|_| "corrupt_remote_task".to_string())?;
                            if expected_version != 0 && expected_version != record.version {
                                return Err("remote_task_stale_version".into());
                            }
                            let now = crate::task_memory::now_millis() as i64;
                            match operation.as_str() {
                                "cancel" => {
                                    let version = record.version;
                                    bridge::cancel(&mut record, version, now)
                                        .map_err(|e| e.to_string())?;
                                }
                                "poll" => {
                                    let request: RemoteTaskPollRequest =
                                        serde_json::from_slice(&payload)
                                            .map_err(|_| "invalid_remote_task_poll".to_string())?;
                                    bridge::lease_for_poll(
                                        &mut record,
                                        &request.lease_owner,
                                        now,
                                        &policy,
                                    )
                                    .map_err(|e| e.to_string())?;
                                }
                                "result" => {
                                    let request: RemoteTaskResultRequest =
                                        serde_json::from_slice(&payload).map_err(|_| {
                                            "invalid_remote_task_result".to_string()
                                        })?;
                                    let status = request.status;
                                    if !matches!(
                                        status,
                                        bridge::RemoteTaskStatus::InputRequired
                                            | bridge::RemoteTaskStatus::Completed
                                            | bridge::RemoteTaskStatus::Failed
                                            | bridge::RemoteTaskStatus::Cancelled
                                            | bridge::RemoteTaskStatus::Unknown
                                    ) {
                                        return Err("invalid_remote_task_transition".into());
                                    }
                                    record.status = status;
                                    record.transport_status = request.transport_status;
                                    record.result_artifact_ref = request.result_artifact_ref;
                                    record.version += 1;
                                    record.updated_at_ms = now;
                                    bridge::validate_record(
                                        &record,
                                        &bridge::RemoteTaskToolset {
                                            schema_version: 1,
                                            id: record.toolset_id.clone(),
                                            version: 1,
                                            provider_kind: bridge::RemoteProviderKind::Mcp,
                                            provider_ref: "trusted-adapter".into(),
                                            operation_names: vec![record.operation.clone()],
                                            content_hash: "trusted".into(),
                                        },
                                        &policy,
                                    )
                                    .map_err(|e| e.to_string())?;
                                }
                                _ => unreachable!(),
                            }
                            bridge::refresh_content_hash(&mut record).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&record)
                                .map_err(|_| "serialization_failed".to_string())?;
                            store::put_record(
                                database.connection(),
                                &record.id,
                                record.version,
                                &format!("{:?}", record.status),
                                &record.content_hash,
                                &json,
                                record.updated_at_ms,
                            )
                            .map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&bridge::status_projection(&record))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_remote_task_operation".into()),
                    }
                }
                .await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::DurableRemoteTaskBridge {
                    remote_task_id: event_task_id,
                    operation: event_operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::MessageInterventionPolicies {
                operation,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let result = async {
                    if idempotency_key.is_empty() || idempotency_key.len() > 128 {
                        return Err("invalid_intervention_idempotency_key".into());
                    }
                    if expected_version > 1 {
                        return Err("intervention_stale_version".into());
                    }
                    let request: InterventionRequest = serde_json::from_slice(&payload)
                        .map_err(|_| "invalid_intervention_payload".to_string())?;
                    let verdict = crate::message_intervention_policies::evaluate(
                        &request.policy,
                        &request.context,
                        request.seen,
                    )
                    .map_err(|e| e.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "status": "evaluated",
                        "operation": operation,
                        "version": 1,
                        "verdict": verdict,
                        "redacted": true,
                    }))
                    .map_err(|_| "serialization_failed".to_string())
                }
                .await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::MessageInterventionPolicies {
                    operation: event_operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::BatchInvocationRuntime {
                operation,
                batch_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_id = batch_id.clone();
                let event_operation = operation.clone();
                let result = async {
                    if idempotency_key.is_empty() {
                        return Err("invalid_batch_idempotency_key".into());
                    }
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::batch_invocation_runtime as batch;
                    use evohime_local_storage::batch_invocation_runtime_store as store;
                    let policy = batch::default_policy();
                    match operation.as_str() {
                        "create" => {
                            let request: batch::CreateBatchRequest =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_batch_payload".to_string())?;
                            let value = batch::new_batch(batch::NewBatchInput {
                                id: batch_id.clone(),
                                definition_ref: request.definition_ref,
                                definition_version: request.definition_version,
                                inputs: request.inputs,
                                max_concurrency: request.max_concurrency,
                                failure_policy: request.failure_policy,
                                now_ms: crate::task_memory::now_millis() as i64,
                                policy: policy.clone(),
                            })
                            .map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&value)
                                .map_err(|_| "serialization_failed".to_string())?;
                            if !store::put(
                                database.connection(),
                                &value.id,
                                value.version,
                                &format!("{:?}", value.status),
                                &value.content_hash,
                                &json,
                                value.updated_at_ms,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            {
                                return Err("batch_duplicate".into());
                            }
                            serde_json::to_vec(&batch::projection(&value))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "get" | "resume" | "start" => {
                            let (version, json) = store::get(database.connection(), &batch_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "batch_not_found".to_string())?;
                            let mut value: batch::BatchInvocation =
                                serde_json::from_slice(&json)
                                    .map_err(|_| "corrupt_batch".to_string())?;
                            if operation == "resume" {
                                batch::resume_pending(
                                    &mut value,
                                    expected_version.max(version),
                                    crate::task_memory::now_millis() as i64,
                                    &policy,
                                )
                                .map_err(|e| e.to_string())?;
                            } else if operation == "start" {
                                batch::start_batch(
                                    &mut value,
                                    expected_version.max(version),
                                    crate::task_memory::now_millis() as i64,
                                    &policy,
                                )
                                .map_err(|e| e.to_string())?;
                            }
                            if operation != "get" {
                                let json = serde_json::to_vec(&value)
                                    .map_err(|_| "serialization_failed".to_string())?;
                                if !store::put(
                                    database.connection(),
                                    &value.id,
                                    value.version,
                                    &format!("{:?}", value.status),
                                    &value.content_hash,
                                    &json,
                                    value.updated_at_ms,
                                )
                                .map_err(|_| "storage_failed".to_string())?
                                {
                                    return Err("batch_stale_version".into());
                                }
                            }
                            serde_json::to_vec(&batch::projection(&value))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "result" => {
                            let (version, json) = store::get(database.connection(), &batch_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "batch_not_found".to_string())?;
                            let mut value: batch::BatchInvocation =
                                serde_json::from_slice(&json)
                                    .map_err(|_| "corrupt_batch".to_string())?;
                            let request: batch::RecordResultRequest =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_batch_result".to_string())?;
                            batch::record_result(batch::RecordResultInput {
                                batch: &mut value,
                                item_id: &request.item_id,
                                expected_version: expected_version.max(version),
                                status: request.status,
                                result_ref: request.result_ref,
                                error_class: request.error_class,
                                now_ms: crate::task_memory::now_millis() as i64,
                                policy: &policy,
                            })
                            .map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&value)
                                .map_err(|_| "serialization_failed".to_string())?;
                            if !store::put(
                                database.connection(),
                                &value.id,
                                value.version,
                                &format!("{:?}", value.status),
                                &value.content_hash,
                                &json,
                                value.updated_at_ms,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            {
                                return Err("batch_stale_version".into());
                            }
                            serde_json::to_vec(&batch::projection(&value))
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_batch_operation".into()),
                    }
                }
                .await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::BatchInvocationRuntime {
                    batch_id: event_id,
                    operation: event_operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ArchitectureSnapshot {
                operation,
                snapshot_id,
                workspace_root,
                payload,
                expected_version: _,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_architecture_snapshot_idempotency_key".into()); }
                    if workspace_root.len() > crate::architecture_snapshot::MAX_ID * 4 { return Err("workspace_root_too_long".into()); }
                    let request: ArchitectureSnapshotRequest = if payload.is_empty() {
                        ArchitectureSnapshotRequest::default()
                    } else {
                        serde_json::from_slice(&payload).map_err(|_| "invalid_architecture_snapshot_payload".to_string())?
                    };
                    let root = request.workspace_root.as_deref().filter(|v| !v.is_empty()).unwrap_or(&workspace_root);
                    let id = if snapshot_id.is_empty() { "architecture-current" } else { snapshot_id.as_str() };
                    match operation.as_str() {
                        "get" | "evidence" | "open_evidence" | "upstream" | "downstream" | "route" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let record = evohime_local_storage::architecture_snapshot_store::get(db.connection(), id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "architecture_snapshot_not_found".to_string())?;
                            let json = record.record_json;
                            let snapshot: crate::architecture_snapshot::ArchitectureSnapshot = serde_json::from_slice(&json).map_err(|_| "corrupt_architecture_snapshot".to_string())?;
                            if operation == "evidence" || operation == "open_evidence" { return serde_json::to_vec(&serde_json::json!({"status":"ok","snapshot_id":id,"evidence":snapshot.components.iter().flat_map(|c| c.evidence.iter()).collect::<Vec<_>>(),"open_mode":operation == "open_evidence","redacted":true})).map_err(|_| "serialization_failed".to_string()); }
                            if operation == "get" { return serde_json::to_vec(&serde_json::json!({"status":"ok","snapshot":snapshot,"redacted":true})).map_err(|_| "serialization_failed".to_string()); }
                            let subject = request.subject_id.as_deref().unwrap_or_default();
                            let ids: Vec<&str> = match operation.as_str() {
                                "upstream" => snapshot.relationships.iter().filter(|r| r.to == subject).map(|r| r.from.as_str()).take(64).collect(),
                                "downstream" => snapshot.relationships.iter().filter(|r| r.from == subject).map(|r| r.to.as_str()).take(64).collect(),
                                _ => snapshot.relationships.iter().filter(|r| r.from == subject || r.to == subject).flat_map(|r| [r.from.as_str(), r.to.as_str()]).take(64).collect(),
                            };
                            serde_json::to_vec(&serde_json::json!({"status":"ok","operation":operation,"subject_id":subject,"related_ids":ids,"route_is_not_impact":true,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "refresh" | "rebuild" | "current" => {
                            let revision = request.source_revision.as_deref().unwrap_or("working-tree");
                            let root_path = std::path::Path::new(root);
                            let allowed_roots = if request.allowed_roots.is_empty() {
                                vec![root.to_owned()]
                            } else {
                                request.allowed_roots.clone()
                            };
                            crate::architecture_snapshot_runtime::authorize_root(root_path, &allowed_roots).map_err(|e| e.to_string())?;
                            let workspace_identity = crate::architecture_snapshot_runtime::source_fingerprint(root_path, revision);
                            let snapshot = crate::architecture_snapshot_runtime::extract(root_path, &workspace_identity, revision, id).map_err(|e| e.to_string())?;
                            let hash = crate::architecture_snapshot::snapshot_hash(&snapshot).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&snapshot).map_err(|_| "serialization_failed".to_string())?;
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            evohime_local_storage::architecture_snapshot_store::set_refresh_state(db.connection(), id, "accepted", None, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            evohime_local_storage::architecture_snapshot_store::put(db.connection(), evohime_local_storage::architecture_snapshot_store::PutInput { snapshot_id: id, workspace_identity: &workspace_identity, source_revision: revision, snapshot_hash: &hash, state: "accepted", record_json: &json, updated_at_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"accepted","snapshot_id":id,"snapshot_hash":hash,"projection":snapshot,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "compare" => {
                            let before = request.before.ok_or_else(|| "before_required".to_string())?;
                            let after = request.after.ok_or_else(|| "after_required".to_string())?;
                            let delta = crate::architecture_snapshot::delta(&before, &after).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"compared","delta":delta,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "review" => {
                            let expected = request.expected.ok_or_else(|| "expected_required".to_string())?;
                            let actual = request.actual.ok_or_else(|| "actual_required".to_string())?;
                            let result = crate::architecture_snapshot::review(&expected, &actual).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"reviewed","review":result,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "inspect" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                            let db = journal.database().lock().await;
                            let identity = crate::architecture_snapshot_runtime::source_fingerprint(std::path::Path::new(root), "working-tree");
                            let records = evohime_local_storage::architecture_snapshot_store::list(db.connection(), &identity, 64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"ok","snapshots":records,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_architecture_snapshot_operation".into()),
                    }
                }.await;
                let projection_json = match String::from_utf8(result.clone().unwrap_or_default()) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(%error, "architecture snapshot projection is not UTF-8");
                        "{}".into()
                    }
                };
                let event = CoreEvent::ArchitectureSnapshot {
                    snapshot_id,
                    operation,
                    version: 1,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::LocalModelRuntimeManager {
                operation,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_local_model_manager_idempotency_key".into()); }
                    let request: LocalModelRuntimeRequest = if payload.is_empty() {
                        LocalModelRuntimeRequest::default()
                    } else {
                        serde_json::from_slice(&payload).map_err(|_| "invalid_local_model_manager_payload".to_string())?
                    };
                    let value = serde_json::to_value(&request).map_err(|_| "invalid_local_model_manager_payload".to_string())?;
                    match operation.as_str() {
                        "hardware" => {
                            let profile = crate::local_model_runtime_manager::discover_hardware().map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"discovered","hardware":profile,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "start" => {
                            let model_id = request.model_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "model_id_required".to_string())?;
                            let request_id = request.request_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "request_id_required".to_string())?;
                            #[cfg(windows)]
                            { let response = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"launch","model_id":model_id,"request_id":request_id})).await.map_err(|_| "runtime_unavailable".to_string())?; if response.get("accepted") != Some(&serde_json::Value::Bool(true)) { return Err("runtime_unavailable".into()); } let health = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"probe","model_id":model_id})).await.map_err(|_| "health_gate_unavailable".to_string())?; if health.get("healthy") != Some(&serde_json::Value::Bool(true)) { let _ = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"stop","model_id":model_id,"request_id":request_id})).await; return Err("health_gate_failed".into()); } serde_json::to_vec(&serde_json::json!({"status":"ready","model_id":model_id,"supervised":true,"health_gate":"passed","redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                            #[cfg(not(windows))]
                            { let _ = (model_id, request_id); Err("runtime_unavailable".into()) }
                        }
                        "stop" => {
                            let model_id = request.model_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "model_id_required".to_string())?;
                            let request_id = request.request_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "request_id_required".to_string())?;
                            #[cfg(windows)]
                            { let response = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"stop","model_id":model_id,"request_id":request_id})).await.map_err(|_| "runtime_unavailable".to_string())?; if response.get("accepted") != Some(&serde_json::Value::Bool(true)) { return Err("runtime_unavailable".into()); } serde_json::to_vec(&serde_json::json!({"status":"stopped","model_id":model_id,"supervised":true,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                            #[cfg(not(windows))]
                            { let _ = (model_id, request_id); Err("runtime_unavailable".into()) }
                        }
                        "probe" => {
                            let model_id = request.model_id.as_deref().filter(|v| !v.is_empty()).ok_or_else(|| "model_id_required".to_string())?;
                            #[cfg(windows)]
                            { let response = crate::analysis_kernel::supervisor_command(serde_json::json!({"op":"probe","model_id":model_id})).await.map_err(|_| "health_gate_unavailable".to_string())?; if response.get("healthy") != Some(&serde_json::Value::Bool(true)) { return Err("health_gate_failed".into()); } serde_json::to_vec(&serde_json::json!({"status":"ready","model_id":model_id,"health_gate":"passed","redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                            #[cfg(not(windows))]
                            { let _ = model_id; Err("runtime_unavailable".into()) }
                        }
                        "verify_artifact" => {
                            let state = request.state.ok_or_else(|| "state_required".to_string())?;
                            let trust = request.trust.ok_or_else(|| "trust_required".to_string())?;
                            let observed = request.observed_hash.as_deref().ok_or_else(|| "observed_hash_required".to_string())?;
                            let expected = request.expected_hash.as_deref().ok_or_else(|| "expected_hash_required".to_string())?;
                            crate::local_model_runtime_manager::allow_artifact_promotion(state, trust, observed, expected).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"verified","artifact_state":"installed","content_hash":expected,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "promote_artifact" => {
                            let staging = request.staging_relative_path.as_deref().ok_or_else(|| "staging_path_required".to_string())?;
                            let destination = request.destination_relative_path.as_deref().ok_or_else(|| "destination_path_required".to_string())?;
                            let staging = std::path::Path::new(staging);
                            let destination = std::path::Path::new(destination);
                            crate::local_model_runtime_manager::validate_artifact_relative_path(staging).map_err(|e| e.to_string())?;
                            crate::local_model_runtime_manager::validate_artifact_relative_path(destination).map_err(|e| e.to_string())?;
                            let expected = value.get("expected_hash").and_then(serde_json::Value::as_str).ok_or_else(|| "expected_hash_required".to_string())?;
                            let expected_size = value.get("expected_size_bytes").and_then(serde_json::Value::as_u64).ok_or_else(|| "expected_size_required".to_string())?;
                            let root = crate::get_data_directory();
                            let models_root = root.join("models");
                            std::fs::create_dir_all(&models_root).map_err(|_| "artifact_root_unavailable".to_string())?;
                            let staging_path = crate::local_model_runtime_manager::managed_artifact_path(&models_root, staging).map_err(|e| e.to_string())?;
                            let destination_path = crate::local_model_runtime_manager::managed_artifact_path(&models_root, destination).map_err(|e| e.to_string())?;
                            crate::local_model_runtime_manager::atomic_promote_verified_artifact(&staging_path, &destination_path, expected, expected_size).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"installed","artifact_state":"installed","content_hash":expected,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "transition" => {
                            let from: crate::local_model_runtime_manager::ArtifactState = serde_json::from_value(value.get("from").cloned().ok_or_else(|| "from_required".to_string())?).map_err(|_| "invalid_from_state".to_string())?;
                            let to: crate::local_model_runtime_manager::ArtifactState = serde_json::from_value(value.get("to").cloned().ok_or_else(|| "to_required".to_string())?).map_err(|_| "invalid_to_state".to_string())?;
                            crate::local_model_runtime_manager::allow_transition(from, to).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"transitioned","from":from,"to":to,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "profile" => {
                            let session: crate::local_model_runtime_manager::LocalModelRuntimeSession = serde_json::from_value(value.get("session").cloned().ok_or_else(|| "session_required".to_string())?).map_err(|_| "invalid_session".to_string())?;
                            let descriptor: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_value(value.get("model").cloned().ok_or_else(|| "model_required".to_string())?).map_err(|_| "invalid_model".to_string())?;
                            let runtime: crate::local_model_runtime_manager::LocalInferenceRuntime = serde_json::from_value(value.get("runtime").cloned().ok_or_else(|| "runtime_required".to_string())?).map_err(|_| "invalid_runtime".to_string())?;
                            let profile = crate::local_model_runtime_manager::managed_profile(&session, &descriptor, &runtime).map_err(|e| e.to_string())?;
                            let resilience = crate::local_model_runtime_manager::resilience_profile_ref(&profile);
                            serde_json::to_vec(&serde_json::json!({"profile": profile, "resilience_profile": resilience, "redacted": true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "register_model" | "register_runtime" | "register_artifact" | "register_session" => {
                            let (kind, record_id, revision, record_json) = match operation.as_str() {
                                "register_model" => { let record: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_value(value.get("model").cloned().ok_or_else(|| "model_required".to_string())?).map_err(|_| "invalid_model".to_string())?; record.validate().map_err(|e| e.to_string())?; let id = format!("model:{}:{}", record.model_id, record.revision); ("model", id, record.revision, serde_json::to_vec(&record).map_err(|_| "serialization_failed".to_string())?) }
                                "register_runtime" => { let record: crate::local_model_runtime_manager::LocalInferenceRuntime = serde_json::from_value(value.get("runtime").cloned().ok_or_else(|| "runtime_required".to_string())?).map_err(|_| "invalid_runtime".to_string())?; record.validate().map_err(|e| e.to_string())?; let id = format!("runtime:{}:{}", record.runtime_id, record.revision); ("runtime", id, record.revision, serde_json::to_vec(&record).map_err(|_| "serialization_failed".to_string())?) }
                                "register_artifact" => { let record: crate::local_model_runtime_manager::LocalArtifactRecord = serde_json::from_value(value.get("artifact").cloned().ok_or_else(|| "artifact_required".to_string())?).map_err(|_| "invalid_artifact".to_string())?; record.validate().map_err(|e| e.to_string())?; let id = format!("artifact:{}:{}", record.model_id, record.model_revision); ("artifact", id, record.model_revision, serde_json::to_vec(&record).map_err(|_| "serialization_failed".to_string())?) }
                                _ => { let record: crate::local_model_runtime_manager::LocalModelRuntimeSession = serde_json::from_value(value.get("session").cloned().ok_or_else(|| "session_required".to_string())?).map_err(|_| "invalid_session".to_string())?; record.validate().map_err(|e| e.to_string())?; let id = format!("session:{}", record.session_id); ("session", id, expected_version.max(1), serde_json::to_vec(&record).map_err(|_| "serialization_failed".to_string())?) }
                            };
                            let hash = crate::local_model_runtime_manager::canonical_hash(&record_json);
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                            if !evohime_local_storage::local_model_runtime_manager_store::put_record(db.connection(), &record_id, kind, revision, &hash, &record_json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? { return Err("stale_local_model_manager_record".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"registered","record_id":record_id,"record_kind":kind,"revision":revision,"content_hash":hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "recover" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                            let artifacts = evohime_local_storage::local_model_runtime_manager_store::list_records(db.connection(), "artifact", 256).map_err(|_| "storage_failed".to_string())?;
                            let runtimes = evohime_local_storage::local_model_runtime_manager_store::list_records(db.connection(), "runtime", 256).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"reconciled","artifacts":artifacts.len(),"runtimes":runtimes.len(),"ready_requires_fresh_probe":true,"orphan_processes":"unavailable_until_identity_probe","redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "download_artifact" => {
                            let url = value.get("url").and_then(serde_json::Value::as_str).ok_or_else(|| "artifact_url_required".to_string())?;
                            let staging = value.get("staging_relative_path").and_then(serde_json::Value::as_str).ok_or_else(|| "staging_path_required".to_string())?;
                            let expected = value.get("expected_hash").and_then(serde_json::Value::as_str).ok_or_else(|| "expected_hash_required".to_string())?;
                            let expected_size = value.get("expected_size_bytes").and_then(serde_json::Value::as_u64).ok_or_else(|| "expected_size_required".to_string())?;
                            let relative = std::path::Path::new(staging);
                            crate::local_model_runtime_manager::validate_artifact_relative_path(relative).map_err(|e| e.to_string())?;
                            let root = crate::get_data_directory();
                            let models_root = root.join("models");
                            std::fs::create_dir_all(&models_root).map_err(|_| "artifact_root_unavailable".to_string())?;
                            let staging_path = crate::local_model_runtime_manager::managed_artifact_path(&models_root, relative).map_err(|e| e.to_string())?;
                            crate::local_model_runtime_manager::download_verified_artifact(url, &staging_path, expected, expected_size, &tokio_util::sync::CancellationToken::new()).await.map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"verified_staging","artifact_state":"verifying","staging_relative_path":staging,"expected_size_bytes":expected_size,"content_hash":expected,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "inspect" => serde_json::to_vec(&serde_json::json!({"schema_version":1,"contract_id":crate::local_model_runtime_manager::CONTRACT_ID,"status":"metadata_only","runtime_execution":"supervisor_boundary_required","artifact_download":"not_started","redacted":true})).map_err(|_| "serialization_failed".to_string()),
                        "fit" => {
                            let hardware: crate::local_model_runtime_manager::LocalHardwareProfile = serde_json::from_value(value.get("hardware").cloned().ok_or_else(|| "hardware_required".to_string())?).map_err(|_| "invalid_hardware".to_string())?;
                            let model: crate::local_model_runtime_manager::LocalModelDescriptor = serde_json::from_value(value.get("model").cloned().ok_or_else(|| "model_required".to_string())?).map_err(|_| "invalid_model".to_string())?;
                            let fit = crate::local_model_runtime_manager::compute_fit(&hardware, &model).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&fit).map_err(|_| "serialization_failed".to_string())
                        }
                        "get_policy" => {
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                            let stored = evohime_local_storage::local_model_runtime_manager_store::get(db.connection(), crate::local_model_runtime_manager::CONTRACT_ID).map_err(|_| "storage_failed".to_string())?;
                            if let Some((version, hash, json)) = stored { return serde_json::to_vec(&serde_json::json!({"status":"loaded","version":version,"content_hash":hash,"policy":serde_json::from_slice::<serde_json::Value>(&json).unwrap_or(serde_json::json!({})),"redacted":true})).map_err(|_| "serialization_failed".to_string()); }
                            serde_json::to_vec(&serde_json::json!({"status":"missing","version":0,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "save_policy" => {
                            let policy: crate::local_model_runtime_manager::LocalModelManagerPolicy = serde_json::from_value(value.get("policy").cloned().ok_or_else(|| "policy_required".to_string())?).map_err(|_| "invalid_policy".to_string())?;
                            policy.validate().map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&policy).map_err(|_| "serialization_failed".to_string())?; let hash = crate::local_model_runtime_manager::canonical_hash(&policy);
                            let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                            if !evohime_local_storage::local_model_runtime_manager_store::put(db.connection(), &policy.policy_id, policy.version, &hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? { return Err("stale_local_model_manager_policy".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","policy_id":policy.policy_id,"version":policy.version,"content_hash":hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_local_model_manager_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let version = serde_json::from_str::<serde_json::Value>(&projection_json)
                    .ok()
                    .and_then(|v| v.get("version").and_then(serde_json::Value::as_u64))
                    .unwrap_or(expected_version);
                let event = CoreEvent::LocalModelRuntimeManager {
                    operation: operation.clone(),
                    version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ModelPurposeRouting {
                operation,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_model_purpose_idempotency_key".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use evohime_local_storage::model_purpose_routing_store as store;
                    match operation.as_str() {
                        "get" => {
                            let stored = store::get(database.connection(), crate::model_purpose_routing::CONTRACT_ID).map_err(|_| "storage_failed".to_string())?;
                            let (version, hash, policy) = stored
                                .and_then(|(version, hash, json)| serde_json::from_slice(&json).ok().map(|policy| (version, hash, policy)))
                                .unwrap_or_else(|| { let policy = crate::model_purpose_routing::builtin_policy(); let hash = policy.canonical_hash().unwrap_or_default(); (policy.version, hash, policy) });
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"policy_id":crate::model_purpose_routing::CONTRACT_ID,"version":version,"content_hash":hash,"routes":policy.routes,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "put" => {
                            let policy: crate::model_purpose_routing::ModelPurposeRoutingPolicy = serde_json::from_slice(&payload).map_err(|_| "invalid_model_purpose_policy".to_string())?;
                            policy.validate().map_err(|e| e.to_string())?;
                            if policy.version != expected_version.saturating_add(1) && expected_version != 0 { return Err("stale_model_purpose_policy".into()); }
                            let json = serde_json::to_vec(&policy).map_err(|_| "serialization_failed".to_string())?;
                            let hash = policy.canonical_hash().map_err(|e| e.to_string())?;
                            if !store::put(database.connection(), &policy.policy_id, policy.version, &hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? { return Err("stale_model_purpose_policy".into()); }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"policy_id":policy.policy_id,"version":policy.version,"content_hash":hash,"route_count":policy.routes.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_model_purpose_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event_version = serde_json::from_str::<serde_json::Value>(&projection_json)
                    .ok()
                    .and_then(|v| v.get("version").and_then(serde_json::Value::as_u64))
                    .unwrap_or(expected_version);
                let event = CoreEvent::ModelPurposeRouting {
                    operation: operation.clone(),
                    version: event_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::CodeAnchoredIntentMarkers {
                operation,
                file_path,
                revision,
                payload,
                idempotency_key,
                reply,
            } => {
                let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_marker_idempotency_key".into()); }
                    let ranges: Vec<crate::code_anchored_intent_markers::CommentRange> = serde_json::from_slice(&payload).map_err(|_| "invalid_comment_ranges".to_string())?;
                    if operation != "scan" && operation != "propose" { return Err("unsupported_marker_operation".into()); }
                    let provenance = if operation == "scan" { crate::code_anchored_intent_markers::Provenance::ExistingRepository } else { crate::code_anchored_intent_markers::Provenance::UserTrusted };
                    let mut markers = crate::code_anchored_intent_markers::parse_comment_ranges(&file_path, &revision, &ranges, provenance).map_err(|e| e.to_string())?;
                    crate::code_anchored_intent_markers::deduplicate(&mut markers);
                    if operation == "scan" {
                        state.lock().await.marker_gate.admit_scan(&mut markers, crate::task_memory::now_millis());
                    }
                    if operation == "propose" {
                        let marker = markers.first_mut().ok_or_else(|| "marker_not_found".to_string())?;
                        crate::code_anchored_intent_markers::can_auto_propose(marker).map_err(|e| e.to_string())?;
                        let task_id = format!("code-intent-{}", marker.marker_id);
                        let prompt = format!("Code intent at {}:{}-{}: {}", marker.file_path, marker.range_start, marker.range_end, marker.text);
                        marker.status = crate::code_anchored_intent_markers::MarkerStatus::Proposed;
                        let command_tx = state.lock().await.command_tx.clone();
                        command_tx.send(CoreCommand::StartTask { task_id: task_id.clone(), prompt, workspace_root: None, preferred_route_hint: None }).await.map_err(|_| "task_queue_closed".to_string())?;
                        return serde_json::to_vec(&serde_json::json!({"status":"task_started","task_id":task_id,"marker_id":marker.marker_id,"redacted":true})).map_err(|_| "serialization_failed".to_string());
                    }
                    serde_json::to_vec(&serde_json::json!({"status":"candidates","count":markers.len(),"markers":markers.iter().map(|m|serde_json::json!({"marker_id":m.marker_id,"kind":m.kind,"range_start":m.range_start,"range_end":m.range_end,"revision":m.revision,"provenance":m.provenance})).collect::<Vec<_>>(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::CodeAnchoredIntentMarkers {
                    operation: operation.clone(),
                    version: 1,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::AgentGitChangeSets {
                operation,
                change_set_id,
                payload,
                expected_version,
                idempotency_key: _,
                reply,
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::agent_git_change_sets as git_sets;
                    use evohime_local_storage::agent_git_change_sets_store as store;
                    match operation.as_str() {
                        "observe" => {
                            let set: git_sets::AgentGitChangeSet = serde_json::from_slice(&payload).map_err(|_| "invalid_agent_git_change_set".to_string())?;
                            git_sets::validate_change_set(&set).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&set).map_err(|_| "serialization_failed".to_string())?;
                            store::put_change_set(database.connection(), &set.id, set.version, &set.content_hash, &json, set.created_at_ms).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"change_set_id":set.id,"status":"observed","path_count":set.paths.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "candidate" => {
                            let json = store::get_change_set(database.connection(), &change_set_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "change_set_not_found".to_string())?;
                            let set: git_sets::AgentGitChangeSet = serde_json::from_slice(&json).map_err(|_| "corrupt_agent_git_change_set".to_string())?;
                            if expected_version != 0 && expected_version != set.version as u64 { return Err("agent_git_change_set_stale_version".into()); }
                            let message = serde_json::from_slice::<serde_json::Value>(&payload).ok().and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_owned)).unwrap_or_else(|| "Agent change set".into());
                            let candidate = git_sets::build_candidate(&set, message, crate::task_memory::now_millis() as i64).map_err(|e| e.to_string())?;
                            let candidate_json = serde_json::to_vec(&candidate).map_err(|_| "serialization_failed".to_string())?;
                            store::put_candidate(database.connection(), &candidate.id, &set.id, &candidate.diff_hash, &candidate_json, candidate.created_at_ms).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"candidate_id":candidate.id,"change_set_id":set.id,"included_paths":candidate.included_paths,"excluded_paths":candidate.excluded_paths,"diff_hash":candidate.diff_hash,"verification_status":candidate.verification_status,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "get_candidate" => {
                            let json = store::get_candidate(database.connection(), &change_set_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "candidate_not_found".to_string())?;
                            let candidate: git_sets::GitCommitCandidate = serde_json::from_slice(&json).map_err(|_| "corrupt_agent_git_candidate".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"candidate_id":candidate.id,"change_set_id":candidate.change_set_ref,"included_paths":candidate.included_paths,"excluded_paths":candidate.excluded_paths,"diff_hash":candidate.diff_hash,"proposed_message":candidate.proposed_message,"verification_status":candidate.verification_status,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "keep" | "undo" | "commit" => Err("agent_git_effect_requires_explicit_git_preflight".into()),
                        _ => Err("unsupported_agent_git_change_sets_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::AgentGitChangeSets {
                    change_set_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::PolicyAwareToolResultCache {
                operation,
                cache_key,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_key = cache_key.clone();
                let event_operation = operation.clone();
                let result = async {
                    if idempotency_key.is_empty() { return Err("invalid_cache_idempotency_key".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::policy_aware_tool_result_cache as cache;
                    use evohime_local_storage::policy_aware_tool_result_cache_store as store;
                    let policy = cache::default_policy();
                    match operation.as_str() {
                        "inspect" => serde_json::to_vec(&serde_json::json!({"status":"available","cache_key":cache_key,"default_cacheability":"never","max_entries":policy.max_entries,"redacted":true})).map_err(|_| "serialization_failed".to_string()),
                        "put" => { let entry: cache::CacheEntry = serde_json::from_slice(&payload).map_err(|_| "invalid_cache_entry".to_string())?; cache::validate_entry(&entry, &policy, crate::task_memory::now_millis() as i64, cache::Freshness::UseCache).map_err(|e| e.to_string())?; let json=serde_json::to_vec(&entry).map_err(|_| "serialization_failed".to_string())?; if !store::put(database.connection(), &cache_key, expected_version.max(1), &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? { return Err("cache_stale_version".into()); } serde_json::to_vec(&serde_json::json!({"status":"stored","cache_key":cache_key,"redacted":true})).map_err(|_| "serialization_failed".to_string()) },
                        "get" => { let hit=store::get(database.connection(), &cache_key).map_err(|_| "storage_failed".to_string())?.and_then(|(_,json)|serde_json::from_slice::<cache::CacheEntry>(&json).ok()).and_then(|entry|cache::validate_entry(&entry,&policy,crate::task_memory::now_millis() as i64,cache::Freshness::UseCache).ok().map(|_|entry)); serde_json::to_vec(&serde_json::json!({"status":if hit.is_some(){"hit"}else{"miss"},"cache_key":cache_key,"provenance_ref":hit.map(|e|e.provenance_ref),"redacted":true})).map_err(|_| "serialization_failed".to_string()) },
                        "invalidate" => { if let Some((version,json))=store::get(database.connection(), &cache_key).map_err(|_| "storage_failed".to_string())? { let mut entry:cache::CacheEntry=serde_json::from_slice(&json).map_err(|_|"corrupt_cache".to_string())?; entry.status=cache::CacheStatus::Invalidated; let json=serde_json::to_vec(&entry).map_err(|_|"serialization_failed".to_string())?; if !store::put(database.connection(),&cache_key,version+1,&json,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())? {return Err("cache_stale_version".into())}; } serde_json::to_vec(&serde_json::json!({"status":"invalidated","cache_key":cache_key,"redacted":true})).map_err(|_|"serialization_failed".to_string()) },
                        _ => Err("unsupported_cache_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::PolicyAwareToolResultCache {
                    cache_key: event_key,
                    operation: event_operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ArchitectEditorModelPipeline {
                operation,
                pipeline_id,
                payload,
                expected_version,
                idempotency_key: _,
                reply,
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::architect_editor_model_pipeline as pipeline;
                    use evohime_local_storage::architect_editor_model_pipeline_store as store;
                    match operation.as_str() {
                        "create" => { let p: pipeline::ModelPhasePipeline = serde_json::from_slice(&payload).map_err(|_| "invalid_pipeline".to_string())?; pipeline::validate_pipeline(&p).map_err(|e| e.to_string())?; let json=serde_json::to_vec(&p).map_err(|_| "serialization_failed".to_string())?; store::put(database.connection(),&p.id,p.schema_version,&p.content_hash,&json,crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"pipeline_id":p.id,"status":p.status,"same_model":p.same_model,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        "accept_intent" => { let json=store::get(database.connection(),&pipeline_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "pipeline_not_found".to_string())?; let mut p:pipeline::ModelPhasePipeline=serde_json::from_slice(&json).map_err(|_| "corrupt_pipeline".to_string())?; let req:AcceptIntentRequest=serde_json::from_slice(&payload).map_err(|_| "invalid_intent".to_string())?; pipeline::accept_intent(&mut p,req.intent,&req.workspace_revision).map_err(|e| e.to_string())?; if expected_version!=0 && expected_version!=1 {return Err("pipeline_stale_version".into())}; serde_json::to_vec(&serde_json::json!({"schema_version":1,"pipeline_id":p.id,"status":p.status,"intent_ready":true,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        "get" => { let json=store::get(database.connection(),&pipeline_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "pipeline_not_found".to_string())?; let p:pipeline::ModelPhasePipeline=serde_json::from_slice(&json).map_err(|_| "corrupt_pipeline".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"pipeline_id":p.id,"status":p.status,"workspace_revision":p.workspace_revision,"same_model":p.same_model,"intent_present":p.intent.is_some(),"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        _ => Err("unsupported_architect_editor_operation".into())
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::ArchitectEditorModelPipeline {
                    pipeline_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::EventVisualizerRegistry {
                operation,
                visualizer_id,
                payload,
                expected_version,
                idempotency_key: _,
                reply,
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::event_visualizer_registry as registry;
                    use evohime_local_storage::event_visualizer_registry_store as store;
                    match operation.as_str() {
                        "list" => { let mut descriptors = registry::builtins(); let rows = store::list(database.connection()).map_err(|_| "storage_failed".to_string())?; for row in rows { if let Ok(d) = serde_json::from_slice(&row) { descriptors.push(d); } } serde_json::to_vec(&serde_json::json!({"schema_version":1,"descriptors":descriptors,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        "register" => { let d: registry::VisualizerDescriptor = serde_json::from_slice(&payload).map_err(|_| "invalid_visualizer_descriptor".to_string())?; registry::validate_descriptor(&d).map_err(|e| e.to_string())?; let json=serde_json::to_vec(&d).map_err(|_| "serialization_failed".to_string())?; store::put(database.connection(),&d.id,d.version,&d.content_hash,&json,crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"visualizer_id":d.id,"status":"registered","redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        "resolve" => { let matcher: registry::VisualizerMatcher = serde_json::from_slice(&payload).map_err(|_| "invalid_visualizer_matcher".to_string())?; let mut descriptors=registry::builtins(); let rows=store::list(database.connection()).map_err(|_| "storage_failed".to_string())?; for row in rows { if let Ok(d)=serde_json::from_slice(&row) { descriptors.push(d); } } let resolution=registry::resolve(&descriptors,&matcher).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"resolution":resolution,"redacted":true})).map_err(|_| "serialization_failed".to_string()) }
                        _ => Err("unsupported_event_visualizer_registry_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::EventVisualizerRegistry {
                    visualizer_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ReasoningOperatorLibrary {
                operation,
                operator_id,
                payload,
                expected_version,
                idempotency_key: _,
                reply,
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::reasoning_operator_library as operators;
                    use evohime_local_storage::reasoning_operator_library_store as store;
                    match operation.as_str() {
                        "list" => { let mut defs=operators::builtins(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? { if let Ok(d)=serde_json::from_slice(&row){defs.push(d)} } serde_json::to_vec(&serde_json::json!({"schema_version":1,"operators":defs,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "register" => { let d:operators::ReasoningOperatorDefinition=serde_json::from_slice(&payload).map_err(|_|"invalid_operator_definition".to_string())?; operators::validate(&d).map_err(|e|e.to_string())?; let j=serde_json::to_vec(&d).map_err(|_|"serialization_failed".to_string())?; store::put(database.connection(),&d.id,d.version,&d.content_hash,&j,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"operator_id":d.id,"status":"registered","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "execute" => { let req:operators::OperatorRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_operator_request".to_string())?; operators::validate_request(&req).map_err(|e|e.to_string())?; if req.operator_id!=operator_id{return Err("operator_id_mismatch".into())}; if expected_version>3{return Err("operator_stale_version".into())}; serde_json::to_vec(&serde_json::json!({"schema_version":1,"operator_id":operator_id,"status":"proposed","output_contract":"typed_json","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        _=>Err("unsupported_reasoning_operator_operation".into())
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::ReasoningOperatorLibrary {
                    operator_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::OutputGuardrailPipeline {
                operation,
                pipeline_id,
                payload,
                expected_version,
                idempotency_key: _,
                reply,
            } => {
                let result = async { if operation != "evaluate" { return Err("unsupported_output_guardrail_operation".into()); } let p: crate::output_guardrail_pipeline::GuardrailPipeline = serde_json::from_slice(&payload).map_err(|_| "invalid_guardrail_pipeline".to_string())?; let r=crate::output_guardrail_pipeline::evaluate(&p, &payload).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"pipeline_id":pipeline_id,"version":expected_version,"result":r,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::OutputGuardrailPipeline {
                    pipeline_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::CustomizationInventory {
                operation,
                item_id,
                payload,
                expected_version,
                reply,
                ..
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::customization_inventory as inventory;
                    use evohime_local_storage::customization_inventory_store as store;
                    match operation.as_str() {
                        "list" => {
                            let mut items = Vec::new();
                            for row in store::list(database.connection()).map_err(|_| "storage_failed".to_string())? {
                                if let Ok(item) = serde_json::from_slice(&row) { items.push(item); }
                            }
                            inventory::sort(&mut items).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"items":items,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "register" => {
                            let item: inventory::CustomizationItem = serde_json::from_slice(&payload).map_err(|_| "invalid_customization_item".to_string())?;
                            inventory::validate(&item).map_err(|e| e.to_string())?;
                            if !item_id.is_empty() && item.id != item_id { return Err("item_id_mismatch".into()); }
                            let data = serde_json::to_vec(&item).map_err(|_| "serialization_failed".to_string())?;
                            store::put(database.connection(), &item.id, &format!("{:?}", item.kind), item.version, &data, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"item_id":item.id,"version":item.version,"status":"registered","redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "remove" => {
                            if expected_version == 0 { return Err("expected_version_required".into()); }
                            database.connection().execute("DELETE FROM customization_inventory WHERE id=?1 AND version=?2", rusqlite::params![item_id, expected_version]).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"item_id":item_id,"version":expected_version,"status":"removed","redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_customization_inventory_operation".into())
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::CustomizationInventory {
                    item_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::StandingApprovalProfiles {
                operation,
                profile_id,
                payload,
                expected_version,
                reply,
                ..
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::standing_approval_profiles as profiles;
                    use evohime_local_storage::standing_approval_profiles_store as store;
                    match operation.as_str() {
                        "list" => { let mut values:Vec<profiles::StandingApprovalProfile>=Vec::new(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? { if let Ok(p)=serde_json::from_slice(&row){values.push(p)} } serde_json::to_vec(&serde_json::json!({"schema_version":1,"profiles":values,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "create"|"update" => { let p:profiles::StandingApprovalProfile=serde_json::from_slice(&payload).map_err(|_|"invalid_profile".to_string())?; profiles::validate(&p).map_err(|e|e.to_string())?; if !profile_id.is_empty()&&p.id!=profile_id{return Err("profile_id_mismatch".into())}; let j=serde_json::to_vec(&p).map_err(|_|"serialization_failed".to_string())?; store::put(database.connection(),&p.id,p.version,p.enabled,&j,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":p.id,"version":p.version,"status":"saved","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "revoke" => { if expected_version==0{return Err("expected_version_required".into())}; database.connection().execute("UPDATE standing_approval_profiles SET enabled=0, version=version+1 WHERE id=?1 AND version=?2",rusqlite::params![profile_id,expected_version]).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":profile_id,"version":expected_version+1,"status":"revoked","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "match" => { let req:profiles::ApprovalRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_approval_request".to_string())?; let mut decisions=Vec::new(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? { if let Ok(p)=serde_json::from_slice(&row){ if let Ok(d)=profiles::match_request(&p,&req){decisions.push(d)} } } let approved=decisions.iter().find(|d|d.approved).cloned(); serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":approved.as_ref().and_then(|d|d.profile_id.clone()),"approved":approved.is_some(),"reason":approved.map(|d|d.reason).unwrap_or_else(||"no_match".into()),"execution_policy_required":true,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        _=>Err("unsupported_standing_approval_operation".into())
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::StandingApprovalProfiles {
                    profile_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ApprovalPolicyProfiles {
                operation,
                profile_id,
                payload,
                expected_version,
                reply,
                ..
            } => {
                let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::approval_policy_profiles as policy;
                    use evohime_local_storage::approval_policy_profiles_store as store;
                    match operation.as_str() {
                        "list" => { let mut values:Vec<policy::ApprovalPolicyProfile>=Vec::new(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? { if let Ok(p)=serde_json::from_slice(&row){values.push(p)} } serde_json::to_vec(&serde_json::json!({"schema_version":1,"profiles":values,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "create"|"update" => { let p:policy::ApprovalPolicyProfile=serde_json::from_slice(&payload).map_err(|_|"invalid_policy_profile".to_string())?; policy::validate(&p).map_err(|e|e.to_string())?; if !profile_id.is_empty()&&p.id!=profile_id{return Err("profile_id_mismatch".into())}; let j=serde_json::to_vec(&p).map_err(|_|"serialization_failed".to_string())?; store::put(database.connection(),&p.id,p.version,p.enabled,&j,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":p.id,"version":p.version,"status":"saved","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "revoke" => { if expected_version==0{return Err("expected_version_required".into())}; database.connection().execute("UPDATE approval_policy_profiles SET enabled=0, version=version+1 WHERE id=?1 AND version=?2",rusqlite::params![profile_id,expected_version]).map_err(|_|"storage_failed".to_string())?; serde_json::to_vec(&serde_json::json!({"schema_version":1,"profile_id":profile_id,"version":expected_version+1,"status":"revoked","redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        "decide" => { let req:PolicyDecisionRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_policy_request".to_string())?; let mut decisions=Vec::new(); for row in store::list(database.connection()).map_err(|_|"storage_failed".to_string())? {if let Ok(p)=serde_json::from_slice::<policy::ApprovalPolicyProfile>(&row){decisions.push(policy::decide(&p,&req.scope_id,&req.action_class,&req.resource,req.risk,req.now_ms).map_err(|e|e.to_string())?)}} let d=decisions.into_iter().find(|x|!x.require_prompt).unwrap_or(policy::PolicyDecision{require_prompt:true,profile_id:None,reason:"prompt_required".into(),hard_requirement:req.risk>=3}); serde_json::to_vec(&serde_json::json!({"schema_version":1,"require_prompt":d.require_prompt,"profile_id":d.profile_id,"reason":d.reason,"hard_requirement":d.hard_requirement,"redacted":true})).map_err(|_|"serialization_failed".to_string()) }
                        _=>Err("unsupported_approval_policy_operation".into())
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::ApprovalPolicyProfiles {
                    profile_id,
                    operation,
                    version: expected_version,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::CheckpointForking {
                operation,
                fork_run_id,
                payload,
                reply,
            } => {
                let result = async {
                    if operation != "fork" {
                        return Err("unsupported_checkpoint_fork_operation".into());
                    };
                    let request: crate::checkpoint_forking_and_replay::ForkRequest =
                        serde_json::from_slice(&payload)
                            .map_err(|_| "invalid_fork_request".to_string())?;
                    let lineage =
                        crate::checkpoint_forking_and_replay::create(request, fork_run_id.clone())
                            .map_err(|e| e.to_string())?;
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    let json = serde_json::to_vec(&lineage)
                        .map_err(|_| "serialization_failed".to_string())?;
                    evohime_local_storage::checkpoint_forking_store::put(
                        db.connection(),
                        &lineage.fork_run_id,
                        &lineage.source_checkpoint_id,
                        &lineage.parent_run_id,
                        &json,
                        crate::task_memory::now_millis() as i64,
                    )
                    .map_err(|_| "storage_failed".to_string())?;
                    Ok(json)
                }
                .await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::CheckpointForking {
                    fork_run_id,
                    operation,
                    version: 1,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::PrivacyTelemetryGovernance {
                operation,
                category,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                #[expect(clippy::possible_missing_else, clippy::needless_question_mark, reason = "compact privacy command state machine uses sequential guards")]
                let result = async { use crate::privacy_and_telemetry_governance as g; use evohime_local_storage::privacy_telemetry_store as store; let journal=state.lock().await.journal.clone().ok_or_else(||"storage journal is not configured".to_string())?; let db=journal.database().lock().await; if idempotency_key.is_empty() || idempotency_key.len()>128 { return Err("invalid_idempotency_key".to_string()); } if expected_version != 0 { let current=store::consent_revision(db.connection()).map_err(|_|"storage_failed".to_string())?.unwrap_or(0); if current != expected_version { return Err("stale_version".to_string()); } } if !store::claim_idempotency(db.connection(),&idempotency_key,&operation).map_err(|_|"storage_failed".to_string())? { return Ok(serde_json::to_vec(&serde_json::json!({"status":"replayed","redacted":true})).map_err(|_|"serialization_failed".to_string())?); } match operation.as_str(){"consent"=>{let c:g::ConsentState=serde_json::from_slice(&payload).map_err(|_|"invalid_consent".to_string())?;g::validate_consent(&c).map_err(|e|e.to_string())?;let j=serde_json::to_vec(&c).map_err(|_|"serialization_failed".to_string())?;store::put_consent(db.connection(),&j,c.revision).map_err(|_|"storage_failed".to_string())?;serde_json::to_vec(&serde_json::json!({"status":"consent_saved","redacted":true})).map_err(|_|"serialization_failed".to_string())},"enqueue"=>{let request:g::TelemetryEnqueueRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_enqueue_request".to_string())?;let e=request.event;g::enqueue(&request.consent,e.clone()).map_err(|e|e.to_string())?;let j=serde_json::to_vec(&e).map_err(|_|"serialization_failed".to_string())?;let inserted=store::put_event(db.connection(),&e.event_id,&format!("{:?}",e.category),&j,e.created_at_ms).map_err(|_|"storage_failed".to_string())?;serde_json::to_vec(&serde_json::json!({"queued":inserted,"redacted":true})).map_err(|_|"serialization_failed".to_string())},"list"=>serde_json::to_vec(&serde_json::json!({"events":store::list(db.connection()).map_err(|_|"storage_failed".to_string())?,"redacted":true})).map_err(|_|"serialization_failed".to_string()),"clear"=>{store::clear(db.connection()).map_err(|_|"storage_failed".to_string())?;serde_json::to_vec(&serde_json::json!({"status":"cleared","redacted":true})).map_err(|_|"serialization_failed".to_string())},_=>Err("unsupported_privacy_telemetry_operation".into())}}.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::PrivacyTelemetryGovernance {
                    operation,
                    category,
                    version: 1,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ConversationBridgeAdapters {
                operation,
                bridge_id,
                payload,
                expected_revision,
                idempotency_key,
                correlation_id,
                reply,
            } => {
                let result = async {
                    use crate::conversation_bridge_adapters as b;
                    use evohime_local_storage::conversation_bridge_adapters_store as store;
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    if idempotency_key.is_empty() || idempotency_key.len() > 128 {
                        return Err("invalid_idempotency_key".into());
                    }
                    if !correlation_id.is_empty() && correlation_id.len() > 128 {
                        return Err("invalid_correlation_id".into());
                    }
                    if !store::claim_idempotency(db.connection(), &idempotency_key, &operation)
                        .map_err(|_| "storage_failed".to_string())?
                    {
                        return serde_json::to_vec(
                            &serde_json::json!({"status":"replayed","redacted":true}),
                        )
                        .map_err(|_| "serialization_failed".to_string());
                    }
                    match operation.as_str() {
                        "create" | "revoke" => {
                            let mut bridge: b::ConversationBridge =
                                serde_json::from_slice(&payload)
                                    .map_err(|_| "invalid_bridge".to_string())?;
                            if bridge.bridge_id != bridge_id {
                                return Err("bridge_id_mismatch".into());
                            }
                            if expected_revision != 0
                                && store::bridge_revision(db.connection(), &bridge_id)
                                    .map_err(|_| "storage_failed".to_string())?
                                    != Some(expected_revision)
                            {
                                return Err("stale_revision".into());
                            }
                            if operation == "revoke" {
                                let stored = store::get_bridge(db.connection(), &bridge_id)
                                    .map_err(|_| "storage_failed".to_string())?
                                    .ok_or_else(|| "bridge_not_found".to_string())?;
                                let stored: b::ConversationBridge =
                                    serde_json::from_slice(&stored)
                                        .map_err(|_| "corrupt_bridge".to_string())?;
                                if stored.principal_id != bridge.principal_id
                                    || stored.state != b::BridgeState::Paired
                                {
                                    return Err("principal_binding_denied".into());
                                }
                            }
                            if operation == "revoke" {
                                bridge.state = b::BridgeState::Revoked;
                                bridge.revision = bridge.revision.saturating_add(1);
                            }
                            b::validate_bridge(&bridge).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&bridge)
                                .map_err(|_| "serialization_failed".to_string())?;
                            store::put_bridge(
                                db.connection(),
                                &bridge.bridge_id,
                                &json,
                                bridge.revision,
                            )
                            .map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({
                                "status": if operation == "revoke" { "revoked" } else { "paired" },
                                "bridge_id": bridge.bridge_id,
                                "revision": bridge.revision,
                                "redacted": true
                            }))
                            .map_err(|_| "serialization_failed".to_string())
                        }
                        "bind" => {
                            let binding: b::ThreadBinding = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_binding".to_string())?;
                            if binding.bridge_id != bridge_id {
                                return Err("bridge_id_mismatch".into());
                            }
                            b::validate_binding(&binding).map_err(|e| e.to_string())?;
                            let stored_bridge = store::get_bridge(db.connection(), &bridge_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "bridge_not_found".to_string())?;
                            let stored_bridge: b::ConversationBridge =
                                serde_json::from_slice(&stored_bridge)
                                    .map_err(|_| "corrupt_bridge".to_string())?;
                            b::authorize_principal(
                                &stored_bridge,
                                &binding.principal_id,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            let inserted = store::put_binding(
                                db.connection(),
                                &payload,
                                &binding.binding_id,
                                &binding.bridge_id,
                                &binding.external_thread_id,
                                binding.revision,
                            )
                            .map_err(|_| "binding_conflict".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":if inserted{"bound"}else{"duplicate"},"binding_id":binding.binding_id,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "inbound" => {
                            let message: b::InboundMessage = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_inbound".to_string())?;
                            b::validate_inbound(&message).map_err(|e| e.to_string())?;
                            let stored_binding = store::get_binding(
                                db.connection(),
                                &message.binding_id,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "binding_not_found".to_string())?;
                            let stored_binding: b::ThreadBinding =
                                serde_json::from_slice(&stored_binding)
                                    .map_err(|_| "corrupt_binding".to_string())?;
                            if stored_binding.bridge_id != bridge_id
                                || stored_binding.principal_id != message.principal_id
                            {
                                return Err("principal_binding_denied".into());
                            }
                            let stored_bridge = store::get_bridge(db.connection(), &bridge_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "bridge_not_found".to_string())?;
                            let stored_bridge: b::ConversationBridge =
                                serde_json::from_slice(&stored_bridge)
                                    .map_err(|_| "corrupt_bridge".to_string())?;
                            b::authorize_principal(
                                &stored_bridge,
                                &message.principal_id,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            let accepted = store::put_inbound(
                                db.connection(),
                                &message.message_id,
                                &message.binding_id,
                                &payload,
                                message.created_at_ms,
                            )
                            .map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":if accepted{"accepted"}else{"duplicate_or_bounded"},"message_id":message.message_id,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "remote_command" => {
                            let command: b::RemoteCommand = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_remote_command".to_string())?;
                            b::validate_remote_command(&command).map_err(|e| e.to_string())?;
                            let stored_binding = store::get_binding(
                                db.connection(),
                                &command.binding_id,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "binding_not_found".to_string())?;
                            let stored_binding: b::ThreadBinding =
                                serde_json::from_slice(&stored_binding)
                                    .map_err(|_| "corrupt_binding".to_string())?;
                            if stored_binding.bridge_id != bridge_id
                                || stored_binding.principal_id != command.principal_id
                            {
                                return Err("principal_binding_denied".into());
                            }
                            let stored_bridge = store::get_bridge(db.connection(), &bridge_id)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "bridge_not_found".to_string())?;
                            let stored_bridge: b::ConversationBridge =
                                serde_json::from_slice(&stored_bridge)
                                    .map_err(|_| "corrupt_bridge".to_string())?;
                            b::authorize_principal(
                                &stored_bridge,
                                &command.principal_id,
                                expected_revision,
                            )
                            .map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"accepted_for_core_dispatch","command_id":command.command_id,"kind":format!("{:?}",command.kind),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "project" => {
                            let request: BridgeProjectionRequest = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_projection_request".to_string())?;
                            let projection = b::redacted_projection(
                                &request.binding,
                                &request.kind,
                                &request.status,
                                &request.provenance_id,
                            )
                            .map_err(|e| e.to_string())?;
                            serde_json::to_vec(&projection)
                                .map_err(|_| "serialization_failed".to_string())
                        }
                        "list" => {
                            let count = store::list_inbound(db.connection())
                                .map_err(|_| "storage_failed".to_string())?
                                .len();
                            serde_json::to_vec(&serde_json::json!({"bridge_id":bridge_id,"inbound_count":count,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "clear" => {
                            store::clear_bridge(db.connection(), &bridge_id)
                                .map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"cleared","bridge_id":bridge_id,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_bridge_operation".into()),
                    }
                }
                .await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::ConversationBridgeAdapters {
                    operation,
                    bridge_id,
                    revision: 1,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskSnapshot {
                project_id,
                task_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let task = journal
                        .get_work_item(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "task not found".to_string())?;
                    if task.project_id != project_id {
                        return Err("task does not belong to project".to_string());
                    }
                    let snapshot = journal
                        .latest_snapshot_for_task(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "snapshot not found".to_string())?;
                    let snapshot_json =
                        serde_json::from_slice::<serde_json::Value>(&snapshot.payload)
                            .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "id": snapshot.id,
                        "run_id": snapshot.run_id,
                        "workspace_hash": snapshot.workspace_hash,
                        "created_at": snapshot.created_at,
                        "snapshot": snapshot_json,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RestoreTaskSnapshot {
                project_id,
                task_id,
                snapshot_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal
                        .get_project(&project_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let task = journal
                        .get_work_item(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "task not found".to_string())?;
                    if task.project_id != project_id {
                        return Err("task does not belong to project".to_string());
                    }
                    let snapshot = journal
                        .get_snapshot(&snapshot_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "snapshot not found".to_string())?;
                    let run = journal
                        .get_run(&snapshot.run_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    if run.as_ref().map(|run| run.work_item_id.as_str()) != Some(task_id.as_str()) {
                        return Err("snapshot ownership could not be verified".to_string());
                    }
                    let run_id = snapshot.run_id.clone();
                    let workspace_snapshot = serde_json::from_slice::<
                        crate::build::WorkspaceSnapshot,
                    >(&snapshot.payload)
                    .map_err(|error| format!("invalid snapshot: {error}"))?;
                    crate::workspace_state_checkpoints::restore_build_snapshot_safe(
                        &project.workspace_path,
                        &workspace_snapshot,
                    )
                    .map_err(|error| error.to_string())?;
                    let audit_payload = serde_json::to_vec(&serde_json::json!({
                        "task_id": task_id,
                        "snapshot_id": snapshot_id,
                        "run_id": run_id,
                        "operation": "workspace_restore",
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&task_id, "snapshot.rollback.applied", &audit_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        task_id.clone(),
                        "snapshot.rollback.applied",
                        [
                            ("snapshot_id".to_owned(), snapshot_id.clone()),
                            ("run_id".to_owned(), run_id.clone()),
                            ("operation".to_owned(), "workspace_restore".to_owned()),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({
                        "snapshot_id": snapshot_id,
                        "restored": true,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetBuildPolicy { project_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal.get_project(&project_id).await.map_err(|error| error.to_string())?.ok_or_else(|| "project not found".to_string())?;
                    let (policy, version) = journal.get_build_policy(&project.id, &default_build_policy()).await?;
                    serde_json::to_vec(&serde_json::json!({ "project_id": project_id, "version": version, "policy": policy })).map_err(|error| error.to_string())
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::SaveBuildPolicy {
                project_id,
                policy_json,
                expected_version,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    journal.get_project(&project_id).await.map_err(|error| error.to_string())?.ok_or_else(|| "project not found".to_string())?;
                    let policy = harden_build_policy(serde_json::from_slice::<crate::scope::BuildScope>(&policy_json).map_err(|error| format!("invalid build policy: {error}"))?);
                    if let Some(violation) = crate::scope::validate_build_scope(&policy, &[]).first() { return Err(format!("invalid build policy: {}", violation.reason)); }
                    let saved = journal.save_build_policy(&project_id, &policy, Some(expected_version)).await?;
                    serde_json::to_vec(&serde_json::json!({ "project_id": project_id, "version": saved.version, "policy": policy })).map_err(|error| error.to_string())
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::ApplyApprovedBuild {
                project_id,
                run_id,
                task_id,
                approved_build_json,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal
                        .get_project(&project_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let approved =
                        serde_json::from_slice::<crate::build::ApprovedBuild>(&approved_build_json)
                            .map_err(|error| format!("invalid approved build: {error}"))?;
                    let _effect = journal
                        .begin_build_effect(&run_id, &task_id, &approved.intent_hash)
                        .await
                        .map_err(|error| error.to_string())?;
                    let heartbeat_failure = Arc::new(StdMutex::new(None::<String>));
                    let heartbeat_cancel = CancellationToken::new();
                    let heartbeat_journal = journal.clone();
                    let heartbeat_run_id = run_id.clone();
                    let heartbeat_failure_slot = heartbeat_failure.clone();
                    let heartbeat_cancel_for_task = heartbeat_cancel.clone();
                    let heartbeat_task = tokio::spawn(async move {
                        let mut interval = tokio::time::interval(Duration::from_secs(10));
                        loop {
                            tokio::select! {
                                _ = heartbeat_cancel_for_task.cancelled() => break,
                                _ = interval.tick() => {
                                    if let Err(error) = heartbeat_journal.heartbeat_build_effect(&heartbeat_run_id).await {
                                        *heartbeat_failure_slot.lock().expect("heartbeat failure lock") = Some(error.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                    });
                    let apply_result = tokio::task::spawn_blocking({
                        let workspace_path = project.workspace_path.clone();
                        let run_id = run_id.clone();
                        let approved = approved.clone();
                        move || crate::build::apply_approved_build(&workspace_path, &run_id, &approved)
                    })
                    .await;
                    heartbeat_cancel.cancel();
                    let _ = heartbeat_task.await;
                    let apply_result =
                        apply_result.map_err(|error| format!("build worker failed: {error}"))?;
                    let snapshot = match apply_result {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            let _ = journal.complete_build_effect(&run_id, false, None).await;
                            Self::record_audit(
                                &state,
                                crate::audit::AuditKind::Failure,
                                if task_id.is_empty() {
                                    run_id.clone()
                                } else {
                                    task_id.clone()
                                },
                                "build.apply_failed",
                                [
                                    ("run_id".to_owned(), run_id.clone()),
                                    ("task_id".to_owned(), task_id.clone()),
                                    ("intent_hash".to_owned(), approved.intent_hash.clone()),
                                    ("error".to_owned(), error.to_string()),
                                ],
                            )
                            .await;
                            return Err(error.to_string());
                        }
                    };
                    let payload =
                        serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
                    journal
                        .save_snapshot(
                            &snapshot.id,
                            &run_id,
                            &snapshot.baseline_workspace_hash,
                            &payload,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    if let Some(error) = heartbeat_failure
                        .lock()
                        .expect("heartbeat failure lock")
                        .clone()
                    {
                        return Err(format!(
                            "build lease heartbeat failed; outcome requires reconciliation: {error}"
                        ));
                    }
                    let audit_payload = serde_json::to_vec(&serde_json::json!({
                        "run_id": run_id,
                        "snapshot_id": snapshot.id,
                        "intent_hash": approved.intent_hash,
                        "effective_permissions_hash": approved.effective_permissions_hash,
                        "workspace_hash": snapshot.baseline_workspace_hash,
                        "diff_count": snapshot.diff.len(),
                        "diff": &snapshot.diff,
                    }))
                    .map_err(|error| error.to_string())?;
                    let audit_subject = if task_id.is_empty() {
                        &run_id
                    } else {
                        &task_id
                    };
                    journal
                        .record_audit(audit_subject, "build.applied", &audit_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    journal
                        .complete_build_effect(&run_id, true, Some(&snapshot.id))
                        .await
                        .map_err(|error| error.to_string())?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Diff,
                        audit_subject.to_string(),
                        "build.applied",
                        [
                            ("run_id".to_owned(), run_id.clone()),
                            ("task_id".to_owned(), task_id.clone()),
                            ("snapshot_id".to_owned(), snapshot.id.clone()),
                            ("intent_hash".to_owned(), approved.intent_hash.clone()),
                            ("diff_count".to_owned(), snapshot.diff.len().to_string()),
                        ],
                    )
                    .await;
                    Ok(payload)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::PrepareBuild {
                project_id,
                proposal_json,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal
                        .get_project(&project_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let proposal =
                        serde_json::from_slice::<crate::build::BuildProposal>(&proposal_json)
                            .map_err(|error| format!("invalid build proposal: {error}"))?;
                    let policy = journal
                        .get_or_create_build_policy(&project_id, &default_build_policy())
                        .await?;
                    let effective_scope =
                        crate::scope::restrict_to_policy(&policy, &proposal.scope).map_err(
                            |violations| {
                                match serde_json::to_string(&violations) {
                                    Ok(value) => value,
                                    Err(error) => {
                                        tracing::warn!(%error, "failed to serialize build policy violations");
                                        "build policy violation".into()
                                    }
                                }
                            },
                        )?;
                    let effective_proposal = crate::build::BuildProposal {
                        scope: effective_scope,
                        changes: proposal.changes,
                    };
                    let approved =
                        crate::build::prepare_build(&project.workspace_path, &effective_proposal)
                            .map_err(|error| error.to_string())?;
                    let payload =
                        serde_json::to_vec(&approved).map_err(|error| error.to_string())?;
                    let audit_subject = format!("proposal-{}", approved.intent_hash);
                    let audit_payload = serde_json::to_vec(&serde_json::json!({
                        "intent_hash": approved.intent_hash,
                        "effective_permissions_hash": approved.effective_permissions_hash,
                        "expected_workspace_hash": approved.expected_workspace_hash,
                        "change_count": approved.changes.len(),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&audit_subject, "build.approval_prepared", &audit_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Budget,
                        project_id.clone(),
                        "build.approval_prepared",
                        [
                            ("intent_hash".to_owned(), approved.intent_hash.clone()),
                            (
                                "change_count".to_owned(),
                                approved.changes.len().to_string(),
                            ),
                            (
                                "max_files_changed".to_owned(),
                                policy.max_files_changed.to_string(),
                            ),
                            (
                                "max_bytes_changed".to_owned(),
                                policy.max_bytes_changed.to_string(),
                            ),
                        ],
                    )
                    .await;
                    Ok(payload)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RunDoctor {
                project_id,
                protocol_major,
                expected_protocol_major,
                provider,
                approval_required,
                registered_tools,
                expected_tools,
                unavailable_tools,
                detail_level,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let storage = match &journal {
                        Some(journal) => {
                            let (path, schema_version) = journal
                                .storage_snapshot()
                                .await
                                .map_err(|error| error.to_string())?;
                            let exists = path.exists();
                            let writable = exists
                                && std::fs::metadata(&path)
                                    .map(|meta| !meta.permissions().readonly())
                                    .unwrap_or(false);
                            crate::doctor::StorageProbe {
                                path_label: path.display().to_string(),
                                exists,
                                writable,
                                schema_version: Some(schema_version),
                                expected_schema_version: evohime_local_storage::SCHEMA_VERSION,
                            }
                        }
                        None => crate::doctor::StorageProbe {
                            path_label: "not-configured".into(),
                            exists: false,
                            writable: false,
                            schema_version: None,
                            expected_schema_version: evohime_local_storage::SCHEMA_VERSION,
                        },
                    };

                    let pipe = crate::doctor::PipeProbe {
                        pipe_label: "desktop-ipc".into(),
                        reachable: true,
                        protocol_major,
                        expected_protocol_major,
                    };

                    let recovery = match &journal {
                        Some(journal) => journal
                            .recovery_probe()
                            .await
                            .map_err(|error| error.to_string())?,
                        None => crate::doctor::RecoveryProbe {
                            state: "NOT_CONFIGURED".into(),
                            unknown_effects: 0,
                            lease_expired: false,
                            resumable_runs: 0,
                        },
                    };

                    let permissions = match (&journal, project_id.is_empty()) {
                        (Some(journal), false) => {
                            match journal
                                .get_project(&project_id)
                                .await
                                .map_err(|error| error.to_string())?
                            {
                                Some(project) => {
                                    let workspace = std::path::Path::new(&project.workspace_path);
                                    let workspace_readable = workspace.is_dir();
                                    let workspace_writable = workspace_readable
                                        && std::fs::metadata(workspace)
                                            .map(|meta| !meta.permissions().readonly())
                                            .unwrap_or(false);
                                    let protected_paths_intact = [".git", ".evohime"]
                                        .iter()
                                        .all(|segment| workspace.join(segment).exists());
                                    crate::doctor::PermissionsProbe {
                                        workspace_readable,
                                        workspace_writable,
                                        protected_paths_intact,
                                        approval_required,
                                    }
                                }
                                None => unresolved_permissions_probe(approval_required),
                            }
                        }
                        _ => unresolved_permissions_probe(approval_required),
                    };

                    let scheduler = crate::export::scheduler_probe();

                    let snapshot = crate::doctor::DoctorSnapshot {
                        storage,
                        pipe,
                        provider,
                        recovery,
                        permissions,
                        tools: crate::doctor::ToolsProbe {
                            registered_tools,
                            expected_tools,
                            unavailable_tools,
                        },
                        scheduler,
                    };
                    let report = crate::doctor::DoctorReport::from_snapshot_with_detail(
                        &snapshot,
                        detail_level,
                    )
                    .map_err(|error| format!("{error:?}"))?;
                    Ok(report.to_bounded_json().into_bytes())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::CreateDiagnosticsSnapshot {
                project_id,
                conversation_id,
                run_id,
                max_event_count,
                max_log_bytes,
                protocol_major,
                expected_protocol_major,
                provider,
                approval_required,
                registered_tools,
                expected_tools,
                unavailable_tools,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let started = std::time::Instant::now();
                    let (schema_version, recovery_state) = match &journal {
                        Some(journal) => {
                            let (_, version) = journal.storage_snapshot().await.map_err(|e| e.to_string())?;
                            let recovery = journal.recovery_probe().await.map_err(|e| e.to_string())?;
                            (version, recovery.state)
                        }
                        None => (0, "NOT_CONFIGURED".to_owned()),
                    };
                    let doctor = serde_json::json!({
                        "contract_version": 1,
                        "checks": [
                            {"id":"storage", "status": if schema_version > 0 { "OK" } else { "BLOCKED" }, "summary":"Хранилище и схема доступны", "action":"Действий не требуется"},
                            {"id":"pipe", "status": if protocol_major == Some(expected_protocol_major) { "OK" } else { "BLOCKED" }, "summary":"Core pipe доступен", "action":"Проверь версии UI и Core"},
                            {"id":"provider", "status": if provider.configured && provider.metadata_valid { "OK" } else { "WARN" }, "summary":"Состояние провайдера проверено", "action":"Проверь настройки провайдера"},
                            {"id":"recovery", "status": if recovery_state == "CLEAN" { "OK" } else { "WARN" }, "summary":"Состояние recovery: {recovery_state}", "action":"Проверь recovery state"},
                            {"id":"permissions", "status": if approval_required { "WARN" } else { "OK" }, "summary":"Политика разрешений проверена", "action":"Подтверди требуемое разрешение явно"},
                            {"id":"tools", "status": if registered_tools >= expected_tools && unavailable_tools.is_empty() { "OK" } else { "WARN" }, "summary":"Каталог tools проверен", "action":"Проверь регистрацию tools"}
                        ]
                    });
                    let doctor_json = serde_json::to_vec(&doctor).map_err(|e| e.to_string())?;
                    let run_status = if run_id.is_empty() {
                        String::new()
                    } else {
                        let run = journal
                            .as_ref()
                            .ok_or_else(|| "run_not_found".to_owned())?
                            .get_run(&run_id)
                            .await
                            .map_err(|e| e.to_string())?
                            .ok_or_else(|| "run_not_found".to_owned())?;
                        run.status
                    };
                    crate::support_bundle::build_snapshot(&doctor_json, conversation_id, run_id, run_status, max_event_count, max_log_bytes, started.elapsed().as_millis() as u64)
                }.await;
                let _ = reply.send(result);
                let _ = project_id;
            }
            CoreCommand::ExportDoctorLogs {
                destination_path,
                reply,
            } => {
                let result = crate::export::export_logs(std::path::Path::new(&destination_path))
                    .map(|summary| summary.to_bounded_json().into_bytes())
                    .map_err(|error| format!("{error:?}"));
                let _ = reply.send(result);
            }
            CoreCommand::CreateDatabaseBackup {
                operation_id,
                destination_path,
                progress,
                reply,
            } => {
                let cancellation = CancellationToken::new();
                let (journal, events) = {
                    let guard = state.lock().await;
                    (guard.journal.clone(), guard.events.clone())
                };
                state
                    .lock()
                    .await
                    .backup_cancellations
                    .insert(operation_id.clone(), cancellation.clone());
                let background_permit = state
                    .lock()
                    .await
                    .background_tasks
                    .try_acquire();
                let Some(background_permit) = background_permit else {
                    state.lock().await.backup_cancellations.remove(&operation_id);
                    let _ = reply.send(Err("background task capacity is exhausted".into()));
                    return;
                };
                tokio::spawn(async move {
                    let _background_permit = background_permit;
                    let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let start_payload = serde_json::to_vec(&serde_json::json!({
                        "operation_id": operation_id,
                        "result": "started",
                        "destination_name": safe_file_name(&destination_path),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&operation_id, "storage.started", &start_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    let operation_for_events = operation_id.clone();
                    let progress = progress;
                    let operation_cancellation = cancellation.clone();
                    let result = journal
                        .create_database_backup_with_cancel(
                            std::path::Path::new(&destination_path),
                            env!("CARGO_PKG_VERSION"),
                            |item| {
                                let _ = progress.send(item.clone());
                                let _ = events.send(CoreEvent::StorageProgress {
                                    operation_id: operation_for_events.clone(),
                                    progress: item,
                                });
                            },
                            move || operation_cancellation.is_cancelled(),
                        )
                        .await
                        .map_err(|error| error.to_string());
                    let audit = serde_json::to_vec(&serde_json::json!({
                        "operation_id": operation_id,
                        "result": if result.is_ok() { "created" } else if result.as_ref().err().is_some_and(|error| error.to_string().contains("cancelled")) { "cancelled" } else { "failed" },
                        "destination_name": safe_file_name(&destination_path),
                        "error_category": result.as_ref().err().map(|error| error_category(error)),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&operation_id, "storage.completed", &audit)
                        .await
                        .map_err(|error| error.to_string())?;
                    result.and_then(|value| {
                        serde_json::to_vec(&value).map_err(|error| error.to_string())
                    })
                    }
                    .await;
                    state
                        .lock()
                        .await
                        .backup_cancellations
                        .remove(&operation_id);
                    let _ = reply.send(result);
                });
            }
            CoreCommand::PrepareDatabaseRestore {
                operation_id,
                backup_path,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let preview = LocalDatabase::preview_backup(&backup_path)
                        .map_err(|error| error.to_string())?;
                    let approval_id = uuid::Uuid::new_v4().to_string();
                    state
                        .lock()
                        .await
                        .backup_approvals
                        .insert(approval_id.clone(), backup_path.clone());
                    if let Some(journal) = journal {
                        let payload = serde_json::to_vec(&serde_json::json!({
                            "operation_id": operation_id,
                            "result": "previewed",
                            "backup_name": safe_file_name(&backup_path),
                            "schema_version": preview.schema_version,
                            "checksum_sha256": preview.checksum_sha256,
                        }))
                        .map_err(|error| error.to_string())?;
                        journal
                            .record_audit(&operation_id, "storage.previewed", &payload)
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    serde_json::to_vec(&serde_json::json!({
                        "operation_id": operation_id,
                        "approval_id": approval_id,
                        "preview": preview,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RestoreDatabase {
                operation_id,
                backup_path,
                approval_id,
                progress,
                reply,
            } => {
                let cancellation = CancellationToken::new();
                let approved = {
                    let mut guard = state.lock().await;
                    guard
                        .backup_approvals
                        .get(&approval_id)
                        .is_some_and(|path| path == &backup_path)
                        .then(|| guard.backup_approvals.remove(&approval_id))
                        .flatten()
                        .is_some()
                };
                let (journal, events) = {
                    let guard = state.lock().await;
                    (guard.journal.clone(), guard.events.clone())
                };
                if approved {
                    state
                        .lock()
                        .await
                        .backup_cancellations
                        .insert(operation_id.clone(), cancellation.clone());
                }
                let background_permit = state
                    .lock()
                    .await
                    .background_tasks
                    .try_acquire();
                let Some(background_permit) = background_permit else {
                    if approved {
                        state.lock().await.backup_cancellations.remove(&operation_id);
                    }
                    let _ = reply.send(Err("background task capacity is exhausted".into()));
                    return;
                };
                tokio::spawn(async move {
                    let _background_permit = background_permit;
                    let result = async {
                    if !approved {
                        if let Some(journal) = &journal {
                            let payload = serde_json::to_vec(&serde_json::json!({
                                "operation_id": operation_id,
                                "result": "rejected",
                                "backup_name": safe_file_name(&backup_path),
                                "error_category": "approval",
                            }))
                            .map_err(|error| error.to_string())?;
                            journal
                                .record_audit(&operation_id, "storage.restore.rejected", &payload)
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                        return Err("restore approval is missing or does not match the preview".into());
                    }
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let (database_path, _) = journal
                        .storage_snapshot()
                        .await
                        .map_err(|error| error.to_string())?;
                    let safety_path = database_path.with_file_name(format!(
                        "{}.pre-restore-{}.evohime",
                        safe_file_stem(&database_path),
                        uuid::Uuid::new_v4()
                    ));
                    let operation_for_events = operation_id.clone();
                    let progress = progress;
                    let operation_cancellation = cancellation.clone();
                    let restore = journal
                        .restore_database_with_cancel(
                            std::path::Path::new(&backup_path),
                            &safety_path,
                            env!("CARGO_PKG_VERSION"),
                            |item| {
                                let _ = progress.send(item.clone());
                                let _ = events.send(CoreEvent::StorageProgress {
                                    operation_id: operation_for_events.clone(),
                                    progress: item,
                                });
                            },
                            move || operation_cancellation.is_cancelled(),
                        )
                        .await;
                    let audit = serde_json::to_vec(&serde_json::json!({
                        "operation_id": operation_id,
                        "result": if restore.is_ok() { "restored" } else if restore.as_ref().err().is_some_and(|error| error.to_string().contains("cancelled")) { "cancelled" } else { "failed" },
                        "backup_name": safe_file_name(&backup_path),
                        "error_category": restore.as_ref().err().map(|error| error_category(&error.to_string())),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&operation_id, "storage.restore.completed", &audit)
                        .await
                        .map_err(|error| error.to_string())?;
                    restore
                        .map(|value| serde_json::to_vec(&value).map_err(|error| error.to_string()))
                        .map_err(|error| error.to_string())?
                    }
                    .await;
                    state
                        .lock()
                        .await
                        .backup_cancellations
                        .remove(&operation_id);
                    let _ = reply.send(result);
                });
            }
            CoreCommand::CancelDatabaseOperation {
                operation_id,
                reply,
            } => {
                let accepted = state
                    .lock()
                    .await
                    .backup_cancellations
                    .get(&operation_id)
                    .map(CancellationToken::cancel)
                    .is_some();
                let result = serde_json::to_vec(&serde_json::json!({
                    "operation_id": operation_id,
                    "accepted": accepted,
                }))
                .map_err(|error| error.to_string());
                let _ = reply.send(result);
            }
            CoreCommand::SaveResearchEvidence {
                work_item_id,
                source_kind,
                source_ref,
                title,
                publisher,
                content_type,
                raw_excerpt,
                retrieved_at_ms,
                ttl_ms,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if work_item_id.trim().is_empty() {
                        return Err("work_item_id must not be empty".to_string());
                    }
                    let source = crate::research::SourceMetadata::new(
                        source_ref,
                        title,
                        publisher,
                        content_type,
                        retrieved_at_ms,
                    )
                    .map_err(|error| error.to_string())?;
                    let captured_at_ms = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let evidence = crate::research::ResearchEvidence::capture(
                        source,
                        raw_excerpt,
                        captured_at_ms,
                        ttl_ms,
                    )
                    .map_err(|error| error.to_string())?;
                    let id = uuid::Uuid::new_v4().to_string();
                    let record = evohime_local_storage::research_store::ResearchEvidenceRecord {
                        id: id.clone(),
                        source_kind: source_kind.clone(),
                        source_ref: evidence.source.url.clone(),
                        redacted_excerpt: evidence.excerpt.clone(),
                        source_hash: evidence.excerpt_sha256.clone(),
                        fetched_at: evidence.captured_at_ms.to_string(),
                        ttl_seconds: evidence.ttl_ms.div_ceil(1_000),
                        provenance_link: Some(work_item_id.clone()),
                    };
                    journal.save_research_evidence(&record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        work_item_id.clone(),
                        "research.evidence.saved",
                        [
                            ("evidence_id".to_owned(), id.clone()),
                            ("source_kind".to_owned(), source_kind),
                            ("source_hash".to_owned(), evidence.excerpt_sha256.clone()),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({
                        "id": id,
                        "work_item_id": work_item_id,
                        "evidence": evidence,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListResearchEvidence {
                work_item_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let records = journal.list_research_evidence(&work_item_id).await?;
                    serde_json::to_vec(&serde_json::json!({
                        "work_item_id": work_item_id,
                        "records": records,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RunResearchFetch {
                work_item_id,
                url,
                title,
                allowed_domains,
                max_bytes,
                max_latency_ms,
                max_cost_micros,
                ttl_ms,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if work_item_id.trim().is_empty() {
                        return Err("work_item_id must not be empty".to_string());
                    }
                    let policy = crate::research_pipeline::ResearchPolicy {
                        network_allowed: true,
                        allowed_domains,
                        max_bytes,
                        max_latency_ms,
                        max_cost_micros,
                    };
                    let fetch_result = crate::research_fetch::run_research_fetch(
                        &work_item_id,
                        &url,
                        &title,
                        &policy,
                        ttl_ms,
                        false,
                    )
                    .await;
                    match fetch_result {
                        Ok(outcome) => {
                            let id = uuid::Uuid::new_v4().to_string();
                            let record =
                                evohime_local_storage::research_store::ResearchEvidenceRecord {
                                    id: id.clone(),
                                    source_kind: "url".to_string(),
                                    source_ref: outcome.evidence.source.url.clone(),
                                    redacted_excerpt: outcome.evidence.excerpt.clone(),
                                    source_hash: outcome.evidence.excerpt_sha256.clone(),
                                    fetched_at: outcome.evidence.captured_at_ms.to_string(),
                                    ttl_seconds: outcome.evidence.ttl_ms.div_ceil(1_000),
                                    provenance_link: Some(work_item_id.clone()),
                                };
                            journal.save_research_evidence(&record).await?;
                            Self::record_audit(
                                &state,
                                crate::audit::AuditKind::Evidence,
                                work_item_id.clone(),
                                "research.fetch.completed",
                                [
                                    ("evidence_id".to_owned(), id.clone()),
                                    ("url".to_owned(), outcome.citation.url.clone()),
                                    (
                                        "source_hash".to_owned(),
                                        outcome.citation.source_hash.clone(),
                                    ),
                                ],
                            )
                            .await;
                            serde_json::to_vec(&serde_json::json!({
                                "id": id,
                                "work_item_id": work_item_id,
                                "state": outcome.state,
                                "evidence": outcome.evidence,
                                "citation": outcome.citation,
                            }))
                            .map_err(|error| error.to_string())
                        }
                        Err(error) => {
                            Self::record_audit(
                                &state,
                                crate::audit::AuditKind::Failure,
                                work_item_id.clone(),
                                "research.fetch.failed",
                                [
                                    ("url".to_owned(), url.clone()),
                                    ("state".to_owned(), format!("{:?}", error.state)),
                                    ("error".to_owned(), error.message.clone()),
                                ],
                            )
                            .await;
                            Err(error.message)
                        }
                    }
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::CreateMemory {
                scope_kind,
                project_id,
                secondary_id,
                title,
                content,
                provenance_kind,
                provenance_id,
                provenance_locator,
                privacy,
                ttl_ms,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let domain_scope =
                        memory_domain_scope(&scope_kind, &project_id, &secondary_id)?;
                    let provenance = crate::memory_domain::ProvenanceRef::new(
                        provenance_kind,
                        provenance_id,
                        (!provenance_locator.trim().is_empty()).then_some(provenance_locator),
                    )
                    .map_err(|error| error.to_string())?;
                    let privacy_label = parse_memory_privacy(&privacy)?;
                    let created_at_ms = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let id = uuid::Uuid::new_v4().to_string();
                    let record = crate::memory_domain::MemoryDomain::new()
                        .create(crate::memory_domain::CreateMemory {
                            id: id.clone(),
                            scope: domain_scope,
                            title,
                            content,
                            provenance,
                            privacy: privacy_label,
                            created_at_ms,
                            ttl_ms,
                        })
                        .map_err(|error| error.to_string())?;
                    let store_scope = memory_store_scope(&scope_kind)?;
                    let store_privacy = memory_store_privacy(record.privacy)?;
                    let provenance_json = serde_json::to_string(&record.provenance)
                        .map_err(|error| error.to_string())?;
                    let store_record = evohime_local_storage::memory_store::MemoryRecord::new(
                        evohime_local_storage::memory_store::MemoryRecordInput {
                            id: record.id.clone(),
                            scope: store_scope,
                            scope_id: encode_memory_scope_id(&project_id, &secondary_id),
                            title: record.title.clone(),
                            content: record.content.clone(),
                            provenance: provenance_json,
                            privacy: store_privacy,
                            created_at: record.created_at_ms.to_string(),
                            expires_at: Some(record.expires_at_ms.to_string()),
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    journal.save_memory(&store_record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        project_id.clone(),
                        "memory.created",
                        [
                            ("memory_id".to_owned(), record.id.clone()),
                            ("scope_kind".to_owned(), scope_kind),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "record": record }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListMemory {
                scope_kind,
                project_id,
                secondary_id,
                include_archived,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let store_scope = memory_store_scope(&scope_kind)?;
                    let scope_id = encode_memory_scope_id(&project_id, &secondary_id);
                    let records = journal
                        .list_memory(store_scope, &scope_id, include_archived, limit)
                        .await?;
                    let records = records
                        .iter()
                        .map(memory_record_to_json)
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({ "records": records }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SearchMemory {
                scope_kind,
                project_id,
                secondary_id,
                query,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let store_scope = memory_store_scope(&scope_kind)?;
                    let scope_id = encode_memory_scope_id(&project_id, &secondary_id);
                    let now_ms = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let records = journal
                        .search_memory(store_scope, &scope_id, &query, &now_ms.to_string(), limit)
                        .await?;
                    let records = records
                        .iter()
                        .map(memory_record_to_json)
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({ "records": records }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ArchiveMemory {
                id,
                approval_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    crate::memory_api::Approval::new(
                        approval_id.clone(),
                        crate::memory_api::MemoryOperation::Archive,
                    )
                    .map_err(|error| error.to_string())?;
                    let changed = journal.archive_memory(&id).await?;
                    if !changed {
                        return Err(
                            "memory record was not found or is already archived/forgotten"
                                .to_string(),
                        );
                    }
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        id.clone(),
                        "memory.archived",
                        [
                            ("memory_id".to_owned(), id.clone()),
                            ("approval_id".to_owned(), approval_id),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "id": id, "archived": true }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ForgetMemory {
                id,
                approval_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    crate::memory_api::Approval::new(
                        approval_id.clone(),
                        crate::memory_api::MemoryOperation::Forget,
                    )
                    .map_err(|error| error.to_string())?;
                    // The tombstone id is random and unlinkable to the erased
                    // body: audit keeps only kind, scope, timestamps, a reason
                    // class and a digest.
                    let tombstone_id = uuid::Uuid::new_v4().to_string();
                    let forgotten_at = memory_now_ms().to_string();
                    let changed = journal
                        .forget_memory_with_tombstone(
                            &id,
                            &tombstone_id,
                            "user_request",
                            &forgotten_at,
                        )
                        .await?;
                    if !changed {
                        return Err(
                            "memory record was not found or is already forgotten".to_string()
                        );
                    }
                    // The erased statement still exists inside every backup
                    // taken before this point, so forget also rotates the
                    // containers that have aged past the retention window.
                    let rotated = evohime_local_storage::LocalDatabase::purge_expired_backups(
                        crate::export::local_data_dir(),
                        crate::memory_extraction::FORGET_BACKUP_RETENTION_MS,
                        memory_now_ms(),
                    )
                    .map(|removed| removed.len())
                    .unwrap_or(0);
                    // План 01.5: каскад удаляет производные записи scratchpad и
                    // task artifacts. Содержимое стирается, а факт удаления
                    // остаётся в redacted аудите.
                    let (removed_notes, removed_artifacts) = journal
                        .forget_context_derivatives(&id, &id)
                        .await
                        .unwrap_or((0, 0));
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        id.clone(),
                        "memory.forgotten",
                        [
                            ("memory_id".to_owned(), id.clone()),
                            ("approval_id".to_owned(), approval_id),
                            ("tombstone_id".to_owned(), tombstone_id.clone()),
                            ("reason_class".to_owned(), "user_request".to_owned()),
                            ("rotated_backups".to_owned(), rotated.to_string()),
                            ("removed_scratchpad".to_owned(), removed_notes.to_string()),
                            (
                                "removed_artifacts".to_owned(),
                                removed_artifacts.to_string(),
                            ),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({
                        "id": id,
                        "forgotten": true,
                        "tombstone_id": tombstone_id,
                        "rotated_backups": rotated,
                        "removed_scratchpad": removed_notes,
                        "removed_artifacts": removed_artifacts,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::MemoryViewsAndAdaptiveRecall {
                operation,
                view_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_view_id = view_id.clone();
                let result = async {
                    use crate::memory_views_and_adaptive_recall as v;
                    use evohime_local_storage::memory_views_and_adaptive_recall_store as store;
                    if view_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 {
                        return Err("invalid_memory_view_request".to_string());
                    }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    match operation.as_str() {
                        "save_view" => {
                            let request: MemoryViewSaveRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_memory_view_payload".to_string())?;
                            let view = request.view;
                            if view.id != view_id { return Err("view_id_mismatch".into()); }
                            v::validate_view(&view).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&view).map_err(|_| "serialization_failed".to_string())?;
                            let hash = v::canonical_hash(&view).map_err(|e| e.to_string())?;
                            let saved = store::save_view(db.connection(), store::ViewInput { view_id: &view.id, owner_scope: &view.owner_scope, revision: view.revision, view_json: &json, content_hash: &hash, expected_version, idempotency_key: &idempotency_key, now_ms: memory_now_ms() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            if !saved { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"view_saved","view_id":view.id,"revision":view.revision,"content_hash":hash,"rights":view.rights,"scope_count":view.scopes.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "inspect" => {
                            let record = store::load_view(db.connection(), &view_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "view_not_found".to_string())?;
                            let view: v::MemoryView = serde_json::from_slice(&record.view_json).map_err(|_| "corrupt_memory_view".to_string())?;
                            v::validate_view(&view).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"view","view_id":view.id,"revision":record.revision,"owner_scope":record.owner_scope,"rights":view.rights,"root_scope_ids":view.root_scope_ids,"scope_count":view.scopes.len(),"content_hash":record.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "recall" => {
                            let request: MemoryRecallRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_memory_view_payload".to_string())?;
                            let record = store::load_view(db.connection(), &view_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "view_not_found".to_string())?;
                            let view: v::MemoryView = serde_json::from_slice(&record.view_json).map_err(|_| "corrupt_memory_view".to_string())?;
                            v::validate_view(&view).map_err(|e| e.to_string())?;
                            let scope_id = request.scope_id.as_deref().unwrap_or(&view.root_scope_ids[0]);
                            v::authorize_read(&view, scope_id).map_err(|e| e.to_string())?;
                            let decision = v::decide_recall(&view, &request.policy, request.mode, request.complexity, &request.query, request.read_barrier_generation).map_err(|e| e.to_string())?;
                            let ranked = v::rank_candidates(&view, request.candidates).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&decision).map_err(|_| "serialization_failed".to_string())?;
                            let saved = store::save_recall(db.connection(), store::RecallInput { view_id: &view_id, view_revision: record.revision, barrier_generation: request.read_barrier_generation, decision_json: &json, expected_version, idempotency_key: &idempotency_key, now_ms: memory_now_ms() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            if !saved { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"recall_planned","view_id":view_id,"view_revision":record.revision,"decision":decision,"ranked_candidates":ranked,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_memory_view_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::MemoryViewsAndAdaptiveRecall {
                    operation: event_operation,
                    view_id: event_view_id,
                    version: expected_version.saturating_add(1),
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ModelEditProtocolRegistry {
                operation,
                protocol_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_protocol_id = protocol_id.clone();
                let result = async {
                    use crate::model_edit_protocol_registry as v;
                    use evohime_local_storage::model_edit_protocol_registry_store as store;
                    if protocol_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 { return Err("invalid_model_edit_request".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    let request: ModelEditRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_model_edit_payload".to_string())?;
                    match operation.as_str() {
                        "register" => {
                            let definition = request.definition.ok_or_else(|| "definition_required".to_string())?;
                            if definition.protocol_id != protocol_id { return Err("protocol_id_mismatch".into()); }
                            v::validate(&definition).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&definition).map_err(|_| "serialization_failed".to_string())?;
                            let content_hash = v::canonical_hash(&definition).map_err(|e| e.to_string())?;
                            let saved = store::save(db.connection(), store::DefinitionInput { protocol_id: &protocol_id, revision: definition.revision, model_profile_id: &definition.model_profile_id, definition_json: &json, content_hash: &content_hash, idempotency_key: &idempotency_key, expected_version, now_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            if !saved { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"registered","protocol_id":protocol_id,"revision":definition.revision,"model_profile_id":definition.model_profile_id,"content_hash":content_hash,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "inspect" => {
                            let record = store::load(db.connection(), &protocol_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "protocol_not_found".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"registered","protocol_id":protocol_id,"revision":record.revision,"model_profile_id":record.model_profile_id,"content_hash":record.content_hash,"version":record.version,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "preflight" | "apply" => {
                            let record = store::load(db.connection(), &protocol_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "protocol_not_found".to_string())?;
                            if expected_version != record.version { return Err("stale_protocol_version".into()); }
                            let definition: v::EditProtocolDefinition = serde_json::from_slice(&record.definition_json).map_err(|_| "corrupt_edit_protocol".to_string())?;
                            let original = request.original.as_deref().ok_or_else(|| "original_required".to_string())?;
                            let preflight = v::preflight(&definition, original).map_err(|e| e.to_string())?;
                            if operation == "apply" { return Err("apply_requires_approved_revision_safe_files_tool".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"preflight_ok","protocol_id":protocol_id,"version":record.version,"preflight":preflight,"mutation":"not_dispatched","redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "repair_feedback" => { let error_code = request.error_code.as_deref().unwrap_or("edit_failed"); let feedback = v::repair_feedback(&v::EditProtocolError::Invalid("edit_failed"), request.attempt).map_err(|_| format!("{error_code}:repair_exhausted"))?; serde_json::to_vec(&feedback).map_err(|_| "serialization_failed".into()) }
                        _ => Err("unsupported_model_edit_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::ModelEditProtocolRegistry {
                    operation: event_operation,
                    protocol_id: event_protocol_id,
                    version: expected_version.saturating_add(1),
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::RemoteConversationChannels {
                operation,
                connection_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_connection_id = connection_id.clone();
                let result = async {
                    use crate::remote_conversation_channels as v; use evohime_local_storage::remote_conversation_channels_store as store;
                    if connection_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 { return Err("invalid_remote_channel_request".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                    let request: RemoteChannelRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_remote_channel_payload".to_string())?;
                    match operation.as_str() {
                        "save" => { let connection = request.connection.ok_or_else(|| "connection_required".to_string())?; if connection.connection_id != connection_id{return Err("connection_id_mismatch".into())}; v::validate_connection(&connection).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&connection).map_err(|_|"serialization_failed".to_string())?; let h=v::canonical_hash(&connection).map_err(|e|e.to_string())?; if !store::save(db.connection(),store::ConnectionInput{id:&connection_id,owner_scope:&connection.owner_scope,connection_json:&json,content_hash:&h,expected_version,idempotency_key:&idempotency_key,now_ms:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())?{return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"saved","connection_id":connection_id,"provider":connection.provider,"state":connection.state,"content_hash":h,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "inspect" => { let row=store::load(db.connection(),&connection_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"connection_not_found".to_string())?; serde_json::to_vec(&serde_json::json!({"status":"stored","connection_id":connection_id,"owner_scope":row.owner_scope,"content_hash":row.content_hash,"version":row.version,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "pair" => { let row=store::load(db.connection(),&connection_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"connection_not_found".to_string())?; let connection:v::ChannelConnection=serde_json::from_slice(&row.connection_json).map_err(|_|"corrupt_channel".to_string())?; let code=request.code.as_deref().ok_or_else(||"pairing_code_required".to_string())?; let identity=request.external_identity.as_deref().ok_or_else(||"external_identity_required".to_string())?; let now=crate::task_memory::now_millis() as i64; let ok=store::consume_pairing(db.connection(),&connection_id,&v::hash_pairing_code(code).map_err(|e|e.to_string())?,identity,now).map_err(|_|"storage_failed".to_string())?; if !ok{return Err("pairing_invalid_or_expired".into())}; if identity!=connection.external_identity{return Err("identity_mismatch".into())}; serde_json::to_vec(&serde_json::json!({"status":"paired","connection_id":connection_id,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "admit" => { let row=store::load(db.connection(),&connection_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"connection_not_found".to_string())?; let connection:v::ChannelConnection=serde_json::from_slice(&row.connection_json).map_err(|_|"corrupt_channel".to_string())?; let message=request.message.ok_or_else(||"message_required".to_string())?; let ok=store::claim_message(db.connection(),&connection_id,&message.message_id,crate::task_memory::now_millis() as i64).map_err(|_|"storage_failed".to_string())?; v::admit_message(&connection,&message,0,!ok,crate::task_memory::now_millis() as i64).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"admitted","message_id":message.message_id,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "revoke" => { let row=store::load(db.connection(),&connection_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"connection_not_found".to_string())?; let mut connection:v::ChannelConnection=serde_json::from_slice(&row.connection_json).map_err(|_|"corrupt_channel".to_string())?; connection.state=v::ConnectionState::Revoked; connection.revision=connection.revision.saturating_add(1); let json=serde_json::to_vec(&connection).map_err(|_|"serialization_failed".to_string())?; let h=v::canonical_hash(&connection).map_err(|e|e.to_string())?; if !store::save(db.connection(),store::ConnectionInput{id:&connection_id,owner_scope:&connection.owner_scope,connection_json:&json,content_hash:&h,expected_version,idempotency_key:&idempotency_key,now_ms:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())?{return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"revoked","connection_id":connection_id,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        _ => Err("unsupported_remote_channel_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::RemoteConversationChannels {
                    operation: event_operation,
                    connection_id: event_connection_id,
                    version: expected_version.saturating_add(1),
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::PromptCachePlanner {
                operation,
                plan_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_plan_id = plan_id.clone();
                let result=async { use crate::prompt_cache_planner as v; if plan_id.is_empty()||idempotency_key.is_empty(){return Err("invalid_prompt_cache_request".into())}; let request:PromptCacheRequest=serde_json::from_slice(&payload).map_err(|_|"invalid_prompt_cache_payload".to_string())?; match operation.as_str(){"plan"=>{let segments=request.segments;let profile=request.profile.ok_or_else(||"profile_required".to_string())?;let plan=v::build_plan(segments,&profile,&request.context_revision,&request.policy_version,request.keepalive_ms).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":"planned","plan_id":plan_id,"cache_key":plan.cache_key,"segment_count":plan.segments.len(),"provider_profile_id":plan.provider_profile_id,"keepalive_ms":plan.keepalive_ms,"redacted":true})).map_err(|_|"serialization_failed".into())},"metric"=>{let metric=request.metric.ok_or_else(||"metric_required".to_string())?;v::validate_metric(&metric).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":"metric_accepted","plan_id":plan_id,"cache_key":metric.cache_key,"hit":metric.hit,"cached_tokens":metric.cached_tokens,"redacted":true})).map_err(|_|"serialization_failed".into())},"inspect"=>serde_json::to_vec(&serde_json::json!({"status":"available","plan_id":plan_id,"version":expected_version,"idempotency_key_present":!idempotency_key.is_empty(),"redacted":true})).map_err(|_|"serialization_failed".into()),_=>Err("unsupported_prompt_cache_operation".into())}}.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::PromptCachePlanner {
                    operation: event_operation,
                    plan_id: event_plan_id,
                    version: expected_version.saturating_add(1),
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                };
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::DeclarativeRuntimeComponents {
                operation,
                component_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_component_id = component_id.clone();
                let result = async {
                    use crate::declarative_runtime_components as v;
                    use evohime_local_storage::declarative_runtime_components_store as store;
                    if component_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 { return Err("invalid_declarative_component_request".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let db = journal.database().lock().await;
                    let request: DeclarativeComponentRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_declarative_component_payload".to_string())?;
                    match operation.as_str() {
                        "save" => {
                            let config = request.config.ok_or_else(|| "config_required".to_string())?;
                            let providers = request.registry.ok_or_else(|| "registry_required".to_string())?;
                            if config.component_id != component_id { return Err("component_id_mismatch".into()); }
                            v::validate(&config, &providers).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&config).map_err(|_| "serialization_failed".to_string())?;
                            if !store::save(db.connection(), store::SaveInput { id: &component_id, expected: expected_version, revision: config.revision, json: &json, hash: &config.content_hash, idem: &idempotency_key, now: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())? { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"saved","component_id":component_id,"revision":config.revision,"content_hash":config.content_hash,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "inspect" => {
                            let (revision, _, hash) = store::load(db.connection(), &component_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "component_not_found".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"stored","component_id":component_id,"revision":revision,"content_hash":hash,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "rehydrate" => {
                            let (_, json, _) = store::load(db.connection(), &component_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "component_not_found".to_string())?;
                            let config: v::ComponentConfig = serde_json::from_slice(&json).map_err(|_| "corrupt_component".to_string())?;
                            let providers = request.registry.ok_or_else(|| "registry_required".to_string())?;
                            let policy = request.policy.ok_or_else(|| "policy_required".to_string())?;
                            v::rehydrate(&config, &providers, &policy).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"status":"rehydrated","component_id":component_id,"revision":config.revision,"state":config.runtime_state,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        "transition" => {
                            let (revision, json, _) = store::load(db.connection(), &component_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "component_not_found".to_string())?;
                            let mut config: v::ComponentConfig = serde_json::from_slice(&json).map_err(|_| "corrupt_component".to_string())?;
                            let next = request.state.ok_or_else(|| "state_required".to_string())?;
                            v::validate_transition(&config.runtime_state, &next).map_err(|e| e.to_string())?;
                            config.runtime_state = next; config.revision = revision.saturating_add(1); config.content_hash = v::canonical_hash(&config).map_err(|e| e.to_string())?;
                            let out = serde_json::to_vec(&config).map_err(|_| "serialization_failed".to_string())?;
                            if !store::save(db.connection(), store::SaveInput { id: &component_id, expected: expected_version.max(revision), revision: config.revision, json: &out, hash: &config.content_hash, idem: &idempotency_key, now: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())? { return Err("stale_version_or_idempotency_conflict".into()); }
                            serde_json::to_vec(&serde_json::json!({"status":"transitioned","component_id":component_id,"revision":config.revision,"state":config.runtime_state,"redacted":true})).map_err(|_| "serialization_failed".into())
                        }
                        _ => Err("unsupported_declarative_component_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::DeclarativeRuntimeComponents {
                    operation: event_operation,
                    component_id: event_component_id,
                    version: expected_version.saturating_add(1),
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::GuidedCalibrationSessions {
                operation,
                session_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_session_id = session_id.clone();
                let result = async {
                    use crate::guided_calibration_sessions as v;
                    use evohime_local_storage::guided_calibration_sessions_store as store;
                    if session_id.is_empty() || idempotency_key.is_empty() || idempotency_key.len() > 128 { return Err("invalid_calibration_request".into()); }
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?; let db = journal.database().lock().await;
                    let request: CalibrationRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_calibration_payload".to_string())?;
                    match operation.as_str() {
                        "create" => { let owner=request.owner_scope.as_deref().ok_or_else(||"owner_scope_required".to_string())?; let subject=request.subject_ref.as_deref().ok_or_else(||"subject_ref_required".to_string())?; let actor=request.actor_ref.as_deref().ok_or_else(||"actor_ref_required".to_string())?; let policy=request.policy_snapshot_hash.as_deref().ok_or_else(||"policy_snapshot_hash_required".to_string())?; let s=v::new_session(session_id.clone(),owner.into(),subject.into(),actor.into(),policy.into()); v::validate_session(&s).map_err(|e|e.to_string())?; let json=serde_json::to_vec(&s).map_err(|_|"serialization_failed".to_string())?; if !store::save(db.connection(),store::SaveInput{id:&session_id,expected:expected_version,revision:s.revision,json:&json,dataset_hash:&s.dataset_hash,idempotency_key:&idempotency_key,now:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())? {return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"created","session_id":session_id,"revision":s.revision,"dataset_hash":s.dataset_hash,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "inspect" | "replay" => { let (revision,json,dataset)=store::load(db.connection(),&session_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"session_not_found".to_string())?; let s:v::CalibrationSession=serde_json::from_slice(&json).map_err(|_|"corrupt_calibration_session".to_string())?; v::validate_session(&s).map_err(|e|e.to_string())?; serde_json::to_vec(&serde_json::json!({"status":"available","session_id":session_id,"revision":revision,"iteration_count":s.iterations.len(),"candidate_count":s.candidates.len(),"dataset_hash":dataset,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "iteration" => { let (revision, json, _)=store::load(db.connection(),&session_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"session_not_found".to_string())?; let mut s:v::CalibrationSession=serde_json::from_slice(&json).map_err(|_|"corrupt_calibration_session".to_string())?; let i=request.iteration.ok_or_else(||"iteration_required".to_string())?; v::add_iteration(&mut s,i).map_err(|e|e.to_string())?; let out=serde_json::to_vec(&s).map_err(|_|"serialization_failed".to_string())?; if !store::save(db.connection(),store::SaveInput{id:&session_id,expected:expected_version.max(revision),revision:s.revision,json:&out,dataset_hash:&s.dataset_hash,idempotency_key:&idempotency_key,now:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())? {return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"iteration_recorded","session_id":session_id,"revision":s.revision,"iteration_count":s.iterations.len(),"dataset_hash":s.dataset_hash,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "consolidate" => { let (revision,json,_)=store::load(db.connection(),&session_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"session_not_found".to_string())?; let mut s:v::CalibrationSession=serde_json::from_slice(&json).map_err(|_|"corrupt_calibration_session".to_string())?; let pattern=request.pattern_key.as_deref().ok_or_else(||"pattern_key_required".to_string())?; let guidance=request.guidance_text.as_deref().ok_or_else(||"guidance_required".to_string())?; let candidate_id=request.candidate_id.as_deref().ok_or_else(||"candidate_id_required".to_string())?; let c=v::consolidate(&s,candidate_id,pattern,guidance).map_err(|e|e.to_string())?; let source=s.iterations.iter().filter(|i|c.source_iteration_ids.contains(&i.iteration_id)).collect::<Vec<_>>(); let evidence=source.iter().filter_map(|i|i.feedback.as_ref().map(|f|crate::refinement::EvidenceRefV1{source_id:f.provenance_ref.clone(),source_kind:"calibration_feedback".into(),owner_scope:crate::refinement::OwnerScope::Session,content_hash:f.correction_hash.clone(),observed_at_ms:crate::task_memory::now_millis() as i64,redacted:true})).collect::<Vec<_>>(); let task_ids=source.iter().map(|i|i.task_ref.clone()).collect::<Vec<_>>(); let rc=crate::refinement::RefinementCandidateV1::new(crate::refinement::RefinementCandidateInput{id:c.refinement_candidate_id.clone(),kind:crate::refinement::CandidateKind::Memory,target:"session_guidance".into(),scope:crate::refinement::OwnerScope::Session,pattern_key:pattern.into(),title:"guided calibration guidance".into(),rationale:"human-confirmed repeated feedback".into(),proposed_content:guidance.into(),source_task_ids:task_ids,evidence,policy_snapshot_hash:s.policy_snapshot_hash.clone(),idempotency_key:idempotency_key.clone()}).map_err(|e|e.to_string())?; crate::refinement::RefinementService::new(db.connection(),crate::refinement::AdmissionPolicy::default()).propose_memory(rc,crate::task_memory::now_millis() as i64)?; s.candidates.push(c.clone()); s.revision=revision.saturating_add(1); s.dataset_hash=v::dataset_hash(&s).map_err(|e|e.to_string())?; let out=serde_json::to_vec(&s).map_err(|_|"serialization_failed".to_string())?; if !store::save(db.connection(),store::SaveInput{id:&session_id,expected:revision,revision:s.revision,json:&out,dataset_hash:&s.dataset_hash,idempotency_key:&idempotency_key,now:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())? {return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"candidate_proposed_for_refinement","session_id":session_id,"candidate_id":c.candidate_id,"guidance_hash":c.guidance_hash,"refinement_candidate_id":c.refinement_candidate_id,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        "close" => { let (revision,json,_)=store::load(db.connection(),&session_id).map_err(|_|"storage_failed".to_string())?.ok_or_else(||"session_not_found".to_string())?; let mut s:v::CalibrationSession=serde_json::from_slice(&json).map_err(|_|"corrupt_calibration_session".to_string())?; s.status=if request.cancelled{v::SessionStatus::Cancelled}else{v::SessionStatus::Completed}; s.revision=revision.saturating_add(1); let out=serde_json::to_vec(&s).map_err(|_|"serialization_failed".to_string())?; if !store::save(db.connection(),store::SaveInput{id:&session_id,expected:revision,revision:s.revision,json:&out,dataset_hash:&s.dataset_hash,idempotency_key:&idempotency_key,now:crate::task_memory::now_millis() as i64}).map_err(|_|"storage_failed".to_string())? {return Err("stale_version_or_idempotency_conflict".into())}; serde_json::to_vec(&serde_json::json!({"status":"closed","session_id":session_id,"revision":s.revision,"redacted":true})).map_err(|_|"serialization_failed".into()) }
                        _ => Err("unsupported_calibration_operation".into()),
                    }
                }.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::GuidedCalibrationSessions {
                    operation: event_operation,
                    session_id: event_session_id,
                    version: expected_version.saturating_add(1),
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::ExtensionConformanceKit {
                operation,
                subject_id,
                payload,
                expected_version,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_subject_id = subject_id.clone();
                let result=async { use crate::extension_conformance_kit as v; if subject_id.is_empty()||idempotency_key.is_empty()||expected_version>u64::MAX-1{return Err("invalid_conformance_request".into())}; let value:serde_json::Value=serde_json::from_slice(&payload).map_err(|_|"invalid_conformance_payload".to_string())?; match operation.as_str(){"run"=>{let d:v::ExtensionDescriptor=serde_json::from_value(value.get("descriptor").cloned().ok_or_else(||"descriptor_required".to_string())?).map_err(|_|"invalid_descriptor".to_string())?;let p:v::ConformanceProbe=serde_json::from_value(value.get("probe").cloned().ok_or_else(||"probe_required".to_string())?).map_err(|_|"invalid_probe".to_string())?;let fault:v::FaultMode=serde_json::from_value(value.get("fault").cloned().unwrap_or(serde_json::json!("none"))).map_err(|_|"invalid_fault".to_string())?;if d.subject_id!=subject_id{return Err("subject_id_mismatch".into())};let report=v::run(&d,&p,fault).map_err(|e|e.to_string())?;serde_json::to_vec(&report).map_err(|_|"serialization_failed".into())},"register"=>{let descriptors:Vec<v::ExtensionDescriptor>=serde_json::from_value(value.get("descriptors").cloned().ok_or_else(||"descriptors_required".to_string())?).map_err(|_|"invalid_descriptors".to_string())?;let fault:v::FaultMode=serde_json::from_value(value.get("fault").cloned().unwrap_or(serde_json::json!("none"))).map_err(|_|"invalid_fault".to_string())?;let mut t=v::RegistrationTransaction::default();for d in descriptors{t.stage(d).map_err(|e|e.to_string())?};let committed=t.commit(fault).map_err(|e|e.to_string())?;serde_json::to_vec(&serde_json::json!({"status":"registered_ephemeral","count":committed.len(),"redacted":true})).map_err(|_|"serialization_failed".into())},"inspect"=>serde_json::to_vec(&serde_json::json!({"status":"available","schema_version":v::SCHEMA_VERSION,"kinds":["integration_provider","external_agent_adapter","workbench","ui_extension","declarative_component_provider"],"production_execution":false,"redacted":true})).map_err(|_|"serialization_failed".into()),_=>Err("unsupported_conformance_operation".into())}}.await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|b| String::from_utf8(b.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::ExtensionConformanceKit {
                    operation: event_operation,
                    subject_id: event_subject_id,
                    version: expected_version.saturating_add(1),
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::PersistentAgentOrganizationRegistry {
                operation,
                agent_id,
                owner_scope,
                actor,
                payload,
                expected_revision,
                idempotency_key,
                reply,
            } => {
                let event_operation = operation.clone();
                let event_agent_id = agent_id.clone();
                let result = async {
                    let journal = state
                        .lock()
                        .await
                        .journal
                        .clone()
                        .ok_or_else(|| "storage journal is not configured".to_string())?;
                    journal
                        .persistent_agent_registry_command(
                            crate::persistent_agent_registry::RegistryCommand {
                                operation,
                                agent_id,
                                owner_scope,
                                actor,
                                payload,
                                expected_revision,
                                idempotency_key,
                            },
                        )
                        .await
                        .map_err(|error| error.to_string())
                }
                .await;
                let projection_json = result
                    .as_ref()
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                    .unwrap_or_else(|| "{}".into());
                let event = CoreEvent::PersistentAgentOrganizationRegistry {
                    agent_id: event_agent_id,
                    operation: event_operation,
                    revision: expected_revision,
                    projection_json,
                };
                if let Some(journal) = state.lock().await.journal.clone() {
                    let _ = journal.record(&event).await;
                }
                let _ = state.lock().await.events.send(event);
                let _ = reply.send(result);
            }
            CoreCommand::GetMemory { id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let record = journal
                        .get_memory(&id)
                        .await?
                        .ok_or_else(|| "memory record was not found".to_string())?;
                    let chain = journal.memory_supersession_chain(&id, 32).await?;
                    let body = memory_record_body_json(&record)?;
                    serde_json::to_vec(&serde_json::json!({
                        "record": body,
                        "supersession_chain": chain,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListMemoryPending {
                scope_kind,
                project_id,
                secondary_id,
                limit,
                workspace_path,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let store_scope = memory_store_scope(&scope_kind)?;
                    let scope_id = memory_scope_id(&workspace_path, &project_id, &secondary_id);
                    // Expiry is applied before reading so an expired record is
                    // never reported as still awaiting a decision.
                    journal
                        .expire_due_memory(&memory_now_ms().to_string())
                        .await?;
                    let pending = journal
                        .list_memory_by_state(
                            store_scope,
                            &scope_id,
                            crate::memory_extraction::ConfirmationState::PendingConfirmation
                                .as_str(),
                            limit,
                        )
                        .await?;
                    let mut counts = journal
                        .count_memory_by_state(store_scope, &scope_id)
                        .await?
                        .into_iter()
                        .collect::<std::collections::BTreeMap<String, i64>>();
                    let mut pending = pending;
                    // Услышанное живёт в своём scope: речь у стола не
                    // принадлежит рабочему каталогу. Но очередь подтверждения
                    // у пользователя одна, и прятать ambient-кандидатов от
                    // неё значило бы, что подтвердить их негде.
                    let ambient_scope = evohime_local_storage::memory_store::MemoryScope::Workspace;
                    if !(store_scope == ambient_scope && scope_id == AMBIENT_MEMORY_SCOPE_ID) {
                        pending.extend(
                            journal
                                .list_memory_by_state(
                                    ambient_scope,
                                    AMBIENT_MEMORY_SCOPE_ID,
                                    crate::memory_extraction::ConfirmationState::PendingConfirmation
                                        .as_str(),
                                    limit,
                                )
                                .await?,
                        );
                        for (state, count) in journal
                            .count_memory_by_state(ambient_scope, AMBIENT_MEMORY_SCOPE_ID)
                            .await?
                        {
                            *counts.entry(state).or_insert(0) += count;
                        }
                    }
                    let counts = counts
                        .into_iter()
                        .map(|(state, count)| (state, serde_json::json!(count)))
                        .collect::<serde_json::Map<_, _>>();
                    let records = pending
                        .iter()
                        .map(memory_record_to_json)
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({
                        "records": records,
                        "counts": counts,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetMemoryConflicts {
                scope_kind,
                project_id,
                secondary_id,
                limit,
                workspace_path,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let store_scope = memory_store_scope(&scope_kind)?;
                    let scope_id = memory_scope_id(&workspace_path, &project_id, &secondary_id);
                    let pending = journal
                        .list_memory_by_state(
                            store_scope,
                            &scope_id,
                            crate::memory_extraction::ConfirmationState::PendingConfirmation
                                .as_str(),
                            limit,
                        )
                        .await?;
                    let mut conflicts = Vec::new();
                    for candidate in &pending {
                        let active = journal
                            .memory_conflict_candidates(
                                store_scope,
                                &scope_id,
                                &candidate.extraction.kind,
                                100,
                            )
                            .await?;
                        let Some(existing) = memory_conflicting_record(candidate, &active) else {
                            continue;
                        };
                        let chain = journal.memory_supersession_chain(&existing.id, 32).await?;
                        conflicts.push(serde_json::json!({
                            "pending": memory_record_to_json(candidate)?,
                            "active": memory_record_to_json(existing)?,
                            "conflict_key": format!(
                                "{}|{}|{}",
                                candidate.extraction.kind,
                                memory_conflict_subject(candidate),
                                candidate.scope.as_str()
                            ),
                            "supersession_chain": chain,
                        }));
                    }
                    serde_json::to_vec(&serde_json::json!({ "conflicts": conflicts }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ConfirmMemory {
                ids,
                approval_id,
                idempotency_key,
                reply,
            } => {
                let result = Self::apply_memory_decision(
                    &state,
                    ids,
                    approval_id,
                    idempotency_key,
                    crate::memory_api::MemoryOperation::Confirm,
                    crate::memory_extraction::ConfirmationState::Confirmed,
                    "memory.confirmed",
                )
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RejectMemory {
                ids,
                approval_id,
                idempotency_key,
                reply,
            } => {
                let result = Self::apply_memory_decision(
                    &state,
                    ids,
                    approval_id,
                    idempotency_key,
                    crate::memory_api::MemoryOperation::Reject,
                    crate::memory_extraction::ConfirmationState::Rejected,
                    "memory.rejected",
                )
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ReviseMemoryCandidate {
                id,
                statement,
                session_only,
                session_id,
                approval_id,
                idempotency_key,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    crate::memory_api::Approval::new(
                        approval_id.clone(),
                        crate::memory_api::MemoryOperation::Update,
                    )
                    .map_err(|error| error.to_string())?;
                    validate_memory_idempotency_key(&idempotency_key)?;
                    let record = journal
                        .get_memory(&id)
                        .await?
                        .ok_or_else(|| "memory record was not found".to_string())?;
                    let statement = if statement.trim().is_empty() {
                        record.content.clone()
                    } else {
                        statement
                    };

                    if session_only {
                        // "Только на эту сессию": no persistent row survives.
                        // The candidate is rejected outright and the statement
                        // lives on solely as a session note that expires by
                        // itself, so it can never reach long-term retrieval.
                        if session_id.trim().is_empty() {
                            return Err(
                                "session_id is required for a session-only note".to_string()
                            );
                        }
                        let now_ms = memory_now_ms();
                        let expires_at = now_ms
                            .saturating_add(crate::memory_extraction::SESSION_SUMMARY_GRACE_MS);
                        journal
                            .save_memory_session_note(SessionMemoryNote {
                                id: &uuid::Uuid::new_v4().to_string(),
                                session_id: &session_id,
                                scope: record.scope,
                                scope_id: &record.scope_id,
                                kind: &record.extraction.kind,
                                statement: &statement,
                                created_at: &now_ms.to_string(),
                                expires_at: &expires_at.to_string(),
                            })
                            .await?;
                        let actual = journal
                            .transition_memory_state(
                                &id,
                                crate::memory_extraction::ConfirmationState::Rejected.as_str(),
                            )
                            .await?;
                        Self::record_audit(
                            &state,
                            crate::audit::AuditKind::Approval,
                            id.clone(),
                            "memory.session_only",
                            [
                                ("memory_id".to_owned(), id.clone()),
                                ("session_id".to_owned(), session_id.clone()),
                                ("approval_id".to_owned(), approval_id),
                                ("idempotency_key".to_owned(), idempotency_key),
                            ],
                        )
                        .await;
                        return serde_json::to_vec(&serde_json::json!({
                            "id": id,
                            "state": actual,
                            "session_only": true,
                            "expires_at_ms": expires_at,
                        }))
                        .map_err(|error| error.to_string());
                    }

                    journal.revise_pending_memory(&id, &statement).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        id.clone(),
                        "memory.revised",
                        [
                            ("memory_id".to_owned(), id.clone()),
                            ("approval_id".to_owned(), approval_id),
                            ("idempotency_key".to_owned(), idempotency_key),
                        ],
                    )
                    .await;
                    let revised = journal
                        .get_memory(&id)
                        .await?
                        .ok_or_else(|| "memory record was not found".to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "record": memory_record_to_json(&revised)?,
                        "session_only": false,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SupersedeMemory {
                old_id,
                new_id,
                reason,
                approval_id,
                idempotency_key,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    crate::memory_api::Approval::new(
                        approval_id.clone(),
                        crate::memory_api::MemoryOperation::Supersede,
                    )
                    .map_err(|error| error.to_string())?;
                    validate_memory_idempotency_key(&idempotency_key)?;
                    // The reason is a bounded enum, not free text: the chain
                    // has to explain itself without carrying user content.
                    let reason = crate::memory_extraction::SupersessionReason::parse(&reason)
                        .ok_or_else(|| format!("unsupported supersession reason: {reason}"))?;
                    journal
                        .supersede_memory(&old_id, &new_id, reason.as_str())
                        .await?;
                    let chain = journal.memory_supersession_chain(&new_id, 32).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        new_id.clone(),
                        "memory.superseded",
                        [
                            ("old_memory_id".to_owned(), old_id.clone()),
                            ("new_memory_id".to_owned(), new_id.clone()),
                            ("reason".to_owned(), reason.as_str().to_owned()),
                            ("approval_id".to_owned(), approval_id),
                            ("idempotency_key".to_owned(), idempotency_key),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({
                        "old_id": old_id,
                        "new_id": new_id,
                        "reason": reason.as_str(),
                        "supersession_chain": chain,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::InstallCapability {
                manifest_json,
                install_source,
                source_path,
                expected_content_hash,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if install_source != "local_archive" && install_source != "https_archive" {
                        return Err(format!(
                            "unsupported capability install source: {install_source}"
                        ));
                    }
                    let candidate: crate::capability_registry::CapabilityManifest =
                        serde_json::from_str(&manifest_json).map_err(|error| error.to_string())?;
                    candidate.validate().map_err(|error| error.to_string())?;
                    let expected_manifest_source = if install_source == "https_archive" {
                        crate::capability_registry::InstallSource::HttpsArchive
                    } else {
                        crate::capability_registry::InstallSource::LocalArchive
                    };
                    if candidate.install.source != expected_manifest_source {
                        return Err(
                            "manifest install source does not match the requested installer"
                                .to_string(),
                        );
                    }
                    if install_source == "https_archive" {
                        verify_https_capability_archive(&source_path, &expected_content_hash)
                            .await?;
                    }
                    let existing_records = journal
                        .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                        .await?;
                    let mut existing_manifests = Vec::with_capacity(existing_records.len());
                    for record in &existing_records {
                        let manifest: crate::capability_registry::CapabilityManifest =
                            serde_json::from_str(&record.manifest_json)
                                .map_err(|error| error.to_string())?;
                        existing_manifests.push(manifest);
                    }
                    if let Some(current) = existing_manifests
                        .iter()
                        .find(|manifest| manifest.name == candidate.name)
                    {
                        crate::capability_registry::validate_update(current, &candidate)
                            .map_err(|error| error.to_string())?;
                    } else {
                        let mut proposed = existing_manifests.clone();
                        proposed.push(candidate.clone());
                        crate::capability_registry::validate_registry(&proposed)
                            .map_err(|error| error.to_string())?;
                    }
                    let store_record =
                        evohime_local_storage::capability_store::CapabilityManifestRecord {
                            id: candidate.name.clone(),
                            kind: capability_manifest_kind(&candidate),
                            version: candidate.version.clone(),
                            risk_class: capability_risk_class_str(candidate.risk_class).to_string(),
                            content_hash: candidate.content_hash.clone(),
                            manifest_json: serde_json::to_string(&candidate)
                                .map_err(|error| error.to_string())?,
                        };
                    journal.save_capability_manifest(&store_record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        candidate.name.clone(),
                        "capability.installed",
                        [
                            ("manifest_id".to_owned(), candidate.name.clone()),
                            ("version".to_owned(), candidate.version.clone()),
                            ("install_source".to_owned(), install_source),
                            ("source_path".to_owned(), source_path),
                            (
                                "expected_content_hash".to_owned(),
                                if expected_content_hash.is_empty() {
                                    "not_provided".to_owned()
                                } else {
                                    expected_content_hash
                                },
                            ),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "manifest": candidate }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListCapabilities { limit, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let records = journal.list_capability_manifests(limit).await?;
                    let manifests = records
                        .iter()
                        .map(|record| {
                            serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                                &record.manifest_json,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({ "manifests": manifests }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::MatchCapabilities {
                intent,
                required_tools,
                required_domains,
                requested_risk,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let requested_risk = parse_capability_risk_class(&requested_risk)?;
                    let records = journal
                        .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                        .await?;
                    let manifests = records
                        .iter()
                        .map(|record| {
                            serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                                &record.manifest_json,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let query = crate::capability_registry::MatchQuery {
                        intent,
                        required_tools,
                        required_domains,
                        requested_risk,
                    };
                    let matches =
                        crate::capability_registry::match_capabilities(&manifests, &query)
                            .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({ "matches": matches }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RemoveCapability { id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let removed = journal.remove_capability_manifest(&id).await?;
                    if !removed {
                        return Err("capability manifest was not found".to_string());
                    }
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        id.clone(),
                        "capability.removed",
                        [("manifest_id".to_owned(), id.clone())],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "id": id, "removed": true }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetCapabilitySelection {
                task_id,
                intent,
                required_tools,
                required_domains,
                requested_risk,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let requested_risk = parse_capability_risk_class(&requested_risk)?;
                    let records = journal
                        .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                        .await?;
                    let manifests = records
                        .iter()
                        .map(|record| {
                            serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                                &record.manifest_json,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let query = crate::capability_registry::MatchQuery {
                        intent,
                        required_tools,
                        required_domains,
                        requested_risk,
                    };
                    let stored = journal.get_capability_selection(&task_id).await?;
                    let current_state = stored
                        .map(|record| {
                            serde_json::from_str::<
                                crate::capability_selection::CapabilitySelectionState,
                            >(&record.state_json)
                            .map_err(|error| error.to_string())
                        })
                        .transpose()?;
                    let auto_match =
                        crate::capability_selection::select_for_task(&manifests, &query);
                    let reconciled = crate::capability_selection::reconcile_with_pin(
                        current_state.as_ref(),
                        auto_match,
                    )
                    .map_err(|error| error.to_string())?;
                    let state_json = serde_json::to_string(&reconciled)
                        .map_err(|error| error.to_string())?;
                    let selection_record =
                        evohime_local_storage::capability_selection_store::CapabilitySelectionRecord {
                            task_id: task_id.clone(),
                            origin: capability_selection_origin_to_store(reconciled.origin),
                            manifest_name: reconciled.selection.manifest_name.clone(),
                            state_json,
                        };
                    journal.save_capability_selection(&selection_record).await?;
                    serde_json::to_vec(&reconciled).map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::PinCapabilitySelection { task_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let stored = journal
                        .get_capability_selection(&task_id)
                        .await?
                        .ok_or_else(|| {
                            "no capability selection recorded for this task yet".to_string()
                        })?;
                    let current_state = serde_json::from_str::<
                        crate::capability_selection::CapabilitySelectionState,
                    >(&stored.state_json)
                    .map_err(|error| error.to_string())?;
                    let pinned = crate::capability_selection::pin(current_state);
                    let state_json =
                        serde_json::to_string(&pinned).map_err(|error| error.to_string())?;
                    let selection_record =
                        evohime_local_storage::capability_selection_store::CapabilitySelectionRecord {
                            task_id: task_id.clone(),
                            origin: capability_selection_origin_to_store(pinned.origin),
                            manifest_name: pinned.selection.manifest_name.clone(),
                            state_json,
                        };
                    journal.save_capability_selection(&selection_record).await?;
                    serde_json::to_vec(&pinned).map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ReplaceCapabilitySelection {
                task_id,
                manifest_name,
                intent,
                required_tools,
                required_domains,
                requested_risk,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let requested_risk = parse_capability_risk_class(&requested_risk)?;
                    let records = journal
                        .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                        .await?;
                    let manifests = records
                        .iter()
                        .map(|record| {
                            serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                                &record.manifest_json,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let query = crate::capability_registry::MatchQuery {
                        intent,
                        required_tools,
                        required_domains,
                        requested_risk,
                    };
                    let replaced = crate::capability_selection::replace(
                        &manifests,
                        &query,
                        &manifest_name,
                    )
                    .map_err(|error| error.to_string())?;
                    let state_json =
                        serde_json::to_string(&replaced).map_err(|error| error.to_string())?;
                    let selection_record =
                        evohime_local_storage::capability_selection_store::CapabilitySelectionRecord {
                            task_id: task_id.clone(),
                            origin: capability_selection_origin_to_store(replaced.origin),
                            manifest_name: replaced.selection.manifest_name.clone(),
                            state_json,
                        };
                    journal.save_capability_selection(&selection_record).await?;
                    serde_json::to_vec(&replaced).map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RequestChildHandoff {
                handoff_id,
                task_id,
                kind,
                from_role,
                from_name,
                to_role,
                to_name,
                purpose,
                payload,
                sequence,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let parsed_kind = handoff_kind_from_str(&kind)?;
                    let from = role_identity_from_parts(&from_role, &from_name)?;
                    let to = role_identity_from_parts(&to_role, &to_name)?;
                    let handoff_payload = crate::child_roles::HandoffPayload::new(payload)
                        .map_err(|error| error.to_string())?;
                    let envelope = crate::child_roles::HandoffEnvelope::new(
                        crate::child_roles::HandoffEnvelopeInput {
                            handoff_id: handoff_id.clone(),
                            task_id: task_id.clone(),
                            kind: parsed_kind,
                            from: from.clone(),
                            to: to.clone(),
                            purpose,
                            payload: handoff_payload,
                            sequence,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    let record = evohime_local_storage::child_store::HandoffRecord {
                        handoff_id: envelope.handoff_id.clone(),
                        task_id: envelope.task_id.clone(),
                        kind: handoff_kind_str(envelope.kind).to_string(),
                        status: handoff_status_str(envelope.status).to_string(),
                        from_role: role_identity_display(&from),
                        to_role: role_identity_display(&to),
                        sequence: envelope.sequence,
                        envelope_json: envelope.to_deterministic_json(),
                    };
                    journal.save_child_handoff(&record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        task_id.clone(),
                        "child.handoff.requested",
                        [
                            ("handoff_id".to_owned(), envelope.handoff_id.clone()),
                            ("task_id".to_owned(), task_id),
                            ("from_role".to_owned(), record.from_role.clone()),
                            ("to_role".to_owned(), record.to_role.clone()),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "handoff": envelope }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListChildHandoffs {
                task_id,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let records = journal.list_child_handoffs(&task_id, limit).await?;
                    let handoffs = records
                        .iter()
                        .map(|record| {
                            serde_json::from_str::<crate::child_roles::HandoffEnvelope>(
                                &record.envelope_json,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({
                        "task_id": task_id,
                        "handoffs": handoffs,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SubmitChildRequest {
                child_task_id,
                parent_task_id,
                role,
                kind,
                reduced_context,
                max_output_bytes,
                requested_capabilities,
                parent_is_child,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let parsed_kind = child_task_kind_from_str(&kind)?;
                    let request = crate::child_runtime::ChildTaskRequest {
                        child_task_id: child_task_id.clone(),
                        parent_task_id: parent_task_id.clone(),
                        role: role.clone(),
                        kind: parsed_kind,
                        reduced_context,
                        max_output_bytes: max_output_bytes as usize,
                        requested_capabilities,
                        parent_is_child,
                    };
                    // The real bounded contract runs here: rejects nested
                    // children, any non-read-only requested capability, and
                    // oversized context/output. This is the same
                    // `ChildTaskRequest::validate` used by the pure unit
                    // tests, now enforced on the live IPC path.
                    request.validate().map_err(|error| error.to_string())?;
                    let parent_sequence =
                        journal.next_child_parent_sequence(&parent_task_id).await?;
                    let typed_correlation = crate::child_contracts::CorrelationContext::new(
                        crate::child_contracts::CorrelationId::new(parent_task_id.clone())
                            .map_err(|error| error.to_string())?,
                        crate::child_contracts::CorrelationId::new(child_task_id.clone())
                            .map_err(|error| error.to_string())?,
                        parent_sequence,
                    );
                    let typed_request = crate::child_contracts::TypedChildTaskRequest::new(
                        child_task_id.clone(),
                        parent_task_id.clone(),
                        role.clone(),
                        format!("{kind} child workflow"),
                        typed_correlation,
                    )
                    .map_err(|error| error.to_string())?
                    .with_context(request.reduced_context.clone())
                    .map_err(|error| error.to_string())?
                    .with_max_output_bytes(request.max_output_bytes)
                    .map_err(|error| error.to_string())?
                    .with_capabilities(request.requested_capabilities.clone())
                    .map_err(|error| error.to_string())?;
                    crate::child_contracts::validate_contract_version(
                        typed_request.contract_version,
                        crate::child_contracts::CONTRACT_VERSION,
                    )
                    .map_err(|error| error.to_string())?;
                    typed_request
                        .validate()
                        .map_err(|error| error.to_string())?;
                    let request_json =
                        serde_json::to_string(&request).map_err(|error| error.to_string())?;
                    let record = evohime_local_storage::child_store::ChildTaskRequestRecord {
                        child_task_id: request.child_task_id.clone(),
                        parent_task_id: request.parent_task_id.clone(),
                        role: request.role.clone(),
                        kind: child_task_kind_str(request.kind).to_string(),
                        request_json,
                    };
                    journal.save_child_task_request(&record).await?;
                    let now_ms = task_memory::now_millis() as i64;
                    journal
                        .save_coordinator_checkpoint(
                            &evohime_local_storage::child_store::CoordinatorCheckpointRecord {
                                schema_version: 1,
                                child_task_id: request.child_task_id.clone(),
                                parent_task_id: request.parent_task_id.clone(),
                                revision: 0,
                                state: "created".into(),
                                failure_reason: None,
                                dead_letter: false,
                                report_json: None,
                                evidence_locators_json: None,
                                provenance_hashes_json: None,
                                parent_sequence: parent_sequence as i64,
                                lease_deadline_monotonic_ms: Some(
                                    now_ms + crate::child_workflow::DEFAULT_LEASE_MS as i64,
                                ),
                                lease_created_monotonic_ms: Some(now_ms),
                                lease_clock_boot_id: Some("current".into()),
                                lease_holder_process_id: Some(std::process::id().to_string()),
                                last_transition_event: "child.request.submitted".into(),
                                last_transition_at_ms: now_ms,
                                created_at_ms: now_ms,
                            },
                        )
                        .await?;
                    let _ = state
                        .lock()
                        .await
                        .events
                        .send(CoreEvent::ChildWorkflowProjection {
                            task_id: request.parent_task_id.clone(),
                            projection: crate::child_workflow::ChildProjection {
                                event_id: format!("{}:created", request.child_task_id),
                                parent_task_id: request.parent_task_id.clone(),
                                child_task_id: request.child_task_id.clone(),
                                role: request.role.clone(),
                                revision: 0,
                                state: crate::child_workflow::CoordinatorState::Created,
                                reason_code: None,
                                parent_sequence,
                                budget: typed_request.budget.clone(),
                                lease_live: false,
                                dead_letter: false,
                            },
                        });
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        parent_task_id.clone(),
                        "child.request.submitted",
                        [
                            ("child_task_id".to_owned(), request.child_task_id.clone()),
                            ("parent_task_id".to_owned(), parent_task_id.clone()),
                            ("role".to_owned(), request.role.clone()),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "request": request }))
                        .map_err(|error| error.to_string())
                }
                .await;
                if let Err(error) = &result {
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        parent_task_id.clone(),
                        "child.contract.rejected",
                        [("reason".to_owned(), error.clone())],
                    )
                    .await;
                }
                let _ = reply.send(result);
            }
            CoreCommand::SubmitChildReport {
                child_task_id,
                status,
                summary,
                findings,
                sources,
                confidence_percent,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let parsed_status = child_report_status_from_str(&status)?;
                    let confidence_percent: u8 = u8::try_from(confidence_percent)
                        .map_err(|_| "confidence_percent must be between 0 and 255".to_string())?;
                    let report = crate::child_runtime::ChildReport {
                        child_task_id: child_task_id.clone(),
                        status: parsed_status,
                        summary,
                        findings,
                        sources,
                        confidence_percent,
                    };
                    let stored_request = journal
                        .get_child_task_request(&child_task_id)
                        .await?
                        .ok_or_else(|| {
                            "no matching child task request found for child_task_id".to_string()
                        })?;
                    let parent_sequence = journal
                        .get_coordinator_checkpoint(&child_task_id)
                        .await?
                        .map(|checkpoint| checkpoint.parent_sequence as u64)
                        .unwrap_or(0);
                    let request: crate::child_runtime::ChildTaskRequest =
                        serde_json::from_str(&stored_request.request_json)
                            .map_err(|error| error.to_string())?;
                    let typed_request = crate::child_contracts::TypedChildTaskRequest::new(
                        request.child_task_id.clone(),
                        request.parent_task_id.clone(),
                        request.role.clone(),
                        "legacy child workflow",
                        crate::child_contracts::CorrelationContext::new(
                            crate::child_contracts::CorrelationId::new(
                                request.parent_task_id.clone(),
                            )
                            .map_err(|error| error.to_string())?,
                            crate::child_contracts::CorrelationId::new(
                                request.child_task_id.clone(),
                            )
                            .map_err(|error| error.to_string())?,
                            parent_sequence,
                        ),
                    )
                    .map_err(|error| error.to_string())?
                    .with_context(request.reduced_context.clone())
                    .map_err(|error| error.to_string())?
                    .with_max_output_bytes(request.max_output_bytes)
                    .map_err(|error| error.to_string())?
                    .with_capabilities(request.requested_capabilities.clone())
                    .map_err(|error| error.to_string())?;
                    let typed_status = match report.status {
                        crate::child_runtime::ChildReportStatus::Complete => {
                            crate::child_contracts::TypedReportStatus::Complete
                        }
                        crate::child_runtime::ChildReportStatus::Partial => {
                            crate::child_contracts::TypedReportStatus::Partial
                        }
                        crate::child_runtime::ChildReportStatus::Rejected => {
                            crate::child_contracts::TypedReportStatus::Rejected
                        }
                    };
                    let typed_report = crate::child_contracts::TypedChildReport::new(
                        report.child_task_id.clone(),
                        request.parent_task_id.clone(),
                        typed_request.correlation.clone(),
                        crate::child_contracts::Provenance::new(parent_sequence).mark_completed(),
                    )
                    .map_err(|error| error.to_string())?
                    .with_status(typed_status)
                    .with_summary(report.summary.clone())
                    .map_err(|error| error.to_string())?
                    .with_findings(report.findings.clone())
                    .map_err(|error| error.to_string())?
                    .with_sources(report.sources.clone())
                    .map_err(|error| error.to_string())?
                    .with_confidence(report.confidence_percent);
                    let typed_accepted = journal
                        .accept_typed_child_report(
                            &typed_request,
                            &typed_report,
                            task_memory::now_millis() as i64,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    // The real bounded contract runs here: re-validates the
                    // request, validates the report's own bounds, rejects
                    // secret-like content and duplicate sources, and
                    // rejects a child_task_id mismatch -- the same
                    // `accept_report` used by the pure unit tests, now
                    // enforced on the live IPC path.
                    let accepted = crate::child_runtime::accept_report(&request, &report)
                        .map_err(|error| error.to_string())?;
                    let report_json =
                        serde_json::to_string(&accepted).map_err(|error| error.to_string())?;
                    let record = evohime_local_storage::child_store::ChildReportRecord {
                        child_task_id: accepted.child_task_id.clone(),
                        parent_task_id: stored_request.parent_task_id.clone(),
                        status: child_report_status_str(accepted.status).to_string(),
                        confidence_percent: accepted.confidence_percent,
                        report_json,
                    };
                    journal.save_child_report(&record).await?;
                    let now_ms = task_memory::now_millis() as i64;
                    journal
                        .save_coordinator_checkpoint(
                            &evohime_local_storage::child_store::CoordinatorCheckpointRecord {
                                schema_version: 1,
                                child_task_id: accepted.child_task_id.clone(),
                                parent_task_id: stored_request.parent_task_id.clone(),
                                revision: typed_accepted.revision.unwrap_or(0) as i64,
                                state: "accepted".into(),
                                failure_reason: None,
                                dead_letter: false,
                                report_json: Some(
                                    serde_json::to_string(&typed_accepted)
                                        .map_err(|error| error.to_string())?,
                                ),
                                evidence_locators_json: None,
                                provenance_hashes_json: Some(
                                    serde_json::to_string(&typed_accepted.provenance)
                                        .map_err(|error| error.to_string())?,
                                ),
                                parent_sequence: parent_sequence as i64,
                                lease_deadline_monotonic_ms: None,
                                lease_created_monotonic_ms: None,
                                lease_clock_boot_id: None,
                                lease_holder_process_id: None,
                                last_transition_event: "child.report.accepted".into(),
                                last_transition_at_ms: now_ms,
                                created_at_ms: now_ms,
                            },
                        )
                        .await?;
                    let _ = state
                        .lock()
                        .await
                        .events
                        .send(CoreEvent::ChildWorkflowProjection {
                            task_id: stored_request.parent_task_id.clone(),
                            projection: crate::child_workflow::ChildProjection {
                                event_id: format!("{}:accepted", accepted.child_task_id),
                                parent_task_id: stored_request.parent_task_id.clone(),
                                child_task_id: accepted.child_task_id.clone(),
                                role: request.role.clone(),
                                revision: typed_accepted.revision.unwrap_or(0),
                                state: crate::child_workflow::CoordinatorState::Accepted,
                                reason_code: None,
                                parent_sequence,
                                budget: typed_request.budget.clone(),
                                lease_live: false,
                                dead_letter: false,
                            },
                        });
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        stored_request.parent_task_id.clone(),
                        "child.report.accepted",
                        [
                            ("child_task_id".to_owned(), accepted.child_task_id.clone()),
                            (
                                "parent_task_id".to_owned(),
                                stored_request.parent_task_id.clone(),
                            ),
                            (
                                "confidence_percent".to_owned(),
                                accepted.confidence_percent.to_string(),
                            ),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "report": accepted }))
                        .map_err(|error| error.to_string())
                }
                .await;
                if let Err(error) = &result {
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        child_task_id.clone(),
                        "child.contract.rejected",
                        [("reason".to_owned(), error.clone())],
                    )
                    .await;
                }
                let _ = reply.send(result);
            }
            CoreCommand::IndexWorkspace {
                workspace_path,
                enable_embeddings,
                reply,
            } => {
                let key = workspace_path.replace('\\', "/").to_lowercase();
                let cancellation = CancellationToken::new();
                let (journal, events) = {
                    let mut guard = state.lock().await;
                    if guard.workspace_index_cancellations.contains_key(&key) {
                        let _ = reply.send(Err("workspace index run is already active".into()));
                        return;
                    }
                    guard
                        .workspace_index_cancellations
                        .insert(key.clone(), cancellation.clone());
                    (guard.journal.clone(), guard.events.clone())
                };
                let state_after = Arc::clone(&state);
                let Some(background_permit) = state
                    .lock()
                    .await
                    .background_tasks
                    .try_acquire()
                else {
                    state.lock().await.workspace_index_cancellations.remove(&key);
                    let _ = reply.send(Err("background task capacity is exhausted".into()));
                    return;
                };
                tokio::spawn(async move {
                    let _background_permit = background_permit;
                    let result = async {
                        let journal = journal
                            .ok_or_else(|| "storage journal is not configured".to_string())?;
                        let root = std::path::PathBuf::from(&workspace_path);
                        let progress_path = workspace_path.clone();
                        let summary = journal
                            .index_workspace_knowledge(
                                &root,
                                false,
                                &cancellation,
                                move |progress| {
                                    let _ = events.send(CoreEvent::WorkspaceIndexProgress {
                                        workspace_path: progress_path.clone(),
                                        progress,
                                    });
                                },
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                        let vector_index_id = if enable_embeddings {
                            journal
                                .build_workspace_vector_index(&root, &cancellation)
                                .await
                                .map_err(|error| error.to_string())?
                        } else {
                            None
                        };
                        serde_json::to_vec(&serde_json::json!({
                            "summary": summary,
                            "vector_index_id": vector_index_id,
                        }))
                        .map_err(|error| error.to_string())
                    }
                    .await;
                    state_after
                        .lock()
                        .await
                        .workspace_index_cancellations
                        .remove(&key);
                    let _ = reply.send(result);
                });
            }
            CoreCommand::RebuildIndex {
                workspace_path,
                enable_embeddings,
                reply,
            } => {
                let key = workspace_path.replace('\\', "/").to_lowercase();
                let cancellation = CancellationToken::new();
                let (journal, events) = {
                    let mut guard = state.lock().await;
                    if guard.workspace_index_cancellations.contains_key(&key) {
                        let _ = reply.send(Err("workspace index run is already active".into()));
                        return;
                    }
                    guard
                        .workspace_index_cancellations
                        .insert(key.clone(), cancellation.clone());
                    (guard.journal.clone(), guard.events.clone())
                };
                let state_after = Arc::clone(&state);
                let Some(background_permit) = state
                    .lock()
                    .await
                    .background_tasks
                    .try_acquire()
                else {
                    state.lock().await.workspace_index_cancellations.remove(&key);
                    let _ = reply.send(Err("background task capacity is exhausted".into()));
                    return;
                };
                tokio::spawn(async move {
                    let _background_permit = background_permit;
                    let result = async {
                        let journal = journal
                            .ok_or_else(|| "storage journal is not configured".to_string())?;
                        let root = std::path::PathBuf::from(&workspace_path);
                        let progress_path = workspace_path.clone();
                        let summary = journal
                            .index_workspace_knowledge(
                                &root,
                                true,
                                &cancellation,
                                move |progress| {
                                    let _ = events.send(CoreEvent::WorkspaceIndexProgress {
                                        workspace_path: progress_path.clone(),
                                        progress,
                                    });
                                },
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                        let vector_index_id = if enable_embeddings {
                            journal
                                .build_workspace_vector_index(&root, &cancellation)
                                .await
                                .map_err(|error| error.to_string())?
                        } else {
                            None
                        };
                        serde_json::to_vec(&serde_json::json!({
                            "summary": summary,
                            "vector_index_id": vector_index_id,
                        }))
                        .map_err(|error| error.to_string())
                    }
                    .await;
                    state_after
                        .lock()
                        .await
                        .workspace_index_cancellations
                        .remove(&key);
                    let _ = reply.send(result);
                });
            }
            CoreCommand::CancelWorkspaceIndex {
                workspace_path,
                reply,
            } => {
                let key = workspace_path.replace('\\', "/").to_lowercase();
                let cancelled = state
                    .lock()
                    .await
                    .workspace_index_cancellations
                    .get(&key)
                    .map(|token| {
                        token.cancel();
                        true
                    })
                    .unwrap_or(false);
                let _ = reply.send(
                    serde_json::to_vec(&serde_json::json!({ "cancelled": cancelled }))
                        .map_err(|error| error.to_string()),
                );
            }
            CoreCommand::SearchWorkspaceKnowledge {
                workspace_path,
                query,
                path_filter,
                language_filter,
                hybrid,
                reply,
            } => {
                let (journal, event_sender) = {
                    let state = state.lock().await;
                    (state.journal.clone(), state.events.clone())
                };
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let root = std::path::PathBuf::from(&workspace_path);
                    let progress_sender = event_sender.clone();
                    let progress_workspace = workspace_path.clone();
                    let search = journal
                        .search_workspace_knowledge_with_progress(
                            &root,
                            &query,
                            crate::workspace_rag::QueryFilters {
                                path: path_filter,
                                language: language_filter,
                            },
                            hybrid,
                            move |progress| {
                                let _ =
                                    progress_sender.send(CoreEvent::WorkspaceRetrievalProgress {
                                        workspace_path: progress_workspace.clone(),
                                        progress,
                                    });
                            },
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    let context = journal
                        .build_workspace_evidence_context(&root, &search)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "search": search,
                        "context": context,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetIndexStatus {
                workspace_path,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let status = journal
                        .workspace_index_status(std::path::Path::new(&workspace_path))
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({ "status": status }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SubmitFeedback {
                run_id,
                task_id,
                subject_ref,
                signal,
                correction,
                rejection_reason,
                outcome,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let signal_parsed = match signal.as_str() {
                        "useful" => evohime_local_storage::feedback_store::FeedbackSignal::Useful,
                        "not_useful" => {
                            evohime_local_storage::feedback_store::FeedbackSignal::NotUseful
                        }
                        "neutral" => evohime_local_storage::feedback_store::FeedbackSignal::Neutral,
                        other => return Err(format!("unknown feedback signal: {other}")),
                    };
                    let created_at_ms = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let id = uuid::Uuid::new_v4().to_string();
                    let record = evohime_local_storage::feedback_store::FeedbackRecord::new(
                        evohime_local_storage::feedback_store::FeedbackRecordInput {
                            id: id.clone(),
                            run_id: run_id.clone(),
                            task_id,
                            subject_ref,
                            signal: signal_parsed,
                            correction,
                            rejection_reason,
                            outcome,
                            provenance: "user:feedback".to_owned(),
                            created_at: created_at_ms.to_string(),
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    journal.save_feedback(&record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        run_id.clone(),
                        "feedback.submitted",
                        [
                            ("feedback_id".to_owned(), record.id.clone()),
                            ("signal".to_owned(), signal),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "record": record }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListFeedback {
                run_id,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let records = journal.list_feedback(&run_id, limit).await?;
                    let aggregate = journal.aggregate_feedback(20, 20).await?;
                    serde_json::to_vec(&serde_json::json!({
                        "records": records,
                        "aggregate": aggregate,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetContextLedger {
                task_id,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let projections = journal
                        .context_ledger_projection(&task_id, bounded_limit(limit))
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({ "entries": projections }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListTaskScratchpad {
                task_id,
                category,
                status,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let entries = journal
                        .scratchpad_projection(
                            &task_id,
                            category.as_deref(),
                            status.as_deref(),
                            bounded_limit(limit),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({ "entries": entries }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ClearTaskScratchpad { task_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let removed = journal
                        .clear_task_scratchpad(&task_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({ "removed": removed }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SummarizeContextNow { task_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    journal
                        .request_context_summarize(&task_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "requested": true,
                        "scope": "task_context",
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::PinContextItem {
                task_id,
                item_id,
                pinned,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    journal
                        .set_context_pin(&task_id, &item_id, pinned)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "item_id": item_id,
                        "pinned": pinned,
                        // Pin повышает приоритет, но не гарантирует включение:
                        // при нехватке бюджета item отбрасывается последним.
                        "guaranteed": false,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ReadContextArtifact {
                task_id,
                locator,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let content = journal
                        .read_context_artifact(&task_id, &locator)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "locator": locator,
                        "content": content,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RetainChild {
                child,
                now_ms,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    child.validate(now_ms).map_err(|e| e.to_string())?;
                    let journal = journal.ok_or_else(|| "storage_unavailable".to_string())?;
                    let database = journal.database().lock().await;
                    let applied = state.lock().await.retained_children.retain(child.clone(), now_ms).map_err(|e| e.to_string())?;
                    if applied { evohime_local_storage::retained_child_store::RetainedChildStore::upsert_child(database.connection(), evohime_local_storage::retained_child_store::UpsertChildInput { parent_id: &child.parent_id, child_id: &child.child_id, family_root_id: &child.family_root_id, revision: child.revision, registry_version: child.registry_version, lifecycle: "idle_retained", record: &child, created_at_ms: child.created_at_ms, last_active_at_ms: child.last_active_at_ms, retained_until_ms: child.retained_until_ms }).map_err(|e| e.to_string())?; }
                    serde_json::to_vec(&serde_json::json!({"applied":applied,"child_id":child.child_id})).map_err(|e| e.to_string())
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::GetRetainedChild {
                parent_id,
                child_id,
                now_ms,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async { let journal=journal.ok_or_else(||"storage_unavailable".to_string())?; let database=journal.database().lock().await; let child=evohime_local_storage::retained_child_store::RetainedChildStore::get_child::<crate::retained_child::RetainedChildV1>(database.connection(),&parent_id,&child_id).map_err(|e|e.to_string())?.ok_or_else(||"not_found".to_string())?; child.validate(now_ms).map_err(|e|e.to_string())?; let projection=crate::retained_child::RetainedChildProjectionV1::from(&child); serde_json::to_vec(&projection).map_err(|e|e.to_string()) }.await;
                let _ = reply.send(result);
            }
            CoreCommand::SendChildFollowUp {
                request,
                now_ms,
                busy,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = &journal {
                    let database = journal.database().lock().await;
                    if let Ok(Some(child)) =
                        evohime_local_storage::retained_child_store::RetainedChildStore::get_child::<
                            crate::retained_child::RetainedChildV1,
                        >(
                            database.connection(), &request.parent_id, &request.child_id
                        )
                    {
                        let _ = state.lock().await.retained_children.restore(child);
                    }
                }
                let result = async {
                    let durable_duplicate = if let Some(journal) = &journal {
                        let database = journal.database().lock().await;
                        evohime_local_storage::retained_child_store::RetainedChildStore::has_follow_up(
                            database.connection(),
                            &request.idempotency_key,
                        ).map_err(|e| e.to_string())?
                    } else { false };
                    let outcome = if durable_duplicate {
                        crate::retained_child::FollowUpOutcome::Duplicate
                    } else {
                        state.lock().await.retained_children.follow_up(&request, now_ms, busy)
                            .map_err(|e| e.to_string())?
                    };
                    if !matches!(outcome, crate::retained_child::FollowUpOutcome::Duplicate) {
                        let journal = journal.ok_or_else(|| "storage_unavailable".to_string())?;
                        let mut database = journal.database().lock().await;
                        let message_id = uuid::Uuid::new_v4().to_string();
                        let delivery = if matches!(outcome, crate::retained_child::FollowUpOutcome::Dispatched) {
                            crate::retained_child::DeliveryState::Dispatched
                        } else {
                            crate::retained_child::DeliveryState::Pending
                        };
                        evohime_local_storage::retained_child_store::RetainedChildStore::enqueue_follow_up(
                            database.connection_mut(),
                            evohime_local_storage::retained_child_store::EnqueueFollowUpInput {
                                parent_id: &request.parent_id,
                                child_id: &request.child_id,
                                idempotency_key: &request.idempotency_key,
                                expected_revision: request.expected_child_revision,
                                request: &request,
                                message_id: &message_id,
                                build_entry: |sequence| crate::retained_child::MailboxEntryV1 {
                                    version: 1,
                                    message_id: message_id.clone(),
                                    sender_id: request.parent_id.clone(),
                                    receiver_id: request.child_id.clone(),
                                    family_root_id: request.family_root_id.clone(),
                                    mode: request.mode,
                                    kind: "follow_up".into(),
                                    correlation_id: request.correlation_id.clone(),
                                    parent_sequence: sequence,
                                    payload_ref: None,
                                    inline_payload: Some(request.instruction.as_bytes().to_vec()),
                                    sensitivity: "public".into(),
                                    delivery,
                                    delivered_at_ms: None,
                                    idempotency_key: request.idempotency_key.clone(),
                                    created_at_ms: now_ms,
                                },
                                now_ms,
                            },
                        )
                        .map_err(|e| e.to_string())?;
                        if matches!(outcome, crate::retained_child::FollowUpOutcome::Dispatched | crate::retained_child::FollowUpOutcome::Queued) {
                            if let Some(child) = state.lock().await.retained_children.get(&request.parent_id, &request.child_id, now_ms).ok().cloned() {
                                let lifecycle = match child.lifecycle {
                                    crate::retained_child::RetainedLifecycle::RunningFollowUp => "running_follow_up",
                                    crate::retained_child::RetainedLifecycle::QueuedFollowUp => "queued_follow_up",
                                    _ => "idle_retained",
                                };
                                evohime_local_storage::retained_child_store::RetainedChildStore::upsert_child(database.connection(), evohime_local_storage::retained_child_store::UpsertChildInput { parent_id: &child.parent_id, child_id: &child.child_id, family_root_id: &child.family_root_id, revision: child.revision, registry_version: child.registry_version, lifecycle, record: &child, created_at_ms: child.created_at_ms, last_active_at_ms: child.last_active_at_ms, retained_until_ms: child.retained_until_ms }).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                    serde_json::to_vec(&serde_json::json!({
                        "outcome": format!("{outcome:?}").to_ascii_lowercase(),
                        "idempotency_key": request.idempotency_key,
                    }))
                    .map_err(|e| e.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListRetainedChildren {
                parent_id,
                now_ms,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async { let journal=journal.ok_or_else(||"storage_unavailable".to_string())?; let database=journal.database().lock().await; let items=evohime_local_storage::retained_child_store::RetainedChildStore::list_children::<crate::retained_child::RetainedChildV1>(database.connection(),&parent_id,now_ms,limit).map_err(|e|e.to_string())?; let projections: Vec<_>=items.iter().map(crate::retained_child::RetainedChildProjectionV1::from).collect(); serde_json::to_vec(&serde_json::json!({"children": projections})).map_err(|e|e.to_string()) }.await;
                let _ = reply.send(result);
            }
            CoreCommand::DeleteRetainedChild {
                parent_id,
                child_id,
                expected_registry_version,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                if let Some(journal) = &journal {
                    let database = journal.database().lock().await;
                    if let Ok(Some(child)) =
                        evohime_local_storage::retained_child_store::RetainedChildStore::get_child::<
                            crate::retained_child::RetainedChildV1,
                        >(database.connection(), &parent_id, &child_id)
                    {
                        let _ = state.lock().await.retained_children.restore(child);
                    }
                }
                let result = async { let journal=journal.ok_or_else(||"storage_unavailable".to_string())?; let database=journal.database().lock().await; let deleted=evohime_local_storage::retained_child_store::RetainedChildStore::delete_child(database.connection(),&parent_id,&child_id,expected_registry_version).map_err(|e|e.to_string())?; if !deleted{return Err("stale_revision".into())} let _=state.lock().await.retained_children.delete(&parent_id,&child_id); serde_json::to_vec(&serde_json::json!({"deleted":true,"child_id":child_id})).map_err(|e|e.to_string()) }.await;
                let _ = reply.send(result);
            }
        }
    }
}
