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
pub(crate) const TOOL_FILESYSTEM_LIST: &str = "filesystem.list";
/// Read-only filesystem tool used to inspect file contents.
pub(crate) const TOOL_FILESYSTEM_READ: &str = "filesystem.read";
/// Read-only filesystem tool used to search workspace content.
pub(crate) const TOOL_FILESYSTEM_SEARCH: &str = "filesystem.search";

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
    pub(crate) toolset: crate::durable_remote_task_bridge::RemoteTaskToolset,
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
    pub(crate) status: crate::durable_remote_task_bridge::RemoteTaskStatus,
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
        "filesystem.write" => serde_json::json!({
            "path": ".evohime-catalog-probe",
            "content": ""
        }),
        "filesystem.patch" => serde_json::json!({
            "path": ".evohime-catalog-probe",
            "patch": "--- a/.evohime-catalog-probe\n+++ b/.evohime-catalog-probe\n@@\n"
        }),
        _ => serde_json::json!({}),
    }
}

pub(crate) fn requires_workspace_research_catalog(prompt: &str) -> bool {
    let prompt = prompt.to_lowercase();
    [
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
    .any(|marker| prompt.contains(marker))
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
    format!(
        "{AGENT_IDENTITY_PROMPT}\n\n\
Ты работаешь автономно внутри уже выбранного рабочего пространства.\n\
Корень workspace уже выбран и доступен инструментам; не проси пользователя сообщать его повторно.\n\n\
Правила выполнения:\n\
- Выполняй задачу самостоятельно и используй инструменты, когда они нужны для фактической проверки.\n\
- Если пользователь не сформулировал конкретное поручение, не исследуй workspace и не имитируй выполненную работу: задай один короткий уточняющий вопрос и дождись задачи.\n\
- За один ответ вызывай только один инструмент и жди его результата перед следующим вызовом.\n\
- Если пользователь просит изучить, проверить, найти или объяснить проект, сначала вызови filesystem.list с path точкой (.).\n\
- Затем прочитай подходящие manifest-файлы и документацию (например Cargo.toml, package.json, README и архитектурные документы), а для поиска по коду используй filesystem.search.\n\
- Для изучения проекта не используй shell.execute: filesystem.list, filesystem.read и filesystem.search безопаснее и достаточно информативны.\n\
- Не проси пользователя прислать структуру проекта, путь или команды, если workspace уже указан.\n\
- Не утверждай, что изучила файл или выполнила действие, пока соответствующий инструмент не вернул результат.\n\
- Для чтения используй безопасные read-only инструменты. Перед изменениями и опасными действиями учитывай approval.\n\
- Аргументы инструментов всегда передавай полным JSON-объектом: filesystem.list — {{\"path\":\".\"}}, filesystem.read — {{\"path\":\"README.md\"}}, filesystem.search — {{\"query\":\"pattern\",\"path\":\".\"}}; не вызывай их с пустым объектом и не передавай абсолютный путь вместо workspace-relative path.\n\
- Если инструмент вернул ошибку, не завершай задачу: исправь аргументы по тексту ошибки и повтори вызов.\n\
- filesystem.patch принимает только JSON {{\"path\":\"relative/file.rs\",\"patch\":\"--- a/relative/file.rs\\n+++ b/relative/file.rs\\n@@ ...\"}}; поля edits и patches недопустимы.\n\
- Если пользователь просит исправить, изменить или реализовать код, обязательно внеси изменение через filesystem.patch или filesystem.write; для просьбы о проверке запусти соответствующий тест/сборку, а для просьбы о commit вызови git.commit. Не выдавай план за результат.\n\
- Для Git используй специализированные инструменты: сначала git.status, затем git.diff для проверки изменений; git.commit — только с task-only сообщением и после успешной проверки; git.pull — для обновления checkout; git.push — только если пользователь явно попросил опубликовать изменения. Не заменяй эти вызовы shell.execute и не выполняй git-команды из неправильной директории.\n\
- Все улучшения EvoHime выполняй самостоятельно через доступные инструменты Евы. Если попытка не удалась, не завершай задачу: зафиксируй причину в trace, дождись следующего шага настройки и повтори работу через инструменты до успешного результата.\n\
- После исследования дай отчёт: что обнаружено, какие файлы проверены, какие проблемы найдены и что предлагается сделать дальше.\n\n\
Доступные инструменты в этой сессии:\n{}",
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
        .map_err(|error| format!("mcp.call invalid input: {error}"))?;
    if input.url.is_some() {
        return Err("mcp.call model input cannot contain url".into());
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
    .map_err(|error| format!("mcp.call output serialization failed: {error}"))
}

/// Budget for a whole task: many model calls plus tool runs, so it has to be
/// larger than the per-request timeout in `ProviderResilienceConfig`.
/// Максимальная длительность одной автономной задачи в секундах.
pub const DEFAULT_TASK_TIMEOUT_SECONDS: u64 = 900;

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DeliveryRequirements {
    pub(crate) research: bool,
    pub(crate) mutation: bool,
    pub(crate) verification: bool,
    pub(crate) diff_check: bool,
    pub(crate) commit: bool,
}

impl DeliveryRequirements {
    pub(crate) fn from_prompt(prompt: &str) -> Self {
        let prompt = prompt.to_lowercase();
        Self {
            research: ["изучи", "исслед", "ознаком", "найди", "объясни"]
                .iter()
                .any(|marker| prompt.contains(marker)),
            mutation: [
                "исправ",
                "измен",
                "добав",
                "реализ",
                "сделай",
                "улучш",
                "удал",
                "убер",
            ]
            .iter()
            .any(|marker| prompt.contains(marker)),
            verification: ["проверь", "провер", "тест", "test", "собери", "запусти"]
                .iter()
                .any(|marker| prompt.contains(marker)),
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
    let program = program.to_ascii_lowercase();
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
    let verification = matches!(program.as_str(), "cargo" | "dotnet" | "ctest")
        && args
            .first()
            .is_some_and(|arg| matches!(arg.as_str(), "test" | "check" | "build" | "clippy"));
    (
        verification.then_some(status_ok),
        diff_check.then_some(status_ok),
    )
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
    if requirements.research && !progress.research_done {
        if !progress.research_has_overview {
            "НЕМЕДЛЕННО вызови read-only filesystem.list с полным JSON {\"path\":\".\"}. Не пиши отчёт."
        } else if !progress.research_has_content {
            "НЕМЕДЛЕННО прочитай один из ключевых файлов: filesystem.read с JSON {\"path\":\"Cargo.toml\"} или {\"path\":\"README.md\"}. Не повторяй filesystem.list и не пиши отчёт."
        } else if !progress.research_has_search {
            "НЕМЕДЛЕННО вызови filesystem.search с полным JSON {\"query\":\"TODO\",\"path\":\".\"} или найди по коду ключевой компонент. Не используй предположения о структуре вроде crates; путь должен существовать в текущем workspace. Не повторяй уже выполненное чтение и не пиши отчёт."
        } else if progress.research_observations < 3 {
            "НЕМЕДЛЕННО прочитай ещё один конкретный архитектурный файл через filesystem.read, например docs/architecture.md или docs/current-state.md. Не пиши отчёт."
        } else {
            "НЕМЕДЛЕННО подготовь итоговый отчёт по уже собранным данным. Не вызывай инструменты."
        }
    } else if !progress.mutation_done && requirements.mutation {
        "НЕМЕДЛЕННО вызови filesystem.patch или filesystem.write и внеси требуемое изменение. Не вызывай read/search и не пиши отчёт."
    } else if !progress.verification_done && requirements.verification {
        "НЕМЕДЛЕННО вызови shell.execute с полным JSON-объектом, например {\"program\":\"cargo\",\"args\":[\"test\"],\"cwd\":\".\"}. Не вызывай shell.execute с пустыми аргументами и не пиши отчёт."
    } else if !progress.commit_done && requirements.commit {
        "НЕМЕДЛЕННО вызови git.commit с task-only сообщением. Не пиши отчёт."
    } else {
        "НЕМЕДЛЕННО вызови следующий нужный read-only инструмент с полным JSON и продолжи исследование. Не пиши отчёт."
    }
}
