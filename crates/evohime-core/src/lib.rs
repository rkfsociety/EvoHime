#![allow(dead_code, unused_imports)]

mod core_prelude;
pub(crate) use core_prelude::*;
pub use core_prelude::{
    CoreVersion, EventSink, AGENT_IDENTITY_PROMPT, DEFAULT_TASK_TIMEOUT_SECONDS,
};

pub(crate) mod adaptive_tool_catalog;
pub(crate) mod approval_policy_profiles;
pub mod capability_workbenches;
pub(crate) mod checkpoint_forking_and_replay;
pub mod code_diagnostics_feedback_loop;
pub(crate) mod conversation_bridge_adapters;
pub mod core_topic_subscription_event_bus;
pub(crate) mod customization_inventory;
pub mod declarative_agent_component_registry;
pub mod declarative_runtime_components;
pub(crate) mod dependency_aware_task_graph;
pub mod durable_remote_task_bridge;
pub(crate) mod event_visualizer_registry;
pub mod experience_replay_library;
pub mod headless_core_cli;
pub mod knowledge_source_registry_project_role;
pub(crate) mod output_guardrail_pipeline;
pub(crate) mod privacy_and_telemetry_governance;
pub mod project_instruction_stack;
pub(crate) mod reasoning_operator_library;
pub(crate) mod safe_ui_extension_framework;
pub mod schema_driven_agent_configuration;
pub mod sensitive_data_guardrails;
pub(crate) mod standing_approval_profiles;
pub mod team_coordinator;
pub(crate) mod team_sop_protocols;
pub(crate) mod typed_context_references;
pub mod workflow_optimization_lab;
pub mod workspace_bootstrap_manifest;
pub(crate) mod workspace_sets;

mod ipc_bridge;
pub use ipc_bridge::{IpcBridge, IpcBridgeError, ModelConfigSnapshot};
mod legacy_parser;
pub use legacy_parser::visible_agent_text;
#[cfg(test)]
pub(crate) use legacy_parser::LEGACY_TOOL_NAMES;
use legacy_parser::{
    parse_legacy_function_calls, parse_natural_tool_intent, parse_plain_tool_call,
    parse_tagged_tool_call, parse_xml_named_tool_call, strip_legacy_function_blocks,
};
mod logging;
pub(crate) use logging::write_model_trace;
pub use logging::StructuredLogger;
use logging::{append_audit_line, redact_boundary_text, write_observability_hook};
pub(crate) mod paths;
pub use paths::get_data_directory;
mod routing_trace;
use routing_trace::{
    classify_routing_task, routing_failure_trace, routing_success_trace, RoutingSuccessInput,
};

#[cfg(windows)]
mod pipe_server;
#[cfg(windows)]
pub use listener_pipe::run_windows_listener_pipe;
#[cfg(windows)]
pub use pipe_server::{run_windows_pipe, PipeServerConfig};
impl CoreVersion {
    pub const fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

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
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant, SystemTime},
};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub mod agent_benchmark_matrix;
pub mod agent_middleware_pipeline;
pub mod agent_role_profiles;
pub(crate) mod agentic_browser_session;
pub mod ambient;
pub(crate) mod ambient_proactivity;
pub(crate) mod analysis_kernel;
pub(crate) mod artifact_handoff_registry;
pub(crate) mod audit;
pub mod batch_invocation_runtime;
pub(crate) mod browser_backend;
pub(crate) mod build;
pub mod capability_registry;
pub mod capability_selection;
pub(crate) mod causal_collaboration_bus;
pub mod child_contracts;
pub mod child_roles;
pub mod child_runtime;
pub mod child_workflow;
pub mod code_anchored_intent_markers;
pub(crate) mod context_budget;
pub(crate) mod continuation;
pub(crate) mod conversation_event_log;
pub(crate) mod conversation_workbench;
pub(crate) mod conversational_workflow_composer;
pub(crate) mod doctor;
pub mod evals;
pub(crate) mod event_trigger_runtime;
pub(crate) mod execution_backend_registry;
pub(crate) mod export;
pub mod extension_conformance_kit;
pub(crate) mod external_coding_agent_adapter;
pub mod goal;
pub mod guided_calibration_sessions;
pub mod human_work_items;
pub(crate) mod incremental_change_protocol;
pub(crate) mod integration_provider_runtime;
pub(crate) mod integration_provider_sdk;
pub(crate) mod invocation_presets;
#[cfg(windows)]
mod listener_pipe;
pub(crate) mod local_model_runtime_manager;
pub(crate) mod memory_api;
pub(crate) mod memory_domain;
pub(crate) mod memory_extraction;
pub(crate) mod memory_governance;
pub(crate) mod memory_retrieval;
pub mod memory_views_and_adaptive_recall;
pub mod message_intervention_policies;
pub mod model_edit_protocol_registry;
pub mod model_purpose_routing;
pub mod model_resilience_policy;
pub(crate) mod observability;
pub mod permission_rules;
pub(crate) mod persistent_agent_registry;
pub mod plan;
pub mod plan_artifact;
pub mod policy_aware_tool_result_cache;
pub(crate) mod policy_gate;
pub(crate) mod prd;
pub mod prompt_cache_planner;
pub(crate) mod provider_resilience;
pub mod remote_conversation_channels;
pub(crate) mod retained_child;
pub(crate) mod structured_response_contract;
pub(crate) mod support_bundle;
pub mod tool_simulation_runtime;
pub mod workspace_state_checkpoints;
pub use provider_resilience::{
    default_tool_specs, filter_readonly_tools, handle_provider_error, is_retriable_error,
    ProviderResilienceConfig,
};
pub(crate) mod recovery;
pub mod run_policy;
pub use recovery::{classify_tool_outcome, DenialSource, ToolFailureKind, ToolOutcome};
pub mod composable_termination_conditions;
pub(crate) mod refinement;
pub(crate) mod research;
pub(crate) mod research_fetch;
pub(crate) mod research_gate;
pub(crate) mod research_pipeline;
pub(crate) mod research_search;
pub(crate) mod scope;
pub mod skill_registry;
pub mod skill_trust_pipeline;
pub mod task_memory;
pub(crate) mod task_worktree_isolation;
pub mod team_coordination_policies;
pub mod team_resource_budget;
pub mod typed_agent_handoff_contract;
pub use task_memory::project_scope_id;
pub(crate) mod agent_git_change_sets;
pub(crate) mod architect_editor_model_pipeline;
pub(crate) mod architecture_snapshot;
pub(crate) mod architecture_snapshot_runtime;
mod core_protocol;
pub mod plan_context;
pub mod plan_review;
pub(crate) mod task_checkpoint;
pub(crate) mod telemetry;
pub(crate) mod vision_contract;
pub(crate) mod visual_workflow_builder;
pub(crate) mod voice_command;
pub mod workflow;
pub(crate) mod workflow_adapters;
pub(crate) mod workflow_execution;
pub(crate) mod workflow_package;
pub(crate) mod workflow_registry;
pub(crate) mod workflow_runner;
pub(crate) mod workflow_runtime;
pub(crate) mod workflow_templates;
pub mod workspace;
pub(crate) mod workspace_rag;
pub use core_protocol::*;
mod core_journal;
pub use core_journal::*;
mod core_lifecycle;
pub use core_lifecycle::{
    attach_permission_audit_sink, spawn_ambient_retention, spawn_approval_gc,
    spawn_model_provenance_retention, spawn_receipt_retention,
};
mod core_agent;
pub(crate) use core_agent::*;
pub use core_agent::{
    AgentRunError, ApprovalCoordinator, ModelAgent, RoutingApprovalRegistry, SelectedModel,
    TaskExecutor, ToolAgent,
};
mod core_coordinator;
pub use core_coordinator::TaskCoordinator;
mod bounded_tasks;
mod core_domains;
pub(crate) use core_domains::*;
pub(crate) mod adapter_contract;
pub mod automation;
pub mod automation_acceptance;
pub mod automation_runtime;
pub mod automation_scheduler;
pub mod automation_simulation;
#[cfg(test)]
mod core_tests;
pub mod target_contract;
