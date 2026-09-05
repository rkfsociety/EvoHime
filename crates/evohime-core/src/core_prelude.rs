pub struct CoreVersion;

pub mod adaptive_tool_catalog;
pub mod approval_policy_profiles;
pub mod capability_workbenches;
pub mod checkpoint_forking_and_replay;
pub mod code_diagnostics_feedback_loop;
pub mod conversation_bridge_adapters;
pub mod core_topic_subscription_event_bus;
pub mod customization_inventory;
pub mod declarative_agent_component_registry;
pub mod declarative_runtime_components;
pub mod dependency_aware_task_graph;
pub mod durable_remote_task_bridge;
pub mod event_visualizer_registry;
pub mod experience_replay_library;
pub mod headless_core_cli;
pub mod knowledge_source_registry_project_role;
pub mod output_guardrail_pipeline;
pub mod privacy_and_telemetry_governance;
pub mod project_instruction_stack;
pub mod reasoning_operator_library;
pub mod safe_ui_extension_framework;
pub mod schema_driven_agent_configuration;
pub mod sensitive_data_guardrails;
pub mod standing_approval_profiles;
pub mod team_coordinator;
pub mod team_sop_protocols;
pub mod typed_context_references;
pub mod workflow_optimization_lab;
pub mod workspace_bootstrap_manifest;
pub mod workspace_sets;

/// Базовая identity-инструкция, добавляемая к каждому model context.
pub const AGENT_IDENTITY_PROMPT: &str =
    "Ты — Ева, AI-агент приложения EvoHime. Ева — короткое имя EvoHime; понимай обращения к тебе «Ева» и «EvoHime» как к одному агенту.";

/// Канонические имена read-only filesystem-инструментов, используемые в
/// policy preflight и в проверках обязательного исследовательского пути.
const TOOL_FILESYSTEM_LIST: &str = "filesystem.list";
/// Read-only filesystem tool used to inspect file contents.
const TOOL_FILESYSTEM_READ: &str = "filesystem.read";
/// Read-only filesystem tool used to search workspace content.
const TOOL_FILESYSTEM_SEARCH: &str = "filesystem.search";

/// Идентификатор встроенной политики разрешений Core.
const PERMISSION_POLICY_ID: &str = "permission-v1";

/// Типизированные параметры операций над architecture snapshot.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchitectureSnapshotRequest {
    #[serde(default)]
    workspace_root: Option<String>,
    #[serde(default)]
    subject_id: Option<String>,
    #[serde(default)]
    source_revision: Option<String>,
    #[serde(default)]
    allowed_roots: Vec<String>,
    #[serde(default)]
    before: Option<crate::architecture_snapshot::ArchitectureSnapshot>,
    #[serde(default)]
    after: Option<crate::architecture_snapshot::ArchitectureSnapshot>,
    #[serde(default)]
    expected: Option<crate::architecture_snapshot::ExpectedArchitectureDelta>,
    #[serde(default)]
    actual: Option<crate::architecture_snapshot::ArchitectureDelta>,
}

/// Типизированные параметры операций локального model runtime manager.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct LocalModelRuntimeRequest {
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    state: Option<crate::local_model_runtime_manager::ArtifactState>,
    #[serde(default)]
    trust: Option<crate::local_model_runtime_manager::TrustLevel>,
    #[serde(default)]
    observed_hash: Option<String>,
    #[serde(default)]
    expected_hash: Option<String>,
    #[serde(default)]
    staging_relative_path: Option<String>,
    #[serde(default)]
    destination_relative_path: Option<String>,
    #[serde(default)]
    expected_size_bytes: Option<u64>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    from: Option<crate::local_model_runtime_manager::ArtifactState>,
    #[serde(default)]
    to: Option<crate::local_model_runtime_manager::ArtifactState>,
    #[serde(default)]
    session: Option<crate::local_model_runtime_manager::LocalModelRuntimeSession>,
    #[serde(default)]
    model: Option<crate::local_model_runtime_manager::LocalModelDescriptor>,
    #[serde(default)]
    runtime: Option<crate::local_model_runtime_manager::LocalInferenceRuntime>,
    #[serde(default)]
    artifact: Option<crate::local_model_runtime_manager::LocalArtifactRecord>,
}

/// Типизированный запрос проверки approval policy.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyDecisionRequest {
    scope_id: String,
    action_class: String,
    resource: String,
    risk: u8,
    #[serde(default)]
    now_ms: i64,
}

/// Типизированный запрос принятия pipeline intent.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptIntentRequest {
    intent: crate::architect_editor_model_pipeline::EditIntent,
    #[serde(default)]
    workspace_revision: String,
}

/// Типизированный запрос preflight для командного resource budget.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamBudgetPreflightRequest {
    policy: crate::team_resource_budget::TeamBudgetPolicy,
    state: crate::team_resource_budget::TeamBudgetState,
    estimate: crate::team_resource_budget::ResourceLimits,
    #[serde(default)]
    reserve_access: bool,
    #[serde(default)]
    unknown_cost: bool,
}

/// Типизированный запрос evaluate для composable termination conditions.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminationEvaluateRequest {
    policy: crate::composable_termination_conditions::TerminationPolicy,
    state: crate::composable_termination_conditions::TerminationState,
    event: crate::composable_termination_conditions::TerminationEvent,
}

/// Типизированный запрос сохранения termination state.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminationSaveStateRequest {
    state: crate::composable_termination_conditions::TerminationState,
    run_id: String,
    policy_id: String,
}

/// Типизированный запрос подтверждения или отклонения доставки события.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryRequest {
    subscription_id: String,
    event_id: String,
    #[serde(default = "default_delivery_attempt")]
    attempt: u64,
    #[serde(default)]
    error: Option<String>,
}

fn default_delivery_attempt() -> u64 {
    1
}

/// Полный типизированный запрос benchmark evaluation.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BenchmarkEvaluationInput {
    candidate: crate::workflow_optimization_lab::Candidate,
    #[serde(flatten)]
    request: crate::workflow_optimization_lab::BenchmarkEvaluationRequest,
}

/// Типизированные параметры вызова capability workbench.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkbenchCallRequest {
    capability: String,
    #[serde(default)]
    tool_id: Option<String>,
}

/// Типизированные параметры изменения ресурса workbench.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkbenchResourceRequest {
    resource_id: String,
    available: bool,
}

/// Типизированные параметры снимка workbench; logical_state остаётся
/// расширяемым payload, но его envelope и credential refs проверяются строго.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkbenchSnapshotRequest {
    #[serde(default = "default_workbench_logical_state")]
    logical_state: serde_json::Value,
    #[serde(default)]
    credential_refs: Vec<String>,
}

fn default_workbench_logical_state() -> serde_json::Value {
    serde_json::json!({})
}

/// Типизированные параметры построения представления knowledge collection.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct KnowledgeCollectionViewRequest {
    #[serde(default = "default_project_target_kind")]
    target_kind: crate::knowledge_source_registry_project_role::TargetKind,
    target_id: String,
}

/// Типизированные параметры keyword retrieval по knowledge source.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct KnowledgeQueryRequest {
    query: String,
    #[serde(default = "default_project_target_kind")]
    target_kind: crate::knowledge_source_registry_project_role::TargetKind,
    target_id: String,
}

fn default_project_target_kind() -> crate::knowledge_source_registry_project_role::TargetKind {
    crate::knowledge_source_registry_project_role::TargetKind::Project
}

/// Типизированные параметры компиляции project instruction stack.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InstructionStackCompileRequest {
    #[serde(default)]
    explicit_ids: Vec<String>,
    #[serde(default)]
    policy: Option<crate::project_instruction_stack::ProjectInstructionStackPolicy>,
}

/// Типизированные параметры включения или отключения project rule.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InstructionStackToggleRequest {
    rule_id: String,
    enabled: bool,
}

/// Типизированный envelope команд team coordination policies.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamCoordinationRequest {
    team: crate::team_coordination_policies::TeamSpec,
    #[serde(default)]
    state: Option<crate::team_coordination_policies::TeamCoordinationState>,
    #[serde(default)]
    event_ids: Vec<String>,
    #[serde(default)]
    handoff_from: Option<String>,
    #[serde(default)]
    selector_role: Option<String>,
    #[serde(default)]
    event_type: Option<String>,
    #[serde(default)]
    strategy: Option<crate::team_coordination_policies::TeamCoordinationStrategy>,
    #[serde(default)]
    protocol_snapshot: Option<crate::team_sop_protocols::ProtocolSnapshot>,
    #[serde(default)]
    participant: Option<crate::team_coordination_policies::ParticipantIdentity>,
    #[serde(default)]
    strategy_state: Option<crate::team_coordination_policies::StrategySessionState>,
}

/// Типизированные параметры model edit protocol registry.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelEditRequest {
    #[serde(default)]
    definition: Option<crate::model_edit_protocol_registry::EditProtocolDefinition>,
    #[serde(default)]
    original: Option<String>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    attempt: u8,
}

/// Типизированные параметры remote conversation channels.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteChannelRequest {
    #[serde(default)]
    connection: Option<crate::remote_conversation_channels::ChannelConnection>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    external_identity: Option<String>,
    #[serde(default)]
    message: Option<crate::remote_conversation_channels::InboundMessage>,
}

/// Типизированный запрос сохранения memory view.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryViewSaveRequest {
    view: crate::memory_views_and_adaptive_recall::MemoryView,
}

/// Типизированный запрос adaptive recall.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MemoryRecallRequest {
    policy: crate::memory_views_and_adaptive_recall::AdaptiveRecallPolicy,
    mode: crate::memory_views_and_adaptive_recall::RecallMode,
    #[serde(default = "default_query_complexity")]
    complexity: crate::memory_views_and_adaptive_recall::QueryComplexity,
    query: String,
    #[serde(default)]
    scope_id: Option<String>,
    read_barrier_generation: u64,
    #[serde(default)]
    candidates: Vec<crate::memory_views_and_adaptive_recall::RecallCandidate>,
}

fn default_query_complexity() -> crate::memory_views_and_adaptive_recall::QueryComplexity {
    crate::memory_views_and_adaptive_recall::QueryComplexity::Unknown
}

/// Типизированные параметры prompt cache planner.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptCacheRequest {
    #[serde(default)]
    segments: Vec<crate::prompt_cache_planner::PromptSegment>,
    #[serde(default)]
    profile: Option<crate::prompt_cache_planner::ProviderCacheProfile>,
    #[serde(default)]
    context_revision: String,
    #[serde(default)]
    policy_version: String,
    #[serde(default)]
    keepalive_ms: i64,
    #[serde(default)]
    metric: Option<crate::prompt_cache_planner::CacheMetric>,
}

/// Типизированные параметры declarative runtime component.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarativeComponentRequest {
    #[serde(default)]
    config: Option<crate::declarative_runtime_components::ComponentConfig>,
    #[serde(default)]
    registry: Option<crate::declarative_agent_component_registry::Registry>,
    #[serde(default)]
    policy: Option<crate::declarative_runtime_components::PolicySnapshot>,
    #[serde(default)]
    state: Option<crate::declarative_runtime_components::RuntimeState>,
}

/// Типизированные параметры guided calibration sessions.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CalibrationRequest {
    #[serde(default)]
    owner_scope: Option<String>,
    #[serde(default)]
    subject_ref: Option<String>,
    #[serde(default)]
    actor_ref: Option<String>,
    #[serde(default)]
    policy_snapshot_hash: Option<String>,
    #[serde(default)]
    iteration: Option<crate::guided_calibration_sessions::CalibrationIteration>,
    #[serde(default)]
    pattern_key: Option<String>,
    #[serde(default)]
    guidance_text: Option<String>,
    #[serde(default)]
    candidate_id: Option<String>,
    #[serde(default)]
    cancelled: bool,
}

/// Типизированный запрос предложения team assignment.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignmentProposalRequest {
    #[serde(default)]
    item: Option<crate::team_coordinator::TeamWorkItem>,
    #[serde(default)]
    candidates: Vec<crate::team_coordinator::ParticipantCandidate>,
    #[serde(default)]
    termination: Option<AssignmentTerminationRequest>,
}

/// Типизированные termination gates для assignment proposal.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignmentTerminationRequest {
    policy: crate::composable_termination_conditions::TerminationPolicy,
    state: crate::composable_termination_conditions::TerminationState,
    event: crate::composable_termination_conditions::TerminationEvent,
}

/// Типизированный запрос submit для durable remote task bridge.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteTaskSubmitRequest {
    toolset: crate::durable_remote_task_bridge::RemoteTaskToolset,
    #[serde(default)]
    request: serde_json::Value,
    #[serde(default = "default_remote_provenance")]
    provenance_ref: String,
    #[serde(default)]
    operation: String,
}

fn default_remote_provenance() -> String {
    "core".into()
}

/// Типизированный запрос poll для durable remote task bridge.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteTaskPollRequest {
    #[serde(default = "default_remote_provenance")]
    lease_owner: String,
}

/// Типизированный запрос result для durable remote task bridge.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteTaskResultRequest {
    status: crate::durable_remote_task_bridge::RemoteTaskStatus,
    #[serde(default = "default_remote_transport_status")]
    transport_status: String,
    #[serde(default)]
    result_artifact_ref: Option<String>,
}

fn default_remote_transport_status() -> String {
    "reported".into()
}

/// Типизированные параметры назначения team work item.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignmentRequest {
    item: crate::team_coordinator::TeamWorkItem,
    proposal: crate::team_coordinator::DelegationProposal,
    candidate: crate::team_coordinator::ParticipantCandidate,
}

/// Типизированные параметры привязки workspace set к task.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceSetBindingRequest {
    task_id: String,
    #[serde(default)]
    root_ids: Vec<String>,
}

/// Типизированные параметры оценки message intervention policy.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InterventionRequest {
    policy: crate::message_intervention_policies::MessageInterventionPolicy,
    context: crate::message_intervention_policies::MessageInterventionContext,
    #[serde(default)]
    seen: bool,
}

/// Типизированный запрос построения redacted bridge projection.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeProjectionRequest {
    binding: crate::conversation_bridge_adapters::ThreadBinding,
    #[serde(default = "default_bridge_projection_kind")]
    kind: String,
    #[serde(default = "default_bridge_projection_status")]
    status: String,
    #[serde(default = "default_bridge_projection_provenance")]
    provenance_id: String,
}

fn default_bridge_projection_kind() -> String {
    "status".into()
}
fn default_bridge_projection_status() -> String {
    "unknown".into()
}
fn default_bridge_projection_provenance() -> String {
    "event".into()
}

/// Аргументы для policy-only preflight при построении каталога инструментов.
/// Preflight не выполняет инструмент, но path-инструментам всё равно нужен
/// существующий workspace-relative путь, иначе безопасный инструмент ошибочно
/// выпадает из authorized snapshot.
fn catalog_preflight_input(tool_name: &str) -> serde_json::Value {
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

fn requires_workspace_research_catalog(prompt: &str) -> bool {
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
struct ModelMcpCallInput {
    server_id: String,
    tool_name: String,
    #[serde(default)]
    params: Option<serde_json::Value>,
    #[serde(default)]
    timeout_ms: Option<serde_json::Value>,
    #[serde(default)]
    url: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
struct ResolvedMcpInput {
    url: String,
    method: String,
    params: serde_json::Value,
    timeout_ms: serde_json::Value,
}

fn model_is_waiting_instead_of_reporting(content: &str) -> bool {
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

fn build_agent_system_prompt(tool_names: &[String]) -> String {
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

fn resolve_model_mcp_input(
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
struct DeliveryRequirements {
    research: bool,
    mutation: bool,
    verification: bool,
    diff_check: bool,
    commit: bool,
}

impl DeliveryRequirements {
    fn from_prompt(prompt: &str) -> Self {
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

    fn missing(
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

fn strict_delivery_gate_enabled() -> bool {
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
fn classify_shell_verification(
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
struct DeliveryProgress {
    research_done: bool,
    mutation_done: bool,
    verification_done: bool,
    commit_done: bool,
    research_observations: usize,
    research_has_overview: bool,
    research_has_content: bool,
    research_has_search: bool,
}

fn delivery_next_step(
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
pub mod paths;
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
