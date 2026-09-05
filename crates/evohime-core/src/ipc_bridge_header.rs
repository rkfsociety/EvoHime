use evohime_desktop_ipc::{generated, transport, FrameError};
use prost::Message;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use crate::{
    ApprovalCoordinator, CoreCommand, CoreEvent, EventJournal, SelectedModel, TaskCoordinator,
};
use evohime_listener_contract::{ListeningReason, ListeningState};
use evohime_local_storage::{
    execution_ledger, EventRecord, LocalDatabase, StorageError, WorkItemRecord,
};
use evohime_model_gateway::ModelGatewayConfig;
use evohime_permissions::{Permission, PermissionMode};
use evohime_receipts::{
    key_lifecycle::{ReceiptKeyManager, VerificationStatus},
    runtime::ProtectedActionRow,
};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

/// Major version of the authenticated desktop IPC protocol.
const PROTOCOL_MAJOR: u32 = 1;
/// Number of tools `ToolRegistry::bootstrap()` is expected to register.
/// Used only as a Doctor health signal (fewer than expected => Warn), never
/// to gate functionality.
const EXPECTED_TOOL_COUNT: u32 = 23;
/// Minor version of the authenticated desktop IPC protocol.
const PROTOCOL_MINOR: u32 = 0;
/// Maximum number of checkpoint events replayed in one IPC response.
const TASK_CHECKPOINT_IPC_MAX_REPLAY_EVENTS: usize = 256;
/// Maximum number of checkpoint items accepted in one IPC request.
const TASK_CHECKPOINT_IPC_MAX_ITEMS: usize = 32;
/// Maximum text size of one checkpoint item in the IPC projection.
const TASK_CHECKPOINT_IPC_MAX_TEXT_BYTES: usize = 512;
/// Maximum serialized projection size for the goal listing response.
const GOAL_LIST_MAX_PROJECTION_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TaskCheckpointActionRecord {
    task_id: String,
    checkpoint_id: String,
    action: String,
    applied: bool,
    deduplicated: bool,
    error_code: String,
    error_message: String,
}

#[derive(Debug, Deserialize)]
struct TeamSopSessionPayload {
    session_id: String,
    #[serde(default)]
    protocol_id: Option<String>,
    #[serde(default)]
    protocol_version: Option<u64>,
    #[serde(default)]
    workflow_run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgenticBrowserSessionResponse {
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    control_owner: String,
    #[serde(default)]
    control_generation: u64,
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    projection_json: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct HumanWorkItemsResponse {
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    item_id: String,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    state: String,
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    projection_json: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct TeamSopProtocolsResponse {
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    version: u64,
    #[serde(default)]
    state: String,
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    projection_json: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct IpcResponseFields {
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    projection_json: serde_json::Value,
    #[serde(default)]
    state: String,
    #[serde(default)]
    run_id: String,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    version: u64,
    #[serde(default)]
    registry_version: u64,
    #[serde(default)]
    protocol: String,
    #[serde(default)]
    control_level: String,
    #[serde(default)]
    policy_id: String,
    #[serde(default)]
    policy_hash: String,
    #[serde(default)]
    attempts: u32,
    #[serde(default)]
    retries: u32,
    #[serde(default)]
    fallbacks: u32,
    #[serde(default)]
    terminal_outcome: String,
    #[serde(default)]
    profile_id: String,
    #[serde(default)]
    profile_hash: String,
    #[serde(default)]
    backend: String,
    #[serde(default)]
    network_policy: String,
    #[serde(default)]
    environment_policy: String,
    #[serde(default)]
    timeout_ms: u64,
    #[serde(default)]
    max_output_bytes: u64,
    #[serde(default)]
    contract_hash: String,
    #[serde(default)]
    strategy: String,
}

#[derive(Debug, Deserialize)]
struct ArtifactRevisionPayload {
    artifact_id: String,
    revision: u64,
}

#[derive(Debug, Deserialize)]
struct ArtifactHandoffPayload {
    artifact_id: String,
    revision: u64,
    handoff_id: String,
    producer_identity: String,
    consumer_identity: String,
}

#[derive(Debug, Deserialize)]
struct ExternalAgentCommandPayload {
    #[serde(default)]
    run_id: String,
    #[serde(default)]
    conversation_id: String,
    #[serde(default)]
    executable_ref: String,
}

#[derive(Debug, Deserialize)]
struct AgentRoleRuntimePayload {
    #[serde(default)]
    run_id: String,
    #[serde(default)]
    profile_id: String,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    requested_grants: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct InvocationPresetRunPayload {
    #[serde(default)]
    preset_id: String,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    workspace_path: String,
    #[serde(default)]
    temporary_overrides: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct HumanWorkItemCommandPayload {
    #[serde(default)]
    item_id: String,
    #[serde(default)]
    response: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct AgenticBrowserSessionPayload {
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    policy_hash: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    expected_revision: Option<u64>,
    #[serde(default)]
    page_ref: Option<String>,
    #[serde(default)]
    element_ref: Option<String>,
    #[serde(default)]
    artifact_ref: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ExecutionBackendPayload {
    #[serde(default)]
    id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    auth_ref: Option<String>,
    #[serde(default)]
    backend_id: String,
    #[serde(default)]
    protocol_major: u32,
    #[serde(default)]
    protocol_minor: u32,
    #[serde(default)]
    capability_hash: String,
}

#[derive(Debug, Deserialize)]
struct PersistentAgentOrganizationResponse {
    #[serde(default)]
    operation: String,
    #[serde(default = "default_ok_status")]
    status: String,
    #[serde(default)]
    revision: u64,
}

#[derive(Debug, Deserialize)]
struct InvocationPresetResponse {
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    preset_id: String,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    content_hash: String,
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    preview: Option<serde_json::Value>,
    #[serde(default)]
    presets: Option<serde_json::Value>,
}

fn default_ok_status() -> String {
    "ok".to_owned()
}

fn error_response_payload(error_code: impl Into<String>) -> Vec<u8> {
    let error_code = error_code.into();
    match serde_json::to_vec(&serde_json::json!({"error_code": error_code})) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::error!(%error, "failed to serialize IPC error response");
            br#"{"error_code":"serialization_failed"}"#.to_vec()
        }
    }
}

fn bounded_tool_error_code(error: &evohime_tool_runtime::ToolError) -> &'static str {
    match error {
        evohime_tool_runtime::ToolError::UnknownTool(_) => "unknown_tool",
        evohime_tool_runtime::ToolError::InvalidInput { .. } => "invalid_input",
        evohime_tool_runtime::ToolError::PermissionDenied(_) => "permission_denied",
        evohime_tool_runtime::ToolError::NotFound { .. } => "not_found",
        evohime_tool_runtime::ToolError::NeedsApproval(_) => "approval_required",
        evohime_tool_runtime::ToolError::ApprovalMismatch => "approval_mismatch",
        evohime_tool_runtime::ToolError::ApprovalDenied => "approval_denied",
        evohime_tool_runtime::ToolError::Execution(_) => "execution_failed",
        evohime_tool_runtime::ToolError::TimedOut(_) => "timed_out",
    }
}

/// Decodes a typed execution-ledger row (план 08-1/08-2, `payload` = JSON
/// `ExecutionEventV1`) into the additive IPC projection (план 08-3). Never
/// panics on malformed payload — returns `None` so the caller falls back to
/// the generic `event_type`/`payload` path instead of dropping the frame.
fn decode_typed_execution_event(payload: &[u8]) -> Option<generated::ExecutionEvent> {
    let event: execution_ledger::ExecutionEventV1 = serde_json::from_slice(payload).ok()?;
    let body_json = serde_json::to_vec(&event.body).ok()?;
    Some(generated::ExecutionEvent {
        schema_version: event.schema_version,
        event_id: event.event_id,
        run_scope: event.run_scope.as_str().to_string(),
        run_id: event.run_id,
        session_id: event.session_id.unwrap_or_default(),
        created_at_ms: event.created_at_ms,
        state_after: event
            .state_after
            .map(|state| state.as_str().to_string())
            .unwrap_or_default(),
        action_id: event.action_id.unwrap_or_default(),
        tool_call_id: event.tool_call_id.unwrap_or_default(),
        observation_id: event.observation_id.unwrap_or_default(),
        receipt_id: event.receipt_id.unwrap_or_default(),
        failure_id: event.failure_id.unwrap_or_default(),
        workflow_run_id: event.workflow_run_id.unwrap_or_default(),
        node_id: event.node_id.unwrap_or_default(),
        attempt_id: event.attempt_id.unwrap_or_default(),
        effect_id: event.effect_id.unwrap_or_default(),
        model_request_id: event.model_request_id.unwrap_or_default(),
        body_json,
        secrets_present: event.redaction.secrets_present,
        redaction_digest: event.redaction.digest.unwrap_or_default(),
    })
}

fn conversation_event_projection(
    event: crate::conversation_event_log::RendererConversationEvent,
) -> generated::ConversationEventProjection {
    generated::ConversationEventProjection {
        schema_version: event.schema_version,
        conversation_id: event.conversation_id,
        event_id: event.event_id,
        sequence: event.sequence,
        timestamp_ms: event.timestamp_ms,
        kind: event.kind,
        category: event.category,
        payload_json: event.payload_json,
        correlation_id: event.correlation_id,
        causation_id: event.causation_id,
        task_id: event.task_id,
        run_id: event.run_id,
        turn_id: event.turn_id,
        client_message_id: event.client_message_id,
        persistence_class: event.persistence_class,
        sensitivity: event.sensitivity,
    }
}

fn decode_conversation_event(payload: &[u8]) -> Option<generated::ConversationEventProjection> {
    serde_json::from_slice::<crate::conversation_event_log::RendererConversationEvent>(payload)
        .ok()
        .map(conversation_event_projection)
}

/// Bounded set of `ReplayGap.reason` values (план 08-3). The retention case
/// is the pre-existing condition (`journal.replay_bounded` reports a gap);
/// `stale_generation` is new — the client's `CommandEnvelope` names a
/// `core_instance_id`/`session_epoch` that no longer matches this process.
const REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED: &str = "sequence_retention_exceeded";
/// Replay gap reason emitted when the client's runtime generation is stale.
const REPLAY_GAP_REASON_STALE_GENERATION: &str = "stale_generation";

/// Publishes the `ToolCall` typed ledger event for a receipt-tracked action
/// right after its dispatch marker (`mark_started`) is durable — план 08-4's
/// "action → tool call → observation → receipt" chain, anchored to the same
/// `action_id` the signed receipt uses. Never fails the caller: a publish
/// error is logged and swallowed, because the ledger is an additive
/// observability layer, not authoritative for whether the tool call itself
/// may proceed (that authority stays with `evohime-receipts`).
///
/// `session_id` falls back to `task_id` when the tool context carries no
/// explicit chat/session identity — most terminal invocations don't have
/// one separate from the task itself, and `ExecutionEventV1::validate`
/// requires a non-empty `session_id` outside `system`/`legacy` scope.
fn record_ledger_tool_call(
    database: &LocalDatabase,
    request: &evohime_receipts::runtime::ActionRequest,
    session_id: Option<String>,
) {
    let event = execution_ledger::ExecutionEventV1 {
        schema_version: 1,
        event_id: uuid::Uuid::now_v7().to_string(),
        sequence_id: None,
        run_scope: execution_ledger::RunScope::Standalone,
        run_id: request.run_id.clone(),
        session_id: Some(session_id.unwrap_or_else(|| request.task_id.clone())),
        task_id: request.task_id.clone(),
        created_at_ms: now_ms(),
        state_after: Some(execution_ledger::ActionState::Running),
        action_id: Some(request.action_id.to_string()),
        tool_call_id: None,
        observation_id: None,
        receipt_id: None,
        failure_id: None,
        workflow_run_id: None,
        node_id: None,
        attempt_id: None,
        effect_id: None,
        model_request_id: None,
        body: execution_ledger::ExecutionEventBody::ToolCall {
            tool_name: request.tool_name.clone(),
            tool_call_hash: evohime_receipts::runtime::canonical_call_hash(
                &request.tool_name,
                &request.normalized_scope,
                &request.input,
            )
            .unwrap_or_default(),
            manifest_hash: None,
        },
        redaction: tool_request_redaction(request),
    };
    if let Err(error) = database.append_ledger_event(&event) {
        tracing::warn!(
            event = "ledger.tool_call_publish_failed",
            action_id = %request.action_id,
            error = %error,
            "typed ledger event failed to publish"
        );
    }
}

/// Publishes the terminal (`ToolReceipt` on success, `TypedFailure`/
/// `UnknownOutcome` on failure) typed ledger event for a receipt-tracked
/// action, under the same `action_id` [`record_ledger_tool_call`] used.
/// Same failure posture: never propagated to the caller.
fn record_ledger_tool_outcome(
    database: &LocalDatabase,
    request: &evohime_receipts::runtime::ActionRequest,
    session_id: Option<String>,
    state_after: execution_ledger::ActionState,
    body: execution_ledger::ExecutionEventBody,
) {
    let event = execution_ledger::ExecutionEventV1 {
        schema_version: 1,
        event_id: uuid::Uuid::now_v7().to_string(),
        sequence_id: None,
        run_scope: execution_ledger::RunScope::Standalone,
        run_id: request.run_id.clone(),
        session_id: Some(session_id.unwrap_or_else(|| request.task_id.clone())),
        task_id: request.task_id.clone(),
        created_at_ms: now_ms(),
        state_after: Some(state_after),
        action_id: Some(request.action_id.to_string()),
        tool_call_id: None,
        observation_id: None,
        receipt_id: None,
        failure_id: None,
        workflow_run_id: None,
        node_id: None,
        attempt_id: None,
        effect_id: None,
        model_request_id: None,
        body,
        redaction: tool_request_redaction(request),
    };
    if let Err(error) = database.append_ledger_event(&event) {
        tracing::warn!(
            event = "ledger.tool_outcome_publish_failed",
            action_id = %request.action_id,
            error = %error,
            "typed ledger event failed to publish"
        );
    }
}

/// Bounded redaction metadata for a receipt-tracked action's ledger events
/// (план 08-4): scans the request's serialized input and preview for the
/// same secret-shaped markers the audit log already redacts on
/// (`crate::audit::contains_secret`), so the two layers agree. Only
/// presence is recorded — never a hash of the secret itself, avoiding a new
/// cryptographic primitive this codebase doesn't otherwise use for this
/// purpose.
fn tool_request_redaction(
    request: &evohime_receipts::runtime::ActionRequest,
) -> execution_ledger::RedactionMeta {
    let input_text = request.input.to_string();
    let secrets_present = crate::audit::contains_secret(&input_text)
        || crate::audit::contains_secret(&request.preview);
    execution_ledger::RedactionMeta {
        secrets_present,
        digest: None,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

/// Bounded typed action projection for `FullSnapshot.snapshot_json` (план
/// 08-3 п.5): the latest known state per `action_id` among the replayed
/// typed ledger rows, so a reconnecting client can rebuild action cards
/// without replaying the full event list first. Non-ledger rows are
/// ignored; malformed typed payload never panics, it is just skipped.
fn typed_snapshot_actions(events: &[EventRecord]) -> Vec<serde_json::Value> {
    let mut latest: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    for record in events {
        if !record.event_type.starts_with("ledger.") {
            continue;
        }
        let Ok(event) =
            serde_json::from_slice::<execution_ledger::ExecutionEventV1>(&record.payload)
        else {
            continue;
        };
        let Some(action_id) = event.action_id else {
            continue;
        };
        latest.insert(
            action_id.clone(),
            serde_json::json!({
                "action_id": action_id,
                "event_id": event.event_id,
                "run_scope": event.run_scope.as_str(),
                "run_id": event.run_id,
                "state_after": event.state_after.map(|state| state.as_str()),
                "sequence_id": record.sequence_id,
            }),
        );
    }
    latest.into_values().collect()
}

#[derive(Debug, thiserror::Error)]
pub enum IpcBridgeError {
    #[error("IPC frame failed: {0}")]
    Frame(#[from] FrameError),
    #[error("protobuf message failed: {0}")]
    Protobuf(#[from] prost::DecodeError),
    #[error("JSON payload failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelConfigSnapshot {
    pub provider: String,
    pub route: String,
    pub model: String,
    pub configured: bool,
}

type ConversationSubscription =
    Arc<tokio::sync::Mutex<Option<(String, std::collections::BTreeSet<String>)>>>;

pub struct IpcBridge {
    journal: EventJournal,
    receipt_keys: Arc<ReceiptKeyManager>,
    coordinator: Option<TaskCoordinator>,
    approvals: Option<ApprovalCoordinator>,
    tools: Option<Arc<ToolRegistry>>,
    model_config: Option<ModelConfigSnapshot>,
    gateway_config: Option<ModelGatewayConfig>,
    selected_model: SelectedModel,
    core_instance_id: String,
    session_epoch: u64,
    review_tasks: Arc<tokio::sync::Mutex<HashMap<String, CancellationToken>>>,
    review_results: Arc<tokio::sync::Mutex<HashMap<String, crate::plan_review::ReviewResult>>>,
    revision_tasks: Arc<tokio::sync::Mutex<HashMap<String, CancellationToken>>>,
    revision_results: Arc<tokio::sync::Mutex<HashMap<String, crate::plan_review::RevisionResult>>>,
    /// Active kernel runtimes are process-local; only their validated manifest
    /// and object metadata are durable in LocalDatabase.
    analysis_kernels:
        Arc<tokio::sync::Mutex<HashMap<String, crate::analysis_kernel::KernelRuntime>>>,
    /// Единственный источник истины о состоянии постоянного слушания.
    ///
    /// Трей, глобальный хоткей и панель «Слух» — три точки входа одной и той
    /// же команды `SetAmbientListening`; своей копии состояния у них нет, и
    /// обновляются они только событием `ambient.state`.
    ambient: crate::ambient::AmbientListeningRegistry,
    /// Каталог, в котором лежат политика и намерение слушания.
    ///
    /// `None` означает продовый путь из окружения. Поле существует ради
    /// тестов: подмена переменной окружения на процесс сделала бы соседние
    /// тесты зависимыми друг от друга.
    ambient_data_dir: Option<std::path::PathBuf>,
    /// Потолок и счётчики ограниченной проактивности (04.7). Тот же реестр,
    /// что держит агент: иначе мост и производитель предложений считали бы
    /// разные бюджеты.
    proactivity: crate::ambient::AmbientProactivityRegistry,
    /// Реестр подтверждений узлов workflow (план 06.3).
    ///
    /// Отдельной команды approval для workflow нет: карточка решается той же
    /// `ResolveApproval`, а этот реестр помнит, какому узлу принадлежит
    /// идентификатор и было ли решение.
    workflow_approvals: Arc<crate::workflow_runtime::WorkflowApprovalRegistry>,
    /// Core-owned каталог возможностей workflow: он статичен, поэтому
    /// строится один раз на мост.
    workflow_registry: Arc<crate::workflow_registry::WorkflowRegistry>,
    /// Очередь услышанных команд и кэш каталога приложений.
    ///
    /// Живёт на мосту, потому что оба конца пути ведут сюда: endpoint
    /// листенера кладёт карточку, панель её решает. Второй экземпляр означал
    /// бы карточку, которую некому принять.
    voice_commands: Arc<crate::voice_command::VoiceCommandRegistry>,
    tool_simulation: Arc<tokio::sync::Mutex<crate::tool_simulation_runtime::ToolSimulationRuntime>>,
    external_agents:
        Arc<tokio::sync::Mutex<crate::external_coding_agent_adapter::ExternalAgentRegistry>>,
    role_profiles: Arc<tokio::sync::Mutex<crate::agent_role_profiles::AgentRoleProfilesRegistry>>,
    team_sop: Arc<tokio::sync::Mutex<crate::team_sop_protocols::TeamSopRegistry>>,
    human_work_items: Arc<tokio::sync::Mutex<crate::human_work_items::HumanWorkItemsRegistry>>,
    /// One packaged backend per session. The child is supervised by Core's
    /// parent Job Object and is never exposed to renderer/model code.
    browser_backends:
        Arc<tokio::sync::Mutex<HashMap<String, crate::browser_backend::BrowserBackendProcess>>>,
    conversation_subscription: ConversationSubscription,
}

/// Проект, под которым живут принятые предложения. Речь у стола не
/// принадлежит рабочему каталогу, поэтому у неё собственная строка проекта — по
/// той же причине, по которой ambient-память живёт в scope `workspace/ambient`.
pub const AMBIENT_PROPOSAL_PROJECT_ID: &str = "ambient-proposals";

/// `client_id` дедупликации принятых предложений.
const AMBIENT_PROPOSAL_CLIENT_ID: &str = "ambient-proactivity";

/// Признак неисполняемого напоминания, записанный в данных задачи.
pub const AMBIENT_REMINDER_NON_GOAL: &str =
    "Напоминание не выполняется автоматически: это заметка, а не задача агента.";

/// Потолок длины ключа идемпотентности. Совпадает с bounded-лимитом
/// идентификаторов хранилища.
/// Maximum idempotency-key length for ambient proposal commands.
const MAX_PROPOSAL_KEY_BYTES: usize = 128;

/// Отказ запуска workflow: код называется явно, идентификатор не выдумывается.
fn workflow_start_failure(code: &str) -> serde_json::Value {
    serde_json::json!({
        "run_id": "",
        "state": "",
        "graph_hash": "",
        "deduplicated": false,
        "error_code": code,
    })
}

fn continuation_public_json(
    run: &evohime_local_storage::continuation_store::RunRecord,
    gates: &[evohime_local_storage::continuation_store::GateResultRecord],
) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&serde_json::json!({
        "schema_version": crate::continuation::POLICY_SCHEMA_VERSION,
        "run_id": run.run_id,
        "owner_scope": run.owner_scope,
        "policy_id": run.policy_id,
        "policy_revision": run.policy_revision,
        "policy_hash": run.policy_hash,
        "task_id": run.task_id,
        "goal_id": run.goal_id,
        "goal_version": run.goal_version,
        "state": run.state,
        "continuation_index": run.continuation_index,
        "max_continuations": run.max_continuations,
        "used_model_turns": run.used_model_turns,
        "max_model_turns": run.max_model_turns,
        "token_used": run.token_used,
        "cost_used_micros": run.cost_used_micros,
        "stop_reason": run.stop_reason,
        "created_at_ms": run.created_at_ms,
        "updated_at_ms": run.updated_at_ms,
        "error_code": "",
        "gates": gates
    }))
    .map_err(|_| "serialization_failed".into())
}

/// Родительские возможности запуска из оболочки.
///
/// Оболочка не назначает права: набор фиксирован Core и совпадает с тем, что
/// уже разрешено обычной задаче чтения репозитория. Child-узел может получить
/// только подмножество.
fn workflow_parent_capabilities() -> crate::workflow_registry::ParentCapabilities {
    crate::workflow_registry::ParentCapabilities {
        grants: std::collections::BTreeSet::from([
            "fs.read".to_string(),
            "workspace.read".to_string(),
        ]),
        budget: crate::workflow::NodeBudget {
            max_tokens: 64_000,
            max_seconds: 900,
            max_tool_calls: 64,
        },
        context_allowlist: std::collections::BTreeSet::new(),
    }
}

/// Отказ по решению предложения: код называется явно, «применено» не
/// придумывается.
fn resolve_failure(code: evohime_listener_contract::AmbientErrorCode) -> serde_json::Value {
    serde_json::json!({
        "applied": false,
        "state": "",
        "task_id": "",
        "error_code": code.as_str(),
    })
}

fn runtime_identity() -> (String, u64) {
    (
        uuid::Uuid::new_v4().to_string(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or_default(),
    )
}
