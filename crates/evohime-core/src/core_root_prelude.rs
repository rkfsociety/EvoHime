impl CoreVersion {
    pub const fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant, SystemTime},
};

use base64::Engine;
use evohime_local_storage::{
    BackupPreview, BackupProgress, BackupResult, EventRecord, ImportedTask, LocalDatabase,
    ProjectPolicyRecord, RecoveryState, RestoreResult, RunCheckpointRecord, RunEffectRecord,
    RunRecord, RunRecoveryRecord, StorageError, ToolMetricRecord, WorkItemRecord,
};
use evohime_model_gateway::{
    providers::{ChatMessage, ChatRole, ProviderError},
    ModelGateway, PrivacyClass, RoutingMode, RoutingRequest, ToolSpec,
};
use evohime_receipts::{
    key_lifecycle::ReceiptKeyManager,
    runtime::{
        ActionRequest as ReceiptActionRequest, PolicyDecision as ReceiptPolicyDecision,
        PrepareOutcome as ReceiptPrepareOutcome, ProtectedActionRow, ReceiptRuntime, ReceiptSigner,
        RuntimeError as ReceiptRuntimeError,
    },
};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub mod agent_benchmark_matrix;
pub mod agent_middleware_pipeline;
pub mod agent_role_profiles;
pub mod agentic_browser_session;
pub mod ambient;
pub mod ambient_proactivity;
pub mod analysis_kernel;
pub mod artifact_handoff_registry;
pub mod audit;
pub mod batch_invocation_runtime;
pub mod browser_backend;
pub mod build;
pub mod capability_registry;
pub mod capability_selection;
pub mod causal_collaboration_bus;
pub mod child_contracts;
pub mod child_roles;
pub mod child_runtime;
pub mod child_workflow;
pub mod code_anchored_intent_markers;
pub mod context_budget;
pub mod continuation;
pub mod conversation_event_log;
pub mod conversation_workbench;
pub mod conversational_workflow_composer;
pub mod doctor;
pub mod evals;
pub mod event_trigger_runtime;
pub mod execution_backend_registry;
pub mod export;
pub mod extension_conformance_kit;
pub mod external_coding_agent_adapter;
pub mod goal;
pub mod guided_calibration_sessions;
pub mod human_work_items;
pub mod incremental_change_protocol;
pub mod integration_provider_runtime;
pub mod integration_provider_sdk;
pub mod invocation_presets;
#[cfg(windows)]
mod listener_pipe;
pub mod local_model_runtime_manager;
pub mod memory_api;
pub mod memory_domain;
pub mod memory_extraction;
pub mod memory_governance;
pub mod memory_retrieval;
pub mod memory_views_and_adaptive_recall;
pub mod message_intervention_policies;
pub mod model_edit_protocol_registry;
pub mod model_purpose_routing;
pub mod model_resilience_policy;
pub mod observability;
pub mod permission_rules;
pub mod persistent_agent_registry;
pub mod plan;
pub mod plan_artifact;
pub mod policy_aware_tool_result_cache;
pub mod policy_gate;
pub mod prd;
pub mod prompt_cache_planner;
pub mod provider_resilience;
pub mod remote_conversation_channels;
pub mod retained_child;
pub mod structured_response_contract;
pub mod support_bundle;
pub mod tool_simulation_runtime;
pub mod workspace_state_checkpoints;
pub use provider_resilience::{
    default_tool_specs, filter_readonly_tools, handle_provider_error, is_retriable_error,
    ProviderResilienceConfig,
};
pub mod recovery;
pub mod run_policy;
pub use recovery::{classify_tool_outcome, DenialSource, ToolFailureKind, ToolOutcome};
pub mod composable_termination_conditions;
pub mod refinement;
pub mod research;
pub mod research_fetch;
pub mod research_gate;
pub mod research_pipeline;
pub mod research_search;
pub mod scope;
pub mod skill_registry;
pub mod skill_trust_pipeline;
pub mod task_memory;
pub mod task_worktree_isolation;
pub mod team_coordination_policies;
pub mod team_resource_budget;
pub mod typed_agent_handoff_contract;
pub use task_memory::project_scope_id;
pub mod agent_git_change_sets;
pub mod architect_editor_model_pipeline;
pub mod architecture_snapshot;
pub mod architecture_snapshot_runtime;
pub mod plan_context;
pub mod plan_review;
pub mod task_checkpoint;
pub mod telemetry;
pub mod vision_contract;
pub mod visual_workflow_builder;
pub mod voice_command;
pub mod workflow;
pub mod workflow_adapters;
pub mod workflow_execution;
pub mod workflow_package;
pub mod workflow_registry;
pub mod workflow_runner;
pub mod workflow_runtime;
pub mod workflow_templates;
pub mod workspace;
pub mod workspace_rag;
