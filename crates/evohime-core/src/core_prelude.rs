/// Marker type for the Core's stable public version surface.
pub struct CoreVersion;

/// Надёжный вход событий координатора.
///
/// broadcast подходит только для уведомления подписчиков: медленный
/// подписчик может получить Lagged. Обязательные потребители получают
/// события через bounded mpsc и поэтому оказывают обратное давление на
/// producer до освобождения места в очереди.
#[derive(Clone)]
pub struct EventSink {
    sender: tokio::sync::mpsc::Sender<crate::CoreEvent>,
}

impl EventSink {
    pub(crate) fn new(sender: tokio::sync::mpsc::Sender<crate::CoreEvent>) -> Self {
        Self { sender }
    }

    /// Queues an event for a required Core consumer, applying bounded backpressure.
    ///
    /// Unlike a lossy broadcast notification, this waits until queue capacity
    /// is available or the receiver closes.
    ///
    /// # Errors
    ///
    /// Returns an error when the Core event queue has been closed.
    pub async fn send(&self, event: crate::CoreEvent) -> Result<(), &'static str> {
        let queued_at = std::time::Instant::now();
        let result = self
            .sender
            .send(event)
            .await
            .map_err(|_| "core event queue is closed");
        tracing::debug!(
            queue_wait_ms = queued_at.elapsed().as_secs_f64() * 1000.0,
            "core event queue send completed"
        );
        result
    }

    pub(crate) fn blocking_send(&self, event: crate::CoreEvent) -> Result<(), &'static str> {
        self.sender
            .blocking_send(event)
            .map_err(|_| "core event queue is closed")
    }
}

use crate::recovery;

/// Базовая identity-инструкция, добавляемая к каждому model context.
pub const AGENT_IDENTITY_PROMPT: &str =
    "Ты — Ева, AI-агент приложения EvoHime. Ева — короткое имя EvoHime; понимай обращения к тебе «Ева» и «EvoHime» как к одному агенту.";

/// Канонические имена read-only filesystem-инструментов, используемые в
/// policy preflight и в проверках обязательного исследовательского пути.
pub(crate) const TOOL_FILESYSTEM_LIST: &str = "filesystem_list";
/// Read-only filesystem tool used to inspect file contents.
pub(crate) const TOOL_FILESYSTEM_READ: &str = "filesystem_read";
/// Read-only filesystem tool used to search workspace content.
pub(crate) const TOOL_FILESYSTEM_SEARCH: &str = "filesystem_search";
/// Tools that can perform a required workspace mutation. The delivery gate
/// drops a mutation requirement when none of them is in the active loadout.
pub(crate) const DELIVERY_MUTATION_TOOLS: &[&str] = &["filesystem_write", "filesystem_patch"];
/// Tool that runs the required verification command (tests, build, diff check).
pub(crate) const DELIVERY_VERIFICATION_TOOL: &str = "shell_execute";
/// Tool that records the required commit after a successful verification.
pub(crate) const DELIVERY_COMMIT_TOOL: &str = "git_commit";
/// Рабочий набор агента для режима полного доступа. Каждый id добавляется в
/// loadout только если permission preflight уже разрешил его без approval, так
/// что список не расширяет authority. Полный доступ в композере означает, что
/// модель не должна угадывать, какие её инструменты «разрешены по словам».
pub(crate) const FULL_ACCESS_WORKING_SET: &[&str] = &[
    "filesystem_list",
    "filesystem_read",
    "filesystem_search",
    "filesystem_write",
    "filesystem_patch",
    "filesystem_mkdir",
    "filesystem_stat",
    "shell_execute",
    "script_execute",
    "git_status",
    "git_diff",
    "git_commit",
    "eva_workspace_read_file",
    "eva_workspace_write_file",
    "eva_workspace_run_command",
    "eva_workspace_git",
];

/// Complete self-development tool set offered only after workspace validation
/// and the ordinary permission preflight succeeds.
pub(crate) const SELF_DEVELOPMENT_TOOL_IDS: &[&str] = &[
    "eva_workspace_read_file",
    "eva_workspace_write_file",
    "eva_workspace_run_command",
    "eva_workspace_git",
];

/// Идентификатор встроенной политики разрешений Core.
pub(crate) const PERMISSION_POLICY_ID: &str = "permission-v1";

/// Типизированные параметры операций над architecture snapshot.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArchitectureSnapshotRequest {
    #[serde(default)]
    pub(crate) workspace_root: Option<String>,
    #[serde(default)]
    pub(crate) subject_id: Option<String>,
    #[serde(default)]
    pub(crate) source_revision: Option<String>,
    #[serde(default)]
    pub(crate) allowed_roots: Vec<String>,
    #[serde(default)]
    pub(crate) before: Option<crate::architecture_snapshot::ArchitectureSnapshot>,
    #[serde(default)]
    pub(crate) after: Option<crate::architecture_snapshot::ArchitectureSnapshot>,
    #[serde(default)]
    pub(crate) expected: Option<crate::architecture_snapshot::ExpectedArchitectureDelta>,
    #[serde(default)]
    pub(crate) actual: Option<crate::architecture_snapshot::ArchitectureDelta>,
}
/// Типизированные параметры операций локального model runtime manager.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub(crate) struct LocalModelRuntimeRequest {
    #[serde(default)]
    pub(crate) model_id: Option<String>,
    #[serde(default)]
    pub(crate) request_id: Option<String>,
    #[serde(default)]
    pub(crate) job_id: Option<String>,
    #[serde(default)]
    pub(crate) job_revision: Option<u64>,
    #[serde(default)]
    pub(crate) state: Option<crate::local_model_runtime_manager::ArtifactState>,
    #[serde(default)]
    pub(crate) trust: Option<crate::local_model_runtime_manager::TrustLevel>,
    #[serde(default)]
    pub(crate) observed_hash: Option<String>,
    #[serde(default)]
    pub(crate) expected_hash: Option<String>,
    #[serde(default)]
    pub(crate) staging_relative_path: Option<String>,
    #[serde(default)]
    pub(crate) destination_relative_path: Option<String>,
    #[serde(default)]
    pub(crate) expected_size_bytes: Option<u64>,
    #[serde(default)]
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) from: Option<crate::local_model_runtime_manager::ArtifactState>,
    #[serde(default)]
    pub(crate) to: Option<crate::local_model_runtime_manager::ArtifactState>,
    #[serde(default)]
    pub(crate) session: Option<crate::local_model_runtime_manager::LocalModelRuntimeSession>,
    #[serde(default)]
    pub(crate) model: Option<crate::local_model_runtime_manager::LocalModelDescriptor>,
    #[serde(default)]
    pub(crate) runtime: Option<crate::local_model_runtime_manager::LocalInferenceRuntime>,
    #[serde(default)]
    pub(crate) artifact: Option<crate::local_model_runtime_manager::LocalArtifactRecord>,
    #[serde(default)]
    pub(crate) adaptation: Option<crate::local_model_adaptation::AdaptationRequest>,
    #[serde(default)]
    pub(crate) benchmark_suite: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) benchmark_policy: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) benchmark_baselines: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) expected_output_sha256: Option<String>,
    #[serde(default)]
    pub(crate) expected_benchmark_sha256: Option<String>,
    #[serde(default)]
    pub(crate) base_url: Option<String>,
}

/// Типизированный запрос проверки approval policy.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PolicyDecisionRequest {
    pub(crate) scope_id: String,
    pub(crate) action_class: String,
    pub(crate) resource: String,
    pub(crate) risk: u8,
    #[serde(default)]
    pub(crate) now_ms: i64,
}

/// Типизированный запрос принятия pipeline intent.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcceptIntentRequest {
    pub(crate) intent: crate::architect_editor_model_pipeline::EditIntent,
    #[serde(default)]
    pub(crate) workspace_revision: String,
}

/// Типизированный запрос preflight для командного resource budget.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamBudgetPreflightRequest {
    pub(crate) policy: crate::team_resource_budget::TeamBudgetPolicy,
    pub(crate) state: crate::team_resource_budget::TeamBudgetState,
    pub(crate) estimate: crate::team_resource_budget::ResourceLimits,
    #[serde(default)]
    pub(crate) reserve_access: bool,
    #[serde(default)]
    pub(crate) unknown_cost: bool,
}

/// Типизированный запрос evaluate для composable termination conditions.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminationEvaluateRequest {
    pub(crate) policy: crate::composable_termination_conditions::TerminationPolicy,
    pub(crate) state: crate::composable_termination_conditions::TerminationState,
    pub(crate) event: crate::composable_termination_conditions::TerminationEvent,
}

/// Типизированный запрос сохранения termination state.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminationSaveStateRequest {
    pub(crate) state: crate::composable_termination_conditions::TerminationState,
    pub(crate) run_id: String,
    pub(crate) policy_id: String,
}

/// Типизированный запрос подтверждения или отклонения доставки события.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeliveryRequest {
    pub(crate) subscription_id: String,
    pub(crate) event_id: String,
    #[serde(default = "default_delivery_attempt")]
    pub(crate) attempt: u64,
    #[serde(default)]
    pub(crate) error: Option<String>,
}

pub(crate) fn default_delivery_attempt() -> u64 {
    1
}

/// Полный типизированный запрос benchmark evaluation.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BenchmarkEvaluationInput {
    pub(crate) candidate: crate::workflow_optimization_lab::Candidate,
    #[serde(flatten)]
    pub(crate) request: crate::workflow_optimization_lab::BenchmarkEvaluationRequest,
}

/// Типизированные параметры вызова capability workbench.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkbenchCallRequest {
    pub(crate) capability: String,
    #[serde(default)]
    pub(crate) tool_id: Option<String>,
}

/// Типизированные параметры изменения ресурса workbench.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkbenchResourceRequest {
    pub(crate) resource_id: String,
    pub(crate) available: bool,
}

/// Типизированные параметры снимка workbench; logical_state остаётся
/// расширяемым payload, но его envelope и credential refs проверяются строго.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkbenchSnapshotRequest {
    #[serde(default = "default_workbench_logical_state")]
    pub(crate) logical_state: serde_json::Value,
    #[serde(default)]
    pub(crate) credential_refs: Vec<String>,
}

pub(crate) fn default_workbench_logical_state() -> serde_json::Value {
    serde_json::json!({})
}

/// Типизированные параметры построения представления knowledge collection.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeCollectionViewRequest {
    #[serde(default = "default_project_target_kind")]
    pub(crate) target_kind: crate::knowledge_source_registry_project_role::TargetKind,
    pub(crate) target_id: String,
}

/// Типизированные параметры keyword retrieval по knowledge source.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeQueryRequest {
    pub(crate) query: String,
    #[serde(default = "default_project_target_kind")]
    pub(crate) target_kind: crate::knowledge_source_registry_project_role::TargetKind,
    pub(crate) target_id: String,
}

pub(crate) fn default_project_target_kind(
) -> crate::knowledge_source_registry_project_role::TargetKind {
    crate::knowledge_source_registry_project_role::TargetKind::Project
}

/// Типизированные параметры компиляции project instruction stack.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstructionStackCompileRequest {
    #[serde(default)]
    pub(crate) explicit_ids: Vec<String>,
    #[serde(default)]
    pub(crate) policy: Option<crate::project_instruction_stack::ProjectInstructionStackPolicy>,
}

/// Типизированные параметры включения или отключения project rule.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstructionStackToggleRequest {
    pub(crate) rule_id: String,
    pub(crate) enabled: bool,
}

/// Типизированный envelope команд team coordination policies.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamCoordinationRequest {
    pub(crate) team: crate::team_coordination_policies::TeamSpec,
    #[serde(default)]
    pub(crate) state: Option<crate::team_coordination_policies::TeamCoordinationState>,
    #[serde(default)]
    pub(crate) event_ids: Vec<String>,
    #[serde(default)]
    pub(crate) handoff_from: Option<String>,
    #[serde(default)]
    pub(crate) selector_role: Option<String>,
    #[serde(default)]
    pub(crate) event_type: Option<String>,
    #[serde(default)]
    pub(crate) strategy: Option<crate::team_coordination_policies::TeamCoordinationStrategy>,
    #[serde(default)]
    pub(crate) protocol_snapshot: Option<crate::team_sop_protocols::ProtocolSnapshot>,
    #[serde(default)]
    pub(crate) participant: Option<crate::team_coordination_policies::ParticipantIdentity>,
    #[serde(default)]
    pub(crate) strategy_state: Option<crate::team_coordination_policies::StrategySessionState>,
}

/// Типизированные параметры model edit protocol registry.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelEditRequest {
    #[serde(default)]
    pub(crate) definition: Option<crate::model_edit_protocol_registry::EditProtocolDefinition>,
    #[serde(default)]
    pub(crate) original: Option<String>,
    #[serde(default)]
    pub(crate) error_code: Option<String>,
    #[serde(default)]
    pub(crate) attempt: u8,
}

/// Типизированные параметры remote conversation channels.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteChannelRequest {
    #[serde(default)]
    pub(crate) connection: Option<crate::remote_conversation_channels::ChannelConnection>,
    #[serde(default)]
    pub(crate) code: Option<String>,
    #[serde(default)]
    pub(crate) external_identity: Option<String>,
    #[serde(default)]
    pub(crate) message: Option<crate::remote_conversation_channels::InboundMessage>,
}

/// Типизированный запрос сохранения memory view.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemoryViewSaveRequest {
    pub(crate) view: crate::memory_views_and_adaptive_recall::MemoryView,
}

/// Типизированный запрос adaptive recall.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemoryRecallRequest {
    pub(crate) policy: crate::memory_views_and_adaptive_recall::AdaptiveRecallPolicy,
    pub(crate) mode: crate::memory_views_and_adaptive_recall::RecallMode,
    #[serde(default = "default_query_complexity")]
    pub(crate) complexity: crate::memory_views_and_adaptive_recall::QueryComplexity,
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) scope_id: Option<String>,
    pub(crate) read_barrier_generation: u64,
    #[serde(default)]
    pub(crate) candidates: Vec<crate::memory_views_and_adaptive_recall::RecallCandidate>,
}

pub(crate) fn default_query_complexity() -> crate::memory_views_and_adaptive_recall::QueryComplexity
{
    crate::memory_views_and_adaptive_recall::QueryComplexity::Unknown
}

/// Типизированные параметры prompt cache planner.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptCacheRequest {
    #[serde(default)]
    pub(crate) segments: Vec<crate::prompt_cache_planner::PromptSegment>,
    #[serde(default)]
    pub(crate) profile: Option<crate::prompt_cache_planner::ProviderCacheProfile>,
    #[serde(default)]
    pub(crate) context_revision: String,
    #[serde(default)]
    pub(crate) policy_version: String,
    #[serde(default)]
    pub(crate) keepalive_ms: i64,
    #[serde(default)]
    pub(crate) metric: Option<crate::prompt_cache_planner::CacheMetric>,
}

/// Типизированные параметры declarative runtime component.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeclarativeComponentRequest {
    #[serde(default)]
    pub(crate) config: Option<crate::declarative_runtime_components::ComponentConfig>,
    #[serde(default)]
    pub(crate) registry: Option<crate::declarative_agent_component_registry::Registry>,
    #[serde(default)]
    pub(crate) policy: Option<crate::declarative_runtime_components::PolicySnapshot>,
    #[serde(default)]
    pub(crate) state: Option<crate::declarative_runtime_components::RuntimeState>,
}

/// Типизированные параметры guided calibration sessions.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalibrationRequest {
    #[serde(default)]
    pub(crate) owner_scope: Option<String>,
    #[serde(default)]
    pub(crate) subject_ref: Option<String>,
    #[serde(default)]
    pub(crate) actor_ref: Option<String>,
    #[serde(default)]
    pub(crate) policy_snapshot_hash: Option<String>,
    #[serde(default)]
    pub(crate) iteration: Option<crate::guided_calibration_sessions::CalibrationIteration>,
    #[serde(default)]
    pub(crate) pattern_key: Option<String>,
    #[serde(default)]
    pub(crate) guidance_text: Option<String>,
    #[serde(default)]
    pub(crate) candidate_id: Option<String>,
    #[serde(default)]
    pub(crate) cancelled: bool,
}

/// Типизированный запрос предложения team assignment.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssignmentProposalRequest {
    #[serde(default)]
    pub(crate) item: Option<crate::team_coordinator::TeamWorkItem>,
    #[serde(default)]
    pub(crate) candidates: Vec<crate::team_coordinator::ParticipantCandidate>,
    #[serde(default)]
    pub(crate) termination: Option<AssignmentTerminationRequest>,
}

/// Типизированные termination gates для assignment proposal.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssignmentTerminationRequest {
    pub(crate) policy: crate::composable_termination_conditions::TerminationPolicy,
    pub(crate) state: crate::composable_termination_conditions::TerminationState,
    pub(crate) event: crate::composable_termination_conditions::TerminationEvent,
}

/// Типизированный запрос submit для durable remote task bridge.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteTaskSubmitRequest {
    pub(crate) toolset: evohime_remote::durable_remote_task_bridge::RemoteTaskToolset,
    #[serde(default)]
    pub(crate) request: serde_json::Value,
    #[serde(default = "default_remote_provenance")]
    pub(crate) provenance_ref: String,
    #[serde(default)]
    pub(crate) operation: String,
}

pub(crate) fn default_remote_provenance() -> String {
    "core".into()
}

/// Типизированный запрос poll для durable remote task bridge.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteTaskPollRequest {
    #[serde(default = "default_remote_provenance")]
    pub(crate) lease_owner: String,
}

/// Типизированный запрос result для durable remote task bridge.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteTaskResultRequest {
    pub(crate) status: evohime_remote::durable_remote_task_bridge::RemoteTaskStatus,
    #[serde(default = "default_remote_transport_status")]
    pub(crate) transport_status: String,
    #[serde(default)]
    pub(crate) result_artifact_ref: Option<String>,
}

pub(crate) fn default_remote_transport_status() -> String {
    "reported".into()
}

/// Типизированные параметры назначения team work item.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssignmentRequest {
    pub(crate) item: crate::team_coordinator::TeamWorkItem,
    pub(crate) proposal: crate::team_coordinator::DelegationProposal,
    pub(crate) candidate: crate::team_coordinator::ParticipantCandidate,
}

/// Типизированные параметры привязки workspace set к task.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceSetBindingRequest {
    pub(crate) task_id: String,
    #[serde(default)]
    pub(crate) root_ids: Vec<String>,
}

/// Типизированные параметры оценки message intervention policy.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InterventionRequest {
    pub(crate) policy: crate::message_intervention_policies::MessageInterventionPolicy,
    pub(crate) context: crate::message_intervention_policies::MessageInterventionContext,
    #[serde(default)]
    pub(crate) seen: bool,
}

/// Типизированный запрос построения redacted bridge projection.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BridgeProjectionRequest {
    pub(crate) binding: crate::conversation_bridge_adapters::ThreadBinding,
    #[serde(default = "default_bridge_projection_kind")]
    pub(crate) kind: String,
    #[serde(default = "default_bridge_projection_status")]
    pub(crate) status: String,
    #[serde(default = "default_bridge_projection_provenance")]
    pub(crate) provenance_id: String,
}

pub(crate) fn default_bridge_projection_kind() -> String {
    "status".into()
}
pub(crate) fn default_bridge_projection_status() -> String {
    "unknown".into()
}
pub(crate) fn default_bridge_projection_provenance() -> String {
    "event".into()
}

/// Аргументы для policy-only preflight при построении каталога инструментов.
/// Preflight не выполняет инструмент, но path-инструментам всё равно нужен
/// существующий workspace-relative путь, иначе безопасный инструмент ошибочно
/// выпадает из authorized snapshot.
pub(crate) fn catalog_preflight_input(tool_name: &str) -> serde_json::Value {
    match tool_name {
        TOOL_FILESYSTEM_READ | TOOL_FILESYSTEM_LIST => serde_json::json!({ "path": "." }),
        TOOL_FILESYSTEM_SEARCH => serde_json::json!({
            "query": "EvoHime",
            "path": "."
        }),
        "filesystem_write" => serde_json::json!({
            "path": ".evohime-catalog-probe",
            "content": ""
        }),
        "filesystem_patch" => serde_json::json!({
            // Preflight resolves the policy subject but does not apply a patch.
            // Use the existing workspace root so the tool stays in the catalog
            // even when the workspace has no files yet.
            "path": ".",
            "patch": "--- a/.\n+++ b/.\n@@\n",
            "expected_hash": "0000000000000000000000000000000000000000000000000000000000000000"
        }),
        // Probe input must satisfy the declared `analysis_run` schema, otherwise
        // the side-effect-free kernel tool drops out of the authorized snapshot.
        "analysis_run" => serde_json::json!({
            "operation": "json_parse",
            "args": "{}"
        }),
        "workflow_start" => serde_json::json!({ "template_id": "probe", "inputs": {} }),
        "workflow_status" | "workflow_cancel" => serde_json::json!({ "run_id": "probe" }),
        "eva_workspace_read_file" => serde_json::json!({ "path": "Cargo.toml" }),
        "eva_workspace_write_file" => serde_json::json!({
            "path": ".evohime-catalog-probe",
            "content": ""
        }),
        "eva_workspace_run_command" => serde_json::json!({ "command": "cargo test --lib" }),
        "eva_workspace_git" => serde_json::json!({ "action": "status" }),
        // Probe input matches the declared `script_execute` schema; preflight
        // resolves policy only and never starts the interpreter.
        "script_execute" => serde_json::json!({
            "interpreter": "powershell",
            "script": "."
        }),
        _ => serde_json::json!({}),
    }
}

pub(crate) fn requires_workspace_research_catalog(prompt: &str) -> bool {
    let prompt_lower = prompt.to_lowercase();
    let explicit = [
        "изучи",
        "исследуй",
        "ознаком",
        "проверь проект",
        "найди в проекте",
        "объясни проект",
        "understand the project",
        "inspect the project",
    ]
    .iter()
    .any(|marker| prompt_lower.contains(marker));
    // Intent router и catalog query должны согласованно считать research
    // read-only исследованием: иначе модель получает research-intent без
    // filesystem_list/search и вынуждена вызывать недоступные инструменты.
    explicit
        || evohime_context_budget::loadout::route_intent(
            &crate::context_budget::default_intent_rules(),
            prompt,
            &[],
        )
        .intent
            == "research"
}

pub(crate) fn request_allows_mutation(prompt: &str) -> bool {
    evohime_context_budget::loadout::route_intent(
        &crate::context_budget::default_intent_rules(),
        prompt,
        &[],
    )
    .allows_mutation
}

/// Ограниченный workflow-набор, который модель получает при workflow-intent.
///
/// Каталог и обязательные инструменты агентского цикла используют один и тот
/// же список: расхождение отнимает у модели `workflow.*` ровно тогда, когда
/// задача без них невыполнима.
pub(crate) const WORKFLOW_TOOL_IDS: [&str; 4] = [
    "workflow_catalog",
    "workflow_start",
    "workflow_status",
    "workflow_cancel",
];

/// Определяет workflow-intent запроса по устойчивым маркерам.
///
/// Хелпер общий для `adaptive_catalog_query` и обязательного набора
/// инструментов, чтобы лексический top-N не расходился с intent-роутингом.
pub(crate) fn request_mentions_workflow(prompt: &str) -> bool {
    let prompt = prompt.to_lowercase();
    ["workflow", "составн", "сценар", "шаблон", "отмен", "статус"]
        .iter()
        .any(|marker| prompt.contains(marker))
}

/// Определяет запросы, где модели пригодится запуск скриптов.
///
/// Маркеры широкие намеренно: лишний инструмент в каталоге безвреден,
/// а его отсутствие ломает задачу.
pub(crate) fn request_mentions_scripts(prompt: &str) -> bool {
    let prompt = prompt.to_lowercase();
    [
        "скрипт",
        "script",
        ".ps1",
        "powershell",
        "pwsh",
        "python",
        ".py",
        "node",
    ]
    .iter()
    .any(|marker| prompt.contains(marker))
}

pub(crate) fn adaptive_catalog_query(prompt: &str, mutation_intent: bool) -> String {
    let mut query = prompt.to_owned();
    if requires_workspace_research_catalog(prompt) {
        query.push_str(" filesystem_list filesystem_read filesystem_search");
    }
    if request_mentions_workflow(prompt) {
        query.push(' ');
        query.push_str(&WORKFLOW_TOOL_IDS.join(" "));
    }
    if request_mentions_scripts(prompt) {
        query.push_str(" script_execute");
    }
    if mutation_intent {
        query.push_str(
            " filesystem_read filesystem_write filesystem_patch filesystem_mkdir shell_execute",
        );
    }
    query
}

/// Входные данные model-side MCP вызова. Разбор контракта происходит до
/// обращения к registry, поэтому неизвестные или запрещённые поля не
/// протекают дальше как неструктурированный JSON.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelMcpCallInput {
    pub(crate) server_id: String,
    pub(crate) tool_name: String,
    #[serde(default)]
    pub(crate) params: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) timeout_ms: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) url: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct ResolvedMcpInput {
    pub(crate) url: String,
    pub(crate) method: String,
    pub(crate) params: serde_json::Value,
    pub(crate) timeout_ms: serde_json::Value,
}

pub(crate) fn model_is_waiting_instead_of_reporting(content: &str) -> bool {
    let content = content.to_lowercase();
    [
        "жду результата",
        "подожди результаты",
        "что ты хочешь",
        "уточни, пожалуйста",
        "ожидаю результата",
    ]
    .iter()
    .any(|marker| content.contains(marker))
}

pub(crate) fn build_agent_system_prompt(tool_names: &[String]) -> String {
    let has = |name: &str| tool_names.iter().any(|tool| tool == name);
    let is_queen = has(evohime_tool_runtime::agent::HIVE_DELEGATE_NAME);
    let mut rules: Vec<String> = Vec::new();
    if !is_queen {
        rules.push(
            "- Выполняй задачу самостоятельно и используй инструменты, когда они нужны для фактической проверки.".into(),
        );
    }
    rules.push(
        "- Используй только инструменты из списка ниже. Если подходящего инструмента в списке нет, не вызывай его: выполни задачу доступными средствами и явно сообщи об ограничении.".into(),
    );
    rules.push(
        "- Если пользователь не сформулировал конкретное поручение, не исследуй workspace и не имитируй выполненную работу: задай один короткий уточняющий вопрос и дождись задачи.".into(),
    );
    rules.push(
        "- За один ответ вызывай только один инструмент и жди его результата перед следующим вызовом.".into(),
    );
    if is_queen {
        rules.push(
            "Ты — Королева Улья Ева, чистый маршрутизатор и оркестратор. Сама ты НИЧЕГО не делаешь: не читаешь и не правишь файлы, не запускаешь команды, не работаешь с git и браузером. Все действия выполняют только твои агенты через `hive_delegate_agent` (`researcher`, `planner`, `architect`, `implementer`, `tester`, `reviewer`, `releaser`). При каждом вызове `hive_delegate_agent` задавай короткое понятное имя агента на русском языке в поле `agent_name` (например: `Исследователь ядра`, `Разработчик Улья`, `Тестировщик IPC`). Стандартный конвейер для нетривиальной задачи: `researcher` -> `planner` -> `implementer` (обязательно передавай понятный `plan_summary`, чтобы пользователь подтвердил план перед первым изменением) -> `tester` -> `reviewer` -> `releaser` (если нужен коммит). Если для задачи нужна новая специализированная роль и сектор Улья, сначала зарегистрируй её через `hive_register_role`.".into(),
        );
    }
    if has("filesystem_list") {
        rules.push(
            "- Если пользователь просит изучить, проверить, найти или объяснить проект, сначала вызови filesystem_list с path точкой (.).".into(),
        );
    }
    if has("filesystem_read") || has("filesystem_search") {
        let reader = if has("filesystem_read") {
            "прочитай подходящие manifest-файлы и документацию (например Cargo.toml, package.json, README и архитектурные документы)"
        } else {
            "прочитай подходящие файлы доступным инструментом"
        };
        let search = if has("filesystem_search") {
            ", а для поиска по коду используй filesystem_search"
        } else {
            ""
        };
        rules.push(format!("- Затем {reader}{search}."));
    }
    if has("shell_execute") && (has("filesystem_list") || has("filesystem_search")) {
        rules.push(
            "- Для изучения проекта не используй shell_execute: read-only filesystem-инструменты безопаснее и достаточно информативны.".into(),
        );
    }
    rules.push("- Не проси пользователя прислать структуру проекта, путь или команды, если workspace уже указан.".into());
    rules.push("- Не утверждай, что изучила файл или выполнила действие, пока соответствующий инструмент не вернул результат.".into());
    rules.push("- Для чтения файлов и git-истории в указанном проекте подтверждение пользователя не требуется — свободно изучай проект read-only инструментами. Перед первым изменением или опасным действием в том же ответе перед вызовом изменяющего инструмента коротко и понятно опиши пользователю задачу, для чего нужно разрешение, что и в каких файлах планируешь изменить и как проверишь; после подтверждения выполняй оставшиеся шаги плана без повторных запросов.".into());
    let mut examples: Vec<String> = Vec::new();
    if has("filesystem_list") {
        examples.push("filesystem_list — {\"path\":\".\"}".into());
    }
    if has("filesystem_read") {
        examples.push("filesystem_read — {\"path\":\"README.md\"}".into());
    }
    if has("filesystem_search") {
        examples.push("filesystem_search — {\"query\":\"pattern\",\"path\":\".\"}".into());
    }
    if examples.is_empty() {
        rules.push(
            "- Аргументы инструментов всегда передавай полным JSON-объектом по схеме инструмента; не вызывай инструмент с пустым объектом.".into(),
        );
    } else {
        rules.push(format!(
            "- Аргументы инструментов всегда передавай полным JSON-объектом: {}; не вызывай их с пустым объектом и не передавай абсолютный путь вместо workspace-relative path.",
            examples.join("; ")
        ));
    }
    if has("filesystem_read") {
        rules.push(
            "- Результат успешного filesystem_read содержит content_hash и готовый к копированию expected_hash — это один и тот же точный хеш файла. При изменении существующего файла передавай expected_hash из последнего чтения этого файла; Core подставит его, если ты пропустишь поле, но только для того же логического пути. Никогда не выдумывай и не пересчитывай хеш.".into(),
        );
    }
    if has("goal_create") {
        rules.push(
            "- Если задача многоэтапная или пользователь просит поставить цель, создавай её через goal_create только в текущем чате (понятные objective и success_criteria); не создавай постоянные workspace-цели. Если активного чата нет, сообщи об этом; каждый критерий подтверждай через goal_verify только после фактической проверки.".into(),
        );
    }
    if has("workflow_catalog") {
        rules.push(
            "- Если пользователь просит запустить или проверить составной сценарий, сначала вызови workflow_catalog и выбирай только шаблоны с available=true. Если шаблон недоступен, сообщи указанный unavailable_reason; не обещай запуск и не подменяй сценарий ручным запуском узлов.".into(),
        );
        if has("workflow_start") {
            rules.push(
                "- Запускай только фиксированный шаблон через workflow_start; не создавай и не редактируй графы. Для продолжения используй run_id из ответа, workflow_status или workflow_cancel.".into(),
            );
        }
    }
    rules.push("- Если инструмент вернул ошибку, не завершай задачу: исправь аргументы по тексту ошибки и повтори вызов.".into());
    if has("filesystem_patch") {
        rules.push(
            "- filesystem_patch принимает только JSON с path, patch и expected_hash, например {\"path\":\"relative/file.rs\",\"expected_hash\":\"точный content_hash из filesystem_read\",\"patch\":\"--- a/relative/file.rs\\n+++ b/relative/file.rs\\n@@ ...\"}; поля edits и patches недопустимы.".into(),
        );
    }
    rules.push(
        "- Если вызов инструмента отклонён, сверь аргументы с его JSON-схемой: инструменту без аргументов (git_status) отправляй ровно {}, не оборачивай поля в input, не добавляй чужие поля (reference у git_diff) и не повторяй тот же мутирующий вызов. Полный разбор форматов — docs/agent-tool-playbook.md в исходниках проекта.".into(),
    );
    let mutation_tools: Vec<&str> = ["filesystem_patch", "filesystem_write", "git_commit"]
        .into_iter()
        .filter(|name| has(name))
        .collect();
    if mutation_tools.is_empty() {
        rules.push(
            "- Если пользователь просит исправить, изменить или реализовать код, а mutation-инструментов в сессии нет, не выдавай план за результат: выполни доступные read-only проверки и сообщи, что для изменения нужен режим доступа с правом записи.".into(),
        );
    } else {
        rules.push(format!(
            "- Если пользователь просит исправить, изменить или реализовать код, обязательно внеси изменение через {}; для просьбы о проверке запусти соответствующий тест/сборку, а для просьбы о commit вызови git_commit. Не выдавай план за результат.",
            mutation_tools.join(" или ")
        ));
    }
    if has("git_status")
        || has("git_diff")
        || has("git_commit")
        || has("git_pull")
        || has("git_push")
    {
        let mut git_rules = Vec::new();
        if has("git_status") {
            git_rules.push("сначала git_status");
        }
        if has("git_diff") {
            git_rules.push("затем git_diff для проверки изменений");
        }
        if has("git_commit") {
            git_rules.push("git_commit — только с task-only сообщением, явным списком paths текущей задачи (или уже staged изменениями) и после успешной проверки");
        }
        if has("git_pull") {
            git_rules.push("git_pull — для обновления checkout");
        }
        if has("git_push") {
            git_rules
                .push("git_push — только если пользователь явно попросил опубликовать изменения");
        }
        rules.push(format!(
            "- Для Git используй специализированные инструменты: {}. Не заменяй эти вызовы shell_execute и не выполняй git-команды из неправильной директории.",
            git_rules.join("; ")
        ));
    } else {
        rules.push(
            "- Git-инструментов в этой сессии нет: не вызывай git.* и не выполняй git-команды через shell_execute.".into(),
        );
    }
    rules.push("- Все улучшения EvoHime выполняй самостоятельно через доступные инструменты Евы. Если попытка не удалась, не завершай задачу: зафиксируй причину в trace, дождись следующего шага настройки и повтори работу через инструменты до успешного результата.".into());
    rules.push("- После исследования дай отчёт: что обнаружено, какие файлы проверены, какие проблемы найдены и что предлагается сделать дальше.".into());

    format!(
        "{AGENT_IDENTITY_PROMPT}\n\n\
Ты работаешь автономно внутри уже выбранного рабочего пространства.\n\
Корень workspace уже выбран и доступен инструментам; не проси пользователя сообщать его повторно.\n\n\
Правила выполнения:\n{}\n\n\
Доступные инструменты в этой сессии:\n{}",
        rules.join("\n"),
        tool_names
            .iter()
            .map(|name| format!("- {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

pub(crate) fn resolve_model_mcp_input(
    registry: &crate::workflow_registry::WorkflowRegistry,
    input: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let input: ModelMcpCallInput = serde_json::from_value(input)
        .map_err(|error| format!("mcp_call invalid input: {error}"))?;
    if input.url.is_some() {
        return Err("mcp_call model input cannot contain url".into());
    }
    let endpoint = registry
        .resolve_mcp_call(&input.server_id, &input.tool_name)
        .map_err(|error| format!("mcp identity rejected: {}", error.code()))?;
    serde_json::to_value(ResolvedMcpInput {
        url: endpoint,
        method: input.tool_name,
        params: input.params.unwrap_or(serde_json::Value::Null),
        timeout_ms: input.timeout_ms.unwrap_or(serde_json::Value::Null),
    })
    .map_err(|error| format!("mcp_call output serialization failed: {error}"))
}

/// Default whole-task time limit in seconds.
///
/// `0` disables the overall task deadline so autonomous tasks are never cut off
/// by wall-clock time; set `EVOHIME_TASK_TIMEOUT_SECONDS` to a positive value
/// to enforce a deadline explicitly.
pub const DEFAULT_TASK_TIMEOUT_SECONDS: u64 = 0;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DeliveryRequirements {
    pub(crate) research: bool,
    pub(crate) mutation: bool,
    pub(crate) verification: bool,
    pub(crate) diff_check: bool,
    pub(crate) commit: bool,
    /// Prompt named an explicit test/build command, so verification is a real
    /// delivery requirement even when the task does not change anything.
    pub(crate) verification_command: bool,
}

impl DeliveryRequirements {
    pub(crate) fn from_prompt(prompt: &str) -> Self {
        let prompt = prompt.to_lowercase();
        Self {
            research: contains_unnegated_marker(
                &prompt,
                &["изучи", "исслед", "ознаком", "найди", "объясни"],
            ),
            mutation: contains_unnegated_marker(
                &prompt,
                &[
                    "исправ",
                    "измен",
                    "добав",
                    "созда",
                    "напиш",
                    "сгенер",
                    "реализ",
                    "сделай",
                    "улучш",
                    "удал",
                    "убер",
                ],
            ),
            verification: contains_unnegated_marker(
                &prompt,
                &["проверь", "провер", "тест", "test", "собери", "запусти"],
            ),
            verification_command: contains_unnegated_marker(
                &prompt,
                &[
                    "тест",
                    "test",
                    "собери",
                    "сборк",
                    "прогон",
                    "cargo",
                    "npm",
                    "pytest",
                    "gradle",
                    "dotnet",
                    "ctest",
                    "git diff --check",
                ],
            ),
            diff_check: prompt.contains("git diff --check"),
            commit: prompt.contains("коммит") || prompt.contains("commit"),
        }
    }

    pub(crate) fn missing(
        self,
        research_done: bool,
        mutation_done: bool,
        verification_done: bool,
        commit_done: bool,
    ) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.research && !research_done {
            missing.push("изучить workspace и подготовить отчёт");
        }
        if self.mutation && !mutation_done {
            missing.push("внести изменение");
        }
        if self.verification && !verification_done {
            missing.push("проверить результат");
        }
        if self.commit && !commit_done {
            missing.push("создать commit");
        }
        missing
    }

    /// Drops delivery requirements that the active loadout cannot satisfy at all.
    ///
    /// Core never fails a task because a tool is missing: the model keeps the
    /// session it was granted and answers with what it can actually do. The
    /// returned requirement names go to the bounded trace, not to the user.
    pub(crate) fn degrade_unavailable(&mut self, tool_names: &[String]) -> Vec<&'static str> {
        let available = |id: &str| tool_names.iter().any(|name| name == id);
        let mut dropped = Vec::new();
        if self.mutation && !DELIVERY_MUTATION_TOOLS.iter().any(|id| available(id)) {
            self.mutation = false;
            dropped.push("mutation");
        }
        // A commit is only allowed after verification, so it needs both tools.
        if self.commit
            && (!available(DELIVERY_COMMIT_TOOL) || !available(DELIVERY_VERIFICATION_TOOL))
        {
            self.commit = false;
            self.diff_check = false;
            dropped.push("commit");
        }
        if self.verification && !available(DELIVERY_VERIFICATION_TOOL) {
            self.verification = false;
            self.diff_check = false;
            dropped.push("verification");
        }
        dropped
    }
}

/// Tools that must stay in the model loadout so the delivery gate can be
/// satisfied. The caller only admits ids that are already authorized by
/// permission preflight; this list never widens authority.
pub(crate) fn delivery_required_tool_ids(requirements: DeliveryRequirements) -> Vec<&'static str> {
    let mut ids = Vec::new();
    if requirements.mutation {
        ids.extend_from_slice(DELIVERY_MUTATION_TOOLS);
    }
    if requirements.verification || requirements.commit {
        ids.push(DELIVERY_VERIFICATION_TOOL);
    }
    if requirements.commit {
        ids.push(DELIVERY_COMMIT_TOOL);
    }
    ids
}

/// Изменения, которые согласование требований с intent router внесло в набор
/// требований. Флаги нужны для bounded диагностики, а не для решения.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeliveryReconciliation {
    /// Требование изменения снято: router не разрешил mutation intent.
    pub(crate) mutation_skipped: bool,
    /// Проверка добавлена как предусловие обязательного commit.
    pub(crate) verification_promoted: bool,
    /// Проверка снята: это осмотр без изменения и без явной команды тестов.
    pub(crate) verification_skipped_inspection: bool,
}

/// Согласует требования поставки с решением intent router.
///
/// Read-only loadout отклоняет write-вызов как policy denial, поэтому
/// требование изменения без mutation intent превращало бы задачу в
/// недостижимый gate. Обязательный commit, наоборот, всегда требует проверки:
/// `git_commit` блокируется до успешного verification. Проверка без изменения
/// и без явной команды тестов — это обычный исследовательский запрос «проверь»,
/// а не обязательство прогнать сборку.
pub(crate) fn reconcile_delivery_requirements(
    requirements: &mut DeliveryRequirements,
    router_mutation: bool,
) -> DeliveryReconciliation {
    let mut result = DeliveryReconciliation::default();
    if requirements.commit && !requirements.verification {
        requirements.verification = true;
        result.verification_promoted = true;
    }
    if requirements.mutation && !router_mutation {
        requirements.mutation = false;
        result.mutation_skipped = true;
    }
    if requirements.verification
        && !requirements.mutation
        && !requirements.commit
        && !requirements.verification_command
    {
        requirements.verification = false;
        requirements.diff_check = false;
        result.verification_skipped_inspection = true;
    }
    result
}

fn contains_unnegated_marker(prompt: &str, markers: &[&str]) -> bool {
    markers
        .iter()
        .any(|marker| evohime_context_budget::loadout::contains_unnegated_keyword(prompt, marker))
}

pub(crate) fn strict_delivery_gate_enabled() -> bool {
    std::env::var("EVOHIME_DELIVERY_GATE_STRICT")
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off"
            )
        })
        .unwrap_or(true)
}

/// Returns `(verification_check, diff_check)` where `None` means that the
/// direct invocation is unrelated to that gate. The result is based on the
/// actual resolved program/arguments and the structured exit status.
pub(crate) fn classify_shell_verification(
    arguments: &str,
    outcome: &recovery::ToolOutcome,
) -> (Option<bool>, Option<bool>) {
    let input =
        serde_json::from_str::<serde_json::Value>(arguments).unwrap_or(serde_json::Value::Null);
    let Some((program, args, _cwd)) = evohime_tool_runtime::shell::resolve_invocation(&input)
    else {
        return (None, None);
    };
    let program = normalized_program_name(&program);
    let args = args
        .iter()
        .map(|arg| arg.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let status_ok = outcome.ok
        && outcome
            .structured
            .get("timed_out")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        && outcome
            .structured
            .get("exit_code")
            .and_then(serde_json::Value::as_i64)
            == Some(0);
    let diff_check = program == "git"
        && args.first().map(String::as_str) == Some("diff")
        && args.iter().any(|arg| arg == "--check");
    let verification = match program.as_str() {
        "cargo" => args
            .first()
            .is_some_and(|arg| matches!(arg.as_str(), "test" | "check" | "build" | "clippy")),
        "dotnet" => args
            .first()
            .is_some_and(|arg| matches!(arg.as_str(), "test" | "build")),
        "ctest" | "pytest" => true,
        "npm" | "pnpm" | "yarn" => match args.first().map(String::as_str) {
            Some("test") => true,
            Some("run") => args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "test" | "build")),
            _ => false,
        },
        "mvn" | "mvnw" | "gradle" | "gradlew" | "go" => args.first().is_some_and(|arg| {
            matches!(
                arg.as_str(),
                "test" | "verify" | "package" | "check" | "build" | "assemble"
            ) || arg.starts_with("test")
        }),
        "python" | "python3" => args
            .windows(2)
            .any(|window| window[0] == "-m" && window[1] == "pytest"),
        _ => false,
    };
    (
        verification.then_some(status_ok),
        diff_check.then_some(status_ok),
    )
}

/// Classifies verification outcomes across both `shell_execute` and structured
/// verification tools (`script_execute`, `cargo_test`, `cargo_check`,
/// `cargo_clippy`, `cargo_build`, `cargo_fmt`).
pub(crate) fn classify_tool_verification(
    tool_name: &str,
    arguments: &str,
    outcome: &recovery::ToolOutcome,
) -> (Option<bool>, Option<bool>) {
    match tool_name {
        "shell_execute" => classify_shell_verification(arguments, outcome),
        "script_execute" => {
            let input = serde_json::from_str::<serde_json::Value>(arguments)
                .unwrap_or(serde_json::Value::Null);
            let script = input
                .get("script")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let is_verification_script = [
                "test", "check", "verify", "lint", "validate", "gate", "audit",
            ]
            .iter()
            .any(|marker| script.contains(marker));
            if !is_verification_script {
                return (None, None);
            }
            let status_ok = outcome.ok
                && outcome
                    .structured
                    .get("timed_out")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                && outcome
                    .structured
                    .get("exit_code")
                    .and_then(serde_json::Value::as_i64)
                    == Some(0);
            (Some(status_ok), None)
        }
        "cargo_test" | "cargo_check" | "cargo_clippy" | "cargo_build" | "cargo_fmt" => {
            let status_ok = outcome.ok
                && outcome
                    .structured
                    .get("timed_out")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                && outcome
                    .structured
                    .get("exit_code")
                    .and_then(serde_json::Value::as_i64)
                    .is_none_or(|code| code == 0);
            (Some(status_ok), None)
        }
        _ => (None, None),
    }
}

/// Program basename without a Windows executable suffix, lowercased, so
/// `.\gradlew.bat`, `C:\repo\gradlew.bat` and `gradlew` classify the same way.
fn normalized_program_name(program: &str) -> String {
    program
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(program)
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd")
        .trim_end_matches(".bat")
        .to_ascii_lowercase()
}

// Аргументы — признаки выполненных требований поставки, по одному булеву на требование.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DeliveryProgress {
    pub(crate) research_done: bool,
    pub(crate) mutation_done: bool,
    pub(crate) verification_done: bool,
    pub(crate) commit_done: bool,
    pub(crate) research_observations: usize,
    pub(crate) research_has_overview: bool,
    pub(crate) research_has_content: bool,
    pub(crate) research_has_search: bool,
}

pub(crate) fn delivery_next_step(
    requirements: DeliveryRequirements,
    progress: DeliveryProgress,
) -> &'static str {
    delivery_next_step_for_agent(requirements, progress, false)
}

pub(crate) fn delivery_next_step_for_agent(
    requirements: DeliveryRequirements,
    progress: DeliveryProgress,
    is_hive_queen: bool,
) -> &'static str {
    if is_hive_queen {
        if requirements.research && !progress.research_done {
            return "НЕМЕДЛЕННО делегируй исследование через hive_delegate_agent с ролью \"researcher\" и русским именем в agent_name. Не пиши отчёт.";
        }
        if !progress.mutation_done && requirements.mutation {
            return "НЕМЕДЛЕННО делегируй внесение изменений через hive_delegate_agent с ролью \"implementer\" (передай plan_summary и русское имя в agent_name). Не пиши отчёт.";
        }
        if !progress.verification_done && requirements.verification {
            return "НЕМЕДЛЕННО делегируй проверку и тесты через hive_delegate_agent с ролью \"tester\" (передай plan_summary и русское имя в agent_name). Не пиши отчёт.";
        }
        if !progress.commit_done && requirements.commit {
            return "НЕМЕДЛЕННО делегируй создание коммита через hive_delegate_agent с ролью \"releaser\" (передай plan_summary и русское имя в agent_name). Не пиши отчёт.";
        }
        return "НЕМЕДЛЕННО продолжи координацию через hive_delegate_agent или заверши задачу итоговым отчётом.";
    }
    if requirements.research && !progress.research_done {
        if !progress.research_has_overview {
            "НЕМЕДЛЕННО вызови read-only filesystem_list с полным JSON {\"path\":\".\"}. Не пиши отчёт."
        } else if !progress.research_has_content {
            "НЕМЕДЛЕННО прочитай один из ключевых файлов: filesystem_read с JSON {\"path\":\"Cargo.toml\"} или {\"path\":\"README.md\"}. Не повторяй filesystem_list и не пиши отчёт."
        } else if !progress.research_has_search {
            "НЕМЕДЛЕННО вызови filesystem_search с полным JSON {\"query\":\"TODO\",\"path\":\".\"} или найди по коду ключевой компонент. Не используй предположения о структуре вроде crates; путь должен существовать в текущем workspace. Не повторяй уже выполненное чтение и не пиши отчёт."
        } else if progress.research_observations < 3 {
            "НЕМЕДЛЕННО прочитай ещё один конкретный архитектурный файл через filesystem_read, например docs/architecture.md или docs/current-state.md. Не пиши отчёт."
        } else {
            "НЕМЕДЛЕННО подготовь итоговый отчёт по уже собранным данным. Не вызывай инструменты."
        }
    } else if !progress.mutation_done && requirements.mutation {
        "НЕМЕДЛЕННО внеси требуемое изменение через filesystem_patch или filesystem_write (если для expected_hash или уточнения текста нужно сначала прочитать целевой файл через filesystem_read — прочитай его и сразу внеси правку). Не пиши отчёт и не задавай вопросов."
    } else if !progress.verification_done && requirements.verification {
        "НЕМЕДЛЕННО выполни проверку: для прямых команд (например git diff --check или cargo) используй shell_execute с полным JSON-объектом, а для скриптов проверки (.ps1/.py/.js) — script_execute. Не запускай интерпретаторы через shell_execute и не пиши отчёт."
    } else if !progress.commit_done && requirements.commit {
        "НЕМЕДЛЕННО вызови git_commit с task-only сообщением и явным списком paths текущей задачи. Не пиши отчёт."
    } else {
        "НЕМЕДЛЕННО вызови следующий нужный read-only инструмент с полным JSON и продолжи исследование. Не пиши отчёт."
    }
}
