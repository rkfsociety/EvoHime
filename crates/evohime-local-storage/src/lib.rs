#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![allow(dead_code)]

use std::path::PathBuf;

use rusqlite::Connection;

pub mod agent_git_change_sets_store;
pub(crate) mod agent_middleware_pipeline_store;
pub mod agent_program_optimizer_store;
pub mod agent_role_profiles_store;
mod ambient_contract;
pub mod ambient_store;
mod ambient_store_cleanup;
mod ambient_store_mapping;
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
mod database_lifecycle;
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
mod memory_inputs;
mod memory_mapping;
mod memory_queries;
mod memory_schema;
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
