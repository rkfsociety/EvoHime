use super::*;

impl EventJournal {
    /// Persists one validated child handoff envelope.
    pub async fn save_child_handoff(
        &self,
        record: &evohime_local_storage::domains::agents::HandoffRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::insert_handoff(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists persisted child handoffs for a task, in sequence order.
    pub async fn list_child_handoffs(
        &self,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::domains::agents::HandoffRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::list_handoffs_by_task(
            database.connection(),
            task_id,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one validated, read-only child task request.
    pub async fn save_child_task_request(
        &self,
        record: &evohime_local_storage::domains::agents::ChildTaskRequestRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::insert_child_task_request(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Fetches one persisted child task request by its child_task_id.
    pub async fn get_child_task_request(
        &self,
        child_task_id: &str,
    ) -> Result<Option<evohime_local_storage::domains::agents::ChildTaskRequestRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::get_child_task_request(
            database.connection(),
            child_task_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one accepted child report.
    pub async fn save_child_report(
        &self,
        record: &evohime_local_storage::domains::agents::ChildReportRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::insert_child_report(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Returns the next sequence number for a parent's child-task events.
    ///
    /// # Errors
    ///
    /// Returns a storage error string if the sequence cannot be read.
    pub async fn next_child_parent_sequence(&self, parent_task_id: &str) -> Result<u64, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::next_parent_sequence(
            database.connection(),
            parent_task_id,
        )
        .map(|value| value as u64)
        .map_err(|error| error.to_string())
    }

    /// Persists or updates a coordinator checkpoint for child-task recovery.
    ///
    /// # Errors
    ///
    /// Returns a storage error string if the checkpoint cannot be stored.
    pub async fn save_coordinator_checkpoint(
        &self,
        record: &evohime_local_storage::domains::agents::CoordinatorCheckpointRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::upsert_coordinator_checkpoint(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Loads the newest persisted coordinator checkpoint for a child task.
    ///
    /// Returns `None` when the child has no checkpoint.
    ///
    /// # Errors
    ///
    /// Returns a storage error string if the lookup fails.
    pub async fn get_coordinator_checkpoint(
        &self,
        child_task_id: &str,
    ) -> Result<Option<evohime_local_storage::domains::agents::CoordinatorCheckpointRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::latest_coordinator_checkpoint(
            database.connection(),
            child_task_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists child-task checkpoints whose retry deadline has expired.
    ///
    /// The query is scoped to one parent and evaluated against `now_ms`.
    ///
    /// # Errors
    ///
    /// Returns a storage error string if the query fails.
    pub async fn list_child_dead_letters(
        &self,
        parent_task_id: &str,
        now_ms: i64,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::domains::agents::CoordinatorCheckpointRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::domains::agents::ChildStoreSql::list_dead_letter_checkpoints(
            database.connection(),
            parent_task_id,
            now_ms,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Validates and persists a typed report submitted by a child task.
    ///
    /// The child workflow may offload large report content while preserving its
    /// typed metadata and linkage.
    ///
    /// # Errors
    ///
    /// Returns a string error if the request/report pairing is invalid or
    /// persistence fails.
    pub async fn accept_typed_child_report(
        &self,
        request: &crate::child_contracts::TypedChildTaskRequest,
        report: &crate::child_contracts::TypedChildReport,
        now_ms: i64,
    ) -> Result<crate::child_contracts::TypedChildReport, String> {
        let database = self.database.lock().await;
        crate::child_workflow::accept_report_with_offload(
            database.connection(),
            request,
            report,
            now_ms,
        )
        .map_err(|error| error.to_string())
    }

    /// Loads the project build policy or persists the supplied default.
    ///
    /// Returned policies are hardened through the Core build-policy boundary.
    ///
    /// # Errors
    ///
    /// Returns a string error for storage or persisted-policy decoding failures.
    pub async fn get_or_create_build_policy(
        &self,
        project_id: &str,
        default_policy: &crate::scope::BuildScope,
    ) -> Result<crate::scope::BuildScope, String> {
        let database = self.database.lock().await;
        if let Some(record) = database
            .get_project_policy(project_id)
            .map_err(|error| error.to_string())?
        {
            return serde_json::from_slice(&record.policy_json)
                .map(harden_build_policy)
                .map_err(|error| format!("invalid persisted build policy: {error}"));
        }
        let policy_json = serde_json::to_vec(default_policy).map_err(|error| error.to_string())?;
        database
            .upsert_project_policy(project_id, &policy_json, None)
            .map_err(|error| error.to_string())?;
        Ok(harden_build_policy(default_policy.clone()))
    }

    /// Loads a project's hardened build policy and its persisted version.
    ///
    /// If no record exists, the default is persisted first.
    ///
    /// # Errors
    ///
    /// Returns a string error for storage, serialization, or policy decoding
    /// failures.
    pub async fn get_build_policy(
        &self,
        project_id: &str,
        default_policy: &crate::scope::BuildScope,
    ) -> Result<(crate::scope::BuildScope, i64), String> {
        let database = self.database.lock().await;
        let record = match database
            .get_project_policy(project_id)
            .map_err(|error| error.to_string())?
        {
            Some(record) => record,
            None => {
                let policy_json =
                    serde_json::to_vec(default_policy).map_err(|error| error.to_string())?;
                database
                    .upsert_project_policy(project_id, &policy_json, None)
                    .map_err(|error| error.to_string())?
            }
        };
        let policy = serde_json::from_slice(&record.policy_json)
            .map(harden_build_policy)
            .map_err(|error| format!("invalid persisted build policy: {error}"))?;
        Ok((policy, record.version))
    }

    /// Stores a project build policy, optionally enforcing optimistic versioning.
    ///
    /// When `expected_version` is supplied, the update is rejected if the
    /// stored record has changed since it was read.
    ///
    /// # Errors
    ///
    /// Returns a string error for serialization, version conflict, or storage
    /// failure.
    pub async fn save_build_policy(
        &self,
        project_id: &str,
        policy: &crate::scope::BuildScope,
        expected_version: Option<i64>,
    ) -> Result<ProjectPolicyRecord, String> {
        let policy_json = serde_json::to_vec(policy).map_err(|error| error.to_string())?;
        let database = self.database.lock().await;
        database
            .upsert_project_policy(project_id, &policy_json, expected_version)
            .map_err(|error| error.to_string())
    }

    /// Loads the persisted hive safety ceiling, or the defaults when unset.
    ///
    /// The read-API validates the stored snapshot on the way out: a corrupt or
    /// out-of-range value is reported instead of silently widening the
    /// sub-agent ceiling. Callers never receive a partially valid ceiling.
    ///
    /// # Errors
    ///
    /// Returns a storage or validation error string.
    pub async fn hive_settings(&self) -> Result<crate::hive_settings::HiveSettings, String> {
        let database = self.database.lock().await;
        let stored = evohime_local_storage::schema_driven_agent_configuration_store::load(
            database.connection(),
            crate::hive_settings::HIVE_SETTINGS_SCOPE,
        )
        .map_err(|error| error.to_string())?;
        match stored {
            Some((_schema, snapshot, _revision)) => {
                crate::hive_settings::HiveSettings::from_json(&snapshot)
                    .map_err(|error| error.to_string())
            }
            None => Ok(crate::hive_settings::HiveSettings::default()),
        }
    }

    /// Validates and persists the hive safety ceiling.
    ///
    /// Returns the new optimistic revision. A concurrent writer that changed
    /// the stored revision first makes the save fail rather than last-write-win.
    ///
    /// # Errors
    ///
    /// Returns a validation, revision-conflict, or storage error string.
    pub async fn save_hive_settings(
        &self,
        settings: &crate::hive_settings::HiveSettings,
    ) -> Result<u64, String> {
        settings.validate().map_err(|error| error.to_string())?;
        let schema = crate::hive_settings::HiveSettings::schema_json();
        let snapshot = settings.to_json().map_err(|error| error.to_string())?;
        let now = crate::task_memory::now_millis() as i64;
        let database = self.database.lock().await;
        let current = evohime_local_storage::schema_driven_agent_configuration_store::load(
            database.connection(),
            crate::hive_settings::HIVE_SETTINGS_SCOPE,
        )
        .map_err(|error| error.to_string())?
        .map(|(_, _, revision)| revision)
        .unwrap_or(0);
        let next = current.saturating_add(1);
        let saved = evohime_local_storage::schema_driven_agent_configuration_store::save(
            database.connection(),
            crate::hive_settings::HIVE_SETTINGS_SCOPE,
            &schema,
            &snapshot,
            next,
            now,
            current,
        )
        .map_err(|error| error.to_string())?;
        if !saved {
            return Err("hive settings revision conflict".to_string());
        }
        Ok(next)
    }
}

impl evohime_hive_contract::HiveJournalProvider for EventJournal {
    fn latest_sequence(&self) -> evohime_hive_contract::BoxFuture<'_, i64> {
        Box::pin(async move { self.latest_sequence().await })
    }

    fn replay_events(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> evohime_hive_contract::BoxFuture<
        '_,
        Result<
            Vec<evohime_hive_contract::HiveEventRecord>,
            evohime_hive_contract::HiveContractError,
        >,
    > {
        Box::pin(async move {
            let records = self.replay(after_sequence, limit).await.map_err(|error| {
                evohime_hive_contract::HiveContractError::Storage(error.to_string())
            })?;
            Ok(records
                .into_iter()
                .map(|record| evohime_hive_contract::HiveEventRecord {
                    sequence_id: record.sequence_id,
                    task_id: record.task_id,
                    event_type: record.event_type,
                    payload: record.payload,
                    created_at_ms: chrono::DateTime::parse_from_rfc3339(&record.created_at)
                        .map(|value| value.timestamp_millis())
                        .unwrap_or(0),
                })
                .collect())
        })
    }

    fn record_audit<'a>(
        &'a self,
        task_id: &'a str,
        event_type: &'a str,
        payload: &'a [u8],
    ) -> evohime_hive_contract::BoxFuture<'a, Result<(), evohime_hive_contract::HiveContractError>>
    {
        Box::pin(async move {
            self.record_audit(task_id, event_type, payload)
                .await
                .map(|_| ())
                .map_err(|error| {
                    evohime_hive_contract::HiveContractError::Storage(error.to_string())
                })
        })
    }

    fn prune_signal_records<'a>(
        &'a self,
        event_types: &'a [&'a str],
        keep_limit: usize,
    ) -> evohime_hive_contract::BoxFuture<'a, ()> {
        Box::pin(async move {
            let Ok(database) = self.database.try_lock() else {
                return;
            };
            let limit = keep_limit as i64;
            for event_type in event_types {
                let _ = database.connection().execute(
                    "DELETE FROM events
                     WHERE event_type = ?1
                       AND sequence_id NOT IN (
                           SELECT sequence_id FROM events
                           WHERE event_type = ?1
                           ORDER BY sequence_id DESC LIMIT ?2
                       )",
                    rusqlite::params![event_type, limit],
                );
            }
        })
    }

    fn load_hive_settings_json(
        &self,
    ) -> evohime_hive_contract::BoxFuture<
        '_,
        Result<Option<serde_json::Value>, evohime_hive_contract::HiveContractError>,
    > {
        Box::pin(async move {
            let settings = self
                .hive_settings()
                .await
                .map_err(evohime_hive_contract::HiveContractError::Storage)?;
            let value = serde_json::to_value(settings).map_err(|error| {
                evohime_hive_contract::HiveContractError::Serialization(error.to_string())
            })?;
            Ok(Some(value))
        })
    }

    fn save_hive_settings_json<'a>(
        &'a self,
        settings: &'a serde_json::Value,
    ) -> evohime_hive_contract::BoxFuture<'a, Result<(), evohime_hive_contract::HiveContractError>>
    {
        Box::pin(async move {
            let parsed: crate::hive_settings::HiveSettings =
                serde_json::from_value(settings.clone()).map_err(|error| {
                    evohime_hive_contract::HiveContractError::Serialization(error.to_string())
                })?;
            self.save_hive_settings(&parsed)
                .await
                .map(|_| ())
                .map_err(evohime_hive_contract::HiveContractError::Storage)
        })
    }

    fn persistent_agent_roles(&self) -> evohime_hive_contract::BoxFuture<'_, Vec<String>> {
        Box::pin(async move {
            let database = self.database().lock().await;
            let Ok(agents) =
                evohime_local_storage::domains::agents::load_agents(database.connection(), 256)
            else {
                return Vec::new();
            };
            let mut roles: Vec<String> = agents
                .iter()
                .filter_map(|bytes| {
                    let agent: crate::persistent_agent_registry::PersistentAgent =
                        serde_json::from_slice(bytes).ok()?;
                    if agent.status != crate::persistent_agent_registry::LifecycleStatus::Active {
                        return None;
                    }
                    Some(agent.role_profile_ref.id)
                })
                .collect();
            roles.sort();
            roles.dedup();
            roles
        })
    }

    fn task_metrics(&self, now_ms: i64) -> evohime_hive_contract::BoxFuture<'_, (u32, u64)> {
        Box::pin(async move {
            let day_start_ms = now_ms.div_euclid(crate::hive_state::HIVE_DAY_WINDOW_MS)
                * crate::hive_state::HIVE_DAY_WINDOW_MS;
            let Some(day_start) =
                chrono::DateTime::<chrono::Utc>::from_timestamp_millis(day_start_ms)
                    .map(|ts| ts.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            else {
                return (0, 0);
            };
            let Ok(lease) = self.checkout_read_database() else {
                return (0, 0);
            };
            let Some(database) = lease.database() else {
                return (0, 0);
            };
            let result = database.connection().query_row(
                "WITH today_tasks AS (
                     SELECT task_id, MIN(sequence_id) AS start_sequence, MIN(created_at) AS started_at
                     FROM events WHERE event_type = 'task.started' AND created_at >= ?1
                     GROUP BY task_id
                 ), completed AS (
                     SELECT tasks.task_id, tasks.started_at, MIN(done.created_at) AS completed_at
                     FROM today_tasks AS tasks
                     JOIN events AS done ON done.task_id = tasks.task_id
                         AND done.event_type = 'task.completed'
                         AND done.sequence_id > tasks.start_sequence
                     GROUP BY tasks.task_id, tasks.started_at
                 )
                 SELECT (SELECT COUNT(*) FROM today_tasks),
                        (SELECT AVG((julianday(completed_at) - julianday(started_at)) * 86400.0)
                         FROM completed)",
                [day_start],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<f64>>(1)?)),
            );
            match result {
                Ok((count, average)) => (
                    u32::try_from(count.max(0)).unwrap_or(u32::MAX),
                    average
                        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                        .map_or(0, |seconds| seconds.round().min(u64::MAX as f64) as u64),
                ),
                Err(_) => (0, 0),
            }
        })
    }

    fn filter_hive_root_tasks(
        &self,
        roots: Vec<String>,
    ) -> evohime_hive_contract::BoxFuture<'_, Vec<String>> {
        Box::pin(async move {
            let mut result = Vec::new();
            let database = self.database().lock().await;
            for root in roots {
                if root.starts_with(crate::eva_pulse_boundary::EVA_PULSE_TASK_PREFIX) {
                    result.push(root);
                    continue;
                }
                if let Ok(Some((conversation_id, _, _))) =
                    evohime_local_storage::domains::audit::task_binding(
                        database.connection(),
                        &root,
                    )
                {
                    if conversation_id == crate::conversation_event_log::HIVE_SYSTEM_CHAT_ID {
                        result.push(root);
                    }
                }
            }
            result
        })
    }

    fn model_dispatch_windows<'a>(
        &'a self,
        provider: &'a str,
        model: &'a str,
        now_ms: i64,
        minute_window_ms: i64,
        hour_window_ms: i64,
        day_window_ms: i64,
    ) -> evohime_hive_contract::BoxFuture<
        'a,
        Result<
            evohime_hive_contract::HiveDispatchWindows,
            evohime_hive_contract::HiveContractError,
        >,
    > {
        Box::pin(async move {
            let windows = self
                .model_dispatch_windows(
                    provider,
                    model,
                    now_ms,
                    minute_window_ms,
                    hour_window_ms,
                    day_window_ms,
                )
                .await
                .map_err(|error| {
                    evohime_hive_contract::HiveContractError::Storage(error.to_string())
                })?;
            Ok(evohime_hive_contract::HiveDispatchWindows {
                last_minute: u32::try_from(windows.last_minute).unwrap_or(u32::MAX),
                last_hour: u32::try_from(windows.last_hour).unwrap_or(u32::MAX),
                last_4_hours: u32::try_from(windows.last_4_hours).unwrap_or(u32::MAX),
                last_day: u32::try_from(windows.last_day).unwrap_or(u32::MAX),
                oldest_minute_ms: windows.oldest_minute_ms,
                oldest_hour_ms: windows.oldest_hour_ms,
                oldest_4_hours_ms: windows.oldest_4_hours_ms,
                oldest_day_ms: windows.oldest_day_ms,
            })
        })
    }

    fn model_usage_since<'a>(
        &'a self,
        start_ms: i64,
        provider: &'a str,
        model: &'a str,
    ) -> evohime_hive_contract::BoxFuture<
        'a,
        Result<
            Vec<evohime_hive_contract::HiveModelUsage>,
            evohime_hive_contract::HiveContractError,
        >,
    > {
        Box::pin(async move {
            let rows = self
                .deepseek_usage_since(start_ms, provider, model)
                .await
                .map_err(|error| {
                    evohime_hive_contract::HiveContractError::Storage(error.to_string())
                })?;
            Ok(rows
                .into_iter()
                .map(|row| evohime_hive_contract::HiveModelUsage {
                    request_at: row.request_at,
                    prompt_tokens: row.prompt_tokens,
                    completion_tokens: row.completion_tokens,
                    cache_read_input_tokens: row.cache_read_input_tokens,
                })
                .collect())
        })
    }

    fn route_tokens_since<'a>(
        &'a self,
        start_ms: i64,
        route_scope: &'a str,
    ) -> evohime_hive_contract::BoxFuture<'a, Result<u64, evohime_hive_contract::HiveContractError>>
    {
        Box::pin(async move {
            self.route_tokens_since(start_ms, route_scope)
                .await
                .map_err(|error| {
                    evohime_hive_contract::HiveContractError::Storage(error.to_string())
                })
        })
    }

    fn register_worktree_lease(
        &self,
        request: evohime_hive_contract::HiveWorktreeLeaseRequest,
    ) -> evohime_hive_contract::BoxFuture<'_, ()> {
        Box::pin(async move {
            let database = self.database().lock().await;
            let task_id = request.task_id.clone();
            let record = evohime_local_storage::domains::workflow::TaskWorktreeRecord {
                worktree_id: request.worktree_id,
                task_id: request.task_id,
                repository_scope: crate::task_memory::workspace_scope_id(std::path::Path::new(
                    &request.base_root,
                )),
                branch: request.branch,
                root_ref: request.root_ref,
                base_commit: request.base_commit,
                state: "ready".into(),
                version: 1,
                idempotency_key: format!("hive-worktree:{task_id}"),
                updated_at_ms: crate::task_memory::now_millis() as i64,
            };
            let _ =
                evohime_local_storage::domains::workflow::create(database.connection(), &record);
        })
    }

    fn has_active_workflow_run<'a>(
        &'a self,
        task_id: &'a str,
    ) -> evohime_hive_contract::BoxFuture<'a, bool> {
        Box::pin(async move { self.has_active_workflow_run(task_id).await.unwrap_or(true) })
    }

    fn mark_worktree_cleanup_pending<'a>(
        &'a self,
        worktree_id: &'a str,
    ) -> evohime_hive_contract::BoxFuture<'a, ()> {
        Box::pin(async move {
            let database = self.database().lock().await;
            if let Ok(Some(current)) =
                evohime_local_storage::domains::workflow::get(database.connection(), worktree_id)
            {
                let _ = evohime_local_storage::domains::workflow::transition(
                    database.connection(),
                    worktree_id,
                    current.version,
                    "cleanup_pending",
                    crate::task_memory::now_millis() as i64,
                );
            }
        })
    }
}

impl evohime_hive_contract::HiveCoordinator for crate::TaskCoordinator {
    fn dispatch_task(
        &self,
        request: evohime_hive_contract::HiveTaskDispatchRequest,
    ) -> evohime_hive_contract::BoxFuture<'_, Result<(), evohime_hive_contract::HiveContractError>>
    {
        Box::pin(async move {
            self.dispatch(crate::CoreCommand::StartTask {
                task_id: request.task_id,
                prompt: request.prompt,
                workspace_root: request.workspace_root.map(std::path::PathBuf::from),
                preferred_route_hint: request.preferred_route_hint,
                conversation: None,
                goal_id: None,
                session_binding: None,
            })
            .await
            .map_err(|error| evohime_hive_contract::HiveContractError::Dispatch(error.to_string()))
        })
    }
}
