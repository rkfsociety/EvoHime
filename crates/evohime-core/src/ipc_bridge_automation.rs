use super::*;

impl IpcBridge {
    pub(crate) fn workflow_runtime(
        &self,
        workspace_path: &str,
    ) -> crate::workflow_runtime::WorkflowRuntime {
        let mut adapter =
            crate::workflow_adapters::CoreNodeAdapter::new(self.journal.clone(), workspace_path);
        if let Some(tools) = &self.tools {
            adapter = adapter.with_tools(Arc::clone(tools));
        }
        crate::workflow_runtime::WorkflowRuntime::new(
            self.journal.clone(),
            Arc::clone(&self.workflow_registry),
            Arc::new(adapter),
            Arc::clone(&self.workflow_approvals)
                as Arc<dyn crate::workflow_runtime::WorkflowApprovalGate>,
            self.core_instance_id.clone(),
        )
    }

    /// Продолжает запуск в фоне. Команда IPC не ждёт выполнения графа:
    /// состояние durable, и оболочка забирает его отдельным `GetWorkflowRun`.
    pub(crate) async fn spawn_workflow_drive(
        &self,
        run_id: String,
        workspace_path: String,
    ) -> bool {
        let runtime = self.workflow_runtime(&workspace_path);
        self.background_tasks
            .try_spawn(async move {
                let _ = runtime.drive(&run_id).await;
            })
            .await
    }

    pub(crate) async fn start_invocation_preset(
        &self,
        preset: crate::invocation_presets::InvocationPreset,
        workspace_path: String,
        idempotency_key: String,
    ) -> Result<String, String> {
        preset.validate().map_err(|error| error.to_string())?;
        let Some(template) = crate::workflow_templates::template(&preset.workflow_id) else {
            return Err("unknown_workflow".into());
        };
        if template.version != preset.workflow_version {
            return Err("needs_migration".into());
        }
        let inputs = preset
            .input_values
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|value| (key.clone(), value.to_string()))
                    .ok_or_else(|| format!("non_string_input:{key}"))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
        let graph = template
            .instantiate(&inputs)
            .map_err(|error| error.code().to_string())?;
        if graph.canonical_hash() != preset.workflow_definition_hash {
            return Err("workflow_definition_drift".into());
        }
        let digest = <sha2::Sha256 as sha2::Digest>::digest(
            format!("{}|{}|{}", preset.id, preset.revision, idempotency_key).as_bytes(),
        );
        let run_id = format!("preset-{}", hex_encode(&digest[..16]));
        if self
            .journal
            .workflow_run(&run_id)
            .await
            .ok()
            .flatten()
            .is_some()
        {
            return Ok(run_id);
        }
        let runtime = self.workflow_runtime(&workspace_path);
        let start = crate::workflow_runtime::StartWorkflowRequest {
            run_id: run_id.clone(),
            task_id: run_id.clone(),
            workspace_path: workspace_path.clone(),
            template_id: template.template_id.clone(),
            template_version: template.version,
            inputs,
            graph,
            parent: workflow_parent_capabilities(),
        };
        let started = runtime
            .start(start)
            .await
            .map_err(|error| error.code().to_string())?;
        if !self
            .spawn_workflow_drive(started.clone(), workspace_path)
            .await
        {
            return Err("background task capacity is exhausted".into());
        }
        Ok(started)
    }

    pub(crate) async fn dispatch_list_automation_schedules(
        &self,
        request: generated::ListAutomationSchedules,
    ) -> serde_json::Value {
        let owner_scope = request.owner_scope;
        if owner_scope.is_empty() || owner_scope.len() > crate::automation::MAX_ID_BYTES {
            return serde_json::json!({
                "schedules": [],
                "error_code": "invalid_owner_scope",
            });
        }
        let limit = request.limit.clamp(1, 256);
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_schedules(
            database.connection(),
            &owner_scope,
            limit,
        ) {
            Ok(schedules) => serde_json::json!({
                "schedules": schedules.into_iter().map(|schedule| serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "definition_id": schedule.definition_id,
                    "revision": schedule.revision,
                    "owner_scope": schedule.owner_scope,
                    "hour": schedule.hour,
                    "minute": schedule.minute,
                    "timezone_minutes": schedule.timezone_minutes,
                    "missed_grace_ms": schedule.missed_grace_ms,
                    "enabled": schedule.enabled,
                    "last_slot": schedule.last_slot,
                    "preset_id": schedule.preset_id,
                    "preset_revision": schedule.preset_revision,
                    "preset_content_hash": schedule.preset_content_hash,
                    "workspace_path": schedule.workspace_path,
                })).collect::<Vec<_>>(),
                "error_code": "",
            }),
            Err(error) => serde_json::json!({
                "schedules": [],
                "error_code": error.to_string(),
            }),
        }
    }

    pub(crate) async fn dispatch_save_automation_schedule(
        &self,
        request: generated::SaveAutomationSchedule,
    ) -> serde_json::Value {
        if request.schedule_id.is_empty()
            || request.schedule_id.len() > crate::automation::MAX_ID_BYTES
            || request.definition_id.is_empty()
            || request.owner_scope.is_empty()
            || request.owner_scope.len() > crate::automation::MAX_ID_BYTES
            || request.revision == 0
        {
            return serde_json::json!({
                "saved": false,
                "error_code": "invalid_schedule_identity",
            });
        }
        if crate::automation_scheduler::DailySchedule::new(
            request.hour as u8,
            request.minute as u8,
            request.timezone_minutes,
            request.missed_grace_ms,
        )
        .is_err()
            || request.hour > 23
            || request.minute > 59
        {
            return serde_json::json!({
                "saved": false,
                "error_code": "invalid_schedule_policy",
            });
        }
        let database = self.journal.database().lock().await;
        if !request.preset_id.is_empty() {
            let valid_preset = evohime_local_storage::invocation_presets_store::read_revision(
                database.connection(),
                &request.owner_scope,
                &request.preset_id,
                request.preset_revision,
            )
            .ok()
            .flatten()
            .and_then(|(content, hash, state)| {
                serde_json::from_str::<crate::invocation_presets::InvocationPreset>(&content)
                    .ok()
                    .filter(|preset| {
                        state == "ready"
                            && preset.content_hash == request.preset_content_hash
                            && preset.canonical_content_hash() == hash
                            && preset.revision == request.preset_revision
                    })
            });
            if valid_preset.is_none() {
                return serde_json::json!({"saved":false,"error_code":"invalid_preset_snapshot"});
            }
        }
        let definition = evohime_local_storage::automation_store::get_definition(
            database.connection(),
            &request.definition_id,
            request.revision,
            &request.owner_scope,
        );
        match definition {
            Ok(None) => serde_json::json!({
                "saved": false,
                "error_code": "unknown_definition",
            }),
            Err(error) => serde_json::json!({
                "saved": false,
                "error_code": error.to_string(),
            }),
            Ok(Some(_)) => {
                let previous = evohime_local_storage::automation_store::get_schedule(
                    database.connection(),
                    &request.schedule_id,
                )
                .ok()
                .flatten();
                let record = evohime_local_storage::automation_store::AutomationScheduleRecord {
                    schedule_id: request.schedule_id.clone(),
                    definition_id: request.definition_id.clone(),
                    revision: request.revision,
                    owner_scope: request.owner_scope.clone(),
                    hour: request.hour as u8,
                    minute: request.minute as u8,
                    timezone_minutes: request.timezone_minutes,
                    missed_grace_ms: request.missed_grace_ms,
                    enabled: request.enabled,
                    last_slot: previous.and_then(|previous| {
                        (previous.definition_id == request.definition_id
                            && previous.revision == request.revision
                            && previous.owner_scope == request.owner_scope
                            && previous.preset_id.as_deref()
                                == (!request.preset_id.is_empty())
                                    .then_some(request.preset_id.as_str())
                            && previous.preset_revision
                                == (!request.preset_id.is_empty())
                                    .then_some(request.preset_revision)
                            && previous.preset_content_hash.as_deref()
                                == (!request.preset_content_hash.is_empty())
                                    .then_some(request.preset_content_hash.as_str()))
                        .then_some(previous.last_slot)
                        .flatten()
                    }),
                    preset_id: (!request.preset_id.is_empty()).then_some(request.preset_id.clone()),
                    preset_revision: (!request.preset_id.is_empty())
                        .then_some(request.preset_revision),
                    preset_content_hash: (!request.preset_content_hash.is_empty())
                        .then_some(request.preset_content_hash.clone()),
                    workspace_path: request.workspace_path.clone(),
                };
                match evohime_local_storage::automation_store::upsert_schedule(
                    database.connection(),
                    &record,
                    now_ms(),
                ) {
                    Ok(saved) => serde_json::json!({
                        "saved": saved,
                        "schedule_id": record.schedule_id,
                        "error_code": "",
                    }),
                    Err(error) => serde_json::json!({
                        "saved": false,
                        "error_code": error.to_string(),
                    }),
                }
            }
        }
    }

    pub(crate) async fn dispatch_trigger_automation(
        &self,
        request: generated::TriggerAutomation,
    ) -> serde_json::Value {
        if request.definition_id.is_empty()
            || request.owner_scope.is_empty()
            || request.trigger_key.is_empty()
            || request.correlation_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.revision == 0
            || request.input_json.len() > crate::automation::MAX_INPUT_BYTES
            || serde_json::from_str::<serde_json::Value>(&request.input_json).is_err()
        {
            return serde_json::json!({ "accepted": false, "run_id": "", "error_code": "invalid_trigger" });
        }
        let mut database = self.journal.database().lock().await;
        let Some(definition) = evohime_local_storage::automation_store::get_definition(
            database.connection(),
            &request.definition_id,
            request.revision,
            &request.owner_scope,
        )
        .ok()
        .flatten() else {
            return serde_json::json!({ "accepted": false, "run_id": "", "error_code": "unknown_definition" });
        };
        let payload_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
            request.input_json.as_bytes(),
        ));
        let run = evohime_local_storage::automation_store::AutomationRunRecord {
            run_id: uuid::Uuid::new_v4().to_string(),
            definition_id: request.definition_id,
            revision: request.revision,
            owner_scope: request.owner_scope,
            idempotency_key: request.idempotency_key,
            payload_hash,
            state: "admitted".into(),
            generation: 1,
            permission_snapshot: "manual".into(),
            approval_snapshot: "manual".into(),
        };
        let now = now_ms();
        match evohime_local_storage::automation_store::admit_run(database.connection(), &run, now) {
            Ok(evohime_local_storage::automation_store::AdmitRunResult::Existing(existing)) => {
                serde_json::json!({ "accepted": true, "run_id": existing.run_id, "state": existing.state, "deduplicated": true, "error_code": "" })
            }
            Ok(evohime_local_storage::automation_store::AdmitRunResult::IdempotencyConflict {
                ..
            }) => {
                serde_json::json!({ "accepted": false, "run_id": "", "deduplicated": false, "error_code": "idempotency_conflict" })
            }
            Ok(evohime_local_storage::automation_store::AdmitRunResult::Inserted) => {
                let payload = serde_json::json!({
                    "definition_hash": definition.definition_hash,
                    "trigger": request.trigger_key,
                    "correlation_id": request.correlation_id,
                });
                let queued = evohime_local_storage::automation_store::transition_run(
                    database.connection_mut(),
                    evohime_local_storage::automation_store::RunTransition {
                        run_id: &run.run_id,
                        from_state: "admitted",
                        to_state: "queued",
                        generation: 1,
                        event_type: "manual_trigger",
                        payload_json: &payload.to_string(),
                        now_ms: now,
                    },
                )
                .unwrap_or(false);
                serde_json::json!({ "accepted": queued, "run_id": run.run_id, "state": if queued { "queued" } else { "admitted" }, "deduplicated": false, "error_code": if queued { "" } else { "transition_failed" } })
            }
            Err(error) => {
                serde_json::json!({ "accepted": false, "run_id": "", "error_code": error.to_string() })
            }
        }
    }

    pub(crate) async fn dispatch_list_automation_runs(
        &self,
        request: generated::ListAutomationRuns,
    ) -> serde_json::Value {
        if request.owner_scope.is_empty() {
            return serde_json::json!({ "runs": [], "error_code": "invalid_owner_scope" });
        }
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_runs(
            database.connection(),
            &request.owner_scope,
            &request.definition_id,
            request.limit.clamp(1, 256),
        ) {
            Ok(runs) => serde_json::json!({ "runs": runs.into_iter().map(|run| serde_json::json!({
                "run_id": run.run_id, "definition_id": run.definition_id, "revision": run.revision,
                "owner_scope": run.owner_scope, "idempotency_key": run.idempotency_key,
                "state": run.state, "generation": run.generation,
            })).collect::<Vec<_>>(), "error_code": "" }),
            Err(error) => serde_json::json!({ "runs": [], "error_code": error.to_string() }),
        }
    }

    pub(crate) async fn dispatch_get_automation_run(
        &self,
        request: generated::GetAutomationRun,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::get_run(
            database.connection(),
            &request.run_id,
        ) {
            Ok(Some(run)) => serde_json::json!({
                "run_id": run.run_id, "definition_id": run.definition_id, "revision": run.revision,
                "owner_scope": run.owner_scope, "state": run.state, "generation": run.generation,
                "error_code": "",
            }),
            Ok(None) => {
                serde_json::json!({ "run_id": request.run_id, "state": "unknown_state", "error_code": "unknown_run" })
            }
            Err(error) => {
                serde_json::json!({ "run_id": request.run_id, "state": "unknown_state", "error_code": error.to_string() })
            }
        }
    }

    pub(crate) async fn dispatch_list_automation_events(
        &self,
        request: generated::ListAutomationEvents,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_run_events(
            database.connection(),
            &request.run_id,
            request.after_sequence,
            request.limit.clamp(1, 256) as u32,
        ) {
            Ok(events) => {
                serde_json::json!({ "run_id": request.run_id, "events": events.into_iter().map(|event| serde_json::json!({
                "sequence": event.run_sequence, "event_type": event.event_type, "generation": event.generation,
                "payload": event.payload_json, "created_at_ms": event.created_at_ms,
            })).collect::<Vec<_>>(), "error_code": "" })
            }
            Err(error) => {
                serde_json::json!({ "run_id": request.run_id, "events": [], "error_code": error.to_string() })
            }
        }
    }

    pub(crate) async fn dispatch_cancel_automation_run(
        &self,
        request: generated::CancelAutomationRun,
    ) -> serde_json::Value {
        let mut database = self.journal.database().lock().await;
        let cancelled = evohime_local_storage::automation_store::cancel_run(
            database.connection_mut(),
            &request.run_id,
            now_ms(),
        )
        .unwrap_or(false);
        serde_json::json!({ "run_id": request.run_id, "cancelled": cancelled, "error_code": if cancelled { "" } else { "not_cancellable" } })
    }

    pub(crate) async fn dispatch_set_automation_schedule_enabled(
        &self,
        request: generated::SetAutomationScheduleEnabled,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        let enabled = evohime_local_storage::automation_store::set_schedule_enabled(
            database.connection(),
            &request.schedule_id,
            request.enabled,
            now_ms(),
        )
        .unwrap_or(false);
        serde_json::json!({ "schedule_id": request.schedule_id, "enabled": request.enabled, "updated": enabled, "error_code": if enabled { "" } else { "unknown_schedule" } })
    }

    /// Polls every enabled schedule once. The compare-and-swap cursor is
    /// advanced before a trigger is admitted, so a second Core generation
    /// cannot publish the same wall-clock slot. The normal automation runtime
    /// consumes the durable admitted run; this method never executes effects.
    pub async fn poll_automation_schedules(&self) {
        let now = now_ms();
        let schedules = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::automation_store::list_enabled_schedules(database.connection())
                .unwrap_or_default()
        };
        for schedule in schedules {
            let Ok(policy) = crate::automation_scheduler::DailySchedule::new(
                schedule.hour,
                schedule.minute,
                schedule.timezone_minutes,
                schedule.missed_grace_ms,
            ) else {
                continue;
            };
            let cursor = crate::automation_scheduler::SchedulerCursor {
                last_slot: schedule.last_slot.clone(),
            };
            let decision =
                match policy.decide(&schedule.definition_id, schedule.revision, &cursor, now) {
                    Ok(decision) => decision,
                    Err(_) => continue,
                };
            let (slot, idempotency_key, missed) = match decision {
                crate::automation_scheduler::SchedulerDecision::NotDue => continue,
                crate::automation_scheduler::SchedulerDecision::Trigger {
                    slot,
                    idempotency_key,
                } => (slot, idempotency_key, false),
                crate::automation_scheduler::SchedulerDecision::Missed {
                    slot,
                    idempotency_key,
                } => (slot, idempotency_key, true),
            };
            let mut database = self.journal.database().lock().await;
            let Some(definition) = evohime_local_storage::automation_store::get_definition(
                database.connection(),
                &schedule.definition_id,
                schedule.revision,
                &schedule.owner_scope,
            )
            .ok()
            .flatten() else {
                // Не сдвигаем cursor: после восстановления definition следующий
                // poll должен повторить попытку, а не потерять слот.
                continue;
            };
            let advanced = evohime_local_storage::automation_store::advance_schedule_slot(
                database.connection(),
                &schedule.schedule_id,
                schedule.last_slot.as_deref(),
                &slot,
                now,
            )
            .unwrap_or(false);
            if !advanced {
                continue;
            }
            if let (Some(preset_id), Some(preset_revision), Some(preset_hash)) = (
                schedule.preset_id.clone(),
                schedule.preset_revision,
                schedule.preset_content_hash.clone(),
            ) {
                let preset = evohime_local_storage::invocation_presets_store::read_revision(
                    database.connection(),
                    &schedule.owner_scope,
                    &preset_id,
                    preset_revision,
                )
                .ok()
                .flatten()
                .and_then(|(content, stored_hash, state)| {
                    if stored_hash != preset_hash || state != "ready" {
                        return None;
                    }
                    serde_json::from_str::<crate::invocation_presets::InvocationPreset>(&content)
                        .ok()
                        .filter(|preset| {
                            preset.content_hash == preset_hash && preset.revision == preset_revision
                        })
                });
                let workspace_path = schedule.workspace_path.clone();
                let schedule_id = schedule.schedule_id.clone();
                let idempotency_key = idempotency_key.clone();
                drop(database);
                match preset {
                    Some(preset) => {
                        let result = self
                            .start_invocation_preset(preset, workspace_path, idempotency_key)
                            .await;
                        if result.is_err() {
                            let database = self.journal.database().lock().await;
                            let payload = serde_json::json!({"schedule_id":schedule_id,"preset_id":preset_id,"revision":preset_revision,"outcome":"blocked","error_code":result.err().unwrap_or_default()});
                            let _ = database.append_event(
                                &schedule_id,
                                "automation.preset_blocked",
                                &serde_json::to_vec(&payload).unwrap_or_default(),
                            );
                        }
                    }
                    None => {
                        let database = self.journal.database().lock().await;
                        let payload = serde_json::json!({"schedule_id":schedule_id,"preset_id":preset_id,"revision":preset_revision,"outcome":"blocked","error_code":"preset_drift_or_rebinding"});
                        let _ = database.append_event(
                            &schedule_id,
                            "automation.preset_blocked",
                            &serde_json::to_vec(&payload).unwrap_or_default(),
                        );
                    }
                }
                continue;
            }
            if missed {
                let payload = serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "definition_id": schedule.definition_id,
                    "revision": schedule.revision,
                    "slot": slot,
                    "idempotency_key": idempotency_key,
                    "reason": "missed_tick",
                });
                let _ = database.append_event(
                    &schedule.schedule_id,
                    "automation.schedule_missed",
                    &serde_json::to_vec(&payload).unwrap_or_default(),
                );
                continue;
            }
            let input_json = "{}".to_string();
            let payload_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
                input_json.as_bytes(),
            ));
            let run = evohime_local_storage::automation_store::AutomationRunRecord {
                run_id: uuid::Uuid::new_v4().to_string(),
                definition_id: schedule.definition_id.clone(),
                revision: schedule.revision,
                owner_scope: schedule.owner_scope.clone(),
                idempotency_key,
                payload_hash,
                state: "admitted".into(),
                generation: 1,
                permission_snapshot: "scheduler".into(),
                approval_snapshot: "scheduler".into(),
            };
            if let Ok(evohime_local_storage::automation_store::AdmitRunResult::Inserted) =
                evohime_local_storage::automation_store::admit_run(database.connection(), &run, now)
            {
                let payload = serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "slot": slot,
                    "definition_hash": definition.definition_hash,
                    "trigger": "timer",
                });
                let _ = evohime_local_storage::automation_store::transition_run(
                    database.connection_mut(),
                    evohime_local_storage::automation_store::RunTransition {
                        run_id: &run.run_id,
                        from_state: "admitted",
                        to_state: "queued",
                        generation: 1,
                        event_type: "scheduled",
                        payload_json: &payload.to_string(),
                        now_ms: now,
                    },
                );
            }
        }
    }
}
