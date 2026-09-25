#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![allow(dead_code)]
//! SQLite-backed local persistence contracts and domain stores for EvoHime.
//!
//! The crate owns local schema access, transactional migrations, and typed
//! repositories used by the Core. It does not expose network or UI runtime.

use std::path::PathBuf;

use rusqlite::Connection;

/// SQLite-backed records for agent-authored Git change sets.
pub mod agent_git_change_sets_store;
pub(crate) mod agent_middleware_pipeline_store;
/// Persistence for agent program optimizer state and results.
pub mod agent_program_optimizer_store;
/// Persistence for reusable agent role profiles.
pub mod agent_role_profiles_store;
mod ambient_contract;
pub mod ambient_store;
mod ambient_store_cleanup;
mod ambient_store_mapping;
pub mod analysis_kernel;
/// Persistence for approval policy profiles.
pub mod approval_policy_profiles_store;
/// Persistence for architect and editor model pipeline configuration.
pub mod architect_editor_model_pipeline_store;
/// Persistence for captured architecture snapshots.
pub mod architecture_snapshot_store;
/// Registry for artifacts handed between runtime components.
pub mod artifact_handoff_registry_store;
pub(crate) mod artifact_store;
/// Persistence for authorized security assessment records.
pub mod authorized_security_assessment_store;
/// Persistence for scheduled and recurring automation state.
pub mod automation_store;
/// Runtime persistence for autonomous metric experiments.
pub mod autonomous_metric_experiment_runtime_store;
pub(crate) mod backup;
/// Persistence for batch invocation runtime state.
pub mod batch_invocation_runtime_store;
pub(crate) mod benchmark_store;
/// Persistence for browser session metadata.
pub mod browser_session_store;
/// Persistence for selected capability state.
pub mod capability_selection_store;
/// Persistence for capability definitions and assignments.
pub mod capability_store;
/// Persistence for capability workbench state.
pub mod capability_workbenches_store;
pub(crate) mod checkpoint_forking_store;
pub(crate) mod child_store;
/// Persistence for code diagnostics and feedback-loop records.
pub mod code_diagnostics_feedback_loop_store;
/// Persistence for code review lane state.
pub mod code_review_lane_store;
/// Persistence for collaboration records.
pub mod collaboration_store;
/// Persistence for command-center state.
pub mod command_center_store;
/// Persistence for composable execution termination conditions.
pub mod composable_termination_conditions_store;
/// Persistence for context command definitions and state.
pub mod context_command_store;
/// Persistence for context ledger entries.
pub mod context_ledger_store;
/// Persistence for context loadout configuration.
pub mod context_loadout_store;
/// Persistence for context namespaces.
pub mod context_namespace_store;
/// Persistence for contextual next-step suggestions.
pub mod contextual_next_step_suggestions_store;
pub(crate) mod continuation_store;
/// Persistence for conversation bridge adapter state.
pub mod conversation_bridge_adapters_store;
pub(crate) mod conversation_event_log_store;
/// Persistence for Core topic subscription event-bus state.
pub mod core_topic_subscription_event_bus_store;
/// Persistence for cross-modal UI grounding records.
pub mod cross_modal_ui_grounding_store;
/// Persistence for customization inventory records.
pub mod customization_inventory_store;
mod database_access;
mod database_lifecycle;
/// Registry for declarative agent components.
pub mod declarative_agent_component_registry_store;
/// Persistence for declarative runtime component configuration.
pub mod declarative_runtime_components_store;
/// Persistence for dependency-aware task graphs.
pub mod dependency_aware_task_graph_store;
/// Persistence for deterministic review execution plans.
pub mod deterministic_review_execution_plan_store;
/// Persistence for metadata-only guided recipe run links.
pub mod capability_recipe_store;
mod diagnostics;
/// Persistence for domain-specific workflow recipes.
pub mod domain_workflow_recipes_store;
/// Domain models and shared persistence contracts.
pub mod domains;
/// Persistence for durable background execution state.
pub mod durable_background_execution_store;
/// Persistence for durable remote task bridge state.
pub mod durable_remote_task_bridge_store;
mod event_export;
mod event_store;
pub(crate) mod event_trigger_runtime_store;
/// Registry for event visualizer definitions.
pub mod event_visualizer_registry_store;
/// Registry for execution backends.
pub mod execution_backend_registry_store;
/// Persistence for execution environment profiles.
pub mod execution_environment_profiles_store;
/// Persistence contracts for execution ledger records.
pub mod execution_ledger;
pub(crate) mod execution_policy_profiles_store;
/// Persistence for experience replay examples.
pub mod experience_replay_library_store;
/// Persistence for external coding agent adapter configuration.
pub mod external_coding_agent_adapter_store;
/// Runtime persistence for external source acquisition.
pub mod external_source_acquisition_runtime_store;
/// Persistence for feedback records.
pub mod feedback_store;
/// Persistence for free-access evidence.
pub mod free_access_evidence_store;
/// Persistence for Git remote publication protocol state.
pub mod git_remote_publication_protocol_store;
/// Persistence for durable user and runtime goals.
pub mod goal;
/// Persistence for grounded research records.
pub mod grounded_research_store;
/// Persistence for guided calibration sessions.
pub mod guided_calibration_sessions_store;
/// Persistence for hardware fit evidence.
pub mod hardware_fit_evidence_store;
/// Persistence for human work item state.
pub mod human_work_items_store;
/// Persistence for IDE companion bridge state.
pub mod ide_companion_bridge_store;
/// Persistence for incremental change protocol state.
pub mod incremental_change_protocol_store;
pub(crate) mod integration_provider_store;
/// Persistence for interactive model comparison workbenches.
pub mod interactive_model_compare_workbench_store;
/// Persistence for reusable invocation presets.
pub mod invocation_presets_store;
/// Persistence contracts for the kernel capability facade.
pub mod kernel_capability_facade_store;
/// Persistence for project roles in the knowledge source registry.
pub mod knowledge_source_registry_project_role_store;
/// Persistence for language intelligence state.
pub mod language_intelligence_store;
mod ledger_helpers;
mod ledger_reconciliation_store;
mod ledger_store;
mod legacy_migration;
/// Persistence for local model compatibility gateway state.
pub mod local_model_compatibility_gateway_store;
/// Persistence for local model performance calibration.
pub mod local_model_performance_calibration_store;
/// Persistence for local model runtime manager state.
pub mod local_model_runtime_manager_store;
/// Persistence for memory extraction records.
pub mod memory_extraction_store;
mod memory_inputs;
mod memory_mapping;
mod memory_queries;
mod memory_schema;
pub(crate) mod memory_store;
pub(crate) mod memory_views_and_adaptive_recall_store;
mod migrations;
/// Persistence for minimal-change policy configuration.
pub mod minimal_change_policy_store;
/// Runtime persistence for mobile device automation.
pub mod mobile_device_automation_runtime_store;
/// Registry for model edit protocols.
pub mod model_edit_protocol_registry_store;
/// Persistence for model limits.
pub mod model_limit_store;
pub(crate) mod model_provenance;
/// Persistence for model-purpose routing rules.
pub mod model_purpose_routing_store;
/// Persistence for multi-reviewer ensemble configuration.
pub mod multi_reviewer_ensemble_store;
/// Runtime persistence for native computer-use state.
pub mod native_computer_use_runtime_store;
/// Persistence for offline experience consolidation.
pub mod offline_experience_consolidation_store;
/// Persistence for optional voice output adapter state.
pub mod optional_voice_output_adapter_store;
pub(crate) mod output_guardrail_pipeline_store;
pub(crate) mod persistent_agent_registry_store;
/// Persistence contracts for plan artifacts.
pub mod plan_artifact;
/// Persistence for policy-aware tool result cache entries.
pub mod policy_aware_tool_result_cache_store;
/// Persistence for privacy telemetry configuration and records.
pub mod privacy_telemetry_store;
/// Persistence for project execution boards.
pub mod project_execution_board_store;
/// Persistence for project instruction stacks.
pub mod project_instruction_stack_store;
/// Persistence for project knowledge notebooks.
pub mod project_knowledge_notebook_store;
mod project_store;
pub(crate) mod prompt_cache_planner_store;
mod provenance_store;
/// Persistence for provider profile catalog entries.
pub mod provider_profile_catalog_store;
/// Persistence for reasoning operator definitions.
pub mod reasoning_operator_library_store;
pub(crate) mod reconciliation_verifier;
mod records;
/// Persistence for refinement workflow state.
pub mod refinement_store;
/// Persistence for remote conversation channel state.
pub mod remote_conversation_channels_store;
/// Persistence for research records.
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
/// Persistence for runtime service graph state.
pub mod runtime_service_graph_store;
/// Persistence for safe UI extension framework configuration.
pub mod safe_ui_extension_framework_store;
/// Persistence for schema-driven agent configuration.
pub mod schema_driven_agent_configuration_store;
/// Persistence for task scratchpad state.
pub mod scratchpad_store;
/// Persistence for semantic activity motion system state.
pub mod semantic_activity_motion_system_store;
/// Persistence for skill source lifecycle records.
pub mod skill_source_lifecycle_store;
pub(crate) mod skill_trust_pipeline_store;
mod snapshot_store;
/// Persistence for standing approval profiles.
pub mod standing_approval_profiles_store;
/// Persistence for static analysis pack configuration.
pub mod static_analysis_pack_store;
mod storage_error;
pub(crate) mod task_checkpoint;
pub(crate) mod task_worktree_isolation_store;
/// Persistence for team coordination policies.
pub mod team_coordination_policies_store;
/// Persistence for team coordinator state.
pub mod team_coordinator_store;
/// Persistence for team resource budget records.
pub mod team_resource_budget_store;
/// Persistence for team standard operating procedure protocols.
pub mod team_sop_protocols_store;
/// Persistence for temporal memory facts.
pub mod temporal_memory_facts_store;
/// Persistence for temporal signal intelligence records.
pub mod temporal_signal_intelligence_store;
/// Persistence for tool catalog and toolkit state.
pub mod toolkit_store;
/// Persistence for typed agent handoff contracts.
pub mod typed_agent_handoff_contract_store;
pub(crate) mod typed_context_references_store;
/// Persistence for verification evidence ledger records.
pub mod verification_evidence_ledger_store;
/// Persistence for verified Git checkpoint metadata.
pub mod verified_git_checkpoints_store;
/// Persistence for verified technical diagram artifacts.
pub mod verified_technical_diagram_artifacts_store;
/// Persistence for visual workflow builder state.
pub mod visual_workflow_builder_store;
/// Persistence for voice input dictation records.
pub mod voice_input_dictation_store;
mod work_item_store;
/// Persistence for workflow optimization experiments.
pub mod workflow_optimization_lab_store;
/// Persistence for workflow package definitions.
pub mod workflow_package_store;
/// Persistence for workflow state.
pub mod workflow_store;
/// Persistence for workspace bootstrap manifests.
pub mod workspace_bootstrap_manifest_store;
/// Persistence for workspace sets.
pub mod workspace_sets_store;
/// Persistence contracts for workspace state checkpoints.
pub mod workspace_state_checkpoint;
pub use backup::{
    BackupObjectSummary, BackupPreview, BackupProgress, BackupProgressPhase, BackupResult,
    RestoreResult, BACKUP_FORMAT_VERSION,
};

/// Current schema version installed by [`LocalDatabase`].
pub const SCHEMA_VERSION: u32 = 176;

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

/// SQLite database handle and schema owner for local Core persistence.
pub struct LocalDatabase {
    path: PathBuf,
    connection: Connection,
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
