//! Durable wake-up/recovery runtime for plan 132.

use crate::{durable_background_execution as contract, EventJournal};
use evohime_local_storage::{automation_store, durable_background_execution_store as store, StorageError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRunRequest {
    pub snapshot: contract::BackgroundRunSnapshot,
    pub payload_hash: String,
    pub definition_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackgroundScheduleRequest {
    schedule_id: String,
    definition_id: String,
    revision: u64,
    owner_scope: String,
    spec: contract::ScheduleSpec,
    enabled: bool,
    missed_fire_policy: contract::MissedFirePolicy,
    overlap_policy: contract::OverlapPolicy,
    #[serde(default)]
    workspace_path: String,
}

fn bounded_json(bytes: &[u8]) -> bool { !bytes.is_empty() && bytes.len() <= contract::MAX_SNAPSHOT_BYTES }

fn error_code(error: &str) -> &'static str {
    if error.contains("idempotency") { "idempotency_conflict" }
    else if error.contains("definition") { "unknown_definition" }
    else if error.contains("revision") || error.contains("stale") { "version_conflict" }
    else if error.contains("storage") || error.contains("SQLite") { "storage_unavailable" }
    else { "background_execution_failed" }
}

fn overflow_policy(value: &str) -> Option<contract::OverflowPolicy> {
    match value {
        "rejectnew" | "reject_new" => Some(contract::OverflowPolicy::RejectNew),
        "dropoldestallowed" | "drop_oldest_allowed" => Some(contract::OverflowPolicy::DropOldestAllowed),
        "coalescebykey" | "coalesce_by_key" => Some(contract::OverflowPolicy::CoalesceByKey),
        "backpressurecaller" | "backpressure_caller" => Some(contract::OverflowPolicy::BackpressureCaller),
        _ => None,
    }
}

fn queue_record_from_snapshot(snapshot: &contract::BackgroundRunSnapshot) -> store::QueueRecord {
    store::QueueRecord {
        queue_id: snapshot.queue_ref.clone(),
        owner_scope: snapshot.owner_scope.clone(),
        revision: 1,
        max_active: 1,
        max_queued: contract::MAX_QUEUE_DEPTH,
        priority: serde_json::to_string(&snapshot.priority).unwrap_or_else(|_| "\"background_workflow\"".into()).trim_matches('"').to_owned(),
        overflow_policy: "reject_new".into(),
        content_hash: "default-background-queue".into(),
    }
}

fn missed_fire_policy(value: &str) -> contract::MissedFirePolicy {
    match value {
        "fireoncenow" | "fire_once_now" => contract::MissedFirePolicy::FireOnceNow,
        "coalesce" => contract::MissedFirePolicy::Coalesce,
        "catchupbounded" | "catch_up_bounded" => contract::MissedFirePolicy::CatchUpBounded,
        _ => contract::MissedFirePolicy::Skip,
    }
}

fn overlap_policy(value: &str) -> contract::OverlapPolicy {
    match value {
        "queuenext" | "queue_next" => contract::OverlapPolicy::QueueNext,
        "skipifrunning" | "skip_if_running" => contract::OverlapPolicy::SkipIfRunning,
        "cancelpreviousthenrun" | "cancel_previous_then_run" => contract::OverlapPolicy::CancelPreviousThenRun,
        "coalesce" => contract::OverlapPolicy::Coalesce,
        _ => contract::OverlapPolicy::Allow,
    }
}

fn background_schedule_slots(
    spec: &contract::ScheduleSpec,
    last_slot: Option<&str>,
    now_ms: i64,
    policy: contract::MissedFirePolicy,
) -> Option<(i64, Vec<i64>)> {
    let last = last_slot.and_then(|value| value.parse::<i64>().ok());
    let latest = match spec {
        contract::ScheduleSpec::OneShotAt { wake_at_ms } if now_ms >= *wake_at_ms => *wake_at_ms,
        contract::ScheduleSpec::Interval { every_ms, first_at_ms } if now_ms >= *first_at_ms => {
            (*first_at_ms).saturating_add(((now_ms - *first_at_ms) / *every_ms).saturating_mul(*every_ms))
        }
        contract::ScheduleSpec::Cron { minute, hour, timezone_minutes } => {
            let local_minute = now_ms.div_euclid(60_000) + i64::from(*timezone_minutes);
            let day_start = local_minute.div_euclid(1_440) * 1_440;
            let target_local = day_start + i64::from(*hour) * 60 + i64::from(*minute);
            let target_local = if target_local > local_minute { target_local - 1_440 } else { target_local };
            (target_local - i64::from(*timezone_minutes)) * 60_000
        }
        _ => return None,
    };
    if last.is_some_and(|value| value >= latest) { return None; }
    let mut slots = match spec {
        contract::ScheduleSpec::Interval { every_ms, first_at_ms } => {
            let first = last.map_or(*first_at_ms, |value| value.saturating_add(*every_ms));
            let count = (latest.saturating_sub(first) / *every_ms).saturating_add(1).max(0) as usize;
            match policy {
                contract::MissedFirePolicy::CatchUpBounded => {
                    let start = count.saturating_sub(contract::MAX_CATCH_UP as usize);
                    (start..count).map(|index| first + index as i64 * *every_ms).collect()
                }
                contract::MissedFirePolicy::Skip if count > 1 => Vec::new(),
                _ => vec![latest],
            }
        }
        _ => match policy {
            contract::MissedFirePolicy::Skip if now_ms.saturating_sub(latest) > 60_000 => Vec::new(),
            _ => vec![latest],
        },
    };
    slots.sort_unstable();
    Some((latest, slots))
}

fn wait_condition_satisfied(c: &rusqlite::Connection, condition: &contract::WaitCondition, now_ms: i64) -> bool {
    match condition {
        contract::WaitCondition::WaitUntil { wake_at_ms } => now_ms >= *wake_at_ms,
        contract::WaitCondition::WaitForDuration { resolved_wake_at_ms, .. } => now_ms >= *resolved_wake_at_ms,
        contract::WaitCondition::WaitForRunState { run_id, state, timeout_at_ms } => {
            automation_store::get_run(c, run_id).ok().flatten().is_some_and(|run| run.state == *state)
                || timeout_at_ms.is_some_and(|timeout| now_ms >= timeout)
        }
        contract::WaitCondition::WaitForHumanWorkItem { work_item_id, timeout_at_ms } => {
            c.query_row("SELECT state FROM human_work_items WHERE id=?1", [work_item_id], |row| row.get::<_, String>(0)).ok().is_some_and(|state| matches!(state.as_str(), "accepted" | "completed" | "resolved" | "cancelled"))
                || timeout_at_ms.is_some_and(|timeout| now_ms >= timeout)
        }
    }
}

impl EventJournal {
    pub async fn recover_durable_background_execution(&self, now_ms: i64) -> Result<u32, StorageError> {
        let database = self.database.lock().await;
        store::reconcile_after_restart(database.connection(), now_ms).map_err(StorageError::from)
    }

    pub async fn poll_durable_background_execution(&self, now_ms: i64) -> Result<u32, StorageError> {
        let scheduled = self.poll_background_schedules(now_ms).await?;
        let mut database = self.database.lock().await;
        let waits = store::list_waits(database.connection(), 256)?;
        let mut resumed = scheduled;
        for wait in waits {
            let condition: contract::WaitCondition = match serde_json::from_slice(&wait.condition_json) {
                Ok(condition) => condition,
                Err(_) => continue,
            };
            if wait_condition_satisfied(database.connection(), &condition, now_ms) {
                let run_id = wait.run_id;
                if let Some(run) = automation_store::get_run(database.connection(), &run_id)? {
                let transitioned = automation_store::transition_run(
                    database.connection_mut(),
                    automation_store::RunTransition { run_id: &run_id, from_state: "waiting", to_state: "queued", generation: run.generation, event_type: "background.wakeup", payload_json: "{}", now_ms },
                )?;
                let transitioned = if transitioned { true } else {
                    automation_store::transition_run(
                        database.connection_mut(),
                        automation_store::RunTransition { run_id: &run_id, from_state: "admitted", to_state: "queued", generation: run.generation, event_type: "background.wakeup", payload_json: "{}", now_ms },
                    )?
                };
                if transitioned && store::complete_wait(database.connection(), &run_id, wait.revision, now_ms)? {
                    resumed += 1;
                }
            }
            }
        }
        Ok(resumed)
    }

    async fn poll_background_schedules(&self, now_ms: i64) -> Result<u32, StorageError> {
        let schedules = {
            let mut database = self.database.lock().await;
            store::list_background_schedules(database.connection(), "", 256)?
        };
        let mut fired = 0;
        for schedule in schedules {
            if !schedule.enabled { continue; }
            let spec: contract::ScheduleSpec = match serde_json::from_str(&schedule.spec_json) {
                Ok(spec) => spec,
                Err(_) => continue,
            };
            let policy = missed_fire_policy(&schedule.missed_fire_policy);
            let Some((latest, slots)) = background_schedule_slots(&spec, schedule.last_slot.as_deref(), now_ms, policy) else { continue; };
            let mut database = self.database.lock().await;
            let Some(definition) = automation_store::get_definition(database.connection(), &schedule.definition_id, schedule.revision, &schedule.owner_scope)? else { continue; };
            let mut cursor = schedule.last_slot.clone();
            if slots.is_empty() {
                if automation_store::advance_schedule_slot(database.connection(), &schedule.schedule_id, cursor.as_deref(), &latest.to_string(), now_ms)? { fired += 1; }
                if matches!(spec, contract::ScheduleSpec::OneShotAt { .. }) {
                    let _ = automation_store::set_schedule_enabled(database.connection(), &schedule.schedule_id, false, now_ms);
                }
                continue;
            }
            for logical_fire_ms in slots {
                let fire_key = contract::schedule_fire_key(&schedule.schedule_id, schedule.revision, logical_fire_ms).map_err(|error| StorageError::InvalidInput(error.to_string()))?;
                let queue_id = "workflow-default";
                let active_run = store::find_active_by_concurrency_key(database.connection(), queue_id, &schedule.owner_scope, &schedule.schedule_id)?;
                match overlap_policy(&schedule.overlap_policy) {
                    contract::OverlapPolicy::SkipIfRunning | contract::OverlapPolicy::Coalesce if active_run.is_some() => {
                        if automation_store::advance_schedule_slot(database.connection(), &schedule.schedule_id, cursor.as_deref(), &logical_fire_ms.to_string(), now_ms)? { cursor = Some(logical_fire_ms.to_string()); }
                        continue;
                    }
                    contract::OverlapPolicy::CancelPreviousThenRun => {
                        if let Some(active_run) = active_run { let _ = automation_store::cancel_run(database.connection_mut(), &active_run, now_ms)?; }
                    }
                    _ => {}
                }
                let snapshot = contract::BackgroundRunSnapshot {
                    schema_version: contract::SCHEMA_VERSION,
                    run_id: format!("schedule:{}", fire_key),
                    kind: contract::BackgroundRunKind::RegisteredBackgroundTask,
                    owner_scope: schedule.owner_scope.clone(),
                    source_definition_ref: schedule.definition_id.clone(),
                    source_definition_revision: schedule.revision,
                    queue_ref: queue_id.into(),
                    concurrency_key: Some(schedule.schedule_id.clone()),
                    priority: contract::PriorityClass::ScheduledWorkflow,
                    environment_snapshot_ref: "schedule:environment".into(),
                    execution_policy_ref: "schedule:policy".into(),
                    approval_policy_ref: Some("schedule:approval".into()),
                    idempotency_key: Some(fire_key.clone()),
                    state: contract::RunState::Accepted,
                    attempt: 0,
                    next_wakeup_at_ms: None,
                    content_hash: definition.definition_hash.clone(),
                };
                let snapshot_json = serde_json::to_vec(&snapshot).map_err(|_| StorageError::InvalidInput("schedule snapshot serialization".into()))?;
                let queue = queue_record_from_snapshot(&snapshot);
                store::save_queue(database.connection(), &queue, now_ms)?;
                let run = automation_store::AutomationRunRecord {
                    run_id: snapshot.run_id.clone(),
                    definition_id: schedule.definition_id.clone(),
                    revision: schedule.revision,
                    owner_scope: schedule.owner_scope.clone(),
                    idempotency_key: fire_key,
                    payload_hash: definition.definition_hash.clone(),
                    state: "admitted".into(),
                    generation: 1,
                    permission_snapshot: snapshot.execution_policy_ref.clone(),
                    approval_snapshot: snapshot.approval_policy_ref.clone().unwrap_or_default(),
                };
                match automation_store::admit_run(database.connection(), &run, now_ms)? {
                    automation_store::AdmitRunResult::IdempotencyConflict { .. } => continue,
                    automation_store::AdmitRunResult::Existing(_) => {},
                    automation_store::AdmitRunResult::Inserted => {
                        store::save_run_metadata(database.connection(), &run.run_id, "background", &snapshot_json, &snapshot.queue_ref, "scheduled_workflow", snapshot.concurrency_key.as_deref(), &snapshot.content_hash, None, now_ms)?;
                        automation_store::transition_run(database.connection_mut(), automation_store::RunTransition { run_id: &run.run_id, from_state: "admitted", to_state: "queued", generation: 1, event_type: "background.schedule_fired", payload_json: "{}", now_ms })?;
                        fired += 1;
                    }
                }
                if automation_store::advance_schedule_slot(database.connection(), &schedule.schedule_id, cursor.as_deref(), &logical_fire_ms.to_string(), now_ms)? {
                    cursor = Some(logical_fire_ms.to_string());
                }
            }
            if matches!(spec, contract::ScheduleSpec::OneShotAt { .. }) {
                let _ = automation_store::set_schedule_enabled(database.connection(), &schedule.schedule_id, false, now_ms);
            }
        }
        Ok(fired)
    }

    pub async fn durable_background_command(
        &self,
        operation: &str,
        run_id: &str,
        owner_scope: &str,
        payload: &[u8],
        expected_revision: u64,
        idempotency_key: &str,
    ) -> Result<Vec<u8>, String> {
        if operation.is_empty() || operation.len() > contract::MAX_ID_BYTES || owner_scope.is_empty() || owner_scope.len() > contract::MAX_SCOPE_BYTES || payload.len() > contract::MAX_SNAPSHOT_BYTES {
            return Err("invalid_request".into());
        }
        let mut database = self.database.lock().await;
        let c = database.connection_mut();
        let now = crate::task_memory::now_millis() as i64;
        let response = match operation {
            "create_run" => {
                let request: CreateRunRequest = serde_json::from_slice(payload).map_err(|_| "invalid_run_snapshot".to_string())?;
                request.snapshot.validate().map_err(|e| e.to_string())?;
                if request.snapshot.owner_scope != owner_scope
                    || request.snapshot.idempotency_key.is_none() && idempotency_key.is_empty()
                    || !idempotency_key.is_empty() && request.snapshot.idempotency_key.as_deref() != Some(idempotency_key)
                { return Err("scope_or_idempotency_mismatch".into()); }
                if automation_store::get_definition(c, &request.snapshot.source_definition_ref, request.snapshot.source_definition_revision, owner_scope)
                    .map_err(|e| e.to_string())?.is_none()
                { return Err("unknown_definition".into()); }
                if let Some(definition_json) = request.definition_json.as_deref() {
                    if definition_json.len() > contract::MAX_SNAPSHOT_BYTES || serde_json::from_str::<Value>(definition_json).is_err() { return Err("invalid_definition_snapshot".into()); }
                }
                let snapshot_json = serde_json::to_vec(&request.snapshot).map_err(|_| "invalid_run_snapshot".to_string())?;
                if !bounded_json(&snapshot_json) { return Err("snapshot_too_large".into()); }
                let queue = store::get_queue(c, &request.snapshot.queue_ref, owner_scope).map_err(|e| e.to_string())?.or_else(|| {
                    if request.snapshot.queue_ref == "workflow-default" {
                        let queue = queue_record_from_snapshot(&request.snapshot);
                        let _ = store::save_queue(c, &queue, now);
                        Some(queue)
                    } else { None }
                }).ok_or_else(|| "unknown_queue".to_string())?;
                let (queued, active) = store::queue_load(c, &queue.queue_id, owner_scope).map_err(|e| e.to_string())?;
                if queued >= queue.max_queued || active >= queue.max_active && queue.max_queued == 0 {
                    match overflow_policy(&queue.overflow_policy) {
                        Some(contract::OverflowPolicy::CoalesceByKey) if request.snapshot.concurrency_key.is_some() => {
                            let key = request.snapshot.concurrency_key.as_deref().unwrap_or_default();
                            let existing = store::find_queued_by_concurrency_key(c, &queue.queue_id, owner_scope, key).map_err(|e| e.to_string())?;
                            if let Some(existing) = existing { return Ok(serde_json::to_vec(&serde_json::json!({"status":"ok","accepted":true,"coalesced":true,"run_id":existing,"state":"queued"})).unwrap_or_default()); }
                            return Err("queue_overflow_coalescing_key_not_found".into());
                        }
                        Some(contract::OverflowPolicy::DropOldestAllowed) => {
                            if store::drop_oldest_queued(c, &queue.queue_id, owner_scope, now).map_err(|e| e.to_string())?.is_none() { return Err("queue_overflow_no_droppable_run".into()); }
                        }
                        Some(contract::OverflowPolicy::BackpressureCaller) => return Err("queue_backpressure".into()),
                        _ => return Err("queue_overflow".into()),
                    }
                }
                let run = automation_store::AutomationRunRecord { run_id: request.snapshot.run_id.clone(), definition_id: request.snapshot.source_definition_ref.clone(), revision: request.snapshot.source_definition_revision, owner_scope: owner_scope.into(), idempotency_key: request.snapshot.idempotency_key.clone().unwrap_or_else(|| idempotency_key.into()), payload_hash: request.payload_hash.clone(), state: "admitted".into(), generation: 1, permission_snapshot: request.snapshot.execution_policy_ref.clone(), approval_snapshot: request.snapshot.approval_policy_ref.clone().unwrap_or_default() };
                match automation_store::admit_run(c, &run, now).map_err(|e| e.to_string())? {
                    automation_store::AdmitRunResult::IdempotencyConflict { .. } => return Err("idempotency_conflict".into()),
                    automation_store::AdmitRunResult::Existing(existing) => serde_json::json!({"status":"ok","accepted":true,"deduplicated":true,"run_id":existing.run_id,"state":existing.state}),
                    automation_store::AdmitRunResult::Inserted => {
                        store::save_run_metadata(c, &run.run_id, "background", &snapshot_json, &request.snapshot.queue_ref, &format!("{:?}", request.snapshot.priority).to_ascii_lowercase(), request.snapshot.concurrency_key.as_deref(), &request.snapshot.content_hash, request.snapshot.next_wakeup_at_ms, now).map_err(|e| e.to_string())?;
                        if let Some(wake) = request.snapshot.next_wakeup_at_ms { store::put_wait(c, &store::WaitRecord { run_id: run.run_id.clone(), revision: request.snapshot.source_definition_revision, condition_json: serde_json::to_vec(&serde_json::json!({"kind":"wait_until","wake_at_ms":wake})).unwrap_or_default(), wake_at_ms: Some(wake), state: "scheduled".into() }, now).map_err(|e| e.to_string())?; }
                        if request.snapshot.next_wakeup_at_ms.is_none() {
                            automation_store::transition_run(c, automation_store::RunTransition { run_id: &run.run_id, from_state: "admitted", to_state: "queued", generation: run.generation, event_type: "background.accepted", payload_json: "{}", now_ms: now }).map_err(|e| e.to_string())?;
                        }
                        serde_json::json!({"status":"ok","accepted":true,"deduplicated":false,"run_id":run.run_id,"state":if request.snapshot.next_wakeup_at_ms.is_some() { "scheduled" } else { "queued" }})
                    }
                }
            }
            "get_run" => {
                let run = automation_store::get_run(c, run_id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown_run".to_string())?;
                if run.owner_scope != owner_scope { return Err("scope_mismatch".into()); }
                let snapshot = store::background_snapshot(c, run_id).map_err(|e| e.to_string())?.unwrap_or_default();
                serde_json::json!({"status":"ok","run_id":run.run_id,"state":run.state,"generation":run.generation,"revision":expected_revision,"snapshot":serde_json::from_slice::<Value>(&snapshot).unwrap_or_else(|_| serde_json::json!({"redacted":true}))})
            }
            "list_runs" => {
                let runs = automation_store::list_runs(c, owner_scope, "", 256).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"ok","runs":runs.into_iter().map(|run| serde_json::json!({"run_id":run.run_id,"definition_id":run.definition_id,"revision":run.revision,"state":run.state,"generation":run.generation,"owner_scope":run.owner_scope})).collect::<Vec<_>>()})
            }
            "list_schedules" => {
                let schedules = store::list_background_schedules(c, owner_scope, 256).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"ok","schedules":schedules.into_iter().map(|schedule| serde_json::json!({"schedule_id":schedule.schedule_id,"definition_id":schedule.definition_id,"revision":schedule.revision,"owner_scope":schedule.owner_scope,"spec":serde_json::from_str::<Value>(&schedule.spec_json).unwrap_or_else(|_| serde_json::json!({"redacted":true})),"missed_fire_policy":schedule.missed_fire_policy,"overlap_policy":schedule.overlap_policy,"enabled":schedule.enabled})).collect::<Vec<_>>()})
            }
            "create_schedule" | "revise_schedule" => {
                let request: BackgroundScheduleRequest = serde_json::from_slice(payload).map_err(|_| "invalid_schedule".to_string())?;
                if request.owner_scope != owner_scope || request.schedule_id.is_empty() || request.schedule_id.len() > contract::MAX_ID_BYTES || request.revision == 0 { return Err("invalid_schedule_scope_or_revision".into()); }
                request.spec.validate().map_err(|e| e.to_string())?;
                if automation_store::get_definition(c, &request.definition_id, request.revision, owner_scope).map_err(|e| e.to_string())?.is_none() { return Err("unknown_definition".into()); }
                let (hour, minute, timezone_minutes) = match &request.spec { contract::ScheduleSpec::Cron { minute, hour, timezone_minutes } => (*hour, *minute, *timezone_minutes), _ => (0, 0, 0) };
                let spec_json = serde_json::to_string(&request.spec).map_err(|_| "invalid_schedule".to_string())?;
                let saved = store::save_background_schedule(c, &automation_store::AutomationScheduleRecord { schedule_id: request.schedule_id.clone(), definition_id: request.definition_id, revision: request.revision, owner_scope: owner_scope.into(), hour, minute, timezone_minutes, missed_grace_ms: 0, enabled: request.enabled, last_slot: None, preset_id: None, preset_revision: None, preset_content_hash: None, workspace_path: request.workspace_path }, &spec_json, &format!("{:?}", request.missed_fire_policy).to_ascii_lowercase(), &format!("{:?}", request.overlap_policy).to_ascii_lowercase(), now).map_err(|e| e.to_string())?;
                serde_json::json!({"status":if saved {"ok"} else {"rejected"},"schedule_id":request.schedule_id,"saved":saved})
            }
            "set_schedule_enabled" => {
                let schedules = store::list_background_schedules(c, owner_scope, 256).map_err(|e| e.to_string())?;
                if !schedules.iter().any(|schedule| schedule.schedule_id == run_id) { return Err("unknown_schedule".into()); }
                let enabled = serde_json::from_slice::<serde_json::Value>(payload).ok().and_then(|value| value.get("enabled").and_then(Value::as_bool)).ok_or_else(|| "invalid_schedule_enabled".to_string())?;
                let changed = automation_store::set_schedule_enabled(c, run_id, enabled, now).map_err(|e| e.to_string())?;
                serde_json::json!({"status":if changed {"ok"} else {"rejected"},"schedule_id":run_id,"enabled":enabled})
            }
            "resume_run" => {
                let run = automation_store::get_run(c, run_id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown_run".to_string())?;
                if run.owner_scope != owner_scope { return Err("scope_mismatch".into()); }
                let transitioned = automation_store::transition_run(c, automation_store::RunTransition { run_id, from_state: "paused", to_state: "queued", generation: run.generation, event_type: "background.resume", payload_json: "{}", now_ms: now }).map_err(|e| e.to_string())?;
                serde_json::json!({"status":if transitioned {"ok"} else {"rejected"},"run_id":run_id,"resumed":transitioned})
            }
            "cancel_run" => {
                let run = automation_store::get_run(c, run_id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown_run".to_string())?;
                if run.owner_scope != owner_scope { return Err("scope_mismatch".into()); }
                let changed = automation_store::cancel_run(c, run_id, now).map_err(|e| e.to_string())?;
                serde_json::json!({"status":if changed {"ok"} else {"rejected"},"run_id":run_id,"cancelled":changed})
            }
            "wait" => {
                let run = automation_store::get_run(c, run_id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown_run".to_string())?;
                if run.owner_scope != owner_scope { return Err("scope_mismatch".into()); }
                let condition: contract::WaitCondition = serde_json::from_slice(payload).map_err(|_| "invalid_wait".to_string())?;
                condition.validate().map_err(|e| e.to_string())?;
                let wake_at = match condition {
                    contract::WaitCondition::WaitUntil { wake_at_ms } => Some(wake_at_ms),
                    contract::WaitCondition::WaitForDuration { resolved_wake_at_ms, .. } => Some(resolved_wake_at_ms),
                    contract::WaitCondition::WaitForRunState { timeout_at_ms, .. } | contract::WaitCondition::WaitForHumanWorkItem { timeout_at_ms, .. } => timeout_at_ms,
                };
                let json = serde_json::to_vec(&condition).map_err(|_| "invalid_wait".to_string())?;
                let saved = store::put_wait(c, &store::WaitRecord { run_id: run_id.into(), revision: expected_revision.max(1), condition_json: json, wake_at_ms: wake_at, state: "waiting".into() }, now).map_err(|e| e.to_string())?;
                serde_json::json!({"status":if saved {"ok"} else {"rejected"},"run_id":run_id,"waiting":saved})
            }
            "list_queues" => {
                let queues = store::list_queues(c, owner_scope, 256).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"ok","queues":queues.into_iter().map(|q| serde_json::json!({"queue_id":q.queue_id,"revision":q.revision,"max_active":q.max_active,"max_queued":q.max_queued,"priority":q.priority,"overflow_policy":q.overflow_policy,"content_hash":q.content_hash})).collect::<Vec<_>>()})
            }
            "upsert_queue" => {
                let queue: contract::BackgroundQueue = serde_json::from_slice(payload).map_err(|_| "invalid_queue".to_string())?;
                queue.validate().map_err(|e| e.to_string())?;
                if queue.owner_scope != owner_scope { return Err("scope_mismatch".into()); }
                let saved = store::save_queue(c, &store::QueueRecord { queue_id: queue.queue_id.clone(), owner_scope: queue.owner_scope, revision: queue.revision, max_active: queue.max_active, max_queued: queue.max_queued, priority: format!("{:?}", queue.priority).to_ascii_lowercase(), overflow_policy: format!("{:?}", queue.overflow).to_ascii_lowercase(), content_hash: queue.content_hash }, now).map_err(|e| e.to_string())?;
                serde_json::json!({"status":if saved {"ok"} else {"rejected"},"queue_id":queue.queue_id,"saved":saved})
            }
            "list_attempts" => {
                let attempts = store::list_attempts(c, run_id, 256).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"ok","run_id":run_id,"attempts":attempts.into_iter().map(|a| serde_json::json!({"attempt_id":a.attempt_id,"generation":a.generation,"dispatcher_id":a.dispatcher_id,"state":a.state,"outcome_code":a.outcome_code,"started_at_ms":a.started_at_ms,"ended_at_ms":a.ended_at_ms})).collect::<Vec<_>>()})
            }
            "wake_due" => {
                let due = store::due_wakeups(c, now, 256).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"ok","due":due.into_iter().map(|(_,run)|run).collect::<Vec<_>>()})
            }
            "dispatch_once" => {
                let run = automation_store::get_run(c, run_id).map_err(|e| e.to_string())?.ok_or_else(|| "unknown_run".to_string())?;
                if run.owner_scope != owner_scope { return Err("scope_mismatch".into()); }
                let attempt_id = format!("background:{}:{}", run_id, run.generation);
                let inserted = store::insert_attempt(c, &store::AttemptRecord { attempt_id, run_id: run_id.into(), generation: run.generation, dispatcher_id: "core-background-plane".into(), state: "blocked".into(), outcome_code: "runtime_adapter_unavailable".into(), started_at_ms: Some(now), ended_at_ms: Some(now) }, now).map_err(|e| e.to_string())?;
                serde_json::json!({"status":"blocked","error_code":"runtime_adapter_unavailable","run_id":run_id,"retryable":false,"attempt_recorded":inserted})
            }
            _ => return Err("unsupported_operation".into()),
        };
        serde_json::to_vec(&response).map_err(|_| "serialization_failed".into())
    }

    pub async fn durable_background_error_projection(&self, error: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"status":"error","error_code":error_code(error),"raw_payload":false,"secrets":false})).unwrap_or_else(|_| br#"{"status":"error","error_code":"serialization_failed"}"#.to_vec())
    }
}
