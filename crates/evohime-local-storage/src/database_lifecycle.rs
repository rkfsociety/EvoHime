//! Database opening, migration, schema installation and restore-safe setup.

use std::{fs, path::Path};

use rusqlite::Connection;

use crate::{
    agent_git_change_sets_store, analysis_kernel, approval_policy_profiles_store,
    architect_editor_model_pipeline_store, architecture_snapshot_store, automation_store,
    benchmark_store, browser_session_store, checkpoint_forking_store, context_ledger_store,
    conversation_bridge_adapters_store, conversation_event_log_store,
    customization_inventory_store, durable_background_execution_store,
    durable_remote_task_bridge_store, event_trigger_runtime_store, event_visualizer_registry_store,
    execution_environment_profiles_store, execution_ledger, goal, human_work_items_store,
    incremental_change_protocol_store, integration_provider_store, invocation_presets_store,
    knowledge_source_registry_project_role_store, local_model_runtime_manager_store,
    memory_extraction_store, memory_store, memory_views_and_adaptive_recall_store, migrations,
    model_edit_protocol_registry_store, model_provenance, model_purpose_routing_store,
    output_guardrail_pipeline_store, persistent_agent_registry_store,
    policy_aware_tool_result_cache_store, privacy_telemetry_store, prompt_cache_planner_store,
    reasoning_operator_library_store, refinement_store, remote_conversation_channels_store,
    retained_child_store, skill_trust_pipeline_store, standing_approval_profiles_store,
    task_checkpoint, team_coordination_policies_store, team_sop_protocols_store, toolkit_store,
    visual_workflow_builder_store, workflow_package_store, workflow_store,
    workspace_state_checkpoint, LocalDatabase, StorageError, SCHEMA_VERSION,
};

impl LocalDatabase {
    /// Opens a database and applies any pending schema migrations.
    ///
    /// This is the standard startup entry point. Existing databases are backed
    /// up before migration; use [`Self::open_prepared`] only when startup has
    /// already completed migration and schema installation.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the path cannot be opened, the stored schema
    /// is newer than this build, or migration fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::open_with_migrations(path)
    }

    /// Opens the application database and applies pending migrations.
    ///
    /// This is the startup path. Callers that only need a connection to an
    /// already prepared database should use [`Self::open_prepared`] instead.
    pub fn open_with_migrations(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::open_internal(path.as_ref(), false)
    }

    /// Opens a database whose schema was prepared by the startup migration
    /// path. No migrations or idempotent schema installers are run here.
    pub fn open_prepared(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if !path.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("prepared database does not exist: {}", path.display()),
            )
            .into());
        }
        let connection = Connection::open(&path)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let version = Self::read_schema_version(&connection)?;
        if version != SCHEMA_VERSION {
            return Err(StorageError::InvalidInput(format!(
                "prepared database schema mismatch: expected {}, got {version}",
                SCHEMA_VERSION
            )));
        }
        Ok(Self { path, connection })
    }

    pub(crate) fn open_internal(path: &Path, fail_migration: bool) -> Result<Self, StorageError> {
        let path = path.to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let existed = path.exists();
        let connection = Connection::open(&path)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let version = Self::read_schema_version(&connection)?;
        if version > SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema(version));
        }
        if version < SCHEMA_VERSION {
            if existed {
                fs::copy(&path, path.with_extension("db.bak"))?;
            }
            if let Err(error) = migrations::run(&connection, version, fail_migration) {
                drop(connection);
                if existed {
                    fs::copy(path.with_extension("db.bak"), &path)?;
                }
                return Err(error);
            }
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        evohime_receipts::runtime::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        model_provenance::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        workflow_store::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        automation_store::install_schema(&connection)?;
        durable_background_execution_store::install_schema(&connection)?;
        toolkit_store::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        execution_ledger::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        memory_store::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        memory_extraction_store::install_schema(&connection)?;
        context_ledger_store::install_compaction_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        task_checkpoint::install_schema(&connection)?;
        workspace_state_checkpoint::install_schema(&connection)?;
        goal::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        retained_child_store::install_schema(&connection)?;
        analysis_kernel::install_schema(&connection)?;
        refinement_store::install_schema(&connection)?;
        workflow_package_store::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        visual_workflow_builder_store::install_schema(&connection)?;
        integration_provider_store::install_schema(&connection)?;
        event_trigger_runtime_store::install_schema(&connection)?;
        invocation_presets_store::install_schema(&connection)?;
        benchmark_store::install_schema(&connection)?;
        skill_trust_pipeline_store::install_schema(&connection)?;
        team_sop_protocols_store::install_schema(&connection)?;
        team_coordination_policies_store::install_schema(&connection)?;
        conversation_event_log_store::install_schema(&connection)?;
        human_work_items_store::install_schema(&connection)?;
        browser_session_store::install_schema(&connection)?;
        incremental_change_protocol_store::install_schema(&connection)?;
        agent_git_change_sets_store::install_schema(&connection)?;
        architect_editor_model_pipeline_store::install_schema(&connection)?;
        event_visualizer_registry_store::install_schema(&connection)?;
        reasoning_operator_library_store::install_schema(&connection)?;
        output_guardrail_pipeline_store::install_schema(&connection)?;
        persistent_agent_registry_store::install_schema(&connection)?;
        execution_environment_profiles_store::install_schema(&connection)?;
        customization_inventory_store::install_schema(&connection)?;
        standing_approval_profiles_store::install_schema(&connection)?;
        approval_policy_profiles_store::install_schema(&connection)?;
        checkpoint_forking_store::install_schema(&connection)?;
        privacy_telemetry_store::install_schema(&connection)?;
        conversation_bridge_adapters_store::install_schema(&connection)?;
        knowledge_source_registry_project_role_store::install_schema(&connection)?;
        durable_remote_task_bridge_store::install_schema(&connection)?;
        memory_views_and_adaptive_recall_store::install_schema(&connection)?;
        model_edit_protocol_registry_store::install_schema(&connection)?;
        remote_conversation_channels_store::install_schema(&connection)?;
        prompt_cache_planner_store::install_schema(&connection)?;
        policy_aware_tool_result_cache_store::install_schema(&connection)?;
        model_purpose_routing_store::install_schema(&connection)?;
        local_model_runtime_manager_store::install_schema(&connection)?;
        architecture_snapshot_store::install_schema(&connection)?;
        let typed_event_columns: i64 = connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('events')
             WHERE name IN ('action_id', 'state_after')",
            [],
            |row| row.get(0),
        )?;
        if typed_event_columns == 2 {
            connection.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_events_action_terminal
                     ON events(action_id, sequence_id DESC)
                     WHERE action_id IS NOT NULL AND state_after IS NOT NULL;
                 CREATE INDEX IF NOT EXISTS idx_events_review_lookup
                     ON events(task_id, event_type, sequence_id DESC);",
            )?;
        }
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { path, connection })
    }
}
