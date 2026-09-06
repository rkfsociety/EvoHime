use super::*;

impl EventJournal {
    pub async fn get_work_item(&self, id: &str) -> Result<Option<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_work_item(id)
    }

    pub async fn create_work_item(
        &self,
        item: &WorkItemRecord,
    ) -> Result<WorkItemRecord, StorageError> {
        let database = self.database.lock().await;
        database.create_work_item(item)
    }

    pub async fn update_work_item_status(
        &self,
        id: &str,
        expected_version: i64,
        status: &str,
    ) -> Result<WorkItemRecord, StorageError> {
        let database = self.database.lock().await;
        database.update_work_item_status(id, expected_version, status)
    }

    pub async fn add_dependency(
        &self,
        from_id: &str,
        to_id: &str,
        kind: &str,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        database.add_dependency(from_id, to_id, kind)
    }

    pub async fn list_work_items(
        &self,
        project_id: &str,
    ) -> Result<Vec<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.list_work_items(project_id)
    }

    pub async fn list_task_graph(
        &self,
        project_id: &str,
    ) -> Result<(Vec<WorkItemRecord>, Vec<(String, String, String)>), StorageError> {
        let database = self.database.lock().await;
        Ok((
            database.list_work_items(project_id)?,
            database.list_dependencies(project_id)?,
        ))
    }

    pub async fn next_ready_task(
        &self,
        project_id: &str,
    ) -> Result<Option<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.next_ready(project_id)
    }

    pub async fn import_prd(
        &self,
        provenance_id: &str,
        project_id: &str,
        origin: &str,
        version: &str,
        source_text: &str,
        tasks: &[ImportedTask],
    ) -> Result<Vec<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.import_prd(
            provenance_id,
            project_id,
            origin,
            version,
            source_text,
            tasks,
        )
    }

    pub async fn save_snapshot(
        &self,
        id: &str,
        run_id: &str,
        workspace_hash: &str,
        payload: &[u8],
    ) -> Result<evohime_local_storage::SnapshotRecord, StorageError> {
        let database = self.database.lock().await;
        database.save_snapshot(id, run_id, workspace_hash, payload)
    }

    pub async fn latest_snapshot_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<evohime_local_storage::SnapshotRecord>, StorageError> {
        let database = self.database.lock().await;
        database.latest_snapshot_for_task(task_id)
    }

    pub async fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<evohime_local_storage::SnapshotRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_snapshot(snapshot_id)
    }

    pub async fn get_run(
        &self,
        run_id: &str,
    ) -> Result<Option<evohime_local_storage::RunRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_run(run_id)
    }

    pub async fn begin_build_effect(
        &self,
        run_id: &str,
        task_id: &str,
        intent_hash: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect_id = format!("effect-{run_id}");
        let checkpoint = RunCheckpointRecord {
            run_id: run_id.into(),
            checkpoint_id: format!("checkpoint-{run_id}"),
            stage: "build".into(),
            node_id: "bounded-build".into(),
            attempt: 1,
            input_hash: intent_hash.into(),
            state_json: serde_json::to_vec(&serde_json::json!({
                "stage": "build", "intent_hash": intent_hash
            }))?,
            pending_effects_json: serde_json::to_vec(&vec![effect_id.clone()])?,
            committed_at: String::new(),
        };
        let effect = RunEffectRecord {
            effect_id: effect_id.clone(),
            run_id: run_id.into(),
            node_id: "bounded-build".into(),
            kind: "bounded_build".into(),
            idempotency_key: format!("{run_id}:bounded-build"),
            immutable_intent_hash: intent_hash.into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        let run = RunRecord {
            id: run_id.into(),
            work_item_id: task_id.into(),
            status: "running".into(),
            policy_snapshot: Vec::new(),
            role_snapshot: Vec::new(),
            skill_snapshot: Vec::new(),
            model_route_snapshot: Vec::new(),
        };
        let stored = database.prepare_run_effect(&run, &checkpoint, &effect)?;
        if stored.immutable_intent_hash != intent_hash {
            return Err(StorageError::InvalidRunEffect(
                "intent hash conflict".into(),
            ));
        }
        match stored.state.as_str() {
            "prepared" => {
                database.acquire_run_lease(run_id, &format!("lease-{run_id}"), "core", 1, 30)?;
                database.mark_effect_executing(&effect_id)
            }
            "executing" => Err(StorageError::InvalidRunEffect(
                "effect is already executing".into(),
            )),
            "completed_success" | "completed_failure" | "unknown" => Err(
                StorageError::InvalidRunEffect(format!("effect is already {}", stored.state)),
            ),
            _ => Err(StorageError::InvalidRunEffect(format!(
                "unsupported state {}",
                stored.state
            ))),
        }
    }

    pub async fn complete_build_effect(
        &self,
        run_id: &str,
        success: bool,
        result_hash: Option<&str>,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect =
            database.complete_run_effect(&format!("effect-{run_id}"), success, result_hash)?;
        database.update_run_status(run_id, if success { "completed" } else { "failed" })?;
        database.release_run_lease(run_id, &format!("lease-{run_id}"), "core", 1)?;
        Ok(effect)
    }

    pub async fn heartbeat_build_effect(
        &self,
        run_id: &str,
    ) -> Result<evohime_local_storage::RunLeaseRecord, StorageError> {
        let database = self.database.lock().await;
        database.heartbeat_run_lease(run_id, &format!("lease-{run_id}"), "core", 1, 30)
    }

    pub async fn begin_agent_run(
        &self,
        run_id: &str,
        task_id: &str,
        intent_hash: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect_id = format!("effect-{run_id}");
        let effect = RunEffectRecord {
            effect_id: effect_id.clone(),
            run_id: run_id.into(),
            node_id: "agent-task".into(),
            kind: "agent_task".into(),
            idempotency_key: format!("{run_id}:agent-task"),
            immutable_intent_hash: intent_hash.into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        let stored = database.prepare_agent_run_effect(&effect, task_id)?;
        if stored.immutable_intent_hash != intent_hash {
            return Err(StorageError::InvalidRunEffect(
                "intent hash conflict".into(),
            ));
        }
        match stored.state.as_str() {
            "prepared" => {
                database.acquire_agent_run_lease(
                    run_id,
                    &format!("lease-{run_id}"),
                    "core",
                    1,
                    30,
                )?;
                database.mark_agent_effect_executing(&effect_id)
            }
            "executing" => Err(StorageError::InvalidRunEffect(
                "effect is already executing".into(),
            )),
            "completed_success" | "completed_failure" | "unknown" => Err(
                StorageError::InvalidRunEffect(format!("effect is already {}", stored.state)),
            ),
            _ => Err(StorageError::InvalidRunEffect(format!(
                "unsupported state {}",
                stored.state
            ))),
        }
    }

    pub async fn heartbeat_agent_run(
        &self,
        run_id: &str,
    ) -> Result<evohime_local_storage::RunLeaseRecord, StorageError> {
        let database = self.database.lock().await;
        database.heartbeat_agent_run_lease(run_id, &format!("lease-{run_id}"), "core", 1, 30)
    }

    pub async fn complete_agent_run(
        &self,
        run_id: &str,
        success: bool,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect =
            database.complete_agent_run_effect(&format!("effect-{run_id}"), success, None)?;
        database.release_agent_run_lease(run_id, &format!("lease-{run_id}"), "core", 1)?;
        Ok(effect)
    }

    pub async fn reconcile_build_effect(
        &self,
        run_id: &str,
        success: bool,
        evidence: &serde_json::Value,
    ) -> Result<evohime_local_storage::RunReconciliationRecord, StorageError> {
        let database = self.database.lock().await;
        let record = database.reconcile_run_effect(
            &format!("effect-{run_id}"),
            success,
            "bounded_build_snapshot",
            &serde_json::to_vec(evidence)?,
        )?;
        if success {
            database.update_run_status(run_id, "completed")?;
        }
        Ok(record)
    }

    pub async fn recover_after_restart(
        &self,
    ) -> Result<Vec<evohime_local_storage::RecoveredRunRecord>, StorageError> {
        let database = self.database.lock().await;
        database.recover_unknown_effects()
    }

    pub async fn recover_and_reconcile_after_restart(
        &self,
    ) -> Result<Vec<evohime_local_storage::RunReconciliationRecord>, StorageError> {
        let database = self.database.lock().await;
        let recovered = database.recover_unknown_effects()?;
        let mut reconciliations = Vec::with_capacity(recovered.len());
        for record in recovered {
            // Durable recovery state machine: RECOVERING -> RECONCILING -> terminal.
            // Each stage uses a distinct idempotency key so a crash between
            // stages replays safely (transition_recovery treats a repeated
            // (idempotency_key, state) pair as a no-op and rejects a reused
            // key against a different state).
            let recovery_transition =
                |state, idempotency_key: &str, verifier: &str, evidence: &[u8], decision: &str| {
                    if record.kind == "agent_task" {
                        database.transition_agent_recovery(
                            evohime_local_storage::RecoveryTransitionInput {
                                run_id: &record.run_id,
                                next: state,
                                effect_id: &record.effect_id,
                                idempotency_key,
                                verifier,
                                evidence_json: evidence,
                                decision,
                            },
                        )
                    } else {
                        database.transition_recovery(
                            evohime_local_storage::RecoveryTransitionInput {
                                run_id: &record.run_id,
                                next: state,
                                effect_id: &record.effect_id,
                                idempotency_key,
                                verifier,
                                evidence_json: evidence,
                                decision,
                            },
                        )
                    }
                };
            recovery_transition(
                RecoveryState::Recovering,
                &format!("{}:{}:recovering", record.run_id, record.effect_id),
                "startup",
                br#"{"reason":"process_restart"}"#,
                "recovery_started",
            )?;
            recovery_transition(
                RecoveryState::Reconciling,
                &format!("{}:{}:reconciling", record.run_id, record.effect_id),
                if record.kind == "agent_task" {
                    "task_event_journal"
                } else {
                    "bounded_build_snapshot"
                },
                br#"{"reason":"verifying_outcome"}"#,
                "verifier_started",
            )?;

            let (success, verifier, idempotency_key, evidence) = if record.kind == "agent_task" {
                let terminal_event = database
                    .read_task_events(&record.work_item_id, 256)?
                    .into_iter()
                    .rev()
                    .find(|event| {
                        matches!(
                            event.event_type.as_str(),
                            "task.completed" | "task.failed" | "task.stopped"
                        )
                    });
                let success = terminal_event
                    .as_ref()
                    .is_some_and(|event| event.event_type == "task.completed");
                let verifier = "task_event_journal";
                let idempotency_key = format!("{}:agent-task", record.run_id);
                let evidence = serde_json::json!({
                    "run_id": record.run_id,
                    "effect_id": record.effect_id,
                    "idempotency_key": idempotency_key,
                    "verifier": verifier,
                    "terminal_event": terminal_event.as_ref().map(|event| serde_json::json!({
                        "event_type": event.event_type,
                        "sequence_id": event.sequence_id,
                    })),
                    "decision": if success { "completed" } else { "blocked" },
                });
                (success, verifier, idempotency_key, evidence)
            } else {
                let snapshot = database.latest_snapshot_for_task(&record.work_item_id)?;
                let success = snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.run_id == record.run_id);
                let verifier = "bounded_build_snapshot";
                let idempotency_key = format!("{}:bounded-build", record.run_id);
                let evidence = serde_json::json!({
                    "run_id": record.run_id,
                    "effect_id": record.effect_id,
                    "idempotency_key": idempotency_key,
                    "verifier": verifier,
                    "snapshot_id": success.then(|| snapshot.as_ref().expect("successful reconciliation has snapshot").id.clone()),
                    "decision": if success { "applied" } else { "blocked" },
                });
                (success, verifier, idempotency_key, evidence)
            };
            let reconciliation = if record.kind == "agent_task" {
                database.reconcile_agent_run_effect(
                    &record.effect_id,
                    success,
                    verifier,
                    &serde_json::to_vec(&evidence)?,
                )?
            } else {
                database.reconcile_run_effect(
                    &record.effect_id,
                    success,
                    verifier,
                    &serde_json::to_vec(&evidence)?,
                )?
            };
            if success {
                database.update_run_status(&record.run_id, "completed")?;
            }
            database.append_event(
                &record.work_item_id,
                if success {
                    "run.reconciliation.completed"
                } else {
                    "run.recovery.blocked"
                },
                &serde_json::to_vec(&evidence)?,
            )?;
            database.append_event(
                &record.work_item_id,
                "run.reconciliation.audit",
                &serde_json::to_vec(&serde_json::json!({
                    "effect_id": record.effect_id,
                    "idempotency_key": idempotency_key,
                    "verifier": verifier,
                    "evidence": evidence,
                    "decision": if success { "applied" } else { "blocked" },
                }))?,
            )?;

            recovery_transition(
                if success {
                    RecoveryState::Resumable
                } else {
                    RecoveryState::Blocked
                },
                &format!(
                    "{}:{}:{}",
                    record.run_id,
                    record.effect_id,
                    if success { "resumable" } else { "blocked" }
                ),
                verifier,
                &serde_json::to_vec(&evidence)?,
                if success { "applied" } else { "blocked" },
            )?;

            reconciliations.push(reconciliation);
        }
        Ok(reconciliations)
    }

    pub async fn record_audit(
        &self,
        subject_id: &str,
        event_type: &str,
        payload: &[u8],
    ) -> Result<i64, StorageError> {
        let database = self.database.lock().await;
        database.append_event(subject_id, event_type, payload)
    }

    pub async fn task_history(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let database = self.database.lock().await;
        database.read_task_events(task_id, limit)
    }

    pub async fn record_deduplicated(
        &self,
        client_id: &str,
        request_id: &str,
        command_hash: &str,
        result: &[u8],
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let database = self.database.lock().await;
        database.record_deduplicated(client_id, request_id, command_hash, result)
    }

    /// Atomically records a TaskCheckpoint user action and its idempotency
    /// result. The event and dedup row must commit together: otherwise a
    /// reconnect between the two writes could either repeat the action or
    /// report a success that is absent from the journal.
    pub async fn record_task_checkpoint_action(
        &self,
        task_id: &str,
        request_id: &str,
        command_hash: &str,
        event_payload: &[u8],
        result: &[u8],
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let mut database = self.database.lock().await;
        let transaction = database.connection_mut().transaction()?;
        let existing = transaction
            .query_row(
                "SELECT command_hash, result FROM command_dedup
                 WHERE client_id = 'task-checkpoint-ipc' AND request_id = ?1",
                [request_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        if let Some((stored_hash, stored_result)) = existing {
            if stored_hash == command_hash {
                transaction.commit()?;
                return Ok(Some(stored_result));
            }
            return Err(StorageError::DeduplicationConflict {
                client_id: "task-checkpoint-ipc".into(),
                request_id: request_id.into(),
            });
        }
        transaction.execute(
            "INSERT INTO events(task_id, event_type, payload)
             VALUES (?1, 'task.checkpoint.action', ?2)",
            rusqlite::params![task_id, event_payload],
        )?;
        transaction.execute(
            "INSERT INTO command_dedup(client_id, request_id, command_hash, result)
             VALUES ('task-checkpoint-ipc', ?1, ?2, ?3)",
            rusqlite::params![request_id, command_hash, result],
        )?;
        transaction.commit()?;
        Ok(None)
    }
}
