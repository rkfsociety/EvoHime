#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::Connection;

pub mod agent_git_change_sets_store;
pub(crate) mod agent_middleware_pipeline_store;
pub mod agent_program_optimizer_store;
pub mod agent_role_profiles_store;
pub mod ambient_store;
pub mod analysis_kernel;
pub mod approval_policy_profiles_store;
pub mod architect_editor_model_pipeline_store;
pub mod architecture_snapshot_store;
pub mod artifact_handoff_registry_store;
pub(crate) mod artifact_store;
pub mod authorized_security_assessment_store;
pub mod automation_store;
pub mod autonomous_metric_experiment_runtime_store;
pub(crate) mod backup;
pub mod batch_invocation_runtime_store;
pub(crate) mod benchmark_store;
pub mod browser_session_store;
pub mod capability_selection_store;
pub mod capability_store;
pub mod capability_workbenches_store;
pub(crate) mod checkpoint_forking_store;
pub(crate) mod child_store;
pub mod code_diagnostics_feedback_loop_store;
pub mod code_review_lane_store;
pub mod collaboration_store;
pub mod command_center_store;
pub mod composable_termination_conditions_store;
pub mod context_command_store;
pub mod context_ledger_store;
pub mod context_loadout_store;
pub mod context_namespace_store;
pub mod contextual_next_step_suggestions_store;
pub(crate) mod continuation_store;
pub mod conversation_bridge_adapters_store;
pub(crate) mod conversation_event_log_store;
pub mod core_topic_subscription_event_bus_store;
pub mod cross_modal_ui_grounding_store;
pub mod customization_inventory_store;
mod database_access;
pub mod declarative_agent_component_registry_store;
pub mod declarative_runtime_components_store;
pub mod dependency_aware_task_graph_store;
pub mod deterministic_review_execution_plan_store;
mod diagnostics;
pub mod domain_workflow_recipes_store;
pub mod domains;
pub mod durable_background_execution_store;
pub mod durable_remote_task_bridge_store;
mod event_export;
mod event_store;
pub(crate) mod event_trigger_runtime_store;
pub mod event_visualizer_registry_store;
pub mod execution_backend_registry_store;
pub mod execution_environment_profiles_store;
pub mod execution_ledger;
pub(crate) mod execution_policy_profiles_store;
pub mod experience_replay_library_store;
pub mod external_coding_agent_adapter_store;
pub mod external_source_acquisition_runtime_store;
pub mod feedback_store;
pub mod free_access_evidence_store;
pub mod git_remote_publication_protocol_store;
pub mod goal;
pub mod grounded_research_store;
pub mod guided_calibration_sessions_store;
pub mod hardware_fit_evidence_store;
pub mod human_work_items_store;
pub mod ide_companion_bridge_store;
pub mod incremental_change_protocol_store;
pub(crate) mod integration_provider_store;
pub mod interactive_model_compare_workbench_store;
pub mod invocation_presets_store;
pub mod kernel_capability_facade_store;
pub mod knowledge_source_registry_project_role_store;
pub mod language_intelligence_store;
mod ledger_helpers;
mod ledger_reconciliation_store;
mod ledger_store;
mod legacy_migration;
pub mod local_model_compatibility_gateway_store;
pub mod local_model_performance_calibration_store;
pub mod local_model_runtime_manager_store;
pub mod memory_extraction_store;
pub(crate) mod memory_store;
pub(crate) mod memory_views_and_adaptive_recall_store;
mod migrations;
pub mod minimal_change_policy_store;
pub mod mobile_device_automation_runtime_store;
pub mod model_edit_protocol_registry_store;
pub mod model_limit_store;
pub(crate) mod model_provenance;
pub mod model_purpose_routing_store;
pub mod multi_reviewer_ensemble_store;
pub mod native_computer_use_runtime_store;
pub mod offline_experience_consolidation_store;
pub mod optional_voice_output_adapter_store;
pub(crate) mod output_guardrail_pipeline_store;
pub(crate) mod persistent_agent_registry_store;
pub mod plan_artifact;
pub mod policy_aware_tool_result_cache_store;
pub mod privacy_telemetry_store;
pub mod project_execution_board_store;
pub mod project_instruction_stack_store;
pub mod project_knowledge_notebook_store;
mod project_store;
pub(crate) mod prompt_cache_planner_store;
mod provenance_store;
pub mod provider_profile_catalog_store;
pub mod reasoning_operator_library_store;
pub(crate) mod reconciliation_verifier;
mod records;
pub mod refinement_store;
pub mod remote_conversation_channels_store;
pub mod research_store;
pub(crate) mod retained_child_store;
mod run_checkpoint_store;
mod run_effect_store;
mod run_lease_store;
mod run_reconciliation_store;
mod run_recovery_store;
mod run_recovery_sweep_store;
mod run_store;
mod runtime_records;
pub mod runtime_service_graph_store;
pub mod safe_ui_extension_framework_store;
pub mod schema_driven_agent_configuration_store;
pub mod scratchpad_store;
pub mod semantic_activity_motion_system_store;
pub mod skill_source_lifecycle_store;
pub(crate) mod skill_trust_pipeline_store;
mod snapshot_store;
pub mod standing_approval_profiles_store;
pub mod static_analysis_pack_store;
mod storage_error;
pub(crate) mod task_checkpoint;
pub(crate) mod task_worktree_isolation_store;
pub mod team_coordination_policies_store;
pub mod team_coordinator_store;
pub mod team_resource_budget_store;
pub mod team_sop_protocols_store;
pub mod temporal_memory_facts_store;
pub mod temporal_signal_intelligence_store;
pub mod toolkit_store;
pub mod typed_agent_handoff_contract_store;
pub(crate) mod typed_context_references_store;
pub mod verification_evidence_ledger_store;
pub mod verified_git_checkpoints_store;
pub mod verified_technical_diagram_artifacts_store;
pub mod visual_workflow_builder_store;
pub mod voice_input_dictation_store;
mod work_item_store;
pub mod workflow_optimization_lab_store;
pub mod workflow_package_store;
pub mod workflow_store;
pub mod workspace_bootstrap_manifest_store;
pub mod workspace_sets_store;
pub mod workspace_state_checkpoint;

pub use backup::{
    BackupObjectSummary, BackupPreview, BackupProgress, BackupProgressPhase, BackupResult,
    RestoreResult, BACKUP_FORMAT_VERSION,
};

pub const SCHEMA_VERSION: u32 = 175;

pub use records::{
    EventRecord, ImportedTask, ProjectPolicyRecord, ProjectRecord, ProvenanceRecord,
    SnapshotRecord, ToolMetricRecord, WorkItemRecord,
};
pub use runtime_records::{
    DiagnosticsEventCount, DiagnosticsSummary, DiagnosticsTableCount, ModelRouteSnapshot,
    PolicySnapshot, RecoveredRunRecord, RecoveryHealthSnapshot, RecoveryState,
    RecoveryTransitionInput, RoleRef, RunCheckpointRecord, RunEffectRecord, RunLeaseRecord,
    RunReconciliationRecord, RunRecord, RunRecoveryRecord, RunSnapshots, SkillRef, ToolMetricInput,
    MAX_DIAGNOSTICS_EVENT_TYPES,
};
pub use storage_error::StorageError;

pub struct LocalDatabase {
    path: PathBuf,
    connection: Connection,
}

impl LocalDatabase {
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

    fn open_internal(path: &Path, fail_migration: bool) -> Result<Self, StorageError> {
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
        // Схема 29: durable-запуски workflow. Ставится идемпотентно тем же
        // способом, что receipts и model provenance, поэтому существующая база
        // получает таблицы без отдельной ветки миграции.
        workflow_store::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        automation_store::install_schema(&connection)?;
        durable_background_execution_store::install_schema(&connection)?;
        toolkit_store::install_schema(&connection)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        // Схема 30 (план 08-2): typed execution ledger поверх events —
        // аддитивные колонки и rebuild CHECK workflow_run_nodes под
        // cancelling. Тем же идемпотентным путём, вызывается после
        // workflow_store, чья таблица здесь пересобирается.
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
        // Schema 40: Integration Provider SDK metadata. Secret material is
        // deliberately absent; the store is metadata-only.
        integration_provider_store::install_schema(&connection)?;
        event_trigger_runtime_store::install_schema(&connection)?;
        invocation_presets_store::install_schema(&connection)?;
        benchmark_store::install_schema(&connection)?;
        skill_trust_pipeline_store::install_schema(&connection)?;
        team_sop_protocols_store::install_schema(&connection)?;
        // Plan 95: durable strategy snapshots are installed idempotently so
        // databases already at the current schema receive the additive table.
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
        // Plan 109: collection metadata extends the existing Knowledge Source
        // Registry without creating a second source/chunk authority.
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
        // These indexes depend on the typed-ledger columns installed above.
        // Some legacy fixtures create the compatibility `events` table later
        // through another store, so do not reference action_id/state_after
        // until those columns are actually present.
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

#[cfg(test)]
mod tests {
    use super::{
        DiagnosticsSummary, ImportedTask, LocalDatabase, ModelRouteSnapshot, PolicySnapshot,
        RecoveryState, RecoveryTransitionInput, RoleRef, RunCheckpointRecord, RunEffectRecord,
        RunRecord, RunSnapshots, SkillRef, StorageError, ToolMetricInput, WorkItemRecord,
        SCHEMA_VERSION,
    };
    use std::path::PathBuf;

    fn temp_database_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("evohime-test-{name}-{}.db", std::process::id()))
    }

    #[test]
    fn latest_schema_installs_hot_event_indexes() {
        let path = temp_database_path("hot-event-indexes");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let count: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='index' AND name IN
                 ('idx_events_task_sequence', 'idx_events_action_terminal',
                  'idx_events_review_lookup')",
                [],
                |row| row.get(0),
            )
            .expect("index query");
        assert_eq!(count, 3);
        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn prepared_open_does_not_install_schema() {
        let path = temp_database_path("prepared-open");
        let _ = std::fs::remove_file(&path);
        let connection = rusqlite::Connection::open(&path).expect("database opens");
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .expect("schema version is writable");
        drop(connection);

        let database = LocalDatabase::open_prepared(&path).expect("prepared database opens");
        assert!(!database.has_events_table().expect("events table query"));
        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn latest_schema_installs_configuration_experience_diagnostics_task_graph_component_registry_refs_workbench_coordinator_instruction_workspace_set_and_knowledge_tables(
    ) {
        let path = temp_database_path("experience-replay-schema-66");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        assert_eq!(
            database.schema_version().expect("schema version"),
            SCHEMA_VERSION
        );
        let has_ui_extension_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='safe_ui_extensions'",
                [],
                |row| row.get(0),
            )
            .expect("UI extension table query");
        assert_eq!(has_ui_extension_table, 1);
        let has_knowledge_collections_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_collections'",
                [],
                |row| row.get(0),
            )
            .expect("knowledge collection table query");
        assert_eq!(has_knowledge_collections_table, 1);
        let has_workbench_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='capability_workbench_instances'",
                [],
                |row| row.get(0),
            )
            .expect("workbench table query");
        assert_eq!(has_workbench_table, 1);
        let has_coordinator_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='team_coordinator_work_items'",
                [],
                |row| row.get(0),
            )
            .expect("coordinator table query");
        assert_eq!(has_coordinator_table, 1);
        let has_instruction_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='project_instruction_rules'",
                [],
                |row| row.get(0),
            )
            .expect("instruction table query");
        assert_eq!(has_instruction_table, 1);
        let has_workspace_sets_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace_sets'",
                [],
                |row| row.get(0),
            )
            .expect("workspace set table query");
        assert_eq!(has_workspace_sets_table, 1);
        let has_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace_bootstrap_manifests'",
                [],
                |row| row.get(0),
            )
            .expect("bootstrap table query");
        assert_eq!(has_table, 1);
        let has_team_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='team_coordination_policies'",
                [],
                |row| row.get(0),
            )
            .expect("team policy table query");
        assert_eq!(has_team_table, 1);
        let has_handoff_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='typed_agent_handoffs'",
                [],
                |row| row.get(0),
            )
            .expect("handoff table query");
        assert_eq!(has_handoff_table, 1);
        let has_configuration_table: i64 = database.connection().query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_agent_configurations'", [], |row| row.get(0)).expect("configuration table query");
        assert_eq!(has_configuration_table, 1);
        let has_experience_table: i64 = database.connection().query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='experience_replay_records'", [], |row| row.get(0)).expect("experience table query");
        assert_eq!(has_experience_table, 1);
        let has_diagnostics_table: i64 = database.connection().query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='code_diagnostics_snapshots'", [], |row| row.get(0)).expect("diagnostics table query");
        assert_eq!(has_diagnostics_table, 1);
        let has_bus_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='core_bus_events'",
                [],
                |row| row.get(0),
            )
            .expect("bus table query");
        assert_eq!(has_bus_table, 1);
        drop(database);
        let _ = std::fs::remove_file(&path);
    }

    /// Clearing the review history must hide earlier runs from the list while
    /// leaving them in the journal, which stays append-only for audit and
    /// export.
    #[test]
    fn review_history_starts_after_the_newest_clear_marker() {
        let path = temp_database_path("review-history");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .append_event("review-old", "task.completed", b"{}")
            .expect("old review records");
        assert_eq!(
            database
                .read_review_events(10)
                .expect("history reads")
                .len(),
            1
        );

        database
            .append_event("review-history-1", "review.history_cleared", b"{}")
            .expect("marker records");
        assert!(database
            .read_review_events(10)
            .expect("history reads")
            .is_empty());

        database
            .append_event("review-new", "task.completed", b"{}")
            .expect("new review records");
        let visible = database.read_review_events(10).expect("history reads");
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].task_id, "review-new");
        // The cleared review is hidden from the list, not removed.
        let all = database
            .read_events_after(0, usize::MAX)
            .expect("journal reads");
        assert!(all.iter().any(|event| event.task_id == "review-old"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn creates_schema_and_reports_version() {
        let path = temp_database_path("schema");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        assert!(database.has_events_table().expect("table exists"));
        for table in [
            "persistent_agents",
            "persistent_agent_revisions",
            "persistent_agent_reporting_history",
            "persistent_agent_goal_bindings",
            "persistent_agent_assignments",
            "persistent_agent_commands",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("persistent agent table lookup");
            assert_eq!(exists, 1, "{table} must exist in a fresh schema");
        }
        for table in [
            "continuation_policies",
            "continuation_runs",
            "continuation_attempts",
            "continuation_actions",
            "continuation_gate_results",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("continuation table lookup");
            assert_eq!(exists, 1, "{table} must exist in a fresh schema");
        }
        let id = database
            .record_tool_metric(ToolMetricInput {
                task_id: "task-1",
                tool_name: "filesystem.read",
                iteration: 2,
                ok: false,
                failure_kind: Some("not_found"),
                recovery_hint: true,
                escalated: false,
            })
            .expect("metric records");
        assert!(id > 0);
        let metrics = database
            .read_tool_metrics("task-1", 10)
            .expect("metrics read");
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].failure_kind.as_deref(), Some("not_found"));
        assert!(metrics[0].recovery_hint);
        drop(database);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn schema_90_migrates_guided_calibration_and_persistent_agent_registry_atomically() {
        let path = temp_database_path("persistent-agent-schema-90");
        let backup = path.with_extension("db.bak");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy db opens");
            connection
                .pragma_update(None, "user_version", 90_u32)
                .expect("legacy version writes");
            connection
                .execute_batch("CREATE TABLE migration_marker(value TEXT NOT NULL);")
                .expect("legacy marker writes");
        }
        let database = LocalDatabase::open(&path).expect("legacy db migrates");
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        for table in ["guided_calibration_sessions", "persistent_agents"] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} must be installed by 90->92");
        }
        assert!(backup.exists(), "migration backup must be retained");
        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);
    }

    #[test]
    fn migrates_schema_32_to_goals_schema_33_with_backup() {
        let path = temp_database_path("migration-32-to-33-goals");
        let _ = std::fs::remove_file(&path);
        let backup_path = path.with_extension("db.bak");
        let _ = std::fs::remove_file(&backup_path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE legacy_marker(id INTEGER);
                     INSERT INTO legacy_marker(id) VALUES (25);
                     PRAGMA user_version = 32;",
                )
                .expect("legacy schema seeds");
        }

        let database = LocalDatabase::open(&path).expect("migration succeeds");
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(backup_path.exists(), "pre-migration backup must be written");
        for table in ["goals", "goal_revisions", "goal_events", "goal_commands"] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} must exist after migration to schema 33");
        }
        let preserved: i64 = database
            .connection()
            .query_row("SELECT id FROM legacy_marker", [], |row| row.get(0))
            .expect("legacy row survives");
        assert_eq!(preserved, 25);

        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);
    }

    #[test]
    fn migrates_schema_37_to_analysis_kernel_schema_38_with_backup() {
        let path = temp_database_path("migration-37-to-38-analysis-kernel");
        let _ = std::fs::remove_file(&path);
        let backup_path = path.with_extension("db.bak");
        let _ = std::fs::remove_file(&backup_path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE legacy_marker(id INTEGER);
                     INSERT INTO legacy_marker(id) VALUES (28);
                     PRAGMA user_version = 37;",
                )
                .expect("legacy schema seeds");
        }
        let database = LocalDatabase::open(&path).expect("migration succeeds");
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(backup_path.exists(), "pre-migration backup must be written");
        for table in [
            "analysis_kernel_sessions",
            "analysis_kernel_objects",
            "analysis_kernel_events",
            "analysis_kernel_idempotency",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} must exist after migration to schema 38");
        }
        let preserved: i64 = database
            .connection()
            .query_row("SELECT id FROM legacy_marker", [], |row| row.get(0))
            .expect("legacy row survives");
        assert_eq!(preserved, 28);
        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);
    }

    #[test]
    fn migrates_schema_18_to_workspace_rag_schema_19_transactionally() {
        let path = temp_database_path("migration-18-to-19-rag");
        let _ = std::fs::remove_file(&path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch("CREATE TABLE legacy_marker(id INTEGER); PRAGMA user_version = 18;")
                .expect("legacy schema seeds");
        }
        let database = LocalDatabase::open(&path).expect("migration succeeds");
        // Открытие всегда доводит базу до текущей версии, поэтому проверяется
        // не «19», а наличие таблиц, которые добавила именно эта миграция.
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        for table in [
            "workspace_index_runs",
            "workspace_documents",
            "document_chunks",
            "workspace_vector_indexes",
            "workspace_chunk_vectors",
            "rag_context_ledger",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} must exist after migration");
        }
        let _ = std::fs::remove_file(path);
    }

    /// Лимиты моделей должны появиться на уже работающей базе: пользователь
    /// обновляет сборку, а не заводит хранилище заново.
    #[test]
    fn migrates_schema_19_to_model_context_limits_schema_20() {
        let path = temp_database_path("migration-19-to-20-limits");
        let _ = std::fs::remove_file(&path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch("CREATE TABLE legacy_marker(id INTEGER); PRAGMA user_version = 19;")
                .expect("legacy schema seeds");
        }
        let database = LocalDatabase::open(&path).expect("migration succeeds");
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'model_context_limits'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "model_context_limits must exist after migration");
        let _ = std::fs::remove_file(path);
    }

    /// Stage 01.4: a pre-22 database gaining `receipt_records` et al. must go
    /// through the same backup-before-migrate path as any other schema
    /// change, not the unconditional idempotent call `open_internal` also
    /// makes after `migrate` returns.
    #[test]
    fn migrates_schema_21_to_receipts_v1_schema_22_with_backup() {
        let path = temp_database_path("migration-21-to-22-receipts");
        let _ = std::fs::remove_file(&path);
        let backup_path = path.with_extension("db.bak");
        let _ = std::fs::remove_file(&backup_path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch("CREATE TABLE legacy_marker(id INTEGER); PRAGMA user_version = 21;")
                .expect("legacy schema seeds");
        }
        let database = LocalDatabase::open(&path).expect("migration succeeds");
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(
            backup_path.exists(),
            "pre-migration backup must be written before schema 22 applies"
        );
        for table in [
            "receipt_records",
            "receipt_actions",
            "receipt_chain_heads",
            "receipt_checkpoints",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} must exist after migration to schema 22");
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);
    }

    /// Этап 04.2: ambient-таблицы приезжают миграцией v25 поверх живой базы,
    /// не трогая уже существующие строки.
    #[test]
    fn migrates_schema_24_to_ambient_schema_25_without_touching_existing_rows() {
        let path = temp_database_path("migration-24-to-25-ambient");
        let _ = std::fs::remove_file(&path);
        let backup_path = path.with_extension("db.bak");
        let _ = std::fs::remove_file(&backup_path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE legacy_marker(id INTEGER);
                     INSERT INTO legacy_marker(id) VALUES (7);
                     PRAGMA user_version = 24;",
                )
                .expect("legacy schema seeds");
        }
        let database = LocalDatabase::open(&path).expect("migration succeeds");
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(
            backup_path.exists(),
            "pre-migration backup must be written before schema 25 applies"
        );
        let preserved: i64 = database
            .connection()
            .query_row("SELECT id FROM legacy_marker", [], |row| row.get(0))
            .expect("legacy row survives");
        assert_eq!(preserved, 7);
        for table in [
            "ambient_episodes",
            "ambient_utterances",
            "ambient_tombstones",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} must exist after migration to schema 25");
        }
        // Повторное открытие не пытается мигрировать второй раз.
        drop(database);
        let reopened = LocalDatabase::open(&path).expect("second open is a no-op");
        assert_eq!(reopened.schema_version().unwrap(), SCHEMA_VERSION);
        drop(reopened);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);
    }

    /// Этап 04.7: таблицы предложений приезжают миграцией v26 поверх живого
    /// ambient-хранилища v25 и не трогают его строк.
    #[test]
    fn migrates_schema_25_to_proactivity_schema_26_without_touching_ambient_rows() {
        let path = temp_database_path("migration-25-to-26-proactivity");
        let _ = std::fs::remove_file(&path);
        let backup_path = path.with_extension("db.bak");
        let _ = std::fs::remove_file(&backup_path);
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE ambient_episodes (
                        episode_id TEXT PRIMARY KEY NOT NULL,
                        started_at TEXT NOT NULL,
                        ended_at TEXT,
                        utterance_count INTEGER NOT NULL,
                        speech_ms INTEGER NOT NULL,
                        engine_version TEXT NOT NULL,
                        model_id TEXT NOT NULL,
                        extraction_state TEXT NOT NULL,
                        expires_at TEXT NOT NULL
                     );
                     INSERT INTO ambient_episodes VALUES
                        ('ep-1','2026-08-21T10:00:00.000Z',NULL,1,900,'whisper','base',
                         'done','2026-09-20T10:00:00.000Z');
                     PRAGMA user_version = 25;",
                )
                .expect("v25 schema seeds");
        }
        let database = LocalDatabase::open(&path).expect("migration succeeds");
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
        assert!(
            backup_path.exists(),
            "pre-migration backup must be written before schema 26 applies"
        );
        let preserved: String = database
            .connection()
            .query_row("SELECT episode_id FROM ambient_episodes", [], |row| {
                row.get(0)
            })
            .expect("ambient row survives");
        assert_eq!(preserved, "ep-1");
        for table in [
            "ambient_proposals",
            "ambient_proposal_mutes",
            "ambient_proactivity_counters",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "{table} must exist after migration to schema 26");
        }
        drop(database);
        let reopened = LocalDatabase::open(&path).expect("second open is a no-op");
        assert_eq!(reopened.schema_version().unwrap(), SCHEMA_VERSION);
        drop(reopened);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);
    }

    /// Схема физически не может хранить аудио: ни одна ambient-колонка не
    /// объявлена как BLOB, поэтому «PCM не пишется на диск» — свойство
    /// таблицы, а не дисциплины вызывающего кода.
    #[test]
    fn ambient_tables_have_no_blob_column() {
        let path = temp_database_path("ambient-no-blob");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        let database = LocalDatabase::open(&path).expect("database opens");
        for table in [
            "ambient_episodes",
            "ambient_utterances",
            "ambient_tombstones",
            "ambient_proposals",
            "ambient_proposal_mutes",
            "ambient_proactivity_counters",
        ] {
            let mut statement = database
                .connection()
                .prepare(&format!("PRAGMA table_info({table})"))
                .expect("pragma prepares");
            let types: Vec<String> = statement
                .query_map([], |row| row.get::<_, String>(2))
                .expect("pragma runs")
                .map(|row| row.expect("column type").to_ascii_uppercase())
                .collect();
            assert!(!types.is_empty(), "{table} must exist");
            assert!(
                !types.iter().any(|kind| kind.contains("BLOB")),
                "{table} must not be able to hold audio: {types:?}"
            );
        }
        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn backs_up_existing_database_before_migration() {
        let path = temp_database_path("backup");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .pragma_update(None, "user_version", 0_u32)
                .expect("legacy version writes");
        }
        let _database = LocalDatabase::open(&path).expect("database migrates");
        assert!(path.with_extension("db.bak").exists());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn migration_12_to_16_is_idempotent_and_preserves_existing_rows() {
        let path = temp_database_path("feedback-migration");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            // Seed a pre-wave (user_version 12) database with an existing
            // memory_entries row, so we can confirm migrations 13 through 16 do not
            // touch unrelated data.
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL,
                        confirmations INTEGER NOT NULL DEFAULT 1,
                        lesson_key TEXT
                    );
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten)
                        VALUES ('m-1', 'project', 'p-1', 'Decision', 'keep this', 'run:1',
                                'internal', '2026-08-01T00:00:00Z', NULL, 0, 0);
                    PRAGMA user_version = 12;",
                )
                .expect("legacy schema and data write");
        }

        let database = LocalDatabase::open(&path).expect("database migrates forward");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        let feedback_table_exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='feedback_entries'",
                [],
                |row| row.get(0),
            )
            .expect("feedback table check");
        assert_eq!(feedback_table_exists, 1);
        let preserved: String = database
            .connection()
            .query_row(
                "SELECT content FROM memory_entries WHERE id = 'm-1'",
                [],
                |row| row.get(0),
            )
            .expect("existing memory row survives migration");
        assert_eq!(preserved, "keep this");
        drop(database);

        // Re-opening an already-migrated database must not error and must
        // not duplicate the feedback_entries table or existing rows
        // (guarded CREATE TABLE IF NOT EXISTS / PRAGMA user_version checks).
        let reopened = LocalDatabase::open(&path).expect("reopen is idempotent");
        assert_eq!(
            reopened.schema_version().expect("version stays current"),
            SCHEMA_VERSION
        );
        let row_count: i64 = reopened
            .connection()
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .expect("row count reads");
        assert_eq!(row_count, 1);
        drop(reopened);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn migration_16_maps_memory_v1_rows_onto_the_extraction_contract() {
        // Memory v1 -> Memory Extraction: явные failure lessons получают
        // kind=lesson, прочие старые факты -- kind=entity; все legacy rows
        // остаются активной памятью с legacy-версиями extractor/policy и
        // пустой цепочкой supersede.
        let path = temp_database_path("memory-extraction-migration");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL,
                        confirmations INTEGER NOT NULL DEFAULT 1,
                        lesson_key TEXT
                    );
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten, confirmations, lesson_key)
                        VALUES
                        ('fact-1', 'project', 'p-1', 'Решение', 'сборка через cargo', 'run:1',
                         'internal', '2026-08-01T00:00:00Z', NULL, 0, 0, 1, NULL),
                        ('lesson-1', 'project', 'p-1', 'Урок', 'проверяй аргументы', 'task:t-1',
                         'private', '2026-08-02T00:00:00Z', NULL, 0, 0, 3, 'lesson-key-1'),
                        ('gone-1', 'project', 'p-1', '', '', '', 'internal',
                         '2026-08-03T00:00:00Z', NULL, 0, 1, 1, NULL);
                    PRAGMA user_version = 12;",
                )
                .expect("legacy schema and data write");
        }

        let database = LocalDatabase::open(&path).expect("database migrates forward");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        // Транзакционность миграции подтверждается наличием backup рядом.
        assert!(path.with_extension("db.bak").exists());

        let mapped = |id: &str| -> (String, String, String, String, f64, f64) {
            database
                .connection()
                .query_row(
                    "SELECT kind, confirmation_state, extractor_version, policy_version,
                            model_confidence, verification_confidence
                     FROM memory_entries WHERE id = ?1",
                    rusqlite::params![id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .expect("mapped row reads")
        };
        assert_eq!(
            mapped("fact-1"),
            (
                "entity".to_owned(),
                "confirmed".to_owned(),
                "v1_legacy".to_owned(),
                "legacy-v1".to_owned(),
                1.0,
                1.0
            )
        );
        assert_eq!(mapped("lesson-1").0, "lesson");
        assert_eq!(mapped("lesson-1").1, "confirmed");
        // Уже забытая запись не воскресает в состоянии confirmed.
        assert_eq!(mapped("gone-1").1, "forgotten");

        let (supersedes, superseded_by): (Option<String>, Option<String>) = database
            .connection()
            .query_row(
                "SELECT supersedes, superseded_by FROM memory_entries WHERE id = 'fact-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("supersede columns read");
        assert_eq!(supersedes, None);
        assert_eq!(superseded_by, None);

        // Исходные statement и provenance сохранены дословно.
        let (content, provenance): (String, String) = database
            .connection()
            .query_row(
                "SELECT content, provenance FROM memory_entries WHERE id = 'fact-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("statement survives");
        assert_eq!(content, "сборка через cargo");
        assert_eq!(provenance, "run:1");

        // canonical_subject остаётся NULL: нормализатор версионируется в Core.
        let subject: Option<String> = database
            .connection()
            .query_row(
                "SELECT canonical_subject FROM memory_entries WHERE id = 'fact-1'",
                [],
                |row| row.get(0),
            )
            .expect("subject reads");
        assert_eq!(subject, None);

        for table in [
            "memory_aliases",
            "memory_tombstones",
            "memory_session_notes",
        ] {
            let exists: i64 = database
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![table],
                    |row| row.get(0),
                )
                .expect("table check");
            assert_eq!(exists, 1, "{table} must exist after migration 16");
        }
        drop(database);

        // Повторное открытие не дублирует колонки и не меняет данные.
        let reopened = LocalDatabase::open(&path).expect("reopen is idempotent");
        let rows: i64 = reopened
            .connection()
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .expect("row count reads");
        assert_eq!(rows, 3);
        drop(reopened);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn restores_backup_when_migration_fails() {
        let path = temp_database_path("rollback");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch("CREATE TABLE marker(value TEXT NOT NULL); INSERT INTO marker VALUES ('legacy');")
                .expect("legacy data writes");
        }
        assert!(LocalDatabase::open_internal(&path, true).is_err());
        let database = LocalDatabase::open(&path).expect("database restores and migrates");
        let marker: String = database
            .connection
            .query_row("SELECT value FROM marker", [], |row| row.get(0))
            .expect("legacy marker survives rollback");
        assert_eq!(marker, "legacy");
        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn appends_and_replays_events_by_sequence() {
        let path = temp_database_path("events");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let first = database
            .append_event("task-1", "task.started", b"one")
            .expect("first event");
        let second = database
            .append_event("task-1", "task.completed", b"two")
            .expect("second event");
        let events = database.read_events_after(first, 10).expect("events read");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence_id, second);
        assert_eq!(events[0].payload, b"two");
        let task_events = database
            .read_task_events("task-1", 10)
            .expect("task events read");
        assert_eq!(task_events.len(), 2);
        assert_eq!(task_events[0].sequence_id, first);
        assert_eq!(task_events[1].sequence_id, second);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn exports_events_as_jsonl() {
        let path = temp_database_path("export");
        let output = path.with_extension("jsonl");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&output);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .append_event("task-export", "task.started", br#"{"ok":true}"#)
            .expect("event writes");
        database
            .export_events_jsonl(&output)
            .expect("export writes");
        let content = std::fs::read_to_string(&output).expect("export reads");
        let record: serde_json::Value = serde_json::from_str(content.trim()).expect("valid JSON");
        assert_eq!(record["task_id"], "task-export");
        assert_eq!(record["payload"]["ok"], true);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn diagnostics_summary_is_bounded_read_only_and_counts_tables_and_events() {
        let path = temp_database_path("diagnostics-summary");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("diagnostics-project", "Diagnostics", "C:\\workspace", None)
            .expect("project creates");
        database
            .append_event("task-1", "task.started", b"one")
            .expect("first event writes");
        database
            .append_event("task-1", "task.started", b"two")
            .expect("second event writes");
        database
            .append_event("task-1", "task.completed", b"three")
            .expect("third event writes");

        let before_version = database.schema_version().expect("schema version reads");
        let summary: DiagnosticsSummary = database
            .read_diagnostics_summary(1)
            .expect("diagnostics summary reads");

        assert_eq!(summary.total_events, 3);
        assert_eq!(summary.event_counts.len(), 1);
        assert_eq!(summary.event_counts[0].event_type, "task.started");
        assert_eq!(summary.event_counts[0].rows, 2);
        assert!(summary.event_types_truncated);
        assert_eq!(summary.table_counts.len(), 24);
        assert_eq!(
            summary
                .table_counts
                .iter()
                .find(|count| count.table == "projects")
                .expect("projects count exists")
                .rows,
            1
        );
        assert_eq!(
            summary
                .table_counts
                .iter()
                .find(|count| count.table == "events")
                .expect("events count exists")
                .rows,
            3
        );
        assert_eq!(
            database.schema_version().expect("schema version reads"),
            before_version
        );
        assert_eq!(
            database
                .read_events_after(0, 10)
                .expect("events remain readable")
                .len(),
            3
        );

        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn creates_and_updates_task_with_optimistic_version() {
        let path = temp_database_path("tasks");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-1", "Demo", "C:\\Projects\\demo", None)
            .expect("project creates");
        let item = WorkItemRecord {
            id: "work-1".into(),
            project_id: "project-1".into(),
            parent_id: None,
            title: "First task".into(),
            description: "desc".into(),
            source_ref: Some("prd:1".into()),
            acceptance_criteria: "tests pass".into(),
            non_goals: "no UI".into(),
            status: "backlog".into(),
            priority: 10,
            estimate: Some(2),
            complexity: Some("small".into()),
            attempt_count: 0,
            version: 1,
        };
        let created = database.create_work_item(&item).expect("task creates");
        let updated = database
            .update_work_item_status(&created.id, 1, "ready")
            .expect("task updates");
        assert_eq!(updated.status, "ready");
        assert_eq!(updated.version, 2);
        assert!(matches!(
            database.update_work_item_status(&created.id, 1, "done"),
            Err(StorageError::VersionConflict { .. })
        ));
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn lists_graph_rejects_cycles_and_selects_next_ready_deterministically() {
        let path = temp_database_path("task-graph");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-graph", "Graph", "C:\\Projects\\graph", None)
            .expect("project creates");
        for (id, title, status, priority) in [
            ("task-a", "A", "ready", 1),
            ("task-b", "B", "ready", 10),
            ("task-c", "C", "done", 100),
        ] {
            database
                .create_work_item(&WorkItemRecord {
                    id: id.into(),
                    project_id: "project-graph".into(),
                    parent_id: None,
                    title: title.into(),
                    description: String::new(),
                    source_ref: None,
                    acceptance_criteria: String::new(),
                    non_goals: String::new(),
                    status: status.into(),
                    priority,
                    estimate: None,
                    complexity: None,
                    attempt_count: 0,
                    version: 1,
                })
                .expect("task creates");
        }
        database
            .add_dependency("task-a", "task-c", "blocks")
            .expect("dependency creates");
        assert!(matches!(
            database.add_dependency("task-c", "task-a", "blocks"),
            Err(StorageError::DependencyCycle { .. })
        ));
        assert_eq!(database.list_work_items("project-graph").unwrap().len(), 3);
        assert_eq!(
            database.list_dependencies("project-graph").unwrap().len(),
            1
        );
        assert_eq!(
            database.next_ready("project-graph").unwrap().unwrap().id,
            "task-b"
        );
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn imports_prd_atomically_and_preserves_provenance() {
        let path = temp_database_path("prd-import");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-prd", "PRD", "C:\\Projects\\prd", None)
            .expect("project creates");
        let source = "# Plan\n\n## Imported\nDescription\n- [ ] Verify\n";
        let tasks = [ImportedTask {
            id: "import-task-1".into(),
            title: "Imported".into(),
            description: "Description".into(),
            source_ref: "prd.md#L3".into(),
            acceptance_criteria: "Verify".into(),
        }];
        let imported = database
            .import_prd("import-1", "project-prd", "prd.md", "v7", source, &tasks)
            .expect("PRD imports");
        assert_eq!(imported[0].status, "backlog");
        let provenance = database
            .get_provenance("import-1")
            .expect("provenance reads")
            .expect("provenance exists");
        assert_eq!(provenance.kind, "prd_import");
        assert_eq!(provenance.source, "prd.md");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&provenance.payload).unwrap()["version"],
            "v7"
        );
        assert!(database
            .import_prd("import-1", "project-prd", "prd.md", "v7", source, &tasks)
            .is_err());
        assert_eq!(database.list_work_items("project-prd").unwrap().len(), 1);
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_run_linked_snapshot_payload_immutably() {
        let path = temp_database_path("snapshots");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let saved = database
            .save_snapshot("snapshot-1", "run-1", "workspace-hash", br#"{"files":[]}"#)
            .expect("snapshot saves");
        assert_eq!(saved.run_id, "run-1");
        assert_eq!(database.get_snapshot("snapshot-1").unwrap(), Some(saved));
        assert!(database
            .save_snapshot("snapshot-1", "run-2", "other", b"changed")
            .is_err());
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_project_policy_with_optimistic_versioning() {
        let path = temp_database_path("project-policy");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-policy", "Policy", ".", None)
            .expect("project creates");
        let first = database
            .upsert_project_policy("project-policy", br#"{"timeout_ms":30000}"#, None)
            .expect("policy creates");
        assert_eq!(first.version, 1);
        let second = database
            .upsert_project_policy("project-policy", br#"{"timeout_ms":15000}"#, Some(1))
            .expect("policy updates");
        assert_eq!(second.version, 2);
        assert!(matches!(
            database.upsert_project_policy("project-policy", b"{}", Some(1)),
            Err(StorageError::VersionConflict {
                entity: "project_policy",
                ..
            })
        ));
        assert_eq!(
            database
                .get_project_policy("project-policy")
                .unwrap()
                .unwrap(),
            second
        );
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn checkpoints_and_unknown_effects_recover_without_retry() {
        let path = temp_database_path("recovery");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-recovery", "Recovery", ".", None)
            .expect("project creates");
        let task = WorkItemRecord {
            id: "task-recovery".into(),
            project_id: "project-recovery".into(),
            parent_id: None,
            title: "recover me".into(),
            description: String::new(),
            source_ref: None,
            acceptance_criteria: String::new(),
            non_goals: String::new(),
            status: "in_progress".into(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        };
        database.create_work_item(&task).expect("task creates");
        let run = RunRecord {
            id: "run-recovery".into(),
            work_item_id: task.id.clone(),
            status: "running".into(),
            policy_snapshot: vec![],
            role_snapshot: vec![],
            skill_snapshot: vec![],
            model_route_snapshot: vec![],
        };
        let checkpoint = RunCheckpointRecord {
            run_id: run.id.clone(),
            checkpoint_id: "checkpoint-1".into(),
            stage: "build".into(),
            node_id: "node-1".into(),
            attempt: 1,
            input_hash: "input-hash".into(),
            state_json: br#"{"stage":"build"}"#.to_vec(),
            pending_effects_json: br#"["effect-1"]"#.to_vec(),
            committed_at: "2026-01-01T00:00:00Z".into(),
        };
        let effect = RunEffectRecord {
            effect_id: "effect-1".into(),
            run_id: run.id.clone(),
            node_id: "node-1".into(),
            kind: "bounded_build".into(),
            idempotency_key: "run-recovery:build".into(),
            immutable_intent_hash: "intent-hash".into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        database
            .prepare_run_effect(&run, &checkpoint, &effect)
            .expect("effect prepares");
        database
            .mark_effect_executing("effect-1")
            .expect("effect starts");
        drop(database);
        let database = LocalDatabase::open(&path).expect("database reopens after restart");
        let recovered = database.recover_unknown_effects().expect("recovery runs");
        assert_eq!(recovered.len(), 1);
        assert_eq!(
            database.get_run("run-recovery").unwrap().unwrap().status,
            "blocked"
        );
        assert_eq!(
            database
                .latest_checkpoint("run-recovery")
                .unwrap()
                .unwrap()
                .checkpoint_id,
            "checkpoint-1"
        );
        assert_eq!(
            database.read_task_events(&task.id, 10).unwrap()[0].event_type,
            "run.recovery.blocked"
        );
        assert!(
            database.recover_unknown_effects().unwrap().is_empty(),
            "recovery is idempotent"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn run_lease_is_single_owner_and_effect_can_be_reconciled() {
        let path = temp_database_path("leases");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-lease", "Lease", ".", None)
            .expect("project creates");
        database
            .create_work_item(&WorkItemRecord {
                id: "task-lease".into(),
                project_id: "project-lease".into(),
                parent_id: None,
                title: "lease".into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: "in_progress".into(),
                priority: 0,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
        let run = RunRecord {
            id: "run-lease".into(),
            work_item_id: "task-lease".into(),
            status: "running".into(),
            policy_snapshot: vec![],
            role_snapshot: vec![],
            skill_snapshot: vec![],
            model_route_snapshot: vec![],
        };
        let checkpoint = RunCheckpointRecord {
            run_id: run.id.clone(),
            checkpoint_id: "checkpoint-lease".into(),
            stage: "build".into(),
            node_id: "bounded-build".into(),
            attempt: 1,
            input_hash: "intent".into(),
            state_json: b"{}".to_vec(),
            pending_effects_json: br#"["effect-lease"]"#.to_vec(),
            committed_at: String::new(),
        };
        let effect = RunEffectRecord {
            effect_id: "effect-lease".into(),
            run_id: run.id.clone(),
            node_id: "bounded-build".into(),
            kind: "bounded_build".into(),
            idempotency_key: "lease-key".into(),
            immutable_intent_hash: "intent".into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        database
            .prepare_run_effect(&run, &checkpoint, &effect)
            .expect("effect prepares");
        database
            .acquire_run_lease("run-lease", "lease-1", "core-a", 1, 30)
            .expect("first owner claims");
        assert!(matches!(
            database.acquire_run_lease("run-lease", "lease-2", "core-b", 2, 30),
            Err(StorageError::InvalidRunEffect(_))
        ));
        database
            .heartbeat_run_lease("run-lease", "lease-1", "core-a", 1, 30)
            .expect("owner heartbeats");
        database
            .mark_effect_executing("effect-lease")
            .expect("effect executes");
        database
            .recover_unknown_effects()
            .expect("unknown effect recovers");
        let reconciliation = database
            .reconcile_run_effect(
                "effect-lease",
                true,
                "snapshot",
                br#"{"snapshot_id":"snapshot-1"}"#,
            )
            .expect("effect reconciles");
        assert_eq!(reconciliation.state, "reconciled_success");
        let retry = database
            .reconcile_run_effect(
                "effect-lease",
                false,
                "different-verifier",
                br#"{"changed":true}"#,
            )
            .expect("duplicate reconciliation is idempotent");
        assert_eq!(retry, reconciliation);
        assert_eq!(
            database
                .get_run_effect("effect-lease")
                .unwrap()
                .unwrap()
                .state,
            "completed_success"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_run_effect_has_an_independent_lease_and_completes() {
        let path = temp_database_path("agent-run-lease");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let effect = RunEffectRecord {
            effect_id: "agent-effect-1".into(),
            run_id: "agent-run-1".into(),
            node_id: "agent-task".into(),
            kind: "agent_task".into(),
            idempotency_key: "agent-run-1:agent-task".into(),
            immutable_intent_hash: "intent-agent".into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        database
            .prepare_agent_run_effect(&effect, "shell-task-1")
            .expect("agent effect prepares");
        database
            .acquire_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1, 30)
            .expect("agent lease claims");
        database
            .heartbeat_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1, 30)
            .expect("agent lease heartbeats");
        database
            .mark_agent_effect_executing("agent-effect-1")
            .expect("agent effect executes");
        let completed = database
            .complete_agent_run_effect("agent-effect-1", true, Some("result"))
            .expect("agent effect completes");
        assert_eq!(completed.state, "completed_success");
        database
            .release_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1)
            .expect("agent lease releases");
        assert!(database
            .get_agent_run_lease("agent-run-1")
            .unwrap()
            .is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn deduplicates_same_request_and_rejects_reused_request_id() {
        let path = temp_database_path("dedup");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        assert_eq!(
            database
                .record_deduplicated("client", "request", "hash", b"ok")
                .expect("first write"),
            None
        );
        assert_eq!(
            database
                .record_deduplicated("client", "request", "hash", b"different")
                .expect("replay"),
            Some(b"ok".to_vec())
        );
        assert!(matches!(
            database.record_deduplicated("client", "request", "other", b"bad"),
            Err(StorageError::DeduplicationConflict { .. })
        ));
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_immutable_run_snapshots() {
        let path = temp_database_path("run-snapshots");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-run", "Run project", "C:\\Projects\\run", None)
            .expect("project creates");
        database
            .create_work_item(&WorkItemRecord {
                id: "task-run".into(),
                project_id: "project-run".into(),
                parent_id: None,
                title: "Run task".into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: "ready".into(),
                priority: 0,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
        let run = RunRecord {
            id: "run-1".into(),
            work_item_id: "task-run".into(),
            status: "queued".into(),
            policy_snapshot: br#"{"max_iterations":1}"#.to_vec(),
            role_snapshot: br#"{"id":"planner","version":1}"#.to_vec(),
            skill_snapshot: br#"{"id":"native","version":1}"#.to_vec(),
            model_route_snapshot: br#"{"route":"local-first"}"#.to_vec(),
        };
        assert_eq!(database.create_run(&run).expect("run creates"), run);
        assert!(
            database.create_run(&run).is_err(),
            "run snapshot is immutable"
        );
        assert_eq!(database.get_run("run-1").expect("run reads"), Some(run));
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn round_trips_typed_snapshot_contracts() {
        let path = temp_database_path("typed-snapshots");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-typed", "Typed", "C:\\Projects\\typed", None)
            .expect("project creates");
        database
            .create_work_item(&WorkItemRecord {
                id: "task-typed".into(),
                project_id: "project-typed".into(),
                parent_id: None,
                title: "Typed task".into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: "ready".into(),
                priority: 0,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
        let snapshots = RunSnapshots {
            role_ref: RoleRef {
                id: "planner".into(),
                version: "1".into(),
                hash: "role-hash".into(),
            },
            skill_ref: SkillRef {
                id: "native".into(),
                version: "2".into(),
                hash: "skill-hash".into(),
            },
            policy: PolicySnapshot {
                schema_version: 1,
                policy_version: 3,
                effective_permissions_hash: "permissions-hash".into(),
                canonical_json: br#"{"tools":["filesystem.read"]}"#.to_vec(),
            },
            model_route: ModelRouteSnapshot {
                requested_route: "local-first".into(),
                resolved_provider: "mock".into(),
                resolved_model: "test-model".into(),
                route_policy_version: 1,
                canonical_json: br#"{"route":"local-first"}"#.to_vec(),
            },
        };
        database
            .create_run_with_snapshots("run-typed", "task-typed", "queued", &snapshots)
            .expect("typed run creates");
        assert_eq!(
            database
                .get_run_snapshots("run-typed")
                .expect("typed run reads"),
            Some(snapshots)
        );
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn recovery_transitions_are_durable_and_audited_without_retry() {
        let path = temp_database_path("recovery-state-machine");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .create_project("project-state", "State", ".", None)
            .expect("project creates");
        database
            .create_work_item(&WorkItemRecord {
                id: "task-state".into(),
                project_id: "project-state".into(),
                parent_id: None,
                title: "State task".into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: "ready".into(),
                priority: 0,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
        database
            .create_run(&RunRecord {
                id: "run-state".into(),
                work_item_id: "task-state".into(),
                status: "running".into(),
                policy_snapshot: Vec::new(),
                role_snapshot: Vec::new(),
                skill_snapshot: Vec::new(),
                model_route_snapshot: Vec::new(),
            })
            .expect("run creates");

        database
            .transition_recovery(RecoveryTransitionInput {
                run_id: "run-state",
                next: RecoveryState::Recovering,
                effect_id: "effect-state",
                idempotency_key: "run-state:effect-state",
                verifier: "startup",
                evidence_json: br#"{"reason":"process_restart"}"#,
                decision: "recovery_started",
            })
            .expect("recovering transition");
        database
            .transition_recovery(RecoveryTransitionInput {
                run_id: "run-state",
                next: RecoveryState::Reconciling,
                effect_id: "effect-state",
                idempotency_key: "run-state:effect-state:reconciling",
                verifier: "file_hash",
                evidence_json: br#"{"path":"src/lib.rs"}"#,
                decision: "verifier_started",
            })
            .expect("reconciling transition");
        let blocked = database
            .transition_recovery(RecoveryTransitionInput {
                run_id: "run-state",
                next: RecoveryState::Blocked,
                effect_id: "effect-state",
                idempotency_key: "run-state:effect-state:blocked",
                verifier: "file_hash",
                evidence_json: br#"{"match":false}"#,
                decision: "outcome_unconfirmed",
            })
            .expect("blocked transition");
        assert_eq!(blocked.state, RecoveryState::Blocked);
        assert_eq!(
            database.latest_recovery("run-state").expect("latest reads"),
            Some(blocked)
        );
        let repeated = database
            .transition_recovery(RecoveryTransitionInput {
                run_id: "run-state",
                next: RecoveryState::Blocked,
                effect_id: "effect-state",
                idempotency_key: "run-state:effect-state:blocked",
                verifier: "file_hash",
                evidence_json: br#"{"match":false}"#,
                decision: "outcome_unconfirmed",
            })
            .expect("repeated decision is idempotent");
        assert_eq!(
            repeated.id,
            database
                .latest_recovery("run-state")
                .expect("latest reads")
                .expect("record exists")
                .id
        );
        assert!(matches!(
            database.transition_recovery(RecoveryTransitionInput {
                run_id: "run-state",
                next: RecoveryState::Resumable,
                effect_id: "effect-state",
                idempotency_key: "run-state:effect-state:blind-retry",
                verifier: "file_hash",
                evidence_json: br#"{}"#,
                decision: "blind_retry",
            }),
            Err(StorageError::InvalidRecovery(_))
        ));
        let events = database.read_events_after(0, 10).expect("events read");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == "run.recovery.decision")
                .count(),
            3
        );
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_12_is_idempotent_and_preserves_pre_existing_memory_rows() {
        // Reproduces the pre-wave-VI state: a v8 `memory_entries` table
        // (no `confirmations` / `lesson_key`) with one real row already in
        // it, then confirms the 11 -> 12 migration both preserves that row
        // and can be re-applied (guarded re-open) without altering the
        // already-migrated columns a second time.
        let path = temp_database_path("migration-12-idempotent");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .execute_batch(
                    "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_memory_entries_scope
                        ON memory_entries(scope_kind, scope_id);
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten)
                    VALUES
                        ('pre-existing', 'project', 'scope-a', 'Old title', 'Old content',
                         'task:pre-wave-vi', 'internal', '2026-01-01T00:00:00Z', NULL, 0, 0);
                    PRAGMA user_version = 11;",
                )
                .expect("v8-shaped legacy memory table seeds");
        }

        let database = LocalDatabase::open(&path).expect("database migrates 11 -> 12");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        let (confirmations, lesson_key): (i64, Option<String>) = database
            .connection()
            .query_row(
                "SELECT confirmations, lesson_key FROM memory_entries WHERE id = 'pre-existing'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("pre-existing row survives migration with new columns defaulted");
        assert_eq!(confirmations, 1, "DEFAULT 1 applied to pre-existing rows");
        assert_eq!(lesson_key, None);
        let title: String = database
            .connection()
            .query_row(
                "SELECT title FROM memory_entries WHERE id = 'pre-existing'",
                [],
                |row| row.get(0),
            )
            .expect("original content untouched by migration");
        assert_eq!(title, "Old title");
        drop(database);

        // Re-opening an already-migrated database must not re-run the
        // ALTER TABLE (which would error on a duplicate column) and must
        // not disturb existing data.
        let database = LocalDatabase::open(&path).expect("re-open is idempotent");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        let confirmations_after_reopen: i64 = database
            .connection()
            .query_row(
                "SELECT confirmations FROM memory_entries WHERE id = 'pre-existing'",
                [],
                |row| row.get(0),
            )
            .expect("row still present after idempotent re-open");
        assert_eq!(confirmations_after_reopen, 1);
        let row_count: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .expect("count reads");
        assert_eq!(row_count, 1, "no duplicate rows created by re-migration");

        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn research_and_memory_stores_round_trip_against_shared_migrated_database() {
        use crate::memory_store::{
            MemoryPrivacy, MemoryRecord, MemoryRecordInput, MemoryScope, MemoryStoreSql,
        };
        use crate::research_store::{ResearchEvidenceRecord, ResearchEvidenceSql};

        let path = temp_database_path("bounded-stores");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );

        let evidence = ResearchEvidenceRecord {
            id: "evidence-1".into(),
            source_kind: "url".into(),
            source_ref: "https://example.test/source".into(),
            redacted_excerpt: "redacted result".into(),
            source_hash: "sha256:abc".into(),
            fetched_at: "2026-08-12T10:00:00Z".into(),
            ttl_seconds: 3600,
            provenance_link: Some("run:shared-db".into()),
        };
        ResearchEvidenceSql::insert(database.connection(), &evidence)
            .expect("evidence inserts against shared connection");
        assert_eq!(
            ResearchEvidenceSql::get_by_id(database.connection(), "evidence-1")
                .expect("evidence reads"),
            Some(evidence)
        );
        assert_eq!(
            ResearchEvidenceSql::list_by_provenance(database.connection(), "run:shared-db")
                .expect("evidence lists")
                .len(),
            1
        );

        let memory = MemoryRecord::new(MemoryRecordInput {
            id: "memory-1".into(),
            scope: MemoryScope::Project,
            scope_id: "project-shared-db".into(),
            title: "Decision".into(),
            content: "keep this fact".into(),
            provenance: "run:shared-db".into(),
            privacy: MemoryPrivacy::Internal,
            created_at: "2026-08-12T10:00:00Z".into(),
            expires_at: Some("2027-01-01T00:00:00Z".into()),
        })
        .expect("memory record builds");
        MemoryStoreSql::insert(database.connection(), &memory)
            .expect("memory inserts against shared connection");
        assert_eq!(
            MemoryStoreSql::get_by_id(database.connection(), "memory-1").expect("memory reads"),
            Some(memory)
        );
        let found = MemoryStoreSql::search(
            database.connection(),
            MemoryScope::Project,
            "project-shared-db",
            "fact",
            "2026-09-01T00:00:00Z",
            10,
        )
        .expect("memory search");
        assert_eq!(found.len(), 1);
        assert!(MemoryStoreSql::archive(database.connection(), "memory-1").expect("archive"));
        assert!(MemoryStoreSql::forget(database.connection(), "memory-1").expect("forget"));

        drop(database);
        let _ = std::fs::remove_file(&path);
    }

    fn sample_ledger_event(
        event_id: &str,
        run_id: &str,
        action_id: &str,
        state_after: crate::execution_ledger::ActionState,
    ) -> crate::execution_ledger::ExecutionEventV1 {
        crate::execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: event_id.to_string(),
            sequence_id: None,
            run_scope: crate::execution_ledger::RunScope::Workflow,
            run_id: run_id.to_string(),
            session_id: Some("session-1".into()),
            task_id: "task-ledger".into(),
            created_at_ms: 1_700_000_000_000,
            state_after: Some(state_after),
            action_id: Some(action_id.to_string()),
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: Some(run_id.to_string()),
            node_id: Some("node-1".into()),
            attempt_id: None,
            effect_id: None,
            model_request_id: None,
            body: crate::execution_ledger::ExecutionEventBody::ToolCall {
                tool_name: "shell".into(),
                tool_call_hash: "hash".into(),
                manifest_hash: None,
            },
            redaction: crate::execution_ledger::RedactionMeta::default(),
        }
    }

    /// План 08-4 acceptance: legacy `events` rows (written before 08-1, or
    /// still written by callers that never adopt the typed ledger) get a
    /// deterministic `event_id` via `execution_ledger::legacy_event_id`,
    /// without the original row — or its `sequence_id` — ever being
    /// touched. Recomputing the mapping from the durably stored row must
    /// reproduce the exact same id every time.
    #[test]
    fn legacy_event_mapping_is_reproducible_and_preserves_the_original_row() {
        let path = temp_database_path("ledger-legacy-mapping");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let sequence_id = database
            .append_event("legacy-task", "task.completed", b"{\"ok\":true}")
            .expect("legacy event appends through the pre-08-1 path");

        let stored = database
            .read_events_after(sequence_id - 1, 1)
            .expect("read back")
            .remove(0);
        // Legacy rows never get the typed columns populated.
        assert_eq!(stored.sequence_id, sequence_id);
        assert_eq!(stored.task_id, "legacy-task");
        assert_eq!(stored.event_type, "task.completed");

        let mapped_once = crate::execution_ledger::legacy_event_id(
            stored.sequence_id,
            &stored.task_id,
            &stored.event_type,
            &stored.payload,
            &stored.created_at,
        );
        let mapped_again = crate::execution_ledger::legacy_event_id(
            stored.sequence_id,
            &stored.task_id,
            &stored.event_type,
            &stored.payload,
            &stored.created_at,
        );
        assert_eq!(
            mapped_once, mapped_again,
            "mapping the same durable row twice must reproduce the same event_id"
        );
        assert_eq!(
            mapped_once.len(),
            crate::execution_ledger::LEGACY_EVENT_ID_HEX_LEN
        );

        // Re-reading the row after the mapping was computed proves the row
        // itself — sequence_id included — was never rewritten.
        let reread = database
            .read_events_after(sequence_id - 1, 1)
            .expect("re-read")
            .remove(0);
        assert_eq!(reread, stored);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn append_ledger_event_round_trips_through_events_table() {
        let path = temp_database_path("ledger-append");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let event = sample_ledger_event(
            "event-1",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Running,
        );
        let sequence_id = database
            .append_ledger_event(&event)
            .expect("typed event appends");

        let stored = database
            .read_events_after(sequence_id - 1, 1)
            .expect("read back")
            .remove(0);
        let round_tripped: crate::execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&stored.payload).expect("payload decodes");
        assert_eq!(round_tripped, event);
        assert_eq!(stored.event_type, "ledger.tool_call");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn append_ledger_events_batches_rows_in_one_commit() {
        let path = temp_database_path("ledger-append-batch");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let first = sample_ledger_event(
            "batch-event-1",
            "batch-run",
            "batch-action-1",
            crate::execution_ledger::ActionState::Running,
        );
        let second = sample_ledger_event(
            "batch-event-2",
            "batch-run",
            "batch-action-2",
            crate::execution_ledger::ActionState::WaitingApproval,
        );

        let sequence_ids = database
            .append_ledger_events(&[first, second])
            .expect("batch appends");

        assert_eq!(sequence_ids, vec![1, 2]);
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn append_ledger_events_rolls_back_on_duplicate_terminal_outcome() {
        let path = temp_database_path("ledger-append-batch-rollback");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let first = sample_ledger_event(
            "batch-terminal-1",
            "batch-run",
            "batch-action",
            crate::execution_ledger::ActionState::Succeeded,
        );
        let second = sample_ledger_event(
            "batch-terminal-2",
            "batch-run",
            "batch-action",
            crate::execution_ledger::ActionState::Failed,
        );

        let error = database
            .append_ledger_events(&[first, second])
            .expect_err("duplicate terminal outcome rejects the whole batch");
        assert!(matches!(
            error,
            StorageError::LedgerContract(
                crate::execution_ledger::LedgerContractError::DuplicateTerminalOutcome { .. }
            )
        ));
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn duplicate_event_id_violates_partial_unique_index() {
        let path = temp_database_path("ledger-dup-id");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let first = sample_ledger_event(
            "same-event-id",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Running,
        );
        let second = sample_ledger_event(
            "same-event-id",
            "run-1",
            "action-2",
            crate::execution_ledger::ActionState::Running,
        );
        database
            .append_ledger_event(&first)
            .expect("first insert succeeds");
        let error = database
            .append_ledger_event(&second)
            .expect_err("duplicate event_id must be rejected");
        assert!(matches!(error, StorageError::Sqlite(_)));
        let _ = std::fs::remove_file(&path);
    }

    /// The single-terminal-outcome guarantee (план 08-1's
    /// `assert_single_terminal`) must hold at the real write path, not just
    /// as an in-memory helper: a second terminal event for the same
    /// `action_id` is rejected even via two independent `append_ledger_event`
    /// calls (no batch, no shared transaction between them).
    #[test]
    fn second_terminal_outcome_for_same_action_is_rejected_at_write_time() {
        let path = temp_database_path("ledger-single-terminal");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let first_terminal = sample_ledger_event(
            "event-terminal-1",
            "run-1",
            "action-guarded",
            crate::execution_ledger::ActionState::Succeeded,
        );
        database
            .append_ledger_event(&first_terminal)
            .expect("first terminal outcome accepted");

        let second_terminal = sample_ledger_event(
            "event-terminal-2",
            "run-1",
            "action-guarded",
            crate::execution_ledger::ActionState::Failed,
        );
        let error = database
            .append_ledger_event(&second_terminal)
            .expect_err("second terminal outcome for the same action must be rejected");
        assert!(matches!(
            error,
            StorageError::LedgerContract(
                crate::execution_ledger::LedgerContractError::DuplicateTerminalOutcome { .. }
            )
        ));
        // Rejected write must not have landed.
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// A non-terminal follow-up (e.g. Running -> WaitingApproval) for the
    /// same action is unaffected by the guard — only a second *terminal*
    /// outcome is rejected.
    #[test]
    fn non_terminal_follow_up_for_same_action_is_accepted() {
        let path = temp_database_path("ledger-non-terminal-followup");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let running = sample_ledger_event(
            "event-followup-1",
            "run-1",
            "action-followup",
            crate::execution_ledger::ActionState::Running,
        );
        database
            .append_ledger_event(&running)
            .expect("running accepted");
        let waiting = sample_ledger_event(
            "event-followup-2",
            "run-1",
            "action-followup",
            crate::execution_ledger::ActionState::WaitingApproval,
        );
        database
            .append_ledger_event(&waiting)
            .expect("non-terminal follow-up accepted");
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 2);
        let _ = std::fs::remove_file(&path);
    }

    fn insert_workflow_fixture(database: &LocalDatabase, run_id: &str, node_id: &str) {
        use crate::workflow_store::{NodeState, RunState, WorkflowNodeRecord, WorkflowRunRecord};
        let run = WorkflowRunRecord {
            run_id: run_id.to_string(),
            task_id: "task-ledger".into(),
            template_id: "template-1".into(),
            template_version: 1,
            graph_id: "graph-1".into(),
            graph_version: 1,
            graph_hash: "graph-hash".into(),
            graph_json: "{}".into(),
            inputs_json: "{}".into(),
            policy_json: "{}".into(),
            state: RunState::Running,
            created_at_ms: 1_700_000_000_000,
            updated_at_ms: 1_700_000_000_000,
            terminal_reason: String::new(),
            cancel_requested: false,
            lease_owner: String::new(),
            lease_expires_at_ms: 0,
        };
        let node = WorkflowNodeRecord {
            run_id: run_id.to_string(),
            node_id: node_id.to_string(),
            action_kind: "shell".into(),
            state: NodeState::Running,
            attempts: 0,
            output_json: String::new(),
            error_code: String::new(),
            error_message: String::new(),
            approval_id: String::new(),
            updated_at_ms: 1_700_000_000_000,
        };
        crate::workflow_store::insert_run(database.connection(), &run, &[node])
            .expect("workflow fixture inserts");
    }

    #[test]
    fn node_transition_and_event_commit_together() {
        let path = temp_database_path("ledger-transition-ok");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        insert_workflow_fixture(&database, "run-1", "node-1");
        let event = sample_ledger_event(
            "event-transition-ok",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Succeeded,
        );
        database
            .append_ledger_event_with_node_transition(
                &event,
                "run-1",
                "node-1",
                crate::execution_ledger::ActionState::Running,
                crate::execution_ledger::ActionState::Succeeded,
                1_700_000_001_000,
            )
            .expect("legal transition commits both parts");

        let node_state: String = database
            .connection()
            .query_row(
                "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
                [],
                |row| row.get(0),
            )
            .expect("node row reads");
        assert_eq!(node_state, "succeeded");
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// План 08-2/08-4: `workflow_run_events.run_sequence` must be linked back
    /// to the global ledger row it corresponds to, in the same transaction
    /// that wrote both.
    #[test]
    fn node_transition_links_workflow_run_sequence_to_global_ledger_event() {
        let path = temp_database_path("ledger-transition-linkage");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        insert_workflow_fixture(&database, "run-1", "node-1");
        let event = sample_ledger_event(
            "event-linkage-1",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Succeeded,
        );
        let sequence_id = database
            .append_ledger_event_with_node_transition(
                &event,
                "run-1",
                "node-1",
                crate::execution_ledger::ActionState::Running,
                crate::execution_ledger::ActionState::Succeeded,
                1_700_000_001_000,
            )
            .expect("legal transition commits both parts");

        let (run_sequence, ledger_sequence_id, ledger_event_id): (
            i64,
            Option<i64>,
            Option<String>,
        ) = database
            .connection()
            .query_row(
                "SELECT run_sequence, ledger_sequence_id, ledger_event_id
                       FROM workflow_run_events WHERE run_id = 'run-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("workflow_run_events row reads");
        assert_eq!(run_sequence, 0, "first event in a fresh run starts at 0");
        assert_eq!(ledger_sequence_id, Some(sequence_id));
        assert_eq!(ledger_event_id.as_deref(), Some("event-linkage-1"));
        let _ = std::fs::remove_file(&path);
    }

    /// A rejected transition must not leave a dangling `workflow_run_events`
    /// row either — both inserts share the one transaction that rolls back.
    #[test]
    fn illegal_transition_rolls_back_workflow_run_events_too() {
        let path = temp_database_path("ledger-transition-illegal-linkage");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        insert_workflow_fixture(&database, "run-1", "node-1");
        let event = sample_ledger_event(
            "event-illegal-linkage-1",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Running,
        );
        let _ = database.append_ledger_event_with_node_transition(
            &event,
            "run-1",
            "node-1",
            crate::execution_ledger::ActionState::Succeeded,
            crate::execution_ledger::ActionState::Running,
            1_700_000_001_000,
        );
        let count: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM workflow_run_events WHERE run_id = 'run-1'",
                [],
                |row| row.get(0),
            )
            .expect("count reads");
        assert_eq!(
            count, 0,
            "rolled-back transition must not leave a linkage row"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// План 08-2 acceptance: `cancelling` is a real, storable transient state
    /// for `workflow_run_nodes.state` after the CHECK-rebuild migration, not
    /// just a value the CHECK constraint happens to tolerate. A node must be
    /// able to pass through it (`Running -> Cancelling -> Cancelled`) via the
    /// same atomic write path as any other transition.
    #[test]
    fn node_passes_through_cancelling_before_reaching_cancelled() {
        let path = temp_database_path("ledger-cancelling-transition");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        insert_workflow_fixture(&database, "run-1", "node-1");

        let cancelling_event = sample_ledger_event(
            "event-cancelling-1",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Cancelling,
        );
        database
            .append_ledger_event_with_node_transition(
                &cancelling_event,
                "run-1",
                "node-1",
                crate::execution_ledger::ActionState::Running,
                crate::execution_ledger::ActionState::Cancelling,
                1_700_000_001_000,
            )
            .expect("Running -> Cancelling is allowed");
        let mid_state: String = database
            .connection()
            .query_row(
                "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
                [],
                |row| row.get(0),
            )
            .expect("node row reads");
        assert_eq!(mid_state, "cancelling");

        let cancelled_event = sample_ledger_event(
            "event-cancelling-2",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Cancelled,
        );
        database
            .append_ledger_event_with_node_transition(
                &cancelled_event,
                "run-1",
                "node-1",
                crate::execution_ledger::ActionState::Cancelling,
                crate::execution_ledger::ActionState::Cancelled,
                1_700_000_002_000,
            )
            .expect("Cancelling -> Cancelled is allowed");
        let final_state: String = database
            .connection()
            .query_row(
                "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
                [],
                |row| row.get(0),
            )
            .expect("node row reads");
        assert_eq!(final_state, "cancelled");
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn illegal_transition_rolls_back_without_writing_event() {
        let path = temp_database_path("ledger-transition-illegal");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        insert_workflow_fixture(&database, "run-1", "node-1");
        let event = sample_ledger_event(
            "event-transition-illegal",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Running,
        );
        let error = database
            .append_ledger_event_with_node_transition(
                &event,
                "run-1",
                "node-1",
                crate::execution_ledger::ActionState::Succeeded,
                crate::execution_ledger::ActionState::Running,
                1_700_000_001_000,
            )
            .expect_err("Succeeded -> Running is not an allowed transition");
        assert!(matches!(
            error,
            StorageError::LedgerContract(
                crate::execution_ledger::LedgerContractError::IllegalTransition { .. }
            )
        ));
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 0);
        let node_state: String = database
            .connection()
            .query_row(
                "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
                [],
                |row| row.get(0),
            )
            .expect("node row reads");
        assert_eq!(
            node_state, "running",
            "node state must not change on rollback"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn node_state_mismatch_rolls_back_without_writing_event() {
        let path = temp_database_path("ledger-transition-conflict");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        insert_workflow_fixture(&database, "run-1", "node-1");
        // Узел на самом деле в 'running', но вызывающий думает, что в 'ready'.
        let event = sample_ledger_event(
            "event-transition-conflict",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Running,
        );
        let error = database
            .append_ledger_event_with_node_transition(
                &event,
                "run-1",
                "node-1",
                crate::execution_ledger::ActionState::Ready,
                crate::execution_ledger::ActionState::Running,
                1_700_000_001_000,
            )
            .expect_err("stale from-state must be rejected");
        assert!(matches!(
            error,
            StorageError::LedgerNodeTransitionConflict { .. }
        ));
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 0);
        let _ = std::fs::remove_file(&path);
    }

    /// План 08-4 acceptance: "SQLite failure с полным rollback" — a genuine
    /// SQLite-level constraint violation (not an application-level guard)
    /// hitting mid-transaction, after the `workflow_run_nodes` UPDATE has
    /// already run, must still roll back that UPDATE along with the failed
    /// INSERT. `event_id`'s partial UNIQUE index is the real constraint
    /// used here — the transition itself is legal, only `event_id` collides
    /// with an already-committed row from a prior, unrelated write.
    #[test]
    fn sqlite_constraint_failure_mid_transaction_rolls_back_the_node_update_too() {
        let path = temp_database_path("ledger-sqlite-failure-rollback");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        insert_workflow_fixture(&database, "run-1", "node-1");

        let already_committed = sample_ledger_event(
            "event-id-collision",
            "run-1",
            "action-unrelated",
            crate::execution_ledger::ActionState::Running,
        );
        database
            .append_ledger_event(&already_committed)
            .expect("first write with this event_id commits");

        // Same event_id, otherwise a perfectly legal Running -> Succeeded
        // transition on a node that really is in 'running'.
        let colliding_event = sample_ledger_event(
            "event-id-collision",
            "run-1",
            "action-1",
            crate::execution_ledger::ActionState::Succeeded,
        );
        let error = database
            .append_ledger_event_with_node_transition(
                &colliding_event,
                "run-1",
                "node-1",
                crate::execution_ledger::ActionState::Running,
                crate::execution_ledger::ActionState::Succeeded,
                1_700_000_001_000,
            )
            .expect_err("duplicate event_id must fail at the SQLite constraint");
        assert!(matches!(error, StorageError::Sqlite(_)));

        // The UPDATE that ran before the failing INSERT must not have
        // survived: the node is still 'running', not 'succeeded'.
        let node_state: String = database
            .connection()
            .query_row(
                "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
                [],
                |row| row.get(0),
            )
            .expect("node row reads");
        assert_eq!(
            node_state, "running",
            "the node update must roll back together with the failed insert"
        );
        // Only the one earlier, unrelated commit is visible — the failed
        // attempt left no trace of its own events/workflow_run_events rows.
        assert_eq!(database.latest_event_sequence().expect("sequence reads"), 1);
        let workflow_event_count: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM workflow_run_events WHERE run_id = 'run-1'",
                [],
                |row| row.get(0),
            )
            .expect("count reads");
        assert_eq!(workflow_event_count, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_core_start_publishes_one_system_scope_event() {
        let path = temp_database_path("ledger-core-start");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let sequence_id = database
            .record_core_start("core-instance-1")
            .expect("core_start publishes");
        assert!(sequence_id > 0);
        let stored = database
            .read_events_after(sequence_id - 1, 1)
            .expect("read back")
            .remove(0);
        assert_eq!(stored.event_type, "ledger.recovery_decision");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reconcile_startup_flags_open_dispatch_marker_as_unknown_outcome() {
        let path = temp_database_path("ledger-reconcile");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        insert_workflow_fixture(&database, "run-1", "node-1");

        database
            .create_project("project-reconcile", "Reconcile", ".", None)
            .expect("project creates");
        let task = WorkItemRecord {
            id: "task-reconcile".into(),
            project_id: "project-reconcile".into(),
            parent_id: None,
            title: "reconcile me".into(),
            description: String::new(),
            source_ref: None,
            acceptance_criteria: String::new(),
            non_goals: String::new(),
            status: "in_progress".into(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        };
        database.create_work_item(&task).expect("task creates");
        let run = RunRecord {
            id: "effect-run-1".into(),
            work_item_id: task.id.clone(),
            status: "running".into(),
            policy_snapshot: vec![],
            role_snapshot: vec![],
            skill_snapshot: vec![],
            model_route_snapshot: vec![],
        };
        let checkpoint = RunCheckpointRecord {
            run_id: run.id.clone(),
            checkpoint_id: "checkpoint-1".into(),
            stage: "build".into(),
            node_id: "node-1".into(),
            attempt: 1,
            input_hash: "input-hash".into(),
            state_json: b"{}".to_vec(),
            pending_effects_json: b"[]".to_vec(),
            committed_at: "2026-01-01T00:00:00Z".into(),
        };
        let effect = RunEffectRecord {
            effect_id: "effect-open-1".into(),
            run_id: run.id.clone(),
            node_id: "node-1".into(),
            kind: "bounded_build".into(),
            idempotency_key: "run-1:node-1".into(),
            immutable_intent_hash: "intent-hash".into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        database
            .prepare_run_effect(&run, &checkpoint, &effect)
            .expect("effect prepares");
        database
            .mark_effect_executing("effect-open-1")
            .expect("effect marker opens (started, not completed)");

        let mut running_event = sample_ledger_event(
            "event-running-1",
            "run-1",
            "action-open-1",
            crate::execution_ledger::ActionState::Running,
        );
        running_event.effect_id = Some("effect-open-1".into());
        database
            .append_ledger_event(&running_event)
            .expect("running action recorded");

        let reconciled = database
            .reconcile_ledger_on_startup()
            .expect("reconciliation runs");
        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].0, "action-open-1");
        assert_eq!(
            reconciled[0].1,
            crate::execution_ledger::ActionState::UnknownOutcome
        );

        // Исходная строка не переписана — reconciliation добавила новую.
        let all_action_events: Vec<crate::execution_ledger::ActionState> = database
            .read_events_after(0, 100)
            .expect("events read")
            .into_iter()
            .filter_map(|record| {
                serde_json::from_slice::<crate::execution_ledger::ExecutionEventV1>(&record.payload)
                    .ok()
            })
            .filter(|event| event.action_id.as_deref() == Some("action-open-1"))
            .filter_map(|event| event.state_after)
            .collect();
        assert_eq!(
            all_action_events,
            vec![
                crate::execution_ledger::ActionState::Running,
                crate::execution_ledger::ActionState::UnknownOutcome,
            ]
        );

        assert!(
            database
                .reconcile_ledger_on_startup()
                .expect("second reconciliation runs")
                .is_empty(),
            "unknown_outcome is terminal; reconciliation must not repeat"
        );
        let _ = std::fs::remove_file(&path);
    }
}
