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
}
