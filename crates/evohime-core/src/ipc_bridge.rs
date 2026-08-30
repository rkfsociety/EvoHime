use evohime_desktop_ipc::{generated, transport, FrameError};
use prost::Message;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
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

const PROTOCOL_MAJOR: u32 = 1;
/// Number of tools `ToolRegistry::bootstrap()` is expected to register.
/// Used only as a Doctor health signal (fewer than expected => Warn), never
/// to gate functionality.
const EXPECTED_TOOL_COUNT: u32 = 23;
const PROTOCOL_MINOR: u32 = 0;
const TASK_CHECKPOINT_IPC_MAX_REPLAY_EVENTS: usize = 256;
const TASK_CHECKPOINT_IPC_MAX_ITEMS: usize = 32;
const TASK_CHECKPOINT_IPC_MAX_TEXT_BYTES: usize = 512;
const GOAL_LIST_MAX_PROJECTION_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskCheckpointActionRecord {
    task_id: String,
    checkpoint_id: String,
    action: String,
    applied: bool,
    deduplicated: bool,
    error_code: String,
    error_message: String,
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

/// Bounded set of `ReplayGap.reason` values (план 08-3). The retention case
/// is the pre-existing condition (`journal.replay_bounded` reports a gap);
/// `stale_generation` is new — the client's `CommandEnvelope` names a
/// `core_instance_id`/`session_epoch` that no longer matches this process.
const REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED: &str = "sequence_retention_exceeded";
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

impl IpcBridge {
    pub fn journal(&self) -> EventJournal {
        self.journal.clone()
    }

    /// Identity this process picked at construction (план 08-2/08-3
    /// `core_instance_id`) — used to publish the `core_start` ledger event
    /// under the exact id this bridge will stamp on every `EventEnvelope`.
    pub fn core_instance_id(&self) -> &str {
        &self.core_instance_id
    }

    /// True when the client's own `CommandEnvelope` names a generation
    /// (`core_instance_id`/`session_epoch`) other than this process's
    /// current one. An empty/zero client field never counts as stale — it
    /// means the client has no known generation yet (first connect).
    fn stale_generation(&self, command: &generated::CommandEnvelope) -> bool {
        (!command.core_instance_id.is_empty() && command.core_instance_id != self.core_instance_id)
            || (command.session_epoch > 0 && command.session_epoch != self.session_epoch)
    }

    /// Builds a typed `ReplayGap` envelope (план 08-3): honestly filled
    /// bounds instead of the generic JSON `"reason"` field this used to be.
    fn replay_gap_envelope(
        &self,
        requested_after_sequence: u64,
        earliest_available_sequence: Option<u64>,
        latest_available_sequence: u64,
        reason: &str,
    ) -> generated::EventEnvelope {
        generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: latest_available_sequence,
            task_id: String::new(),
            event_type: "replay.gap".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ReplayGap(
                generated::ReplayGap {
                    requested_after_sequence,
                    earliest_available_sequence: earliest_available_sequence.unwrap_or(0),
                    latest_available_sequence,
                    reason: reason.to_string(),
                },
            )),
        }
    }

    /// Publishes a typed `ApprovalDecision` ledger event for a resolved
    /// approval, when it is linked to a receipts-tracked action (план 08-4
    /// acceptance: "approval approve/reject/expiry"). Cancellation already
    /// collapses into `granted = false` at the call site — a cancelled
    /// approval and a denied one both land as `Rejected` here, matching the
    /// existing `approval.decision` audit record's own `granted` field.
    /// A no-op when `approval_id` isn't a receipts approval intent (e.g. a
    /// pure workflow-node or routing approval) — those aren't
    /// receipts-tracked actions and get no `ExecutionEventV1` here.
    async fn record_ledger_approval_decision(&self, approval_id: &str, granted: bool) {
        let database = self.journal.database().lock().await;
        let linked: Option<(String, String, String)> = database
            .connection()
            .query_row(
                "SELECT action_id, task_id, run_id FROM receipt_approval_intents
                   WHERE approval_id = ?1",
                [approval_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .unwrap_or(None);
        let Some((action_id, task_id, run_id)) = linked else {
            return;
        };
        let state_after = if granted {
            execution_ledger::ActionState::Running
        } else {
            execution_ledger::ActionState::Denied
        };
        let event = execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: uuid::Uuid::now_v7().to_string(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::Standalone,
            run_id,
            session_id: Some(task_id.clone()),
            task_id,
            created_at_ms: now_ms(),
            state_after: Some(state_after),
            action_id: Some(action_id),
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: None,
            node_id: None,
            attempt_id: None,
            effect_id: None,
            model_request_id: None,
            body: execution_ledger::ExecutionEventBody::ApprovalDecision {
                approval_intent_id: approval_id.to_string(),
                decision: if granted {
                    execution_ledger::ApprovalOutcome::Approved
                } else {
                    execution_ledger::ApprovalOutcome::Rejected
                },
                snapshot_hash: None,
            },
            redaction: execution_ledger::RedactionMeta::default(),
        };
        if let Err(error) = database.append_ledger_event(&event) {
            tracing::warn!(
                event = "ledger.approval_decision_publish_failed",
                approval_id,
                error = %error,
                "typed ledger event failed to publish"
            );
        }
    }

    fn manager_for(journal: &EventJournal) -> Arc<ReceiptKeyManager> {
        let data_dir = journal
            .database_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        Arc::new(ReceiptKeyManager::new(data_dir))
    }
    /// Записывает лимиты каталога в локальную базу. Это подсказка для
    /// планировщика контекста, а не условие работы: провайдер может не сообщить
    /// окно, а база — быть занята другим писателем, и ни то, ни другое не повод
    /// проваливать запрос каталога.
    async fn remember_model_limits(
        &self,
        provider: &str,
        entries: &[evohime_model_gateway::ModelCatalogEntry],
    ) {
        if entries.is_empty() {
            return;
        }
        let records = entries
            .iter()
            .map(
                |entry| evohime_local_storage::model_limit_store::ModelLimitRecord {
                    model: entry.id.clone(),
                    provider: provider.to_string(),
                    context_tokens: entry.context_tokens,
                    max_output_tokens: entry.max_output_tokens,
                },
            )
            .collect::<Vec<_>>();
        let database = self.journal.database().lock().await;
        if let Err(error) = evohime_local_storage::model_limit_store::ModelLimitStoreSql::upsert_all(
            database.connection(),
            &records,
        ) {
            tracing::warn!(target: "model.catalog", %error, "model context limits were not stored");
        }
    }

    pub fn new(journal: EventJournal) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: None,
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
        }
    }

    pub fn with_coordinator(journal: EventJournal, coordinator: TaskCoordinator) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: Some(coordinator),
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
        }
    }

    pub fn with_coordinator_and_approvals(
        journal: EventJournal,
        coordinator: TaskCoordinator,
        approvals: ApprovalCoordinator,
        tools: Arc<ToolRegistry>,
        model_config: Option<ModelConfigSnapshot>,
        gateway_config: Option<ModelGatewayConfig>,
    ) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: Some(coordinator),
            approvals: Some(approvals),
            tools: Some(tools),
            model_config,
            gateway_config,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
        }
    }

    /// Разделяемый реестр состояния слушания.
    pub fn ambient(&self) -> crate::ambient::AmbientListeningRegistry {
        self.ambient.clone()
    }

    pub fn voice_commands(&self) -> Arc<crate::voice_command::VoiceCommandRegistry> {
        self.voice_commands.clone()
    }

    /// Подключает готовый реестр: `main.rs` создаёт его до моста, чтобы
    /// endpoint листенера и мост говорили об одном и том же состоянии.
    pub fn with_ambient(mut self, ambient: crate::ambient::AmbientListeningRegistry) -> Self {
        self.ambient = ambient;
        self
    }

    /// Каталог политики и намерения слушания.
    pub fn with_ambient_data_dir(mut self, directory: std::path::PathBuf) -> Self {
        self.ambient_data_dir = Some(directory);
        self
    }

    /// Разделяемый реестр проактивности.
    pub fn proactivity(&self) -> crate::ambient::AmbientProactivityRegistry {
        self.proactivity.clone()
    }

    /// Подключает готовый реестр проактивности: `main.rs` создаёт его до
    /// агента и до моста, чтобы обе стороны считали один и тот же потолок.
    pub fn with_proactivity(
        mut self,
        proactivity: crate::ambient::AmbientProactivityRegistry,
    ) -> Self {
        self.proactivity = proactivity;
        self
    }

    fn ambient_data_dir(&self) -> std::path::PathBuf {
        self.ambient_data_dir
            .clone()
            .unwrap_or_else(crate::ambient::data_dir)
    }

    /// Пишет ambient-событие в durable journal и будит push к оболочке.
    ///
    /// Без второго шага запись легла бы в базу, но открытое окно узнало бы о
    /// ней только со следующим событием задачи.
    pub async fn publish_ambient(
        &self,
        event: &evohime_listener_contract::AmbientLogEvent,
    ) -> Result<i64, evohime_listener_contract::AmbientErrorCode> {
        let sequence = self.journal.append_ambient_event(event).await?;
        if let Some(coordinator) = &self.coordinator {
            coordinator.notify_journalled(sequence.max(0) as u64);
        }
        Ok(sequence)
    }

    /// Отдаёт закрытый эпизод в ambient-извлечение (04.6).
    ///
    /// Мост здесь только курьер: решают `EVOHIME_AMBIENT_MEMORY`, общий режим
    /// извлечения и ambient-бюджеты, и все три проверяются в Core, а не тут.
    /// Без координатора вызов молча ничего не делает — извлекателя в этой
    /// сборке просто нет.
    pub async fn request_ambient_extraction(&self, episode_id: &str) {
        let Some(coordinator) = &self.coordinator else {
            return;
        };
        let _ = coordinator
            .dispatch(CoreCommand::ExtractAmbientMemory {
                episode_id: episode_id.to_owned(),
            })
            .await;
    }

    /// Shares the agent's model selection so `SelectModelRequest` can change it.
    pub fn with_selected_model(mut self, selected: SelectedModel) -> Self {
        self.selected_model = selected;
        self
    }

    /// Streams journal entries newer than `after_sequence` to a connected
    /// client and returns the sequence it has now seen.
    ///
    /// Task progress reaches the shell this way rather than straight from the
    /// in-memory broadcast: the journal is what assigns sequence numbers, and
    /// the shell relies on them for resync after a reconnect.
    pub async fn push_journal_tail<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        after_sequence: u64,
    ) -> Result<u64, IpcBridgeError> {
        let batch = self
            .journal
            .replay_bounded(after_sequence as i64, 256)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let mut last_sequence = after_sequence;
        for record in batch.events {
            last_sequence = record.sequence_id as u64;
            // Typed ledger rows (план 08-1/08-2) carry ExecutionEventV1 JSON
            // in payload; project it additively into the oneof without
            // touching the generic event_type/payload backward-compat path.
            let execution_event = record
                .event_type
                .starts_with("ledger.")
                .then(|| decode_typed_execution_event(&record.payload))
                .flatten();
            let event = generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: record.sequence_id as u64,
                task_id: record.task_id,
                event_type: record.event_type,
                payload: record.payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: execution_event
                    .map(|event| generated::event_envelope::Event::ExecutionEvent(Box::new(event))),
            };
            transport::write_frame(writer, &event.encode_to_vec()).await?;
        }
        Ok(last_sequence)
    }

    /// Sequence the journal has already durably recorded.
    pub async fn latest_sequence(&self) -> u64 {
        self.journal.latest_sequence().await.max(0) as u64
    }

    /// Listener that fires whenever a task emits, so the server knows there is
    /// a journal tail worth flushing.
    /// Signal that fires once an event is durably journalled. The pipe server
    /// pushes the journal tail on this instead of on the broadcast itself,
    /// which used to overtake the writer and strand the last event of a task.
    pub fn journalled(&self) -> Option<tokio::sync::watch::Receiver<u64>> {
        self.coordinator
            .as_ref()
            .map(|coordinator| coordinator.journalled())
    }

    fn receipt_status(&self) -> serde_json::Value {
        let manager = &self.receipt_keys;
        let active = manager.active_path().exists();
        let history = manager.history_path().exists();
        let status = if !active && !history {
            "not_initialized".to_string()
        } else if !active || !history {
            "key.recovery_required".to_string()
        } else if manager.journal_path().exists() {
            "key.rotation_incomplete".to_string()
        } else {
            match manager.verify_history(None) {
                Ok(VerificationStatus::Verified) => "verified_unpinned".to_string(),
                Ok(VerificationStatus::Untrusted) => {
                    let loaded = manager.load_history().ok();
                    if loaded.as_ref().is_some_and(|items| {
                        items.iter().any(|item| {
                            matches!(item.continuity.as_str(), "broken" | "compromised")
                        })
                    }) {
                        return serde_json::json!({
                            "status": "key.trust_required",
                            "key_id": manager.load_signer().ok().map(|(metadata, _)| metadata.key_id),
                            "history_present": history,
                            "active_present": active,
                            "rotation_journal_present": manager.journal_path().exists(),
                        });
                    }
                    let genesis =
                        loaded.and_then(|items| items.first().map(|item| item.new_key_id.clone()));
                    match genesis.and_then(|key| manager.trusted_genesis(&key).ok()) {
                        Some(true) => "trusted".to_string(),
                        _ => "key.trust_required".to_string(),
                    }
                }
                Ok(VerificationStatus::Broken) => "key.history_incomplete".to_string(),
                Ok(VerificationStatus::Unsupported) => "unsupported".to_string(),
                Err(error) => error.to_string(),
            }
        };
        let key_id = std::fs::read(manager.active_path())
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<evohime_receipts::key_lifecycle::ActiveKeyMetadata>(&bytes)
                    .ok()
            })
            .map(|metadata| metadata.key_id);
        serde_json::json!({"status": status, "key_id": key_id, "history_present": history, "active_present": active, "rotation_journal_present": manager.journal_path().exists()})
    }

    async fn take_receipt_approval<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        approval_id: &str,
        operation: &str,
    ) -> Result<bool, IpcBridgeError> {
        let Some(approvals) = &self.approvals else {
            self.write_response(
                writer,
                "key.approval_required",
                serde_json::to_vec(
                    &serde_json::json!({"operation": operation, "error_code":"approval.required"}),
                )?,
            )
            .await?;
            return Ok(false);
        };
        let Ok(id) = uuid::Uuid::parse_str(approval_id) else {
            self.write_response(
                writer,
                "key.approval_required",
                serde_json::to_vec(
                    &serde_json::json!({"operation": operation, "error_code":"approval.required"}),
                )?,
            )
            .await?;
            return Ok(false);
        };
        if approvals.consume_approved(id).await {
            Ok(true)
        } else {
            self.write_response(writer, "key.approval_required", serde_json::to_vec(&serde_json::json!({"operation": operation, "approval_id": id.to_string(), "error_code":"approval.required"}))?).await?;
            Ok(false)
        }
    }

    async fn dispatch_save_continuation_policy(
        &self,
        request: generated::SaveContinuationPolicy,
        client_id: &str,
        request_id: &str,
        command_hash: &str,
    ) -> Result<Vec<u8>, String> {
        let policy: crate::continuation::ContinuationPolicyV1 =
            serde_json::from_slice(&request.policy_json)
                .map_err(|_| "invalid_argument".to_string())?;
        if request.policy_json.len() > crate::continuation::MAX_POLICY_BYTES
            || (!request.owner_scope.is_empty() && request.owner_scope != policy.scope.owner_scope)
            || (!request.actor.is_empty() && request.actor != policy.actor)
        {
            return Err("invalid_argument".into());
        }
        policy
            .validate()
            .map_err(|_| "invalid_policy".to_string())?;
        for gate in &policy.gates {
            let available = match gate.kind {
                crate::continuation::GateKind::Tool => {
                    self.tools
                        .as_ref()
                        .is_some_and(|tools| tools.manifest_for(&gate.capability_ref).is_some())
                        && gate.capability_ref != "shell"
                        && !gate.capability_ref.starts_with("shell.")
                }
                crate::continuation::GateKind::Workflow => {
                    crate::workflow_templates::template(&gate.capability_ref).is_some()
                }
                crate::continuation::GateKind::Evidence => self
                    .workflow_registry
                    .provider(&gate.capability_ref)
                    .is_some(),
                crate::continuation::GateKind::Approval => gate.capability_ref == "approval",
            };
            if !available {
                return Err("gate_unavailable".into());
            }
        }
        let canonical = policy
            .canonical_json()
            .map_err(|_| "invalid_policy".to_string())?;
        let result = serde_json::to_vec(&serde_json::json!({
            "schema_version": crate::continuation::POLICY_SCHEMA_VERSION,
            "policy_id": policy.id,
            "revision": policy.revision,
            "content_hash": policy.content_hash,
            "enabled": policy.enabled
        }))
        .map_err(|_| "serialization_failed".to_string())?;
        let journal = self.journal.clone();
        let database = journal.database().lock().await;
        if let Some(previous) = database
            .record_deduplicated(client_id, request_id, command_hash, &[])
            .map_err(|_| "idempotency_conflict".to_string())?
        {
            return Ok(previous);
        }
        evohime_local_storage::continuation_store::save_policy(
            database.connection(),
            &evohime_local_storage::continuation_store::PolicyRecord {
                policy_id: policy.id.clone(),
                revision: policy.revision as i64,
                owner_scope: policy.scope.owner_scope.clone(),
                actor: policy.actor.clone(),
                enabled: policy.enabled,
                canonical_json: canonical,
                content_hash: policy.content_hash.clone(),
                created_at_ms: policy.created_at_ms,
                updated_at_ms: policy.updated_at_ms,
            },
        )
        .map_err(|_| "storage_failed".to_string())?;
        database
            .record_deduplicated(client_id, request_id, command_hash, &result)
            .map_err(|_| "idempotency_conflict".to_string())?;
        Ok(result)
    }

    async fn dispatch_start_continuation(
        &self,
        request: generated::StartContinuationRun,
    ) -> Result<Vec<u8>, String> {
        if request.run_id.is_empty()
            || request.policy_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.task_id.is_empty()
        {
            return Err("invalid_argument".into());
        }
        let journal = self.journal.clone();
        let database = journal.database().lock().await;
        if let Some(existing) = evohime_local_storage::continuation_store::get_run_by_idempotency(
            database.connection(),
            &request.owner_scope,
            &request.idempotency_key,
        )
        .map_err(|_| "storage_failed".to_string())?
        {
            if existing.run_id == request.run_id
                && existing.task_id == request.task_id
                && existing.policy_id == request.policy_id
                && existing.policy_revision == request.policy_revision as i64
            {
                return continuation_public_json(&existing, &[]);
            }
            return Err("idempotency_conflict".into());
        }
        let policy = evohime_local_storage::continuation_store::get_policy(
            database.connection(),
            &request.policy_id,
            request.policy_revision as i64,
            &request.owner_scope,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "policy_not_found".to_string())?;
        if !policy.enabled {
            return Err("policy_disabled".into());
        }
        let policy_json: crate::continuation::ContinuationPolicyV1 =
            serde_json::from_slice(&policy.canonical_json)
                .map_err(|_| "policy_corrupt".to_string())?;
        let now = crate::task_memory::now_millis() as i64;
        let record = evohime_local_storage::continuation_store::RunRecord {
            run_id: request.run_id.clone(),
            idempotency_key: request.idempotency_key,
            task_id: request.task_id,
            owner_scope: request.owner_scope,
            policy_id: request.policy_id,
            policy_revision: request.policy_revision as i64,
            policy_hash: policy.content_hash,
            goal_id: (!request.goal_id.is_empty()).then_some(request.goal_id),
            goal_version: (request.goal_version > 0).then_some(request.goal_version as i64),
            state: "running".into(),
            continuation_index: 0,
            max_continuations: policy_json.budget.max_continuations as i64,
            max_model_turns: policy_json.budget.max_model_turns as i64,
            used_model_turns: 0,
            token_budget: policy_json.budget.max_tokens.map(|v| v as i64),
            token_used: 0,
            cost_budget_micros: policy_json.budget.max_cost_micros.map(|v| v as i64),
            cost_used_micros: 0,
            stop_reason: None,
            prompt: None,
            workspace_path: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        evohime_local_storage::continuation_store::create_run(database.connection(), &record)
            .map_err(|error| {
                if matches!(error, rusqlite::Error::SqliteFailure(_, _)) {
                    "run_exists"
                } else {
                    "storage_failed"
                }
                .to_string()
            })?;
        continuation_public_json(&record, &[])
    }

    async fn dispatch_get_continuation(
        &self,
        request: generated::GetContinuationRun,
    ) -> Result<Vec<u8>, String> {
        let database = self.journal.database().lock().await;
        let run = evohime_local_storage::continuation_store::get_run(
            database.connection(),
            &request.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "run_not_found".to_string())?;
        let gates = evohime_local_storage::continuation_store::list_latest_gate_results(
            database.connection(),
            &run.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?;
        continuation_public_json(&run, &gates)
    }

    async fn dispatch_stop_continuation(
        &self,
        request: generated::StopContinuation,
    ) -> Result<Vec<u8>, String> {
        if request.run_id.is_empty() || request.expected_state != "running" {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        evohime_local_storage::continuation_store::apply_transition_action(
            database.connection_mut(),
            &request.run_id,
            &request.idempotency_key,
            "stop",
            &request.expected_state,
            "stopped",
            "user_stop",
            crate::task_memory::now_millis() as i64,
        )
        .map_err(|_| "storage_failed".to_string())
    }

    async fn dispatch_transition_continuation(
        &self,
        run_id: String,
        idempotency_key: String,
        expected_state: String,
        next_state: &'static str,
        action: &'static str,
    ) -> Result<Vec<u8>, String> {
        if run_id.is_empty()
            || idempotency_key.is_empty()
            || (expected_state != "running" && expected_state != "paused")
        {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        evohime_local_storage::continuation_store::apply_transition_action(
            database.connection_mut(),
            &run_id,
            &idempotency_key,
            action,
            &expected_state,
            next_state,
            action,
            crate::task_memory::now_millis() as i64,
        )
        .map_err(|_| "storage_failed".to_string())
    }

    async fn dispatch_resume_continuation(
        &self,
        request: generated::ResumeContinuation,
    ) -> Result<evohime_local_storage::continuation_store::RunRecord, String> {
        if request.run_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.expected_state != "paused"
        {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        let run = evohime_local_storage::continuation_store::get_run(
            database.connection(),
            &request.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "run_not_found".to_string())?;
        if run.prompt.is_none() || run.workspace_path.is_none() {
            return Err("resume_context_unavailable".into());
        }
        let _action_result = evohime_local_storage::continuation_store::apply_transition_action(
            database.connection_mut(),
            &request.run_id,
            &request.idempotency_key,
            "resume",
            "paused",
            "running",
            "approval_resolution",
            crate::task_memory::now_millis() as i64,
        )
        .map_err(|_| "storage_failed".to_string())?;
        evohime_local_storage::continuation_store::get_run(database.connection(), &request.run_id)
            .map_err(|_| "storage_failed".to_string())?
            .ok_or_else(|| "run_not_found".into())
    }

    pub fn process_once<'a, R: AsyncRead + Unpin + 'a, W: AsyncWrite + Unpin + 'a>(
        &'a self,
        reader: &'a mut R,
        writer: &'a mut W,
    ) -> Pin<Box<dyn Future<Output = Result<(), IpcBridgeError>> + 'a>> {
        Box::pin(async move {
            let payload = transport::read_frame(reader).await?;
            let command = generated::CommandEnvelope::decode(payload.as_slice())?;
            let request_id = command.request_id.clone();
            let client_id = command.client_id.clone();
            let command_hash = hex_encode(&payload);
            match command.command {
                Some(generated::command_envelope::Command::Handshake(_)) => {
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "core.ready".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: Some(generated::event_envelope::Event::Ready(generated::Ready {
                            protocol: Some(protocol()),
                            core_version: env!("CARGO_PKG_VERSION").into(),
                            core_info: Some(core_info()),
                        })),
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::GetReceiptKeyStatus(_)) => {
                    let mut status = self.receipt_status();
                    if let Ok(mut database) = self.journal.database().try_lock() {
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        ) {
                            if let Ok(counts) = runtime.counts() {
                                if let Some(object) = status.as_object_mut() {
                                    object.insert(
                                        "runtime_counts".into(),
                                        serde_json::json!({
                                            "pending": counts.pending,
                                            "pending_recovery": counts.pending_recovery,
                                            "quarantined": counts.quarantined,
                                            "approval_pending": counts.approval_pending,
                                        }),
                                    );
                                    if let Ok((rate, version)) = runtime.audit_sampling_config() {
                                        object.insert("audit_sampling".into(), serde_json::json!({"rate": rate, "policy_version": version}));
                                    }
                                    if let Ok(metrics) = runtime.metrics() {
                                        object.insert(
                                            "runtime_metrics".into(),
                                            serde_json::json!(metrics.counters),
                                        );
                                    }
                                    if let Ok(diagnostics) = runtime.diagnostic_counts() {
                                        object.insert(
                                            "runtime_diagnostics".into(),
                                            serde_json::json!(diagnostics),
                                        );
                                    }
                                    if let Ok(rotation) = runtime.storage_rotation_job() {
                                        object.insert("storage_rotation".into(), serde_json::json!(rotation.map(|job| serde_json::json!({"job_id": job.job_id, "old_key_id": job.old_key_id, "new_key_id": job.new_key_id, "cursor": job.cursor, "generation": job.generation, "state": job.state}))));
                                    }
                                }
                            }
                        }
                    }
                    self.write_response(writer, "key.status", serde_json::to_vec(&status)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ClosePendingReceiptAction(request)) => {
                    if !request.operator_confirmed
                        || request.action_id.is_empty()
                        || request.input_json.len()
                            > evohime_receipts::runtime::MAX_CALL_INPUT_BYTES
                    {
                        self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let action_id = uuid::Uuid::parse_str(&request.action_id)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let input: serde_json::Value = serde_json::from_str(&request.input_json)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let mut database = self.journal.database().lock().await;
                    let (task_id, run_id, tool_name, normalized_scope, policy_id, decision, state, approval_id, parent_approval_ref): (String,String,String,String,String,String,String,Option<String>,Option<String>) = database.connection().query_row(
                    "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,state,approval_id,parent_approval_ref FROM receipt_actions WHERE action_id=?1",
                    [action_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                    if state != "pending_recovery" {
                        self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery"}))?).await?;
                        return Ok(());
                    }
                    let policy_decision = match decision.as_str() {
                        "allow" => evohime_receipts::runtime::PolicyDecision::Allow,
                        "approval_required" => {
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired
                        }
                        "deny" => evohime_receipts::runtime::PolicyDecision::Deny,
                        _ => {
                            self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id,
                        task_id,
                        run_id,
                        tool_name,
                        policy_id,
                        normalized_scope,
                        input,
                        policy_decision,
                        approval_id: approval_id
                            .and_then(|value| uuid::Uuid::parse_str(&value).ok()),
                        parent_approval_ref,
                        preview: "unknown result closure".into(),
                    };
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let receipt_hash = runtime
                        .refuse(&receipt_request, "recovery_pending")
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":true,"action_id":request.action_id,"receipt_hash":receipt_hash,"completion_source":"reconciliation"}))?).await?;
                }
                Some(generated::command_envelope::Command::SetReceiptAuditSamplingRate(
                    request,
                )) => {
                    if request.rate > 100 {
                        self.write_response(writer, "receipt.sampling_rate", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    runtime
                        .set_audit_sampling_rate(true, request.rate as u8)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.sampling_rate", serde_json::to_vec(&serde_json::json!({"ok":true,"rate":request.rate,"policy_version":evohime_receipts::SAMPLING_POLICY_VERSION}))?).await?;
                }
                Some(generated::command_envelope::Command::ReconcilePendingReceiptAction(
                    request,
                )) => {
                    const MAX_RECONCILIATION_INPUT_BYTES: usize =
                        evohime_receipts::runtime::MAX_CALL_INPUT_BYTES;
                    let read_only = matches!(
                        request.tool_name.as_str(),
                        "filesystem.read"
                            | "filesystem.list"
                            | "git.status"
                            | "git.diff"
                            | "git.log"
                            | "git.show"
                            | "git.blame"
                            | "git.changed_files"
                            | "workspace.list"
                            | "workspace.read"
                            | "workspace.search"
                    );
                    if request.old_action_id.is_empty()
                        || request.tool_name.len() > 128
                        || !read_only
                        || request.input_json.len() > MAX_RECONCILIATION_INPUT_BYTES
                        || request.workspace_path.is_empty()
                        || request.workspace_path.len() > 32 * 1024
                        || request.workspace_path.contains('\n')
                    {
                        self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let old_action_id = match uuid::Uuid::parse_str(&request.old_action_id) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let input: serde_json::Value = match serde_json::from_str(&request.input_json) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let tools = match self.tools.as_ref() {
                        Some(value) => Arc::clone(value),
                        None => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.tool_unavailable"}))?).await?;
                            return Ok(());
                        }
                    };
                    let (task_id, old_state): (String, String) = {
                        let database = self.journal.database().lock().await;
                        database
                            .connection()
                            .query_row(
                                "SELECT task_id,state FROM receipt_actions WHERE action_id=?1",
                                [old_action_id.to_string()],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .map_err(|error| FrameError::Io(error.to_string()))?
                    };
                    if old_state != "pending_recovery" {
                        self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery"}))?).await?;
                        return Ok(());
                    }
                    let context = ToolContext {
                        workspace_root: std::path::PathBuf::from(&request.workspace_path),
                        task_id: task_id.parse().unwrap_or_else(|_| uuid::Uuid::now_v7()),
                        session_id: None,
                        progress_tx: None,
                    };
                    let (scope, preview) = match tools
                        .preflight(&context, &request.tool_name, &input)
                        .await
                    {
                        Ok(evohime_tool_runtime::ToolPreflightDecision::Allowed {
                            scope,
                            preview,
                        }) => (scope, preview),
                        Ok(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.policy_denied"}))?).await?;
                            return Ok(());
                        }
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.policy_denied"}))?).await?;
                            return Ok(());
                        }
                    };
                    let new_action_id = uuid::Uuid::now_v7();
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id: new_action_id,
                        task_id: task_id.clone(),
                        run_id: format!("reconciliation-{}", new_action_id),
                        tool_name: request.tool_name.clone(),
                        policy_id: "reconciliation:read_only".into(),
                        normalized_scope: scope,
                        input: input.clone(),
                        policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                        approval_id: None,
                        parent_approval_ref: None,
                        preview: serde_json::to_string(&preview)
                            .unwrap_or_else(|_| "read-only reconciliation".into()),
                    };
                    {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        if !matches!(
                            runtime
                                .prepare(receipt_request.clone())
                                .map_err(|error| FrameError::Io(error.to_string()))?,
                            evohime_receipts::runtime::PrepareOutcome::Prepared { .. }
                        ) {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.precondition_failed"}))?).await?;
                            return Ok(());
                        }
                        runtime
                            .mark_started(new_action_id)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                    let result = tools
                        .execute_with_cancellation(
                            &context,
                            &request.tool_name,
                            input,
                            CancellationToken::new(),
                        )
                        .await;
                    let (status, digest, error_category) = match &result {
                        Ok(value) => (
                            "succeeded",
                            evohime_receipts::sha256_hex(value.output.as_bytes()),
                            None,
                        ),
                        Err(_error) => (
                            "failed",
                            evohime_receipts::sha256_hex(b"reconciliation_tool_error"),
                            Some("tool_error"),
                        ),
                    };
                    let receipt_hash = {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        runtime
                            .mark_returned(new_action_id)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                        match runtime.complete_reconciliation(
                            &receipt_request,
                            old_action_id,
                            status,
                            &digest,
                            error_category,
                        ) {
                            Ok(hash) => hash,
                            Err(_error) => {
                                let _ = runtime
                                    .mark_pending_recovery(new_action_id, "signature_failed");
                                self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery","action_id":new_action_id.to_string()}))?).await?;
                                return Ok(());
                            }
                        }
                    };
                    self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":true,"old_action_id":old_action_id.to_string(),"action_id":new_action_id.to_string(),"status":status,"receipt_hash":receipt_hash,"completion_source":"reconciliation"}))?).await?;
                }
                Some(generated::command_envelope::Command::UnquarantineReceiptAction(request)) => {
                    if !request.operator_confirmed
                        || request.action_id.is_empty()
                        || request.input_json.len()
                            > evohime_receipts::runtime::MAX_CALL_INPUT_BYTES
                        || request.checkpoint.is_empty()
                        || request.checkpoint.len() > 256
                        || request.checkpoint.contains('\n')
                    {
                        self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let checkpoint_valid = std::fs::read(self.receipt_keys.checkpoint_path())
                        .ok()
                        .and_then(|bytes| {
                            serde_json::from_slice::<
                                evohime_receipts::key_lifecycle::KeyHistoryCheckpoint,
                            >(&bytes)
                            .ok()
                        })
                        .and_then(|checkpoint| {
                            if checkpoint.checkpoint_id != request.checkpoint {
                                return None;
                            }
                            if !self
                                .receipt_keys
                                .trusted_genesis(&checkpoint.genesis_key_id)
                                .ok()?
                            {
                                return Some(false);
                            }
                            let history = self.receipt_keys.load_history().ok()?;
                            Some(
                                evohime_receipts::key_lifecycle::verify_checkpoint(
                                    &checkpoint,
                                    &history,
                                    Some(&checkpoint.genesis_key_id),
                                )
                                .is_ok(),
                            )
                        })
                        .unwrap_or(false);
                    if !checkpoint_valid {
                        self.write_response(
                        writer,
                        "receipt.unquarantine",
                        serde_json::to_vec(
                            &serde_json::json!({"ok":false,"error_code":"receipt.key_untrusted"}),
                        )?,
                    )
                    .await?;
                        return Ok(());
                    }
                    let action_id = match uuid::Uuid::parse_str(&request.action_id) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let input: serde_json::Value = match serde_json::from_str(&request.input_json) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let mut database = self.journal.database().lock().await;
                    let (task_id, run_id, tool_name, normalized_scope, policy_id, decision, state, approval_id, parent_approval_ref): (String,String,String,String,String,String,String,Option<String>,Option<String>) = database.connection().query_row(
                    "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,state,approval_id,parent_approval_ref FROM receipt_actions WHERE action_id=?1",
                    [action_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                    if state != "quarantined" {
                        self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let policy_decision = match decision.as_str() {
                        "allow" => evohime_receipts::runtime::PolicyDecision::Allow,
                        "approval_required" => {
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired
                        }
                        "deny" => evohime_receipts::runtime::PolicyDecision::Deny,
                        _ => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id,
                        task_id,
                        run_id,
                        tool_name,
                        policy_id,
                        normalized_scope,
                        input,
                        policy_decision,
                        approval_id: approval_id
                            .and_then(|value| uuid::Uuid::parse_str(&value).ok()),
                        parent_approval_ref,
                        preview: "manual quarantine closure".into(),
                    };
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let receipt_hash = runtime
                        .unquarantine(&receipt_request, true, &request.checkpoint)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":true,"action_id":request.action_id,"receipt_hash":receipt_hash,"state":"refused","dispatch_allowed":false}))?).await?;
                }
                Some(generated::command_envelope::Command::ListReceipts(request)) => {
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.listed",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        100
                    } else {
                        request.limit as i64
                    };
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::list_receipts(
                        database.connection(),
                        &filter,
                        limit,
                    ) {
                        Ok(result) => {
                            self.write_response(
                            writer,
                            "receipts.listed",
                            serde_json::to_vec(&serde_json::json!({
                                "ok": true,
                                "snapshot_last_sequence": result.snapshot_last_sequence.to_string(),
                                "rows": result.rows,
                            }))?,
                        )
                        .await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.listed",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::VerifyReceipts(request)) => {
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.verified",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        500
                    } else {
                        request.limit as i64
                    };
                    let trust_key = if request.trust_key_id.is_empty() {
                        None
                    } else {
                        Some(request.trust_key_id.as_str())
                    };
                    let key_history = self.receipt_keys.load_history().unwrap_or_default();
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::verify_receipts(
                        database.connection(),
                        &key_history,
                        trust_key,
                        &filter,
                        limit,
                    ) {
                        Ok(result) => {
                            self.write_response(
                            writer,
                            "receipts.verified",
                            serde_json::to_vec(&serde_json::json!({
                                "ok": true,
                                "status": result.verification.status,
                                "code": result.verification.code,
                                "requested_count": result.requested_count,
                                "actual_verified_count": result.verification.actual_verified_count,
                                "chain_start_hash": result.verification.chain_start_hash,
                                "chain_end_hash": result.verification.chain_end_hash,
                                "rows": result.verification.rows,
                            }))?,
                        )
                        .await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.verified",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::ExportReceipts(request)) => {
                    if request.replace
                        || request.destination_path.is_empty()
                        || request.destination_path.len() > 4096
                    {
                        self.write_response(writer, "receipts.exported", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":if request.replace { "receipts.unsupported_operation" } else { "receipts.invalid_filter" }}))?).await?;
                        return Ok(());
                    }
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.exported",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        100_000
                    } else {
                        request.limit as i64
                    };
                    let destination = std::path::PathBuf::from(&request.destination_path);
                    let key_history = self.receipt_keys.load_history().unwrap_or_default();
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::export_receipts(
                        database.connection(),
                        &key_history,
                        &destination,
                        &filter,
                        limit,
                    ) {
                        Ok(manifest) => {
                            let manifest_sha256 = std::fs::read(destination.join("manifest.json"))
                                .ok()
                                .map(|bytes| evohime_receipts::sha256_hex(&bytes));
                            self.write_response(writer, "receipts.exported", serde_json::to_vec(&serde_json::json!({
                            "ok": true,
                            "export_id": manifest.export_id,
                            "destination_basename": destination.file_name().and_then(|value| value.to_str()),
                            "snapshot_last_sequence": manifest.snapshot_last_sequence.to_string(),
                            "requested_count": manifest.requested_count,
                            "selected_count": manifest.selected_count,
                            "actual_exported_count": manifest.actual_exported_count,
                            "manifest_sha256": manifest_sha256,
                        }))?).await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.exported",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::TrustReceiptGenesis(request)) => {
                    if !self
                        .take_receipt_approval(writer, &request.approval_id, "TrustReceiptGenesis")
                        .await?
                    {
                        return Ok(());
                    }
                    let result = self
                        .receipt_keys
                        .trust_genesis(&request.genesis_key_id, &request.source);
                    let payload = match result {
                        Ok(()) => {
                            serde_json::json!({"status": "trusted", "genesis_key_id": request.genesis_key_id})
                        }
                        Err(error) => {
                            serde_json::json!({"status": error.to_string(), "error_code": error.to_string()})
                        }
                    };
                    self.write_response(writer, "key.trust", serde_json::to_vec(&payload)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateNewReceiptGenesis(request)) => {
                    if !self
                        .take_receipt_approval(
                            writer,
                            &request.approval_id,
                            "CreateNewReceiptGenesis",
                        )
                        .await?
                    {
                        return Ok(());
                    }
                    let manager = self.receipt_keys.clone();
                    let database = self.journal.database().clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let mut database = database.blocking_lock();
                        manager.create_new_genesis_with_database(database.connection_mut(), "user")
                    })
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = match result {
                        Ok(key_id) => {
                            serde_json::json!({"status":"recovered", "key_id":key_id, "trust_required":true})
                        }
                        Err(error) => {
                            serde_json::json!({"status":"failed", "error_code":error.to_string()})
                        }
                    };
                    self.write_response(writer, "key.recovery", serde_json::to_vec(&payload)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::RotateReceiptKey(request)) => {
                    if !self
                        .take_receipt_approval(writer, &request.approval_id, "RotateReceiptKey")
                        .await?
                    {
                        return Ok(());
                    }
                    let reason = request.reason.trim().to_string();
                    if !matches!(reason.as_str(), "manual" | "compromise") {
                        self.write_response(
                            writer,
                            "key.rotation_failed",
                            br#"{"error_code":"key.rotation_failed"}"#.to_vec(),
                        )
                        .await?;
                    } else {
                        let manager = self.receipt_keys.clone();
                        let database = self.journal.database().clone();
                        let rotation_reason = reason.clone();
                        let result = tokio::task::spawn_blocking(move || -> Result<(String, Option<String>), String> {
                        let mut database = database.blocking_lock();
                        let protected_count: i64 = database.connection().query_row(
                            "SELECT COUNT(*) FROM receipt_protected_actions",
                            [],
                            |row| row.get(0),
                        ).map_err(|error| error.to_string())?;
                        let storage_key_id: Option<String> = if protected_count > 0 {
                            let signer = super::CoreReceiptSigner(Arc::clone(&manager));
                            let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                                .map_err(|error| error.to_string())?;
                            let existing_job = runtime.storage_rotation_job().map_err(|error| error.to_string())?;
                            let (job_id, old_storage_key_id, new_storage_key_id, generation) = if let Some(job) = existing_job.filter(|job| matches!(job.state.as_str(), "running" | "failed")) {
                                (job.job_id, job.old_key_id, job.new_key_id, job.generation)
                            } else {
                                let old_storage_key_id = manager.storage_key_id().map_err(|error| error.to_string())?;
                                let new_storage_key_id = manager.rotate_storage_key(true).map_err(|error| error.to_string())?;
                                (format!("storage-{}", uuid::Uuid::now_v7()), old_storage_key_id, new_storage_key_id, SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis() as i64).unwrap_or_default())
                            };
                            loop {
                                let progressed = runtime.rewrap_protected_batch(
                                    &job_id,
                                    &old_storage_key_id,
                                    &new_storage_key_id,
                                    generation,
                                    32,
                                    |envelope| manager.rewrap_storage_with_key_id(envelope, &new_storage_key_id).map_err(|_| evohime_receipts::runtime::RuntimeError::Code("storage_key_unavailable")),
                                ).map_err(|error| error.to_string())?;
                                if !progressed { break; }
                            }
                            Some(new_storage_key_id)
                        } else {
                            None
                        };
                        let signing_key_id = manager.rotate_with_database(
                            database.connection_mut(),
                            &rotation_reason,
                            "user",
                        ).map_err(|error| error.to_string())?;
                        Ok((signing_key_id, storage_key_id))
                    })
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                        let payload = match result {
                            Ok((key_id, storage_key_id)) => {
                                serde_json::json!({"status":"rotated", "key_id":key_id, "storage_key_id":storage_key_id, "reason":reason})
                            }
                            Err(error) => {
                                serde_json::json!({"status":"failed", "error_code":error.to_string()})
                            }
                        };
                        self.write_response(writer, "key.rotation", serde_json::to_vec(&payload)?)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::ResyncRequest(request)) => {
                    evohime_desktop_ipc::validate_resync_request(&request)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    if self.stale_generation(&command) {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            request.after_sequence,
                            None,
                            latest,
                            REPLAY_GAP_REASON_STALE_GENERATION,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    let limit = if request.max_events == 0 {
                        evohime_desktop_ipc::DEFAULT_RESYNC_MAX_EVENTS
                    } else {
                        request.max_events
                    } as usize;
                    let batch = self
                        .journal
                        .replay_bounded(request.after_sequence as i64, limit)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let last_sequence = batch
                        .events
                        .last()
                        .map(|record| record.sequence_id as u64)
                        .unwrap_or(request.after_sequence);
                    if batch.gap_detected {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            request.after_sequence,
                            batch.first_available_sequence.map(|value| value as u64),
                            latest,
                            REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    // Снапшот, не влезающий в кадр IPC, раньше обрывал соединение с
                    // оболочкой: она навсегда оставалась без состояния и рисовала
                    // «нет связи». Теперь превышение лимита деградирует до
                    // поштучной отправки тех же событий.
                    let snapshot = if request.include_full_snapshot {
                        let snapshot_json = serde_json::to_vec(&serde_json::json!({
                            "schema_version": 1,
                            "core_instance_id": self.core_instance_id,
                            "session_epoch": self.session_epoch,
                            "snapshot_sequence_id": last_sequence,
                            "after_sequence": request.after_sequence,
                            "actions": typed_snapshot_actions(&batch.events),
                            "events": batch.events.iter().map(|record| serde_json::json!({
                                "sequence_id": record.sequence_id,
                                "task_id": record.task_id,
                                "event_type": record.event_type,
                                "payload": record.payload,
                                "created_at": record.created_at,
                            })).collect::<Vec<_>>(),
                        }))
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        let candidate = generated::FullSnapshot {
                            sequence_id: last_sequence,
                            snapshot_json,
                        };
                        match evohime_desktop_ipc::validate_full_snapshot(&candidate) {
                            Ok(()) => Some(candidate),
                            Err(error) => {
                                tracing::warn!(
                                    event = "ipc.snapshot_oversized",
                                    error = %error,
                                    events = batch.events.len(),
                                    snapshot_bytes = candidate.snapshot_json.len(),
                                    "снапшот не влез в кадр, переходим на поштучную отправку"
                                );
                                let payload = serde_json::to_vec(&serde_json::json!({
                                    "after_sequence": request.after_sequence,
                                    "last_sequence": last_sequence,
                                    "events": batch.events.len(),
                                    "snapshot_bytes": candidate.snapshot_json.len(),
                                    "reason": "snapshot_too_large",
                                }))
                                .map_err(|error| FrameError::Io(error.to_string()))?;
                                self.write_response(writer, "replay.snapshot_skipped", payload)
                                    .await?;
                                None
                            }
                        }
                    } else {
                        None
                    };
                    if let Some(snapshot) = snapshot {
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: last_sequence,
                            task_id: String::new(),
                            event_type: "replay.full_snapshot".into(),
                            payload: Vec::new(),
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: Some(generated::event_envelope::Event::FullSnapshot(snapshot)),
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    } else {
                        for record in batch.events {
                            let event = generated::EventEnvelope {
                                protocol: Some(protocol()),
                                sequence_id: record.sequence_id as u64,
                                task_id: record.task_id,
                                event_type: record.event_type,
                                payload: record.payload,
                                core_instance_id: self.core_instance_id.clone(),
                                session_epoch: self.session_epoch,
                                event: None,
                            };
                            transport::write_frame(writer, &event.encode_to_vec()).await?;
                        }
                    }
                    // Каждый resync отдаёт не больше `limit` событий за раз. Без
                    // этого флага оболочка узнавала об оставшемся хвосте истории
                    // только по случайному разрыву sequence в живом потоке — и
                    // гонялась за ним кругами, так и не догоняя (план про «нет
                    // связи», возникавшую после больших сессий).
                    let latest_after_batch = self.latest_sequence().await;
                    let end_payload = serde_json::to_vec(&serde_json::json!({
                        "more_available": last_sequence < latest_after_batch,
                        "latest_sequence": latest_after_batch,
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let end = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "resync.end".into(),
                        payload: end_payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &end.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::ReplayEvents(replay)) => {
                    if self.stale_generation(&command) {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            replay.after_sequence,
                            None,
                            latest,
                            REPLAY_GAP_REASON_STALE_GENERATION,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    let batch = self
                        .journal
                        .replay_bounded(replay.after_sequence as i64, 1_000)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let mut last_sequence = batch.last_sequence as u64;
                    if batch.gap_detected {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            replay.after_sequence,
                            batch.first_available_sequence.map(|value| value as u64),
                            latest,
                            REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    for record in batch.events {
                        last_sequence = record.sequence_id as u64;
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: record.sequence_id as u64,
                            task_id: record.task_id,
                            event_type: record.event_type,
                            payload: record.payload,
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: None,
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    }
                    let end = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "replay.end".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &end.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::SelectModel(request)) => {
                    // Bounded: a model identifier is a short single-line token.
                    let model = request.model.trim();
                    if model.len() > 128 || model.contains(char::is_whitespace) {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(&serde_json::json!({ "reason": "invalid_model" }))
                                .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    }
                    let Some(route) = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                    else {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(
                                &serde_json::json!({ "reason": "provider_not_configured" }),
                            )
                            .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    };
                    let available = evohime_model_gateway::fetch_model_catalog(route)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    if !available.iter().any(|entry| entry.id == model) {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(
                                &serde_json::json!({ "reason": "model_not_returned_by_provider" }),
                            )
                            .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    }
                    self.selected_model.set(model);
                    let payload = serde_json::to_vec(&self.current_model_config())
                        .unwrap_or_else(|_| b"null".to_vec());
                    self.write_response(writer, "model.config", payload).await?;
                }
                Some(generated::command_envelope::Command::ModelConfig(_)) => {
                    let payload = serde_json::to_vec(&self.current_model_config())
                        .unwrap_or_else(|_| b"null".to_vec());
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "model.config".into(),
                        payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::ModelCatalog(request)) => {
                    let mode = if request.mode == "paid" {
                        "paid"
                    } else {
                        "free"
                    };
                    let provider = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                        .map(|route| route.provider.as_str().to_string())
                        .unwrap_or_else(|| "unknown".into());
                    let result = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                        .map(|route| async move {
                            evohime_model_gateway::fetch_model_catalog(route)
                                .await
                                .map(|entries| {
                                    entries
                                        .into_iter()
                                        .filter(|entry| {
                                            if mode == "free" {
                                                entry.id.ends_with(":free")
                                            } else {
                                                !entry.id.ends_with(":free")
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                })
                        });
                    let (entries, error) = match result {
                        Some(request) => request.await,
                        None => Err(evohime_model_gateway::providers::ProviderError::Config(
                            "provider is not configured".into(),
                        )),
                    }
                    .map_or_else(
                        |error| (Vec::new(), Some(error.to_string())),
                        |entries| (entries, None),
                    );
                    // Лимиты переживают сессию: планировщик контекста и ревью
                    // должны знать окно модели ещё до первого обновления каталога,
                    // а неудачный запрос не должен стирать то, что уже известно.
                    self.remember_model_limits(&provider, &entries).await;
                    let models = entries
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect::<Vec<_>>();
                    let limits = entries
                        .iter()
                        .map(|entry| {
                            (
                                entry.id.clone(),
                                serde_json::json!({
                                    "context": entry.context_tokens,
                                    "maxOutput": entry.max_output_tokens,
                                }),
                            )
                        })
                        .collect::<serde_json::Map<_, _>>();
                    let payload = serde_json::json!({
                        "mode": mode,
                        "models": models,
                        "limits": limits,
                        "error": error,
                    });
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "model.catalog".into(),
                        payload: serde_json::to_vec(&payload).unwrap_or_default(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::StartPlanReview(request)) => {
                    self.start_plan_review(request, writer).await?;
                }
                Some(generated::command_envelope::Command::StopPlanReview(request)) => {
                    let cancelled = self
                        .review_tasks
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if let Some(ref token) = cancelled {
                        token.cancel();
                    }
                    self.write_response(
                        writer,
                        "review.stop.accepted",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "accepted": cancelled.is_some(),
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListPlanReviews(request)) => {
                    let limit = (request.limit as usize).clamp(1, 50);
                    let results = self.review_results.lock().await;
                    let mut items: Vec<_> = results.values().cloned().collect();
                    drop(results);
                    if let Ok(events) = self.journal.review_history(limit).await {
                        for event in events {
                            if let Some(result) = review_result_from_event(&event.payload) {
                                if !items.iter().any(|item| item.review_id == result.review_id) {
                                    items.push(result);
                                }
                            }
                        }
                    }
                    items.sort_by(|left, right| left.review_id.cmp(&right.review_id));
                    items.truncate(limit);
                    self.write_response(
                        writer,
                        "review.list",
                        serde_json::to_vec(&serde_json::json!({ "reviews": items }))
                            .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ClearPlanReviewHistory(_)) => {
                    // Running reviews keep their own state; only what the history
                    // lists is dropped, and the marker is what listing reads.
                    self.review_results.lock().await.clear();
                    let marker_id = format!("review-history-{}", self.latest_sequence().await);
                    // Recorded directly rather than published: the shell lists again
                    // as soon as this response arrives, and a marker still travelling
                    // through the coordinator's broadcast would not be in the journal
                    // yet, so that listing would return the reviews just cleared.
                    // Nothing subscribes to the marker, so no push is lost.
                    let _ = self
                        .journal
                        .record(&CoreEvent::ReviewHistoryCleared { marker_id })
                        .await;
                    self.write_response(
                        writer,
                        "review.historyCleared",
                        serde_json::to_vec(&serde_json::json!({ "cleared": true }))
                            .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetPlanReview(request)) => {
                    let mut result = self
                        .review_results
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) = self.journal.task_history(&request.review_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| review_result_from_event(&event.payload));
                        }
                    }
                    self.write_response(
                        writer,
                        "review.result",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "result": result,
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExportPlanReview(request)) => {
                    let mut result = self
                        .review_results
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) = self.journal.task_history(&request.review_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| review_result_from_event(&event.payload));
                        }
                    }
                    let result = result.ok_or_else(|| FrameError::Io("review not found".into()))?;
                    let destination = std::path::PathBuf::from(&request.destination_path);
                    if destination.extension().and_then(|value| value.to_str()) != Some("md") {
                        return Err(
                            FrameError::Io("review export must be a Markdown file".into()).into(),
                        );
                    }
                    let content = if request.include_reviewers {
                        serde_json::to_string_pretty(&result).unwrap_or_default()
                    } else {
                        result.final_markdown.clone()
                    };
                    tokio::fs::write(&destination, content)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(
                        writer,
                        "review.exported",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "destination_path": request.destination_path,
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::RevisePlan(request)) => {
                    self.revise_plan(request, writer).await?;
                }
                Some(generated::command_envelope::Command::StopRevision(request)) => {
                    let cancelled = self
                        .revision_tasks
                        .lock()
                        .await
                        .get(&request.revision_id)
                        .cloned();
                    if let Some(ref token) = cancelled {
                        token.cancel();
                    }
                    self.write_response(
                        writer,
                        "revision.stop.accepted",
                        serde_json::to_vec(&serde_json::json!({
                            "revision_id": request.revision_id,
                            "accepted": cancelled.is_some(),
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::SaveRevisedPlan(request)) => {
                    // Правка переживает перезапуск ядра: обновление Евы перезапускает
                    // Core, а нажать «сохранить» пользователь может и после этого.
                    let mut result = self
                        .revision_results
                        .lock()
                        .await
                        .get(&request.revision_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) =
                            self.journal.task_history(&request.revision_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| revision_result_from_event(&event.payload));
                        }
                    }
                    // Отказ отвечает событием, а не ошибкой кадра: ошибка кадра рвёт
                    // соединение с оболочкой, и опечатка в имени файла выглядела бы
                    // как падение ядра.
                    let failure = match &result {
                        None => Some("правка не найдена: запусти её заново".to_string()),
                        Some(_)
                            if std::path::Path::new(&request.destination_path)
                                .extension()
                                .and_then(|value| value.to_str())
                                != Some("md") =>
                        {
                            Some("сохранить план можно только в файл .md".to_string())
                        }
                        Some(_) => None,
                    };
                    let failure = match (failure, result) {
                        (Some(reason), _) => Some(reason),
                        (None, Some(result)) => {
                            tokio::fs::write(&request.destination_path, &result.revised_markdown)
                                .await
                                .err()
                                .map(|error| error.to_string())
                        }
                        (None, None) => Some("правка не найдена: запусти её заново".to_string()),
                    };
                    match failure {
                        Some(error) => {
                            self.write_response(
                                writer,
                                "plan.save_failed",
                                serde_json::to_vec(&serde_json::json!({
                                    "revision_id": request.revision_id,
                                    "destination_path": request.destination_path,
                                    "error": error,
                                }))
                                .unwrap_or_default(),
                            )
                            .await?;
                        }
                        None => {
                            self.write_response(
                                writer,
                                "plan.saved",
                                serde_json::to_vec(&serde_json::json!({
                                    "revision_id": request.revision_id,
                                    "destination_path": request.destination_path,
                                }))
                                .unwrap_or_default(),
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::PermissionMode(request)) => {
                    if let Some(tools) = &self.tools {
                        let mode = match request.mode.as_str() {
                            "full" => PermissionMode::Allow,
                            "read_only" => PermissionMode::Deny,
                            _ => PermissionMode::Ask,
                        };
                        tools.permissions().set_all_modes(mode).await;
                        if request.mode == "read_only" {
                            tools
                                .permissions()
                                .set_mode(Permission::FilesystemRead, PermissionMode::Allow)
                                .await;
                            tools
                                .permissions()
                                .set_mode(Permission::GitRead, PermissionMode::Allow)
                                .await;
                        }
                    }
                }
                Some(generated::command_envelope::Command::CreateProject(request)) => {
                    let result = self
                        .dispatch_create_project(client_id, request_id, command_hash, request)
                        .await?;
                    self.write_response(writer, "project.created", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateTask(request)) => {
                    let item = WorkItemRecord {
                        id: request.task_id,
                        project_id: request.project_id,
                        parent_id: (!request.parent_id.is_empty()).then_some(request.parent_id),
                        title: request.title,
                        description: request.description,
                        source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                        acceptance_criteria: request.acceptance_criteria,
                        non_goals: request.non_goals,
                        status: if request.status.is_empty() {
                            "backlog".into()
                        } else {
                            request.status
                        },
                        priority: request.priority,
                        estimate: (request.estimate != 0).then_some(request.estimate),
                        complexity: (!request.complexity.is_empty()).then_some(request.complexity),
                        attempt_count: 0,
                        version: 1,
                    };
                    let result = self
                        .dispatch_create_task(client_id, request_id, command_hash, item)
                        .await?;
                    self.write_response(writer, "task.created", result).await?;
                }
                Some(generated::command_envelope::Command::UpdateTaskStatus(request)) => {
                    let result = self
                        .dispatch_update_status(
                            client_id,
                            request_id,
                            command_hash,
                            request.task_id,
                            request.expected_version,
                            request.status,
                        )
                        .await?;
                    self.write_response(writer, "task.status_updated", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::AddTaskEdge(request)) => {
                    let result = self
                        .dispatch_add_edge(
                            client_id,
                            request_id,
                            command_hash,
                            request.from_task_id,
                            request.to_task_id,
                            request.kind,
                        )
                        .await?;
                    self.write_response(writer, "task.edge_added", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetTaskGraph(request)) => {
                    let result = self.dispatch_get_task_graph(request.project_id).await?;
                    self.write_response(writer, "task.graph", result).await?;
                }
                Some(generated::command_envelope::Command::NextReadyTask(request)) => {
                    let result = self.dispatch_next_ready_task(request.project_id).await?;
                    self.write_response(writer, "task.next_ready", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ImportPrd(request)) => {
                    let result = self
                        .dispatch_import_prd(client_id, request_id, command_hash, request)
                        .await?;
                    self.write_response(writer, "prd.imported", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskHistory(request)) => {
                    let result = self
                        .dispatch_get_task_history(request.task_id, request.limit as usize)
                        .await?;
                    self.write_response(writer, "task.history", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskContext(request)) => {
                    let result = self
                        .dispatch_get_task_context(
                            request.project_id,
                            request.task_id,
                            request.max_chars as usize,
                        )
                        .await?;
                    self.write_response(writer, "task.context", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskPlanSpec(request)) => {
                    let result = self
                        .dispatch_get_task_plan_spec(
                            request.project_id,
                            request.task_id,
                            request.max_chars as usize,
                        )
                        .await?;
                    self.write_response(writer, "task.plan_spec", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ApplyApprovedBuild(request)) => {
                    let result = self
                        .dispatch_apply_approved_build(
                            request.project_id,
                            request.run_id,
                            request.task_id,
                            request.approved_build_json,
                        )
                        .await?;
                    self.write_response(writer, "build.applied", result).await?;
                }
                Some(generated::command_envelope::Command::PrepareBuild(request)) => {
                    let result = self
                        .dispatch_prepare_build(request.project_id, request.proposal_json)
                        .await?;
                    self.write_response(writer, "build.prepared", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetTaskSnapshot(request)) => {
                    let result = self
                        .dispatch_get_task_snapshot(request.project_id, request.task_id)
                        .await?;
                    self.write_response(writer, "task.snapshot", result).await?;
                }
                Some(generated::command_envelope::Command::RestoreTaskSnapshot(request)) => {
                    let result = self
                        .dispatch_restore_task_snapshot(
                            request.project_id,
                            request.task_id,
                            request.snapshot_id,
                        )
                        .await?;
                    self.write_response(writer, "snapshot.restored", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetBuildPolicy(request)) => {
                    let result = self.dispatch_get_build_policy(request.project_id).await?;
                    self.write_response(writer, "build.policy", result).await?;
                }
                Some(generated::command_envelope::Command::SaveBuildPolicy(request)) => {
                    let result = self
                        .dispatch_save_build_policy(
                            request.project_id,
                            request.policy_json,
                            request.expected_version,
                        )
                        .await?;
                    self.write_response(writer, "build.policy.saved", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::StartTask(start)) => {
                    if let Some(coordinator) = &self.coordinator {
                        coordinator
                            .dispatch(CoreCommand::StartTask {
                                task_id: start.task_id,
                                prompt: start.prompt,
                                workspace_root: (!start.workspace_path.is_empty())
                                    .then(|| std::path::PathBuf::from(start.workspace_path)),
                                preferred_route_hint: match start.preferred_route_hint.as_str() {
                                    "local" | "cloud" => Some(start.preferred_route_hint),
                                    "codex_cli" if start.execution_kind == "coding" => {
                                        Some("codex_cli".into())
                                    }
                                    _ => None,
                                },
                            })
                            .await
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                }
                Some(generated::command_envelope::Command::GetTaskCheckpoint(request)) => {
                    let projection = self.dispatch_get_task_checkpoint(request).await;
                    self.write_task_checkpoint_projection(writer, projection)
                        .await?;
                }
                Some(generated::command_envelope::Command::ResolveTaskCheckpoint(request)) => {
                    let result = self.dispatch_resolve_task_checkpoint(request).await?;
                    self.write_task_checkpoint_action_result(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListSkills(request)) => {
                    self.dispatch_list_skills(request, writer).await?;
                }
                Some(generated::command_envelope::Command::LoadSkill(request)) => {
                    self.dispatch_load_skill(request, writer).await?;
                }
                Some(generated::command_envelope::Command::LoadSkillReference(request)) => {
                    self.dispatch_load_skill_reference(request, writer).await?;
                }
                Some(generated::command_envelope::Command::CreateGoal(request)) => {
                    let result = self.dispatch_create_goal(request, &command_hash).await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::GetGoal(request)) => {
                    let projection = self.dispatch_get_goal(request).await;
                    self.write_goal_projection(writer, projection).await?;
                }
                Some(generated::command_envelope::Command::ListGoals(request)) => {
                    let projection = self.dispatch_list_goals(request).await;
                    self.write_goal_list_projection(writer, projection).await?;
                }
                Some(generated::command_envelope::Command::PauseGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Paused,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::ResumeGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Active,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::CancelGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Cancelled,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::UpdateGoal(request)) => {
                    let result = self.dispatch_update_goal(request, &command_hash).await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::VerifyGoalCriterion(request)) => {
                    let result = self
                        .dispatch_verify_goal_criterion(request, &command_hash)
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::LinkGoalReference(request)) => {
                    let result = self
                        .dispatch_link_goal_reference(request, &command_hash)
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::SaveContinuationPolicy(request)) => {
                    let result = self
                        .dispatch_save_continuation_policy(
                            request,
                            &client_id,
                            &request_id,
                            &command_hash,
                        )
                        .await;
                    self.write_response(
                        writer,
                        "continuation.policy",
                        result.unwrap_or_else(|error| {
                            serde_json::to_vec(&serde_json::json!({"error_code": error}))
                                .unwrap_or_default()
                        }),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::StartContinuationRun(request)) => {
                    let result = self.dispatch_start_continuation(request).await;
                    let payload = result.unwrap_or_else(|error| {
                        serde_json::to_vec(&serde_json::json!({"error_code": error}))
                            .unwrap_or_default()
                    });
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_projection(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.run", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::GetContinuationRun(request)) => {
                    let result = self.dispatch_get_continuation(request).await;
                    let payload = result.unwrap_or_else(|error| {
                        serde_json::to_vec(&serde_json::json!({"error_code": error}))
                            .unwrap_or_default()
                    });
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_projection(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.run", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::StopContinuation(request)) => {
                    let result = self.dispatch_stop_continuation(request).await;
                    let payload = result.unwrap_or_else(|error| {
                        serde_json::to_vec(&serde_json::json!({"error_code": error}))
                            .unwrap_or_default()
                    });
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_action(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.action", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::ListRetainedChildren(request)) => {
                    let (reply, response) = oneshot::channel();
                    let parent_id = client_id.clone();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::ListRetainedChildren {
                            parent_id,
                            now_ms: crate::task_memory::now_millis(),
                            limit: request.limit,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.list", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetRetainedChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::GetRetainedChild {
                            parent_id: client_id.clone(),
                            child_id: request.child_id,
                            now_ms: crate::task_memory::now_millis(),
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::RetainChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let now_ms = crate::task_memory::now_millis();
                    let child = crate::retained_child::RetainedChildV1 {
                        version: 1,
                        child_id: request.child_id,
                        parent_id: client_id.clone(),
                        family_root_id: if request.family_root_id.is_empty() {
                            client_id.clone()
                        } else {
                            request.family_root_id
                        },
                        role: request.role,
                        stable_name: (!request.stable_name.is_empty())
                            .then_some(request.stable_name),
                        lifecycle: crate::retained_child::RetainedLifecycle::Active,
                        revision: request.revision,
                        active_session_id: None,
                        grant_snapshot_hash: request.grant_snapshot_hash,
                        context_scope_hash: request.context_scope_hash,
                        workspace_state_ref: (!request.workspace_state_ref.is_empty())
                            .then_some(request.workspace_state_ref),
                        last_report_ref: (!request.last_report_ref.is_empty())
                            .then_some(request.last_report_ref),
                        retained_until_ms: if request.retained_until_ms == 0 {
                            now_ms.saturating_add(crate::retained_child::DEFAULT_TTL_MS)
                        } else {
                            request.retained_until_ms
                        },
                        created_at_ms: if request.created_at_ms == 0 {
                            now_ms
                        } else {
                            request.created_at_ms
                        },
                        last_active_at_ms: if request.last_active_at_ms == 0 {
                            now_ms
                        } else {
                            request.last_active_at_ms
                        },
                        registry_version: request.expected_registry_version.saturating_add(1),
                    };
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::RetainChild {
                            child,
                            now_ms,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.retained", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::SendChildFollowUp(request)) => {
                    let (reply, response) = oneshot::channel();
                    let mode = match request.mode.as_str() {
                        "auto" => crate::retained_child::FollowUpMode::Auto,
                        "follow_up" | "" => crate::retained_child::FollowUpMode::FollowUp,
                        "steer" => crate::retained_child::FollowUpMode::Steer,
                        _ => {
                            self.write_response(
                                writer,
                                "retained_child.follow_up",
                                b"{\"error_code\":\"invalid_scope\"}".to_vec(),
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let follow = crate::retained_child::ChildFollowUpRequestV1 {
                        version: 1,
                        idempotency_key: request.idempotency_key,
                        parent_id: client_id.clone(),
                        child_id: request.child_id,
                        family_root_id: client_id.clone(),
                        parent_sequence: 0,
                        expected_child_revision: request.expected_child_revision,
                        instruction: request.instruction,
                        context_refs: request.context_refs,
                        requested_grants: request.requested_grants,
                        budget_json: request.budget_json,
                        mode,
                        correlation_id: request.correlation_id,
                    };
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::SendChildFollowUp {
                            request: follow,
                            now_ms: crate::task_memory::now_millis(),
                            busy: false,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.follow_up", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::DeleteRetainedChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::DeleteRetainedChild {
                            parent_id: client_id.clone(),
                            child_id: request.child_id,
                            expected_registry_version: request.expected_registry_version,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.delete", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateAnalysisKernel(request)) => {
                    let projection = self.dispatch_create_analysis_kernel(request).await;
                    write_analysis_kernel_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetAnalysisKernel(request)) => {
                    let projection = self.dispatch_get_analysis_kernel(request).await;
                    write_analysis_kernel_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExecuteAnalysisKernel(request)) => {
                    let result = self.dispatch_execute_analysis_kernel(request).await;
                    write_analysis_kernel_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ResetAnalysisKernel(request)) => {
                    let result = self.dispatch_reset_analysis_kernel(request).await;
                    write_analysis_kernel_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListRefinementCandidates(request)) => {
                    let projection = self.dispatch_list_refinement_candidates(request).await;
                    write_refinement_list_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetRefinementCandidate(request)) => {
                    let projection = self.dispatch_get_refinement_candidate(request).await;
                    write_refinement_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::RefinementAction(request)) => {
                    let result = self.dispatch_refinement_action(request).await;
                    write_refinement_action_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::PreviewWorkflowPackage(request)) => {
                    let result = crate::workflow_package::preview_from_json(
                        &request.graph_json,
                        request.name,
                        request.description,
                        request.portable_argument_keys,
                        &request.credential_slots_json,
                        request.created_at,
                    )
                    .map(|preview| serde_json::json!({
                        "status": "previewed",
                        "package_hash": preview.package_hash,
                        "stripped_fields": preview.stripped_fields,
                        "package": preview.package,
                    }))
                    .map_err(|error| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "preview", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::ExportWorkflowPackage(request)) => {
                    let result = crate::workflow_package::preview_from_json(
                        &request.graph_json,
                        request.name,
                        request.description,
                        request.portable_argument_keys,
                        &request.credential_slots_json,
                        request.created_at,
                    )
                    .and_then(|preview| {
                        crate::workflow_package::write_package(
                            std::path::Path::new(&request.destination_path),
                            &preview.package,
                        )?;
                        Ok(serde_json::json!({"status":"exported","package_hash":preview.package_hash,"stripped_fields":preview.stripped_fields}))
                    })
                    .map_err(|error: crate::workflow_package::WorkflowPackageError| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "export", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::CommitWorkflowPackage(request)) => {
                    let result = async {
                        let package = crate::workflow_package::parse_bounded(&request.package_json)?;
                        let database = self.journal.database().lock().await;
                        crate::workflow_package::commit_import(
                            &database,
                            std::path::Path::new(&request.source_path),
                            &package,
                            &request.idempotency_key,
                            now_ms(),
                        )
                    }
                    .await
                    .map(|record| serde_json::json!({"status":"committed","import_id":record.import_id,"local_workflow_id":record.local_workflow_id,"package_hash":record.package_hash}))
                    .map_err(|error: crate::workflow_package::WorkflowPackageError| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "commit", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::RebindWorkflowPackage(request)) => {
                    let result = async {
                        let package =
                            crate::workflow_package::parse_bounded(&request.package_json)?;
                        let database = self.journal.database().lock().await;
                        crate::workflow_package::persist_rebind(
                            &database,
                            &package,
                            &request.slot_id,
                            &request.local_credential_reference,
                            now_ms(),
                        )
                    }
                    .await;
                    let payload = match result {
                        Ok(value) => serde_json::to_vec(
                            &serde_json::json!({"status":"rebound","binding":value}),
                        )?,
                        Err(error) => serde_json::to_vec(
                            &serde_json::json!({"status":"rejected","error_code":error.to_string()}),
                        )?,
                    };
                    self.write_package_response(writer, "rebind", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::StopTask(stop)) => {
                    if let Some(coordinator) = &self.coordinator {
                        coordinator
                            .dispatch(CoreCommand::StopTask {
                                task_id: stop.task_id,
                            })
                            .await
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                }
                Some(generated::command_envelope::Command::ListWorkspace(request)) => {
                    let listing = crate::workspace::list_directory(
                        request.workspace_path,
                        if request.relative_path.is_empty() {
                            "."
                        } else {
                            &request.relative_path
                        },
                        if request.max_entries == 0 {
                            crate::workspace::MAX_LIST_ENTRIES
                        } else {
                            request.max_entries as usize
                        },
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = serde_json::to_vec(&listing)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "workspace.list", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::PauseContinuation(request)) => {
                    let payload = self
                        .dispatch_transition_continuation(
                            request.run_id,
                            request.idempotency_key,
                            request.expected_state,
                            "paused",
                            "pause",
                        )
                        .await
                        .unwrap_or_else(|error| {
                            serde_json::to_vec(&serde_json::json!({"error_code": error}))
                                .unwrap_or_default()
                        });
                    self.write_continuation_action(writer, payload).await?;
                }
                Some(generated::command_envelope::Command::ResumeContinuation(request)) => {
                    let run = self.dispatch_resume_continuation(request).await;
                    let payload = match run {
                        Ok(run) => {
                            if let Some(coordinator) = &self.coordinator {
                                if let (Some(prompt), Some(workspace_path)) =
                                    (run.prompt.clone(), run.workspace_path.clone())
                                {
                                    let _ = coordinator
                                        .dispatch(CoreCommand::StartTask {
                                            task_id: run.task_id.clone(),
                                            prompt,
                                            workspace_root: Some(workspace_path.into()),
                                            preferred_route_hint: None,
                                        })
                                        .await;
                                }
                            }
                            serde_json::to_vec(&serde_json::json!({
                                "run_id": run.run_id,
                                "action": "resume",
                                "applied": true,
                                "error_code": ""
                            }))
                            .unwrap_or_default()
                        }
                        Err(error) => serde_json::to_vec(&serde_json::json!({"error_code": error}))
                            .unwrap_or_default(),
                    };
                    self.write_continuation_action(writer, payload).await?;
                }
                Some(generated::command_envelope::Command::ReadWorkspaceFile(request)) => {
                    let content = crate::workspace::read_text_file(
                        request.workspace_path,
                        &request.relative_path,
                        if request.max_bytes == 0 {
                            crate::workspace::MAX_READ_BYTES
                        } else {
                            request.max_bytes as usize
                        },
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = serde_json::to_vec(&serde_json::json!({
                        "path": request.relative_path,
                        "content": content,
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "workspace.file", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::GitStatus(request)) => {
                    let payload = self
                        .dispatch_git_read(
                            request.workspace_path,
                            "git.status",
                            serde_json::Value::Null,
                            request.max_bytes,
                        )
                        .await?;
                    self.write_response(writer, "git.status", payload).await?;
                }
                Some(generated::command_envelope::Command::GitDiff(request)) => {
                    let input = if request.relative_path.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!({"path": request.relative_path})
                    };
                    let payload = self
                        .dispatch_git_read(
                            request.workspace_path,
                            "git.diff",
                            input,
                            request.max_bytes,
                        )
                        .await?;
                    self.write_response(writer, "git.diff", payload).await?;
                }
                Some(generated::command_envelope::Command::TerminalExecute(request)) => {
                    self.dispatch_terminal_execute(request, writer).await?;
                }
                Some(generated::command_envelope::Command::RunDoctor(request)) => {
                    let result = self
                        .dispatch_run_doctor(
                            request.project_id,
                            request.detail_level,
                            command.protocol,
                        )
                        .await?;
                    self.write_response(writer, "doctor.report", result).await?;
                }
                Some(generated::command_envelope::Command::ExportDoctorLogs(request)) => {
                    let result = self
                        .dispatch_export_doctor_logs(request.destination_path)
                        .await?;
                    self.write_response(writer, "doctor.export.completed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateDatabaseBackup(request)) => {
                    self.dispatch_create_database_backup(
                        request_id,
                        request.destination_path,
                        writer,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::PrepareDatabaseRestore(request)) => {
                    let result = self
                        .dispatch_prepare_database_restore(request_id, request.backup_path)
                        .await?;
                    self.write_response(writer, "storage.restore.preview", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RestoreDatabase(request)) => {
                    self.dispatch_restore_database(
                        request_id,
                        request.backup_path,
                        request.approval_id,
                        writer,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::CancelDatabaseOperation(request)) => {
                    let result = self
                        .dispatch_cancel_database_operation(request.operation_id)
                        .await?;
                    self.write_response(writer, "storage.cancel.requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SaveResearchEvidence(request)) => {
                    let result = self.dispatch_save_research_evidence(request).await?;
                    self.write_response(writer, "research.evidence.saved", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListResearchEvidence(request)) => {
                    let result = self
                        .dispatch_list_research_evidence(request.work_item_id)
                        .await?;
                    self.write_response(writer, "research.evidence.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RunResearchFetch(request)) => {
                    let result = self.dispatch_run_research_fetch(request).await?;
                    self.write_response(writer, "research.fetch.completed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateMemory(request)) => {
                    let result = self.dispatch_create_memory(request).await?;
                    self.write_response(writer, "memory.created", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListMemory(request)) => {
                    let result = self.dispatch_list_memory(request).await?;
                    self.write_response(writer, "memory.list", result).await?;
                }
                Some(generated::command_envelope::Command::SearchMemory(request)) => {
                    let result = self.dispatch_search_memory(request).await?;
                    self.write_response(writer, "memory.search", result).await?;
                }
                Some(generated::command_envelope::Command::ArchiveMemory(request)) => {
                    let result = self.dispatch_archive_memory(request).await?;
                    self.write_response(writer, "memory.archived", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetMemory(request)) => {
                    let result = self.dispatch_get_memory(request).await?;
                    self.write_response(writer, "memory.record", result).await?;
                }
                Some(generated::command_envelope::Command::ListMemoryPending(request)) => {
                    let result = self.dispatch_list_memory_pending(request).await?;
                    self.write_response(writer, "memory.pending", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetMemoryConflicts(request)) => {
                    let result = self.dispatch_get_memory_conflicts(request).await?;
                    self.write_response(writer, "memory.conflicts", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ConfirmMemory(request)) => {
                    let result = self.dispatch_confirm_memory(request).await?;
                    self.write_response(writer, "memory.confirmed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RejectMemory(request)) => {
                    let result = self.dispatch_reject_memory(request).await?;
                    self.write_response(writer, "memory.rejected", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReviseMemoryCandidate(request)) => {
                    let result = self.dispatch_revise_memory_candidate(request).await?;
                    self.write_response(writer, "memory.revised", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SupersedeMemory(request)) => {
                    let result = self.dispatch_supersede_memory(request).await?;
                    self.write_response(writer, "memory.superseded", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ForgetMemory(request)) => {
                    let result = self.dispatch_forget_memory(request).await?;
                    self.write_response(writer, "memory.forgotten", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::InstallCapability(request)) => {
                    let result = self.dispatch_install_capability(request).await?;
                    self.write_response(writer, "capability.installed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListCapabilities(request)) => {
                    let result = self.dispatch_list_capabilities(request).await?;
                    self.write_response(writer, "capability.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::MatchCapabilities(request)) => {
                    let result = self.dispatch_match_capabilities(request).await?;
                    self.write_response(writer, "capability.match", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RemoveCapability(request)) => {
                    let result = self.dispatch_remove_capability(request).await?;
                    self.write_response(writer, "capability.removed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListToolkits(request)) => {
                    let result = self.dispatch_list_toolkits(request).await?;
                    self.write_response(writer, "toolkit.list", result).await?;
                }
                Some(generated::command_envelope::Command::EnableToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "rollback",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.enabled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::DisableToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "disabled",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.disabled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RollbackToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "enabled",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.rolled_back", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetCapabilitySelection(request)) => {
                    let result = self.dispatch_get_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PinCapabilitySelection(request)) => {
                    let result = self.dispatch_pin_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection.pinned", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReplaceCapabilitySelection(request)) => {
                    let result = self.dispatch_replace_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection.replaced", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RequestChildHandoff(request)) => {
                    let result = self.dispatch_request_child_handoff(request).await?;
                    self.write_response(writer, "child.handoff.requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListChildHandoffs(request)) => {
                    let result = self.dispatch_list_child_handoffs(request).await?;
                    self.write_response(writer, "child.handoff.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitChildRequest(request)) => {
                    let result = self.dispatch_submit_child_request(request).await?;
                    self.write_response(writer, "child.request.submitted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitChildReport(request)) => {
                    let result = self.dispatch_submit_child_report(request).await?;
                    self.write_response(writer, "child.report.accepted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitFeedback(request)) => {
                    let result = self.dispatch_submit_feedback(request).await?;
                    self.write_response(writer, "feedback.submitted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::IndexWorkspace(request)) => {
                    let result = self.dispatch_index_workspace(request, false).await?;
                    self.write_response(writer, "workspace.indexed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RebuildIndex(request)) => {
                    let result = self.dispatch_rebuild_index(request).await?;
                    self.write_response(writer, "workspace.indexed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SearchWorkspaceKnowledge(request)) => {
                    let result = self.dispatch_search_workspace_knowledge(request).await?;
                    self.write_response(writer, "workspace.knowledge", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetIndexStatus(request)) => {
                    let result = self.dispatch_get_index_status(request).await?;
                    self.write_response(writer, "workspace.index_status", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CancelWorkspaceIndex(request)) => {
                    let result = self.dispatch_cancel_workspace_index(request).await?;
                    self.write_response(writer, "workspace.index_cancelled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetContextLedger(request)) => {
                    let result = self.dispatch_get_context_ledger(request).await?;
                    self.write_response(writer, "context.ledger", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListTaskScratchpad(request)) => {
                    let result = self.dispatch_list_task_scratchpad(request).await?;
                    self.write_response(writer, "context.scratchpad", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ClearTaskScratchpad(request)) => {
                    let result = self.dispatch_clear_task_scratchpad(request).await?;
                    self.write_response(writer, "context.scratchpad_cleared", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SummarizeContextNow(request)) => {
                    let result = self.dispatch_summarize_context_now(request).await?;
                    self.write_response(writer, "context.summarize_requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PinContextItem(request)) => {
                    let result = self.dispatch_pin_context_item(request).await?;
                    self.write_response(writer, "context.item_pinned", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReadContextArtifact(request)) => {
                    let result = self.dispatch_read_context_artifact(request).await?;
                    self.write_response(writer, "context.artifact", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListFeedback(request)) => {
                    let result = self.dispatch_list_feedback(request).await?;
                    self.write_response(writer, "feedback.list", result).await?;
                }
                Some(generated::command_envelope::Command::ResolveApproval(resolve)) => {
                    // Cancellation is a terminal rejection at the existing
                    // approval boundary; the immutable approval binding remains
                    // owned by Core and old clients keep the same semantics.
                    let granted = resolve.granted && !resolve.cancel;
                    let approval_id = uuid::Uuid::parse_str(&resolve.approval_id)
                        .map_err(|error| FrameError::Io(format!("invalid approval id: {error}")))?;
                    if let Some(tools) = &self.tools {
                        let _ = tools.permissions().resolve(approval_id, granted).await;
                    }
                    if let Some(approvals) = &self.approvals {
                        let _ = approvals.resolve(approval_id, granted).await;
                    }
                    if !granted {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        ) {
                            let _ = runtime.deny_approval(approval_id);
                        }
                    }
                    let _ = self
                        .journal
                        .record_audit(
                            &resolve.approval_id,
                            "approval.decision",
                            serde_json::to_vec(&serde_json::json!({
                                "approval_id": resolve.approval_id,
                                "granted": granted,
                                "cancelled": resolve.cancel,
                                "idempotency_key": resolve.idempotency_key,
                                "rejection_reason": resolve.rejection_reason,
                            }))
                            .unwrap_or_default()
                            .as_slice(),
                        )
                        .await;
                    self.record_ledger_approval_decision(&resolve.approval_id, granted)
                        .await;
                    // Узел workflow подтверждается той же командой, что и
                    // инструмент: отдельного пути approval у workflow нет. Если
                    // идентификатор принадлежит узлу, запуск продолжается сам —
                    // иначе он остался бы ждать уже принятого решения.
                    if self
                        .workflow_approvals
                        .resolve(&resolve.approval_id, granted)
                    {
                        if let Some(run_id) = self.workflow_approvals.run_for(&resolve.approval_id)
                        {
                            let workspace = self.journal.workflow_run_workspace(&run_id).await;
                            self.spawn_workflow_drive(run_id, workspace);
                        }
                    }
                }
                Some(generated::command_envelope::Command::ResolveRoutingDecision(resolve)) => {
                    let coordinator = self.coordinator.as_ref().ok_or_else(|| {
                        FrameError::Io("core command queue is not configured".into())
                    })?;
                    let (reply, response) = oneshot::channel();
                    coordinator
                        .dispatch(CoreCommand::ResolveRoutingDecision {
                            trace_id: resolve.trace_id,
                            approve: resolve.approve,
                            reply,
                        })
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let result = response
                        .await
                        .map_err(|_| FrameError::Io("routing decision response dropped".into()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "routing.decision", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SetAmbientListening(request)) => {
                    let result = self.dispatch_set_ambient_listening(request).await;
                    self.write_response(writer, "ambient.listening", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAmbientStatus(_)) => {
                    let result = self.dispatch_get_ambient_status().await;
                    self.write_response(writer, "ambient.status", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListAmbientEpisodes(request)) => {
                    let result = self.dispatch_list_ambient_episodes(request).await;
                    self.write_response(writer, "ambient.episodes", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAmbientEpisode(request)) => {
                    let result = self.dispatch_get_ambient_episode(request).await;
                    self.write_response(writer, "ambient.episode", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::DeleteAmbientTranscripts(request)) => {
                    let result = self.dispatch_delete_ambient_transcripts(request).await;
                    self.write_response(writer, "ambient.deleted", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ForgetAmbientWindow(request)) => {
                    let result = self.dispatch_forget_ambient_window(request).await;
                    self.write_response(writer, "ambient.forgotten", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAmbientPolicy(_)) => {
                    let result = self.dispatch_get_ambient_policy().await;
                    self.write_response(writer, "ambient.policy", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::SaveAmbientPolicy(request)) => {
                    let result = self.dispatch_save_ambient_policy(request).await;
                    self.write_response(
                        writer,
                        "ambient.policy_saved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ResolveAmbientProposal(request)) => {
                    let result = self.dispatch_resolve_ambient_proposal(request).await;
                    // Имя ответа отличается от имени журнальной записи
                    // `ambient.proposal`: renderer подписан на неё как на событие,
                    // и ответ на команду не должен подменять собой список карточек.
                    self.write_response(
                        writer,
                        "ambient.proposal_resolved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListAmbientProposals(request)) => {
                    let result = self.dispatch_list_ambient_proposals(request).await;
                    self.write_response(writer, "ambient.proposals", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListVoiceCommands(request)) => {
                    let result = self.dispatch_list_voice_commands(request);
                    self.write_response(
                        writer,
                        "ambient.voice_commands",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ResolveVoiceCommand(request)) => {
                    let result = self.dispatch_resolve_voice_command(request).await;
                    self.write_response(
                        writer,
                        "ambient.voice_command_resolved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListWorkflowTemplates(_)) => {
                    let result = self.dispatch_list_workflow_templates();
                    self.write_response(writer, "workflow.templates", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetWorkflowDefinition(request)) => {
                    let result = self.dispatch_workflow_definition(request);
                    self.write_response(
                        writer,
                        "workflow.definition",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::StartWorkflow(request)) => {
                    let result = self.dispatch_start_workflow(request).await;
                    self.write_response(writer, "workflow.started", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetWorkflowRun(request)) => {
                    let result = self.dispatch_workflow_run(request).await;
                    self.write_response(writer, "workflow.run", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::CancelWorkflow(request)) => {
                    let result = self.dispatch_cancel_workflow(request).await;
                    self.write_response(writer, "workflow.cancelled", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListWorkflowEvents(request)) => {
                    let result = self.dispatch_list_workflow_events(request).await;
                    self.write_response(writer, "workflow.events", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::VisualWorkflowBuilder(request)) => {
                    let result = self.dispatch_visual_workflow_builder(request).await;
                    self.write_response(
                        writer,
                        "workflow_builder.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ConversationalWorkflowComposer(
                    request,
                )) => {
                    let result = self
                        .dispatch_conversational_workflow_composer(request)
                        .await;
                    self.write_response(
                        writer,
                        "workflow_composer.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::IntegrationProviderSdkCatalog(
                    request,
                ))
                | Some(generated::command_envelope::Command::IntegrationProviderSdkAction(
                    request,
                )) => {
                    let result = self.dispatch_integration_provider_sdk(request);
                    self.write_response(
                        writer,
                        "integration_provider_sdk.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::EventTriggerRuntimeList(request))
                | Some(generated::command_envelope::Command::EventTriggerRuntimeAction(request)) => {
                    let result = self.dispatch_event_trigger_runtime(request);
                    self.write_response(
                        writer,
                        "event_trigger_runtime.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::InvocationPresetList(request))
                | Some(generated::command_envelope::Command::InvocationPresetAction(request)) => {
                    let result = self.dispatch_invocation_preset(request).await;
                    self.write_response(
                        writer,
                        "invocation_preset.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListAutomationSchedules(request)) => {
                    let result = self.dispatch_list_automation_schedules(request).await;
                    self.write_response(
                        writer,
                        "automation.schedules",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::SaveAutomationSchedule(request)) => {
                    let result = self.dispatch_save_automation_schedule(request).await;
                    self.write_response(
                        writer,
                        "automation.schedule_saved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::TriggerAutomation(request)) => {
                    let result = self.dispatch_trigger_automation(request).await;
                    self.write_response(
                        writer,
                        "automation.triggered",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListAutomationRuns(request)) => {
                    let result = self.dispatch_list_automation_runs(request).await;
                    self.write_response(writer, "automation.runs", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAutomationRun(request)) => {
                    let result = self.dispatch_get_automation_run(request).await;
                    self.write_response(writer, "automation.run", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListAutomationEvents(request)) => {
                    let result = self.dispatch_list_automation_events(request).await;
                    self.write_response(writer, "automation.events", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::CancelAutomationRun(request)) => {
                    let result = self.dispatch_cancel_automation_run(request).await;
                    self.write_response(
                        writer,
                        "automation.cancelled",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::SetAutomationScheduleEnabled(
                    request,
                )) => {
                    let result = self.dispatch_set_automation_schedule_enabled(request).await;
                    self.write_response(
                        writer,
                        "automation.schedule_enabled",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                None => {}
            }
            Ok(())
        })
    }

    // ------------------------------------------------------------------
    // Постоянное слушание (план 04.5).
    //
    // Девять команд ходят прямо через мост, а не через очередь задач: им
    // нужны журнал, разрешения и реестр состояния, и ни одна из них не
    // запускает агента. Ответ уходит JSON-полезной нагрузкой тем же
    // `write_response`, что и у чеков.
    // ------------------------------------------------------------------

    /// Включение, пауза и смена устройства — одна команда с тремя полями.
    ///
    /// Порядок здесь и есть контракт: сперва проверки, потом сохранение
    /// намерения на диск, потом команда листенеру. Намерение переживает
    /// отсутствие листенера — иначе включение при упавшем процессе молча
    /// пропало бы, а пользователь считал бы, что микрофон включён.
    async fn dispatch_set_ambient_listening(
        &self,
        request: generated::SetAmbientListening,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;

        let data_dir = self.ambient_data_dir();
        let snapshot = self.ambient.snapshot().await;

        // Идентификатор устройства проходит bounded-контракт 04.1: через это
        // поле нельзя протащить фразу.
        if !request.device_id.is_empty()
            && evohime_listener_contract::DeviceId::new(request.device_id.clone()).is_err()
        {
            return listening_result(snapshot.state, Some(Code::InvalidArgument));
        }
        if !request.device_id.is_empty()
            && !snapshot
                .devices
                .iter()
                .any(|device| device.device_id == request.device_id)
        {
            return listening_result(snapshot.state, Some(Code::DeviceDisconnected));
        }

        // Микрофон открывается только явным именованным вызовом: общий режим
        // доступа его не трогает (инвариант 04.1), поэтому и здесь он
        // выставляется отдельно и по имени.
        if let Some(tools) = &self.tools {
            tools
                .permissions()
                .set_mode(
                    Permission::MicrophoneListen,
                    if request.enabled {
                        PermissionMode::Allow
                    } else {
                        PermissionMode::Deny
                    },
                )
                .await;
        }

        let mut policy = crate::ambient::load_policy(&data_dir);
        policy.paused = request.paused;
        if crate::ambient::save_policy(&data_dir, &policy).is_err() {
            return listening_result(snapshot.state, Some(Code::StorageFailed));
        }
        let control = crate::ambient::AmbientControl {
            enabled: request.enabled,
            device_id: if request.device_id.is_empty() {
                snapshot.active_device_id.clone()
            } else {
                request.device_id.clone()
            },
        };
        if crate::ambient::save_control(&data_dir, &control).is_err() {
            return listening_result(snapshot.state, Some(Code::StorageFailed));
        }

        let sent = self
            .ambient
            .send(crate::ambient::ListenerControl::Policy(Box::new((
                policy,
                control.clone(),
            ))))
            .await;
        if let Err(code) = sent {
            // Листенера нет. Намерение уже сохранено и применится при его
            // следующем подключении, но утверждать, что микрофон включён,
            // нельзя.
            self.ambient
                .set_state(
                    ListeningState::EngineUnavailable,
                    ListeningReason::EngineUnavailable,
                    None,
                )
                .await;
            self.publish_ambient_state().await;
            return listening_result(ListeningState::EngineUnavailable, Some(code));
        }

        // Устройство занято другим приложением — включать нечего, и
        // оптимистичное «запускаюсь» здесь было бы враньём.
        if request.enabled && snapshot.state == ListeningState::DeviceConflict {
            return listening_result(snapshot.state, Some(Code::DeviceConflict));
        }

        // Оптимистичное состояние: настоящее приедет от листенера отдельным
        // `ambient.state`, и именно оно останется в реестре.
        let (state, reason) = if !request.enabled {
            (ListeningState::Stopped, ListeningReason::UserRequest)
        } else if request.paused {
            (ListeningState::PausedByUser, ListeningReason::UserRequest)
        } else {
            (ListeningState::Starting, ListeningReason::UserRequest)
        };
        let device_id = control.device_id.clone();
        if self.ambient.set_state(state, reason, Some(device_id)).await {
            self.publish_ambient_state().await;
        }
        let engine_ready = self.ambient.engine_ready().await;
        let failure =
            (request.enabled && !request.paused && !engine_ready).then_some(Code::EngineNotReady);
        listening_result(state, failure)
    }

    /// Публикует текущее состояние реестра одним `ambient.state`.
    async fn publish_ambient_state(&self) {
        let snapshot = self.ambient.snapshot().await;
        let _ = self
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::State {
                state: snapshot.state,
                reason: snapshot.reason,
                active_device_id: evohime_listener_contract::DeviceId::new(
                    snapshot.active_device_id,
                )
                .ok(),
            })
            .await;
    }

    async fn dispatch_get_ambient_status(&self) -> serde_json::Value {
        let snapshot = self.ambient.snapshot().await;
        serde_json::json!({
            "state": snapshot.state,
            "reason": snapshot.reason,
            "active_device_id": snapshot.active_device_id,
            "engine_version": snapshot.engine_version,
            "engine_ready": snapshot.engine_ready,
            "devices": snapshot.devices,
            "watching_devices": snapshot.watching_devices,
        })
    }

    /// Список эпизодов. Текста здесь нет: он отдаётся только
    /// `GetAmbientEpisode` и только по явному клику пользователя.
    async fn dispatch_list_ambient_episodes(
        &self,
        request: generated::ListAmbientEpisodes,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            50usize
        } else {
            (request.limit as usize).min(200)
        };
        // Стор отдаёт свежие первыми и не умеет курсора, поэтому окно
        // вырезается здесь: берётся на одну строку больше запрошенного, и
        // лишняя строка и есть ответ на вопрос «есть ли ещё».
        let records = match self.journal.list_ambient_episodes(limit * 4).await {
            Ok(records) => records,
            Err(code) => return serde_json::json!({ "error_code": code.as_str() }),
        };
        let mut rows: Vec<serde_json::Value> = Vec::new();
        let mut skipping = !request.cursor.is_empty();
        let mut next_cursor = String::new();
        for record in records {
            if skipping {
                if record.episode_id == request.cursor {
                    skipping = false;
                }
                continue;
            }
            let started_at_ms = parse_timestamp_ms(&record.started_at);
            if request.since_ms > 0 && started_at_ms < request.since_ms {
                continue;
            }
            if rows.len() == limit {
                next_cursor = record.episode_id;
                break;
            }
            rows.push(serde_json::json!({
                "episode_id": record.episode_id,
                "started_at_ms": started_at_ms,
                "speech_duration_ms": record.speech_ms,
                "utterance_count": record.utterance_count,
                "extraction_state": record.extraction_state.as_str(),
            }));
        }
        serde_json::json!({ "episodes": rows, "next_cursor": next_cursor })
    }

    /// Единственный путь, по которому распознанный текст пересекает границу
    /// IPC. Вызывается только явным раскрытием эпизода в панели.
    async fn dispatch_get_ambient_episode(
        &self,
        request: generated::GetAmbientEpisode,
    ) -> serde_json::Value {
        if request.episode_id.is_empty() {
            return serde_json::json!({
                "error_code": evohime_listener_contract::AmbientErrorCode::InvalidArgument.as_str()
            });
        }
        match self
            .journal
            .list_ambient_utterances(&request.episode_id, 500)
            .await
        {
            Ok(records) => serde_json::json!({
                "episode_id": request.episode_id,
                "utterances": records
                    .into_iter()
                    .map(|record| serde_json::json!({
                        "utterance_id": record.utterance_id,
                        "started_at_ms": parse_timestamp_ms(&record.started_at),
                        "duration_ms": record.duration_ms,
                        "text": record.text,
                        "language": record.language,
                        "redacted": record.redacted,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Err(code) => serde_json::json!({ "error_code": code.as_str() }),
        }
    }

    /// Удаление транскриптов. Без `confirmed` команда отвергается здесь, а не
    /// только модальным окном оболочки: обход UI не должен давать больше прав.
    async fn dispatch_delete_ambient_transcripts(
        &self,
        request: generated::DeleteAmbientTranscripts,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        if !request.confirmed {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::ConfirmationRequired.as_str(),
            });
        }
        let now_ms = crate::task_memory::now_millis();
        let targets: Vec<String> = if request.all {
            match self.journal.list_ambient_episodes(500).await {
                Ok(records) => records.into_iter().map(|r| r.episode_id).collect(),
                Err(code) => {
                    return serde_json::json!({
                        "deleted_count": 0,
                        "error_code": code.as_str(),
                    })
                }
            }
        } else {
            request.episode_ids
        };
        if targets.is_empty() && !request.all {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::InvalidArgument.as_str(),
            });
        }
        let mut deleted = 0u32;
        for episode_id in targets {
            match self
                .journal
                .delete_ambient_episode(&episode_id, now_ms)
                .await
            {
                Ok(deletion) => {
                    deleted = deleted.saturating_add(deletion.utterances_removed as u32)
                }
                Err(code) => {
                    return serde_json::json!({
                        "deleted_count": deleted,
                        "error_code": code.as_str(),
                    })
                }
            }
        }
        let _ = self
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Retention {
                deleted_count: deleted,
                trigger: evohime_listener_contract::RetentionTrigger::Manual,
            })
            .await;
        serde_json::json!({ "deleted_count": deleted, "error_code": "" })
    }

    /// «Забыть последние N минут». Окно приходит в миллисекундах и
    /// округляется вверх: половина минуты — это тоже минута, и оставить её
    /// значило бы не забыть то, что просили забыть.
    async fn dispatch_forget_ambient_window(
        &self,
        request: generated::ForgetAmbientWindow,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        if !request.confirmed {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::ConfirmationRequired.as_str(),
            });
        }
        if request.window_ms <= 0 {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::InvalidArgument.as_str(),
            });
        }
        let minutes = u32::try_from((request.window_ms + 59_999) / 60_000).unwrap_or(u32::MAX);
        let now_ms = crate::task_memory::now_millis();
        match self.journal.forget_ambient_window(minutes, now_ms).await {
            Ok(deletion) => {
                let deleted = deletion.utterances_removed as u32;
                let _ = self
                    .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Retention {
                        deleted_count: deleted,
                        trigger: evohime_listener_contract::RetentionTrigger::ForgetWindow,
                    })
                    .await;
                serde_json::json!({ "deleted_count": deleted, "error_code": "" })
            }
            Err(code) => serde_json::json!({
                "deleted_count": 0,
                "error_code": code.as_str(),
            }),
        }
    }

    fn ambient_policy_json(policy: &evohime_listener_contract::AmbientPolicy) -> serde_json::Value {
        serde_json::json!({
            "quiet_hours": policy
                .quiet_hours
                .iter()
                .map(|window| serde_json::json!({
                    "start_minute": window.start_minute,
                    "end_minute": window.end_minute,
                }))
                .collect::<Vec<_>>(),
            "blocklist_patterns": policy.process_blocklist,
            "window_title_blocklist": policy.window_title_blocklist,
            "retention_days": policy.retention_days,
            "voice_commands": policy.voice_commands,
            "voice_commands_autorun": policy.voice_commands_autorun,
        })
    }

    async fn dispatch_get_ambient_policy(&self) -> serde_json::Value {
        let policy = crate::ambient::load_policy(&self.ambient_data_dir());
        Self::ambient_policy_json(&policy)
    }

    /// Сохранение политики. Невалидная политика не применяется целиком:
    /// частичное применение превратило бы «запретить zoom» в «слушать всё».
    async fn dispatch_save_ambient_policy(
        &self,
        request: generated::SaveAmbientPolicy,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        let Some(incoming) = request.policy else {
            return serde_json::json!({ "applied": false, "error_code": Code::InvalidArgument.as_str() });
        };
        let data_dir = self.ambient_data_dir();
        let previous = crate::ambient::load_policy(&data_dir);
        let mut quiet_hours = Vec::new();
        for window in &incoming.quiet_hours {
            let (Ok(start), Ok(end)) = (
                u32::try_from(window.start_minute),
                u32::try_from(window.end_minute),
            ) else {
                return serde_json::json!({ "applied": false, "error_code": Code::PolicyInvalid.as_str() });
            };
            match evohime_listener_contract::QuietHours::new(start, end) {
                Ok(window) => quiet_hours.push(window),
                Err(error) => {
                    return serde_json::json!({
                        "applied": false,
                        "error_code": error.code().as_str(),
                    })
                }
            }
        }
        let Ok(retention_days) = u32::try_from(incoming.retention_days) else {
            return serde_json::json!({ "applied": false, "error_code": Code::PolicyInvalid.as_str() });
        };
        let policy = evohime_listener_contract::AmbientPolicy {
            // Пауза не редактируется политикой: она принадлежит переключателю
            // и меняется только `SetAmbientListening`.
            paused: previous.paused,
            quiet_hours,
            process_blocklist: incoming.blocklist_patterns,
            window_title_blocklist: incoming.window_title_blocklist,
            retention_days,
            // Поля добавлены позже самого сообщения: клиент, который о них не
            // знает, не шлёт их вовсе, и сохранённое значение остаётся своим.
            // Подстановка `false` вместо этого выключала бы голосовые команды
            // при любом сохранении блок-листа старым клиентом.
            voice_commands: incoming.voice_commands.unwrap_or(previous.voice_commands),
            voice_commands_autorun: incoming
                .voice_commands_autorun
                .unwrap_or(previous.voice_commands_autorun),
        };
        if let Err(error) = policy.validate() {
            return serde_json::json!({
                "applied": false,
                "error_code": error.code().as_str(),
            });
        }
        if crate::ambient::save_policy(&data_dir, &policy).is_err() {
            return serde_json::json!({ "applied": false, "error_code": Code::StorageFailed.as_str() });
        }
        // Сохранённая политика ничего не значит, пока листенер её не получил:
        // недоступный листенер называется своим кодом, а не «применено».
        let control = crate::ambient::load_control(&data_dir);
        match self
            .ambient
            .send(crate::ambient::ListenerControl::Policy(Box::new((
                policy, control,
            ))))
            .await
        {
            Ok(()) => serde_json::json!({ "applied": true, "error_code": "" }),
            Err(code) => serde_json::json!({ "applied": false, "error_code": code.as_str() }),
        }
    }

    // ------------------------------------------------------------------
    // Workflow orchestration (план 06.3).
    //
    // Мост здесь только курьер. Он не планирует граф, не решает порядок и не
    // выполняет узлы: всё это делает `workflow_runtime`, а наружу уходит
    // bounded projection — идентификаторы, состояния и коды. Ни prompt, ни
    // сырой вывод child, ни содержимое контекста через эти команды не
    // проходят.
    // ------------------------------------------------------------------

    /// Собирает runtime под конкретный рабочий каталог.
    ///
    /// Runtime создаётся на команду, а не хранится: состояние запуска durable,
    /// поэтому «живого» объекта между командами не требуется, а рабочий
    /// каталог у каждого запуска свой.
    fn workflow_runtime(&self, workspace_path: &str) -> crate::workflow_runtime::WorkflowRuntime {
        let mut adapter =
            crate::workflow_adapters::CoreNodeAdapter::new(self.journal.clone(), workspace_path);
        if let Some(tools) = &self.tools {
            adapter = adapter.with_tools(Arc::clone(tools));
        }
        crate::workflow_runtime::WorkflowRuntime::new(
            self.journal.clone(),
            Arc::clone(&self.workflow_registry),
            Arc::new(adapter),
            Arc::clone(&self.workflow_approvals)
                as Arc<dyn crate::workflow_runtime::WorkflowApprovalGate>,
            self.core_instance_id.clone(),
        )
    }

    /// Продолжает запуск в фоне. Команда IPC не ждёт выполнения графа:
    /// состояние durable, и оболочка забирает его отдельным `GetWorkflowRun`.
    fn spawn_workflow_drive(&self, run_id: String, workspace_path: String) {
        let runtime = self.workflow_runtime(&workspace_path);
        tokio::spawn(async move {
            let _ = runtime.drive(&run_id).await;
        });
    }

    async fn dispatch_list_automation_schedules(
        &self,
        request: generated::ListAutomationSchedules,
    ) -> serde_json::Value {
        let owner_scope = request.owner_scope;
        if owner_scope.is_empty() || owner_scope.len() > crate::automation::MAX_ID_BYTES {
            return serde_json::json!({
                "schedules": [],
                "error_code": "invalid_owner_scope",
            });
        }
        let limit = request.limit.clamp(1, 256);
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_schedules(
            database.connection(),
            &owner_scope,
            limit,
        ) {
            Ok(schedules) => serde_json::json!({
                "schedules": schedules.into_iter().map(|schedule| serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "definition_id": schedule.definition_id,
                    "revision": schedule.revision,
                    "owner_scope": schedule.owner_scope,
                    "hour": schedule.hour,
                    "minute": schedule.minute,
                    "timezone_minutes": schedule.timezone_minutes,
                    "missed_grace_ms": schedule.missed_grace_ms,
                    "enabled": schedule.enabled,
                    "last_slot": schedule.last_slot,
                })).collect::<Vec<_>>(),
                "error_code": "",
            }),
            Err(error) => serde_json::json!({
                "schedules": [],
                "error_code": error.to_string(),
            }),
        }
    }

    async fn dispatch_save_automation_schedule(
        &self,
        request: generated::SaveAutomationSchedule,
    ) -> serde_json::Value {
        if request.schedule_id.is_empty()
            || request.schedule_id.len() > crate::automation::MAX_ID_BYTES
            || request.definition_id.is_empty()
            || request.owner_scope.is_empty()
            || request.owner_scope.len() > crate::automation::MAX_ID_BYTES
            || request.revision == 0
        {
            return serde_json::json!({
                "saved": false,
                "error_code": "invalid_schedule_identity",
            });
        }
        if crate::automation_scheduler::DailySchedule::new(
            request.hour as u8,
            request.minute as u8,
            request.timezone_minutes,
            request.missed_grace_ms,
        )
        .is_err()
            || request.hour > 23
            || request.minute > 59
        {
            return serde_json::json!({
                "saved": false,
                "error_code": "invalid_schedule_policy",
            });
        }
        let database = self.journal.database().lock().await;
        let definition = evohime_local_storage::automation_store::get_definition(
            database.connection(),
            &request.definition_id,
            request.revision,
            &request.owner_scope,
        );
        match definition {
            Ok(None) => serde_json::json!({
                "saved": false,
                "error_code": "unknown_definition",
            }),
            Err(error) => serde_json::json!({
                "saved": false,
                "error_code": error.to_string(),
            }),
            Ok(Some(_)) => {
                let previous = evohime_local_storage::automation_store::get_schedule(
                    database.connection(),
                    &request.schedule_id,
                )
                .ok()
                .flatten();
                let record = evohime_local_storage::automation_store::AutomationScheduleRecord {
                    schedule_id: request.schedule_id.clone(),
                    definition_id: request.definition_id.clone(),
                    revision: request.revision,
                    owner_scope: request.owner_scope.clone(),
                    hour: request.hour as u8,
                    minute: request.minute as u8,
                    timezone_minutes: request.timezone_minutes,
                    missed_grace_ms: request.missed_grace_ms,
                    enabled: request.enabled,
                    last_slot: previous.and_then(|previous| {
                        (previous.definition_id == request.definition_id
                            && previous.revision == request.revision
                            && previous.owner_scope == request.owner_scope)
                            .then_some(previous.last_slot)
                            .flatten()
                    }),
                };
                match evohime_local_storage::automation_store::upsert_schedule(
                    database.connection(),
                    &record,
                    now_ms(),
                ) {
                    Ok(()) => serde_json::json!({
                        "saved": true,
                        "schedule_id": record.schedule_id,
                        "error_code": "",
                    }),
                    Err(error) => serde_json::json!({
                        "saved": false,
                        "error_code": error.to_string(),
                    }),
                }
            }
        }
    }

    async fn dispatch_trigger_automation(
        &self,
        request: generated::TriggerAutomation,
    ) -> serde_json::Value {
        if request.definition_id.is_empty()
            || request.owner_scope.is_empty()
            || request.trigger_key.is_empty()
            || request.correlation_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.revision == 0
            || request.input_json.len() > crate::automation::MAX_INPUT_BYTES
            || serde_json::from_str::<serde_json::Value>(&request.input_json).is_err()
        {
            return serde_json::json!({ "accepted": false, "run_id": "", "error_code": "invalid_trigger" });
        }
        let mut database = self.journal.database().lock().await;
        let Some(definition) = evohime_local_storage::automation_store::get_definition(
            database.connection(),
            &request.definition_id,
            request.revision,
            &request.owner_scope,
        )
        .ok()
        .flatten() else {
            return serde_json::json!({ "accepted": false, "run_id": "", "error_code": "unknown_definition" });
        };
        let payload_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
            request.input_json.as_bytes(),
        ));
        let run = evohime_local_storage::automation_store::AutomationRunRecord {
            run_id: uuid::Uuid::new_v4().to_string(),
            definition_id: request.definition_id,
            revision: request.revision,
            owner_scope: request.owner_scope,
            idempotency_key: request.idempotency_key,
            payload_hash,
            state: "admitted".into(),
            generation: 1,
            permission_snapshot: "manual".into(),
            approval_snapshot: "manual".into(),
        };
        let now = now_ms();
        match evohime_local_storage::automation_store::admit_run(database.connection(), &run, now) {
            Ok(evohime_local_storage::automation_store::AdmitRunResult::Existing(existing)) => {
                serde_json::json!({ "accepted": true, "run_id": existing.run_id, "state": existing.state, "deduplicated": true, "error_code": "" })
            }
            Ok(evohime_local_storage::automation_store::AdmitRunResult::IdempotencyConflict {
                ..
            }) => {
                serde_json::json!({ "accepted": false, "run_id": "", "deduplicated": false, "error_code": "idempotency_conflict" })
            }
            Ok(evohime_local_storage::automation_store::AdmitRunResult::Inserted) => {
                let payload = serde_json::json!({
                    "definition_hash": definition.definition_hash,
                    "trigger": request.trigger_key,
                    "correlation_id": request.correlation_id,
                });
                let queued = evohime_local_storage::automation_store::transition_run(
                    database.connection_mut(),
                    evohime_local_storage::automation_store::RunTransition {
                        run_id: &run.run_id,
                        from_state: "admitted",
                        to_state: "queued",
                        generation: 1,
                        event_type: "manual_trigger",
                        payload_json: &payload.to_string(),
                        now_ms: now,
                    },
                )
                .unwrap_or(false);
                serde_json::json!({ "accepted": queued, "run_id": run.run_id, "state": if queued { "queued" } else { "admitted" }, "deduplicated": false, "error_code": if queued { "" } else { "transition_failed" } })
            }
            Err(error) => {
                serde_json::json!({ "accepted": false, "run_id": "", "error_code": error.to_string() })
            }
        }
    }

    async fn dispatch_list_automation_runs(
        &self,
        request: generated::ListAutomationRuns,
    ) -> serde_json::Value {
        if request.owner_scope.is_empty() {
            return serde_json::json!({ "runs": [], "error_code": "invalid_owner_scope" });
        }
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_runs(
            database.connection(),
            &request.owner_scope,
            &request.definition_id,
            request.limit.clamp(1, 256),
        ) {
            Ok(runs) => serde_json::json!({ "runs": runs.into_iter().map(|run| serde_json::json!({
                "run_id": run.run_id, "definition_id": run.definition_id, "revision": run.revision,
                "owner_scope": run.owner_scope, "idempotency_key": run.idempotency_key,
                "state": run.state, "generation": run.generation,
            })).collect::<Vec<_>>(), "error_code": "" }),
            Err(error) => serde_json::json!({ "runs": [], "error_code": error.to_string() }),
        }
    }

    async fn dispatch_get_automation_run(
        &self,
        request: generated::GetAutomationRun,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::get_run(
            database.connection(),
            &request.run_id,
        ) {
            Ok(Some(run)) => serde_json::json!({
                "run_id": run.run_id, "definition_id": run.definition_id, "revision": run.revision,
                "owner_scope": run.owner_scope, "state": run.state, "generation": run.generation,
                "error_code": "",
            }),
            Ok(None) => {
                serde_json::json!({ "run_id": request.run_id, "state": "unknown_state", "error_code": "unknown_run" })
            }
            Err(error) => {
                serde_json::json!({ "run_id": request.run_id, "state": "unknown_state", "error_code": error.to_string() })
            }
        }
    }

    async fn dispatch_list_automation_events(
        &self,
        request: generated::ListAutomationEvents,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_run_events(
            database.connection(),
            &request.run_id,
            request.after_sequence,
            request.limit.clamp(1, 256) as u32,
        ) {
            Ok(events) => {
                serde_json::json!({ "run_id": request.run_id, "events": events.into_iter().map(|event| serde_json::json!({
                "sequence": event.run_sequence, "event_type": event.event_type, "generation": event.generation,
                "payload": event.payload_json, "created_at_ms": event.created_at_ms,
            })).collect::<Vec<_>>(), "error_code": "" })
            }
            Err(error) => {
                serde_json::json!({ "run_id": request.run_id, "events": [], "error_code": error.to_string() })
            }
        }
    }

    async fn dispatch_cancel_automation_run(
        &self,
        request: generated::CancelAutomationRun,
    ) -> serde_json::Value {
        let mut database = self.journal.database().lock().await;
        let cancelled = evohime_local_storage::automation_store::cancel_run(
            database.connection_mut(),
            &request.run_id,
            now_ms(),
        )
        .unwrap_or(false);
        serde_json::json!({ "run_id": request.run_id, "cancelled": cancelled, "error_code": if cancelled { "" } else { "not_cancellable" } })
    }

    async fn dispatch_set_automation_schedule_enabled(
        &self,
        request: generated::SetAutomationScheduleEnabled,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        let enabled = evohime_local_storage::automation_store::set_schedule_enabled(
            database.connection(),
            &request.schedule_id,
            request.enabled,
            now_ms(),
        )
        .unwrap_or(false);
        serde_json::json!({ "schedule_id": request.schedule_id, "enabled": request.enabled, "updated": enabled, "error_code": if enabled { "" } else { "unknown_schedule" } })
    }

    /// Polls every enabled schedule once. The compare-and-swap cursor is
    /// advanced before a trigger is admitted, so a second Core generation
    /// cannot publish the same wall-clock slot. The normal automation runtime
    /// consumes the durable admitted run; this method never executes effects.
    pub async fn poll_automation_schedules(&self) {
        let now = now_ms();
        let schedules = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::automation_store::list_enabled_schedules(database.connection())
                .unwrap_or_default()
        };
        for schedule in schedules {
            let Ok(policy) = crate::automation_scheduler::DailySchedule::new(
                schedule.hour,
                schedule.minute,
                schedule.timezone_minutes,
                schedule.missed_grace_ms,
            ) else {
                continue;
            };
            let cursor = crate::automation_scheduler::SchedulerCursor {
                last_slot: schedule.last_slot.clone(),
            };
            let decision =
                match policy.decide(&schedule.definition_id, schedule.revision, &cursor, now) {
                    Ok(decision) => decision,
                    Err(_) => continue,
                };
            let (slot, idempotency_key, missed) = match decision {
                crate::automation_scheduler::SchedulerDecision::NotDue => continue,
                crate::automation_scheduler::SchedulerDecision::Trigger {
                    slot,
                    idempotency_key,
                } => (slot, idempotency_key, false),
                crate::automation_scheduler::SchedulerDecision::Missed {
                    slot,
                    idempotency_key,
                } => (slot, idempotency_key, true),
            };
            let mut database = self.journal.database().lock().await;
            let Some(definition) = evohime_local_storage::automation_store::get_definition(
                database.connection(),
                &schedule.definition_id,
                schedule.revision,
                &schedule.owner_scope,
            )
            .ok()
            .flatten() else {
                // Не сдвигаем cursor: после восстановления definition следующий
                // poll должен повторить попытку, а не потерять слот.
                continue;
            };
            let advanced = evohime_local_storage::automation_store::advance_schedule_slot(
                database.connection(),
                &schedule.schedule_id,
                schedule.last_slot.as_deref(),
                &slot,
                now,
            )
            .unwrap_or(false);
            if !advanced {
                continue;
            }
            if missed {
                let payload = serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "definition_id": schedule.definition_id,
                    "revision": schedule.revision,
                    "slot": slot,
                    "idempotency_key": idempotency_key,
                    "reason": "missed_tick",
                });
                let _ = database.append_event(
                    &schedule.schedule_id,
                    "automation.schedule_missed",
                    &serde_json::to_vec(&payload).unwrap_or_default(),
                );
                continue;
            }
            let input_json = "{}".to_string();
            let payload_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
                input_json.as_bytes(),
            ));
            let run = evohime_local_storage::automation_store::AutomationRunRecord {
                run_id: uuid::Uuid::new_v4().to_string(),
                definition_id: schedule.definition_id.clone(),
                revision: schedule.revision,
                owner_scope: schedule.owner_scope.clone(),
                idempotency_key,
                payload_hash,
                state: "admitted".into(),
                generation: 1,
                permission_snapshot: "scheduler".into(),
                approval_snapshot: "scheduler".into(),
            };
            if let Ok(evohime_local_storage::automation_store::AdmitRunResult::Inserted) =
                evohime_local_storage::automation_store::admit_run(database.connection(), &run, now)
            {
                let payload = serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "slot": slot,
                    "definition_hash": definition.definition_hash,
                    "trigger": "timer",
                });
                let _ = evohime_local_storage::automation_store::transition_run(
                    database.connection_mut(),
                    evohime_local_storage::automation_store::RunTransition {
                        run_id: &run.run_id,
                        from_state: "admitted",
                        to_state: "queued",
                        generation: 1,
                        event_type: "scheduled",
                        payload_json: &payload.to_string(),
                        now_ms: now,
                    },
                );
            }
        }
    }

    fn dispatch_list_workflow_templates(&self) -> serde_json::Value {
        let templates: Vec<serde_json::Value> = crate::workflow_templates::catalog()
            .into_iter()
            .map(|template| {
                serde_json::json!({
                    "template_id": template.template_id,
                    "version": template.version,
                    "display_name": template.display_name,
                    "description": template.description,
                    "inputs": template
                        .inputs
                        .iter()
                        .map(|input| serde_json::json!({
                            "name": input.name,
                            "title": input.title,
                            "required": input.required,
                            "max_chars": input.max_chars,
                        }))
                        .collect::<Vec<_>>(),
                    "required_capabilities": template.required_capabilities,
                    "schedule_eligibility": template.schedule_eligibility.as_str(),
                    "preview": template.preview,
                    "node_count": template.graph().nodes.len(),
                })
            })
            .collect();
        serde_json::json!({ "templates": templates, "error_code": "" })
    }

    fn dispatch_workflow_definition(
        &self,
        request: generated::GetWorkflowDefinition,
    ) -> serde_json::Value {
        let Some(template) = crate::workflow_templates::template(&request.template_id) else {
            return serde_json::json!({
                "template_id": request.template_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "edges": Vec::<serde_json::Value>::new(),
                "error_code": "unknown_template",
            });
        };
        let graph = template.graph();
        serde_json::json!({
            "template_id": template.template_id,
            "version": template.version,
            "display_name": template.display_name,
            "graph_id": graph.graph_id,
            "graph_version": graph.version,
            "graph_hash": graph.canonical_hash(),
            "schedule_eligibility": template.schedule_eligibility.as_str(),
            "preview": template.preview,
            "nodes": graph
                .nodes
                .iter()
                .map(|node| serde_json::json!({
                    "node_id": node.id,
                    "action_kind": node.node_type.action_kind(),
                    "approval_required": node.execution.approval.required,
                    "block_id": node
                        .block
                        .as_ref()
                        .map(|block| block.block_id.clone())
                        .unwrap_or_default(),
                    "block_version": node
                        .block
                        .as_ref()
                        .map(|block| block.block_version)
                        .unwrap_or_default(),
                }))
                .collect::<Vec<_>>(),
            "edges": graph
                .edges
                .iter()
                .map(|edge| serde_json::json!({
                    "from_node": edge.from_node,
                    "to_node": edge.to_node,
                    "channel": match edge.channel {
                        crate::workflow::EdgeChannel::Failure => "failure",
                        crate::workflow::EdgeChannel::Data => "data",
                    },
                }))
                .collect::<Vec<_>>(),
            "error_code": "",
        })
    }

    async fn dispatch_start_workflow(
        &self,
        request: generated::StartWorkflow,
    ) -> serde_json::Value {
        let Some(template) = crate::workflow_templates::template(&request.template_id) else {
            return workflow_start_failure("unknown_template");
        };
        let inputs: std::collections::BTreeMap<String, String> = request
            .inputs
            .iter()
            .map(|input| (input.name.clone(), input.value.clone()))
            .collect();
        let graph = match template.instantiate(&inputs) {
            Ok(graph) => graph,
            Err(error) => return workflow_start_failure(error.code()),
        };

        // Идемпотентность: тот же ключ даёт тот же `run_id`, поэтому двойной
        // клик возвращает первый запуск, а не создаёт второй.
        let run_id = if request.idempotency_key.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            let digest = <sha2::Sha256 as sha2::Digest>::digest(
                format!("{}|{}", request.template_id, request.idempotency_key).as_bytes(),
            );
            format!("wf-{}", hex_encode(&digest[..16]))
        };
        if let Ok(Some(existing)) = self.journal.workflow_run(&run_id).await {
            return serde_json::json!({
                "run_id": existing.run_id,
                "state": existing.state.as_str(),
                "graph_hash": existing.graph_hash,
                "deduplicated": true,
                "error_code": "",
            });
        }

        let workspace_path = request.workspace_path.clone();
        let runtime = self.workflow_runtime(&workspace_path);
        let start = crate::workflow_runtime::StartWorkflowRequest {
            run_id: run_id.clone(),
            task_id: if request.task_id.trim().is_empty() {
                run_id.clone()
            } else {
                request.task_id.clone()
            },
            workspace_path: workspace_path.clone(),
            template_id: template.template_id.clone(),
            template_version: template.version,
            inputs,
            graph,
            parent: workflow_parent_capabilities(),
        };
        match runtime.start(start).await {
            Ok(run_id) => {
                self.spawn_workflow_drive(run_id.clone(), workspace_path);
                serde_json::json!({
                    "run_id": run_id,
                    "state": "pending",
                    "graph_hash": "",
                    "deduplicated": false,
                    "error_code": "",
                })
            }
            Err(error) => workflow_start_failure(error.code()),
        }
    }

    async fn dispatch_workflow_run(&self, request: generated::GetWorkflowRun) -> serde_json::Value {
        let workspace = self.journal.workflow_run_workspace(&request.run_id).await;
        let runtime = self.workflow_runtime(&workspace);
        match runtime.projection(&request.run_id).await {
            Ok(Some(projection)) => {
                let mut value = serde_json::to_value(&projection).unwrap_or_default();
                if let Some(object) = value.as_object_mut() {
                    object.insert("error_code".into(), serde_json::json!(""));
                }
                value
            }
            Ok(None) => serde_json::json!({
                "run_id": request.run_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "state": "unknown_state",
                "error_code": "unknown_run",
            }),
            Err(error) => serde_json::json!({
                "run_id": request.run_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "state": "unknown_state",
                "error_code": error.code(),
            }),
        }
    }

    async fn dispatch_cancel_workflow(
        &self,
        request: generated::CancelWorkflow,
    ) -> serde_json::Value {
        let now_ms = crate::task_memory::now_millis() as i64;
        let cancelled = self
            .journal
            .request_workflow_cancel(&request.run_id, now_ms)
            .await
            .unwrap_or(false);
        if cancelled {
            let workspace = self.journal.workflow_run_workspace(&request.run_id).await;
            self.spawn_workflow_drive(request.run_id.clone(), workspace);
        }
        serde_json::json!({
            "run_id": request.run_id,
            "cancelled": cancelled,
            "error_code": if cancelled { "" } else { "not_cancellable" },
        })
    }

    async fn dispatch_list_workflow_events(
        &self,
        request: generated::ListWorkflowEvents,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            100usize
        } else {
            (request.limit as usize).min(500)
        };
        match self
            .journal
            .list_workflow_events(&request.run_id, request.after_sequence, limit)
            .await
        {
            Ok(events) => serde_json::json!({
                "run_id": request.run_id,
                "events": events
                    .into_iter()
                    .map(|event| serde_json::json!({
                        "sequence": event.run_sequence,
                        "node_id": event.node_id,
                        "event_type": event.event_type,
                        "payload": event.payload_json,
                        "created_at_ms": event.created_at_ms,
                    }))
                    .collect::<Vec<_>>(),
                "error_code": "",
            }),
            Err(error) => serde_json::json!({
                "run_id": request.run_id,
                "events": Vec::<serde_json::Value>::new(),
                "error_code": error.to_string(),
            }),
        }
    }

    async fn dispatch_visual_workflow_builder(
        &self,
        request: generated::VisualWorkflowBuilderCommand,
    ) -> serde_json::Value {
        if request.operation == "catalog" {
            let blocks = self.workflow_registry.blocks().map(|block| serde_json::json!({"block_id": block.block_id, "block_version": block.block_version, "display_name": block.display_name, "description": block.description, "action_kind": block.action_kind, "inputs": block.inputs, "outputs": block.outputs})).collect::<Vec<_>>();
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"catalog","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"","truncated":false,"blocks":blocks});
        }
        if request.operation == "recover" {
            let database = self.journal.database().lock().await;
            return match evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            ) {
                Ok(Some((revision, _definition, execution_hash, layout_hash))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"recovered","draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":"","error_code":"","truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"missing","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"corrupt","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "inspect" {
            let run_id = String::from_utf8(request.payload.to_vec()).unwrap_or_default();
            let workspace = self.journal.workflow_run_workspace(&run_id).await;
            let runtime = self.workflow_runtime(&workspace);
            return match runtime.projection(&run_id).await {
                Ok(Some(projection)) => {
                    let value = serde_json::to_value(projection).unwrap_or_default();
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"inspected","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"","truncated":false,"projection":value})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"unknown","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_run","truncated":false})
                }
                Err(_error) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"runtime_error","truncated":false})
                }
            };
        }
        if request.operation == "edit" {
            let database = self.journal.database().lock().await;
            let draft = evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            );
            return match draft {
                Ok(Some((revision, definition_json, _, _)))
                    if revision == request.expected_revision =>
                {
                    let parsed = serde_json::from_slice::<
                        crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
                    >(&definition_json);
                    let command = serde_json::from_slice::<
                        crate::visual_workflow_builder::DraftCommand,
                    >(&request.payload);
                    match (parsed, command) {
                        (Ok(mut definition), Ok(command)) => match command
                            .apply(&mut definition)
                            .and_then(|_| self.validate_visual_workflow_definition(&definition))
                        {
                            Ok(()) => {
                                let definition_json =
                                    serde_json::to_vec(&definition).unwrap_or_default();
                                let layout_json =
                                    serde_json::to_vec(&definition.layout).unwrap_or_default();
                                let execution_hash = definition.execution_hash();
                                let layout_hash = definition.layout_hash();
                                match evohime_local_storage::visual_workflow_builder_store::save_draft(database.connection(), evohime_local_storage::visual_workflow_builder_store::SaveDraft { draft_id: &request.draft_id, owner_scope: &request.owner_scope, expected_revision: revision, definition_json: &definition_json, layout_json: &layout_json, execution_hash: &execution_hash, layout_hash: &layout_hash, composer_provenance_json: None, updated_at_ms: crate::task_memory::now_millis() as i64 }) {
                                    Ok(Ok(next_revision)) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"edited","draft_id":request.draft_id,"revision":next_revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":"","error_code":"","truncated":false}),
                                    Ok(Err(code)) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":code,"truncated":false}),
                                    Err(_) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false}),
                                }
                            }
                            Err(error) => {
                                serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":error.to_string(),"truncated":false})
                            }
                        },
                        _ => {
                            serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"invalid_command","truncated":false})
                        }
                    }
                }
                Ok(Some((revision, _, _, _))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"stale_revision","truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "issue_handoff" {
            let database = self.journal.database().lock().await;
            let draft = evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            );
            return match draft {
                Ok(Some((revision, _definition, execution_hash, _layout_hash))) => {
                    let handle = format!("builder-handoff:{}:{}", request.draft_id, revision);
                    let precondition = format!("{}:{}", revision, execution_hash);
                    let result =
                        evohime_local_storage::visual_workflow_builder_store::issue_handoff(
                            database.connection(),
                            evohime_local_storage::visual_workflow_builder_store::Handoff {
                                handle: &handle,
                                draft_id: &request.draft_id,
                                owner_scope: &request.owner_scope,
                                revision,
                                draft_hash: &execution_hash,
                                precondition: &precondition,
                                created_at_ms: crate::task_memory::now_millis() as i64,
                            },
                        );
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":if result.is_ok(){"handoff_issued"}else{"error"},"draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":"","handoff_handle":if result.is_ok(){handle}else{String::new()},"error_code":if result.is_ok(){""}else{"storage_error"},"truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "publish" {
            let handle = String::from_utf8(request.payload.to_vec()).unwrap_or_default();
            let database = self.journal.database().lock().await;
            let published =
                evohime_local_storage::visual_workflow_builder_store::publish_from_handoff(
                    database.connection(),
                    &handle,
                    &request.draft_id,
                    &request.owner_scope,
                    crate::task_memory::now_millis() as i64,
                );
            return match published {
                Ok(Ok((revision, _definition, execution_hash, layout_hash))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"published","draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":handle,"error_code":"","truncated":false})
                }
                Ok(Err(code)) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":code,"truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "validate" || request.operation == "save" {
            match serde_json::from_slice::<
                crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
            >(&request.payload)
            {
                Ok(definition) => {
                    match self.validate_visual_workflow_definition(&definition) {
                        Ok(()) if request.operation == "validate" => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "valid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                        }
                        Ok(()) if request.operation == "save" => {
                            let database = self.journal.database().lock().await;
                            let graph_json = serde_json::to_vec(&definition).unwrap_or_default();
                            let layout_json =
                                serde_json::to_vec(&definition.layout).unwrap_or_default();
                            let result =
                            evohime_local_storage::visual_workflow_builder_store::save_draft(
                                database.connection(),
                                evohime_local_storage::visual_workflow_builder_store::SaveDraft { draft_id: &request.draft_id, owner_scope: &request.owner_scope, expected_revision: request.expected_revision, definition_json: &graph_json, layout_json: &layout_json, execution_hash: &definition.execution_hash(), layout_hash: &definition.layout_hash(), composer_provenance_json: None, updated_at_ms: crate::task_memory::now_millis() as i64 },
                            );
                            return match result {
                                Ok(Ok(revision)) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "saved", "draft_id": request.draft_id, "revision": revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                                }
                                Ok(Err(code)) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "conflict", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": code, "truncated": false })
                                }
                                Err(_) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "error", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": "storage_error", "truncated": false })
                                }
                            };
                        }
                        Ok(()) => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "valid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                        }
                        Err(error) => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "invalid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": error.to_string(), "truncated": false })
                        }
                    }
                }
                Err(_) => {
                    return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "invalid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": "invalid_payload", "truncated": false })
                }
            }
        }
        serde_json::json!({
            "schema_version": 1,
            "request_id": request.request_id,
            "status": "unavailable",
            "draft_id": request.draft_id,
            "revision": 0,
            "execution_hash": "",
            "layout_hash": "",
            "handoff_handle": "",
            "error_code": "builder_authoring_not_wired",
            "truncated": false,
        })
    }

    async fn dispatch_conversational_workflow_composer(
        &self,
        request: generated::ConversationalWorkflowComposerCommand,
    ) -> serde_json::Value {
        if request.idempotency_key.trim().is_empty() {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":request.expected_revision,"proposal_id":"","execution_hash":"","layout_hash":"","error_code":"missing_idempotency_key","projection_json":[],"truncated":false});
        }
        let command_hash =
            hex_encode(&[request.operation.as_bytes(), request.payload.as_ref()].concat());
        match self
            .journal
            .record_deduplicated(
                "workflow-composer",
                &request.idempotency_key,
                &command_hash,
                &[],
            )
            .await
        {
            Ok(Some(bytes)) => {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    return value;
                }
            }
            Err(_) => {
                return serde_json::json!({
                    "schema_version": 1,
                    "request_id": request.request_id,
                    "status": "conflict",
                    "draft_id": request.draft_id,
                    "revision": request.expected_revision,
                    "proposal_id": "",
                    "execution_hash": "",
                    "layout_hash": "",
                    "error_code": "idempotency_conflict",
                    "projection_json": [],
                    "truncated": false
                });
            }
            Ok(None) => {}
        }
        let result = self
            .dispatch_conversational_workflow_composer_inner(request.clone())
            .await;
        if let Ok(bytes) = serde_json::to_vec(&result) {
            let _ = self
                .journal
                .record_deduplicated(
                    "workflow-composer",
                    &request.idempotency_key,
                    &command_hash,
                    &bytes,
                )
                .await;
        }
        result
    }

    async fn dispatch_conversational_workflow_composer_inner(
        &self,
        request: generated::ConversationalWorkflowComposerCommand,
    ) -> serde_json::Value {
        use crate::conversational_workflow_composer as composer;
        let base = |status: &str, error: &str| {
            serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "status": status,
                "draft_id": request.draft_id,
                "revision": request.expected_revision,
                "proposal_id": "",
                "execution_hash": "",
                "layout_hash": "",
                "error_code": error,
                "projection_json": [],
                "truncated": false
            })
        };
        if request.schema_version != 0 && request.schema_version != 1 {
            return base("invalid", "unsupported_schema_version");
        }
        if request.owner_scope.trim().is_empty() || request.draft_id.trim().is_empty() {
            return base("invalid", "invalid_scope");
        }
        match request.operation.as_str() {
            "generate" => {
                let Ok(request_hash) = composer::request_hash(&request.payload) else {
                    return base("invalid", "request_too_large");
                };
                let Some(config) = self.gateway_config.clone() else {
                    return base("unavailable", "model_unavailable");
                };
                let Ok(gateway) = evohime_model_gateway::ModelGateway::from_config(&config) else {
                    return base("unavailable", "model_unavailable");
                };
                let prompt = String::from_utf8_lossy(&request.payload).into_owned();
                let messages = vec![
                    evohime_model_gateway::providers::ChatMessage::text(
                        evohime_model_gateway::providers::ChatRole::System,
                        "Return only JSON matching composer-proposal/v1 with schema_version, proposal_id, definition, assumptions. Never add tools, permissions, credentials or executable identities.",
                    ),
                    evohime_model_gateway::providers::ChatMessage::text(
                        evohime_model_gateway::providers::ChatRole::User,
                        prompt,
                    ),
                ];
                let routing = evohime_model_gateway::RoutingRequest {
                    required_capabilities: vec!["chat".into()],
                    max_cost_micros_per_1k_tokens: None,
                    max_latency_ms: Some(30_000),
                    required_privacy: evohime_model_gateway::PrivacyClass::Internal,
                    allow_fallback: true,
                    preferred_route: Some(config.default_route.clone()),
                    task_class: Some("workflow_composer".into()),
                    offline: false,
                    allow_cloud: true,
                    estimated_input_tokens: (request.payload.len() / 4) as u32,
                    quality_delta: 0.05,
                };
                let response = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    gateway.chat_with_tools_with_policy_and_route(
                        evohime_model_gateway::RoutingMode::Balanced,
                        &routing,
                        self.selected_model.get().as_deref(),
                        &messages,
                        &[],
                    ),
                )
                .await;
                let content = match response {
                    Ok(Ok(result)) => result.result.content,
                    Ok(Err(_)) => return base("unavailable", "model_unavailable"),
                    Err(_) => return base("unavailable", "model_timeout"),
                };
                let Ok(proposal) = composer::parse_proposal(content.as_bytes()) else {
                    return base("invalid", "malformed_proposal");
                };
                let projection = serde_json::json!({
                    "proposal_id": proposal.proposal_id,
                    "assumptions": proposal.assumptions,
                    "definition": proposal.definition,
                    "request_hash": request_hash,
                    "requires_review": true,
                    "risk": "review_required",
                });
                let mut result = base("proposal", "");
                result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                result["execution_hash"] = serde_json::json!(proposal.definition.execution_hash());
                result["layout_hash"] = serde_json::json!(proposal.definition.layout_hash());
                result["projection_json"] =
                    serde_json::to_vec(&projection).unwrap_or_default().into();
                result
            }
            "validate" => {
                let Ok(proposal) = composer::parse_proposal(&request.payload) else {
                    return base("invalid", "malformed_proposal");
                };
                if self
                    .validate_visual_workflow_definition(&proposal.definition)
                    .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let mut result = base("valid", "");
                result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                result["execution_hash"] = serde_json::json!(proposal.definition.execution_hash());
                result["layout_hash"] = serde_json::json!(proposal.definition.layout_hash());
                result["projection_json"] = serde_json::to_vec(&serde_json::json!({"risk":"review_required","assumptions":proposal.assumptions})).unwrap_or_default().into();
                result
            }
            "save" => {
                let Ok(proposal) = composer::parse_proposal(&request.payload) else {
                    return base("invalid", "malformed_proposal");
                };
                if self
                    .validate_visual_workflow_definition(&proposal.definition)
                    .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let definition_json = serde_json::to_vec(&proposal.definition).unwrap_or_default();
                let layout_json =
                    serde_json::to_vec(&proposal.definition.layout).unwrap_or_default();
                let execution_hash = proposal.definition.execution_hash();
                let layout_hash = proposal.definition.layout_hash();
                let database = self.journal.database().lock().await;
                let provenance_json = serde_json::to_vec(&composer::ComposerProvenance {
                    schema_version: composer::PROVENANCE_VERSION.into(),
                    request_hash: composer::request_hash(&request.payload).unwrap_or_default(),
                    proposal_hash: composer::canonical_proposal(&proposal)
                        .ok()
                        .map(|bytes| hex::encode(<sha2::Sha256 as sha2::Digest>::digest(bytes)))
                        .unwrap_or_default(),
                    catalog_hash: "core-workflow-registry-v1".into(),
                    model_route: "core-model-gateway".into(),
                    model_version: "bounded-v1".into(),
                })
                .ok();
                match evohime_local_storage::visual_workflow_builder_store::save_draft(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::SaveDraft {
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        expected_revision: request.expected_revision,
                        definition_json: &definition_json,
                        layout_json: &layout_json,
                        execution_hash: &execution_hash,
                        layout_hash: &layout_hash,
                        composer_provenance_json: provenance_json.as_deref(),
                        updated_at_ms: crate::task_memory::now_millis() as i64,
                    },
                ) {
                    Ok(Ok(revision)) => {
                        let mut result = base("saved", "");
                        result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                        result["revision"] = serde_json::json!(revision);
                        result["execution_hash"] = serde_json::json!(execution_hash);
                        result["layout_hash"] = serde_json::json!(layout_hash);
                        result
                    }
                    Ok(Err(code)) => base("conflict", code),
                    Err(_) => base("error", "storage_error"),
                }
            }
            "edit" => {
                let database = self.journal.database().lock().await;
                let Ok(Some((revision, definition_json, _, _))) =
                    evohime_local_storage::visual_workflow_builder_store::read_draft(
                        database.connection(),
                        &request.draft_id,
                        &request.owner_scope,
                    )
                else {
                    return base("error", "unknown_draft");
                };
                if revision != request.expected_revision {
                    return base("conflict", "stale_revision");
                }
                let Ok(mut definition) = serde_json::from_slice::<
                    crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
                >(&definition_json) else {
                    return base("error", "corrupt_draft");
                };
                let Ok(command) = serde_json::from_slice::<
                    crate::visual_workflow_builder::DraftCommand,
                >(&request.payload) else {
                    return base("invalid", "invalid_edit");
                };
                if composer::apply_edit(&mut definition, &command).is_err()
                    || self
                        .validate_visual_workflow_definition(&definition)
                        .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let definition_json = serde_json::to_vec(&definition).unwrap_or_default();
                let layout_json = serde_json::to_vec(&definition.layout).unwrap_or_default();
                let execution_hash = definition.execution_hash();
                let layout_hash = definition.layout_hash();
                match evohime_local_storage::visual_workflow_builder_store::save_draft(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::SaveDraft {
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        expected_revision: revision,
                        definition_json: &definition_json,
                        layout_json: &layout_json,
                        execution_hash: &execution_hash,
                        layout_hash: &layout_hash,
                        composer_provenance_json: None,
                        updated_at_ms: crate::task_memory::now_millis() as i64,
                    },
                ) {
                    Ok(Ok(next)) => {
                        let mut result = base("edited", "");
                        result["revision"] = serde_json::json!(next);
                        result["execution_hash"] = serde_json::json!(execution_hash);
                        result["layout_hash"] = serde_json::json!(layout_hash);
                        result
                    }
                    Ok(Err(code)) => base("conflict", code),
                    Err(_) => base("error", "storage_error"),
                }
            }
            "handoff" => {
                let database = self.journal.database().lock().await;
                let Ok(Some((revision, _, execution_hash, layout_hash))) =
                    evohime_local_storage::visual_workflow_builder_store::read_draft(
                        database.connection(),
                        &request.draft_id,
                        &request.owner_scope,
                    )
                else {
                    return base("error", "unknown_draft");
                };
                let handle = format!("composer-handoff:{}:{}", request.draft_id, revision);
                let precondition = format!("{}:{}", revision, execution_hash);
                let result = evohime_local_storage::visual_workflow_builder_store::issue_handoff(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::Handoff {
                        handle: &handle,
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        revision,
                        draft_hash: &execution_hash,
                        precondition: &precondition,
                        created_at_ms: crate::task_memory::now_millis() as i64,
                    },
                );
                let mut value = base(
                    if result.is_ok() { "handoff" } else { "error" },
                    if result.is_ok() { "" } else { "storage_error" },
                );
                value["revision"] = serde_json::json!(revision);
                value["execution_hash"] = serde_json::json!(execution_hash);
                value["layout_hash"] = serde_json::json!(layout_hash);
                value["projection_json"] = serde_json::to_vec(
                    &serde_json::json!({"handoff_handle":handle,"save_precondition":precondition}),
                )
                .unwrap_or_default()
                .into();
                value
            }
            "discard" => base("discarded", ""),
            _ => base("unavailable", "composer_operation_unavailable"),
        }
    }

    fn validate_visual_workflow_definition(
        &self,
        definition: &crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
    ) -> Result<(), crate::visual_workflow_builder::BuilderError> {
        definition.validate()?;
        self.workflow_registry
            .validate_bindings(
                &definition.graph,
                &crate::workflow_registry::ParentCapabilities::default().unrestricted_context(),
            )
            .map_err(|_| crate::visual_workflow_builder::BuilderError::RegistryRejected)
    }

    /// Список ожидающих карточек (этап 04.7).
    ///
    /// Это единственный путь, по которому человекочитаемый текст предложения
    /// пересекает границу IPC: durable journal его не несёт, потому что
    /// `events` — append-only таблица, из которой ambient-содержимое пришлось
    /// бы вычищать. Тот же принцип, по которому `memory.pending` не несёт
    /// `statement`.
    ///
    /// Просроченные карточки снимаются здесь же: список, показывающий
    /// вчерашнее предложение как ждущее ответа, врал бы пользователю.
    async fn dispatch_list_ambient_proposals(
        &self,
        request: generated::ListAmbientProposals,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            50usize
        } else {
            (request.limit as usize).min(200)
        };
        let now_ms = crate::task_memory::now_millis();
        let _ = self.journal.expire_stale_ambient_proposals(now_ms).await;
        let budget = self.proactivity.budget().await;
        match self.journal.list_open_ambient_proposals(limit).await {
            Ok(records) => serde_json::json!({
                "proposals": records
                    .into_iter()
                    .map(|record| serde_json::json!({
                        "proposal_id": record.proposal_id,
                        "kind": record.kind.as_str(),
                        "subject": record.subject,
                        "title": record.title,
                        "source_episode_id": record.source_episode_id.unwrap_or_default(),
                        "created_at_ms": parse_timestamp_ms(&record.created_at),
                        "expires_at_ms": parse_timestamp_ms(&record.expires_at),
                        "occurrences": record.occurrences,
                        "state": record.state.as_str(),
                    }))
                    .collect::<Vec<_>>(),
                "max_per_hour": budget.max_per_hour,
                "max_per_day": budget.max_per_day,
                "min_interval_ms": budget.min_interval_ms,
                "error_code": "",
            }),
            Err(code) => serde_json::json!({
                "proposals": Vec::<serde_json::Value>::new(),
                "max_per_hour": budget.max_per_hour,
                "max_per_day": budget.max_per_day,
                "min_interval_ms": budget.min_interval_ms,
                "error_code": code.as_str(),
            }),
        }
    }

    /// Решение по ограниченному предложению (этап 04.7).
    ///
    /// Три исхода, а не два: принять, отклонить и «больше не предлагать
    /// такое». Принятие создаёт обычную задачу или неисполняемое напоминание
    /// штатным механизмом Core с сохранением провенанса; ни один другой
    /// эффект здесь недостижим.
    ///
    /// `idempotency_key` обязателен: без него двойной клик по карточке
    /// породил бы две задачи. Повтор с тем же ключом возвращает первое
    /// решение, а не создаёт второе.
    async fn dispatch_resolve_ambient_proposal(
        &self,
        request: generated::ResolveAmbientProposal,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        use evohime_listener_contract::ProposalState;

        let Ok(proposal_id) =
            evohime_listener_contract::ProposalId::new(request.proposal_id.clone())
        else {
            return resolve_failure(Code::InvalidArgument);
        };
        let idempotency_key = request.idempotency_key.trim().to_owned();
        if idempotency_key.is_empty() || idempotency_key.len() > MAX_PROPOSAL_KEY_BYTES {
            return resolve_failure(Code::InvalidArgument);
        }
        // Повтор того же клика: ответ берётся из уже принятого решения, и
        // вторая задача не создаётся.
        match self
            .journal
            .find_ambient_proposal_by_idempotency(&idempotency_key)
            .await
        {
            Ok(Some(existing)) => {
                return serde_json::json!({
                    "applied": true,
                    "state": existing.state.as_str(),
                    "task_id": existing.accepted_task_id.unwrap_or_default(),
                    "error_code": "",
                })
            }
            Ok(None) => {}
            Err(code) => return resolve_failure(code),
        }

        let record = match self
            .journal
            .get_ambient_proposal(proposal_id.as_str())
            .await
        {
            Ok(Some(record)) => record,
            // Нет такого предложения — это честное «не применено», а не
            // вымышленный успех.
            Ok(None) => return resolve_failure(Code::InvalidArgument),
            Err(code) => return resolve_failure(code),
        };
        if record.state.is_terminal() {
            return serde_json::json!({
                "applied": false,
                "state": record.state.as_str(),
                "task_id": record.accepted_task_id.unwrap_or_default(),
                "error_code": Code::InvalidArgument.as_str(),
            });
        }

        let now_ms = crate::task_memory::now_millis();
        let next_state = if request.mute {
            ProposalState::Muted
        } else if request.accepted {
            ProposalState::Accepted
        } else {
            ProposalState::Declined
        };

        // Задача создаётся только при принятии и только до перевода карточки
        // в терминальное состояние: обратный порядок оставил бы «принято» без
        // задачи, если бы создание не удалось.
        let task_id = if next_state == ProposalState::Accepted {
            match self.create_proposal_effect(&record, &idempotency_key).await {
                Ok(task_id) => Some(task_id),
                Err(code) => return resolve_failure(code),
            }
        } else {
            None
        };

        match self
            .journal
            .resolve_ambient_proposal_row(
                proposal_id.as_str(),
                next_state,
                now_ms,
                task_id.as_deref(),
                Some(&idempotency_key),
            )
            .await
        {
            Ok(true) => {}
            // Кто-то решил карточку между чтением и записью: первый клик
            // выигрывает.
            Ok(false) => return resolve_failure(Code::InvalidArgument),
            Err(code) => return resolve_failure(code),
        }

        if next_state == ProposalState::Muted {
            let _ = self
                .proactivity
                .mute(
                    &self.journal,
                    &record.mute_key,
                    record.kind,
                    &record.subject_key,
                    now_ms,
                )
                .await;
        }

        if let Ok(subject_key) =
            evohime_listener_contract::SubjectKey::new(record.subject_key.clone())
        {
            let _ = self
                .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Proposal {
                    proposal_id,
                    episode_id: record
                        .source_episode_id
                        .as_ref()
                        .and_then(|id| evohime_listener_contract::EpisodeId::new(id.clone()).ok()),
                    kind: record.kind,
                    subject_key,
                    proposal_state: next_state,
                })
                .await;
        }

        serde_json::json!({
            "applied": true,
            "state": next_state.as_str(),
            "task_id": task_id.unwrap_or_default(),
            "error_code": "",
        })
    }

    /// Единственный эффект принятого предложения: строка в списке задач.
    ///
    /// Оба вида — обычная запись `work_items` в статусе `backlog`, то есть
    /// ничего не запускающая сама. Напоминание отличается явным `non_goals`:
    /// «не выполняется автоматически» записано в данных, а не подразумевается.
    /// `source_ref` несёт `episode_id` — тот же провенанс, по которому
    /// удаление эпизода находит своих кандидатов памяти.
    async fn create_proposal_effect(
        &self,
        record: &evohime_local_storage::ambient_store::AmbientProposalRecord,
        idempotency_key: &str,
    ) -> Result<String, evohime_listener_contract::AmbientErrorCode> {
        use evohime_listener_contract::AmbientErrorCode as Code;

        // Проектная строка для услышанного заводится один раз и переиспользуется:
        // `work_items.project_id` — внешний ключ, и задача без проекта не
        // сохранится.
        self.journal
            .create_project(
                AMBIENT_PROPOSAL_PROJECT_ID,
                "Услышанное",
                "",
                Some(AMBIENT_PROPOSAL_PROJECT_ID),
            )
            .await
            .map_err(|_| Code::StorageFailed)?;

        let task_id = uuid::Uuid::new_v4().to_string();
        let non_goals = if record.kind == evohime_listener_contract::ProposalKind::Reminder {
            AMBIENT_REMINDER_NON_GOAL.to_owned()
        } else {
            String::new()
        };
        let item = evohime_local_storage::WorkItemRecord {
            id: task_id.clone(),
            project_id: AMBIENT_PROPOSAL_PROJECT_ID.to_owned(),
            parent_id: None,
            title: record.title.clone(),
            description: String::new(),
            source_ref: record.source_episode_id.clone(),
            acceptance_criteria: String::new(),
            non_goals,
            // `backlog`, а не `ready`: подбор следующей задачи берёт только
            // `ready`, поэтому принятое предложение не начинает выполняться
            // само по себе.
            status: "backlog".to_owned(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        };
        // Тот же dedup-путь, что у `CreateTask`: повторный запрос с этим
        // ключом не создаёт второй записи, а возвращает **ту** задачу, что
        // была создана первым кликом. Свежий идентификатор здесь был бы
        // ссылкой в пустоту.
        if let Some(replay) = self
            .journal
            .record_deduplicated(
                AMBIENT_PROPOSAL_CLIENT_ID,
                idempotency_key,
                &record.proposal_id,
                b"",
            )
            .await
            .map_err(|_| Code::StorageFailed)?
        {
            return String::from_utf8(replay).map_err(|_| Code::StorageFailed);
        }
        self.journal
            .create_work_item(&item)
            .await
            .map_err(|_| Code::StorageFailed)?;
        self.journal
            .record_deduplicated(
                AMBIENT_PROPOSAL_CLIENT_ID,
                idempotency_key,
                &record.proposal_id,
                task_id.as_bytes(),
            )
            .await
            .map_err(|_| Code::StorageFailed)?;
        Ok(task_id)
    }

    /// Очередь услышанных команд. Заголовок приложения приходит только здесь:
    /// событие журнала несёт лишь ключ каталога.
    fn dispatch_list_voice_commands(
        &self,
        request: generated::ListVoiceCommands,
    ) -> serde_json::Value {
        let now_ms = crate::task_memory::now_millis();
        let policy = crate::ambient::load_policy(&self.ambient_data_dir());
        let limit = usize::try_from(request.limit)
            .unwrap_or(crate::voice_command::MAX_PENDING)
            .clamp(1, crate::voice_command::MAX_PENDING);
        let commands = self
            .voice_commands
            .list(now_ms)
            .into_iter()
            .take(limit)
            .map(|command| {
                serde_json::json!({
                    "command_id": command.command_id,
                    "kind": command.kind.as_str(),
                    "app_id": command.app_id,
                    "title": command.title,
                    "created_at_ms": command.created_at_ms,
                    "expires_at_ms": command.expires_at_ms(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "commands": commands,
            "requires_confirmation": !policy.voice_commands_autorun,
        })
    }

    /// Решение по услышанной команде.
    ///
    /// Карточка снимается с очереди до запуска, а не после: иначе двойной клик
    /// открыл бы два окна. Второй клик поэтому находит пустоту и получает
    /// `not_found`, а не второй запуск.
    async fn dispatch_resolve_voice_command(
        &self,
        request: generated::ResolveVoiceCommand,
    ) -> serde_json::Value {
        use evohime_listener_contract::VoiceCommandState;

        let now_ms = crate::task_memory::now_millis();
        let Some(command) = self.voice_commands.take(&request.command_id, now_ms) else {
            return serde_json::json!({
                "launched": false,
                "state": VoiceCommandState::Expired.as_str(),
                "app_id": "",
                "error_code": "not_found",
            });
        };
        if !request.accepted {
            self.publish_voice_command(&command, VoiceCommandState::Declined)
                .await;
            return serde_json::json!({
                "launched": false,
                "state": VoiceCommandState::Declined.as_str(),
                "app_id": command.app_id,
                "error_code": "",
            });
        }
        let registry = self.voice_commands.clone();
        let launch_command = command.clone();
        let launched = tokio::task::spawn_blocking(move || {
            crate::voice_command::launch(&registry, &launch_command, now_ms)
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        match launched {
            Ok(_) => {
                self.publish_voice_command(&command, VoiceCommandState::Launched)
                    .await;
                serde_json::json!({
                    "launched": true,
                    "state": VoiceCommandState::Launched.as_str(),
                    "app_id": command.app_id,
                    "error_code": "",
                })
            }
            Err(error) => {
                self.publish_voice_command(&command, VoiceCommandState::Failed)
                    .await;
                // Текст ошибки идёт в трассу, а не в ответ: в нём путь к
                // исполняемому файлу, которому в UI делать нечего.
                crate::write_model_trace(
                    "ambient.voice_command.launch_failed",
                    serde_json::json!({ "app_id": command.app_id, "error": error }),
                );
                serde_json::json!({
                    "launched": false,
                    "state": VoiceCommandState::Failed.as_str(),
                    "app_id": command.app_id,
                    "error_code": "launch_failed",
                })
            }
        }
    }

    async fn publish_voice_command(
        &self,
        command: &crate::voice_command::PendingCommand,
        state: evohime_listener_contract::VoiceCommandState,
    ) {
        let (Ok(command_id), Ok(app_id)) = (
            evohime_listener_contract::CommandId::new(command.command_id.clone()),
            evohime_listener_contract::AppId::new(command.app_id.clone()),
        ) else {
            return;
        };
        let _ = self
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::VoiceCommand {
                command_id,
                kind: command.kind,
                app_id,
                command_state: state,
            })
            .await;
    }

    async fn dispatch_create_project(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        request: generated::CreateProject,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateProject {
                client_id,
                request_id,
                command_hash,
                project_id: request.project_id,
                title: request.title,
                workspace_path: request.workspace_path,
                source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_create_task(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        item: WorkItemRecord,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateTask {
                client_id,
                request_id,
                command_hash,
                item,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_update_status(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        task_id: String,
        expected_version: i64,
        status: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::UpdateTaskStatus {
                client_id,
                request_id,
                command_hash,
                task_id,
                expected_version,
                status,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_add_edge(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        from_task_id: String,
        to_task_id: String,
        kind: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AddTaskEdge {
                client_id,
                request_id,
                command_hash,
                from_task_id,
                to_task_id,
                kind,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_graph(&self, project_id: String) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskGraph { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_next_ready_task(
        &self,
        project_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::NextReadyTask { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_import_prd(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        request: generated::ImportPrd,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ImportPrd {
                client_id,
                request_id,
                command_hash,
                import_id: request.import_id,
                project_id: request.project_id,
                origin: request.origin,
                version: request.version,
                source_text: request.source_text,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_history(
        &self,
        task_id: String,
        limit: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskHistory {
                task_id,
                limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_context(
        &self,
        project_id: String,
        task_id: String,
        max_chars: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskContext {
                project_id,
                task_id,
                max_chars,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_plan_spec(
        &self,
        project_id: String,
        task_id: String,
        max_chars: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskPlanSpec {
                project_id,
                task_id,
                max_chars,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_apply_approved_build(
        &self,
        project_id: String,
        run_id: String,
        task_id: String,
        approved_build_json: Vec<u8>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ApplyApprovedBuild {
                project_id,
                run_id,
                task_id,
                approved_build_json,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_prepare_build(
        &self,
        project_id: String,
        proposal_json: Vec<u8>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PrepareBuild {
                project_id,
                proposal_json,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_snapshot(
        &self,
        project_id: String,
        task_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskSnapshot {
                project_id,
                task_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_restore_task_snapshot(
        &self,
        project_id: String,
        task_id: String,
        snapshot_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RestoreTaskSnapshot {
                project_id,
                task_id,
                snapshot_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_build_policy(
        &self,
        project_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetBuildPolicy { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_save_build_policy(
        &self,
        project_id: String,
        policy_json: Vec<u8>,
        expected_version: i64,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SaveBuildPolicy {
                project_id,
                policy_json,
                expected_version,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_run_doctor(
        &self,
        project_id: String,
        detail_level: i32,
        protocol: Option<generated::ProtocolVersion>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let approval_required = match &self.tools {
            Some(tools) => !matches!(
                tools.permissions().mode(Permission::FilesystemWrite).await,
                PermissionMode::Allow
            ),
            None => true,
        };
        let (registered_tools, expected_tools, unavailable_tools) = match &self.tools {
            Some(tools) => {
                let names = tools.list();
                (names.len() as u32, EXPECTED_TOOL_COUNT, Vec::new())
            }
            None => (0, EXPECTED_TOOL_COUNT, Vec::new()),
        };
        let detail_level = if detail_level == 1 {
            crate::doctor::DetailLevel::Detailed
        } else {
            crate::doctor::DetailLevel::Summary
        };
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RunDoctor {
                project_id,
                protocol_major: protocol.map(|version| version.major),
                expected_protocol_major: PROTOCOL_MAJOR,
                provider: self.provider_probe(),
                approval_required,
                registered_tools,
                expected_tools,
                unavailable_tools,
                detail_level,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_export_doctor_logs(
        &self,
        destination_path: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ExportDoctorLogs {
                destination_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_create_database_backup<W: AsyncWrite + Unpin>(
        &self,
        operation_id: String,
        destination_path: String,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        let (reply, _response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateDatabaseBackup {
                operation_id,
                destination_path,
                progress,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let payload = serde_json::to_vec(&serde_json::json!({"accepted": true}))?;
        self.write_response(writer, "storage.backup.started", payload)
            .await?;
        Ok(())
    }

    async fn dispatch_prepare_database_restore(
        &self,
        operation_id: String,
        backup_path: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PrepareDatabaseRestore {
                operation_id,
                backup_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_restore_database<W: AsyncWrite + Unpin>(
        &self,
        operation_id: String,
        backup_path: String,
        approval_id: String,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        let (reply, _response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RestoreDatabase {
                operation_id,
                backup_path,
                approval_id,
                progress,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let payload = serde_json::to_vec(&serde_json::json!({"accepted": true}))?;
        self.write_response(writer, "storage.restore.started", payload)
            .await?;
        Ok(())
    }

    async fn dispatch_cancel_database_operation(
        &self,
        operation_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CancelDatabaseOperation {
                operation_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_save_research_evidence(
        &self,
        request: generated::SaveResearchEvidence,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SaveResearchEvidence {
                work_item_id: request.work_item_id,
                source_kind: request.source_kind,
                source_ref: request.source_ref,
                title: request.title,
                publisher: request.publisher,
                content_type: request.content_type,
                raw_excerpt: request.raw_excerpt,
                retrieved_at_ms: request.retrieved_at_ms,
                ttl_ms: request.ttl_ms,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_research_evidence(
        &self,
        work_item_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListResearchEvidence {
                work_item_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_run_research_fetch(
        &self,
        request: generated::RunResearchFetch,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RunResearchFetch {
                work_item_id: request.work_item_id,
                url: request.url,
                title: request.title,
                allowed_domains: request.allowed_domains,
                max_bytes: request.max_bytes,
                max_latency_ms: request.max_latency_ms,
                max_cost_micros: request.max_cost_micros,
                ttl_ms: request.ttl_ms,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_create_memory(
        &self,
        request: generated::CreateMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                title: request.title,
                content: request.content,
                provenance_kind: request.provenance_kind,
                provenance_id: request.provenance_id,
                provenance_locator: request.provenance_locator,
                privacy: request.privacy,
                ttl_ms: request.ttl_ms,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_memory(
        &self,
        request: generated::ListMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                include_archived: request.include_archived,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_search_memory(
        &self,
        request: generated::SearchMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SearchMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                query: request.query,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_archive_memory(
        &self,
        request: generated::ArchiveMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ArchiveMemory {
                id: request.id,
                approval_id: request.approval_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_forget_memory(
        &self,
        request: generated::ForgetMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ForgetMemory {
                id: request.id,
                approval_id: request.approval_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_memory(
        &self,
        request: generated::GetMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetMemory {
                id: request.id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_memory_pending(
        &self,
        request: generated::ListMemoryPending,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListMemoryPending {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                limit: request.limit,
                workspace_path: request.workspace_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_memory_conflicts(
        &self,
        request: generated::GetMemoryConflicts,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetMemoryConflicts {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                limit: request.limit,
                workspace_path: request.workspace_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_confirm_memory(
        &self,
        request: generated::ConfirmMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ConfirmMemory {
                ids: request.ids,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_reject_memory(
        &self,
        request: generated::RejectMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RejectMemory {
                ids: request.ids,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_revise_memory_candidate(
        &self,
        request: generated::ReviseMemoryCandidate,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ReviseMemoryCandidate {
                id: request.id,
                statement: request.statement,
                session_only: request.session_only,
                session_id: request.session_id,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_supersede_memory(
        &self,
        request: generated::SupersedeMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SupersedeMemory {
                old_id: request.old_id,
                new_id: request.new_id,
                reason: request.reason,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_install_capability(
        &self,
        request: generated::InstallCapability,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let toolkit_manifest = serde_json::from_str::<serde_json::Value>(&request.manifest_json)
            .ok()
            .filter(|value| {
                value.get("kind").and_then(serde_json::Value::as_str) == Some("tool/manifest/v1")
            });
        let toolkit_source = request.install_source.clone();
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::InstallCapability {
                manifest_json: request.manifest_json,
                install_source: request.install_source,
                source_path: request.source_path,
                expected_content_hash: request.expected_content_hash,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let result = response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)?;
        if let Some(manifest) = toolkit_manifest {
            let record = evohime_local_storage::toolkit_store::ToolkitRecord {
                toolkit_id: manifest
                    .get("tool_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                version: manifest
                    .get("version")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                manifest_hash: serde_json::from_value::<evohime_tool_runtime::ToolManifest>(
                    manifest.clone(),
                )
                .ok()
                .and_then(|value| value.canonical_hash().ok())
                .unwrap_or_default(),
                source: toolkit_source,
                package_hash: manifest
                    .get("package_hash")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                license: manifest
                    .get("license")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                status: "available".into(),
                compatible_core: manifest
                    .get("compatible_core")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                manifest_json: serde_json::to_vec(&manifest).unwrap_or_default(),
                created_at: String::new(),
                updated_at: String::new(),
            };
            if !record.toolkit_id.is_empty() && !record.version.is_empty() {
                let database = self.journal.database();
                let database = database.lock().await;
                evohime_local_storage::toolkit_store::discover(database.connection(), &record)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
            }
        }
        Ok(result)
    }

    async fn dispatch_list_capabilities(
        &self,
        request: generated::ListCapabilities,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListCapabilities {
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_match_capabilities(
        &self,
        request: generated::MatchCapabilities,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::MatchCapabilities {
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_remove_capability(
        &self,
        request: generated::RemoveCapability,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RemoveCapability {
                id: request.id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_toolkits(
        &self,
        request: generated::ListToolkits,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let database = self.journal.database();
        let database = database.lock().await;
        let records = evohime_local_storage::toolkit_store::list(
            database.connection(),
            request.limit as usize,
        )
        .map_err(|error| FrameError::Io(error.to_string()))?;
        serde_json::to_vec(&serde_json::json!({"toolkits": records})).map_err(IpcBridgeError::from)
    }

    async fn dispatch_toolkit_status(
        &self,
        toolkit_id: String,
        version: String,
        reason: String,
        status: &str,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let database = self.journal.database();
        let database = database.lock().await;
        if status == "rollback" {
            evohime_local_storage::toolkit_store::rollback(
                database.connection(),
                &toolkit_id,
                &version,
                &reason,
            )
        } else {
            evohime_local_storage::toolkit_store::transition(
                database.connection(),
                &toolkit_id,
                &version,
                status,
                &reason,
            )
        }
        .map_err(|error| FrameError::Io(error.to_string()))?;
        serde_json::to_vec(&serde_json::json!({
            "toolkit_id": toolkit_id,
            "version": version,
            "status": status,
            "applied": true
        }))
        .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_capability_selection(
        &self,
        request: generated::GetCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetCapabilitySelection {
                task_id: request.task_id,
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_pin_capability_selection(
        &self,
        request: generated::PinCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PinCapabilitySelection {
                task_id: request.task_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_replace_capability_selection(
        &self,
        request: generated::ReplaceCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ReplaceCapabilitySelection {
                task_id: request.task_id,
                manifest_name: request.manifest_name,
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_request_child_handoff(
        &self,
        request: generated::RequestChildHandoff,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RequestChildHandoff {
                handoff_id: request.handoff_id,
                task_id: request.task_id,
                kind: request.kind,
                from_role: request.from_role,
                from_name: request.from_name,
                to_role: request.to_role,
                to_name: request.to_name,
                purpose: request.purpose,
                payload: request.payload,
                sequence: request.sequence,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_child_handoffs(
        &self,
        request: generated::ListChildHandoffs,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListChildHandoffs {
                task_id: request.task_id,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_submit_child_request(
        &self,
        request: generated::SubmitChildRequest,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildRequest {
                child_task_id: request.child_task_id,
                parent_task_id: request.parent_task_id,
                role: request.role,
                kind: request.kind,
                reduced_context: request.reduced_context,
                max_output_bytes: request.max_output_bytes,
                requested_capabilities: request.requested_capabilities,
                parent_is_child: request.parent_is_child,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_submit_child_report(
        &self,
        request: generated::SubmitChildReport,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildReport {
                child_task_id: request.child_task_id,
                status: request.status,
                summary: request.summary,
                findings: request.findings,
                sources: request.sources,
                confidence_percent: request.confidence_percent,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_submit_feedback(
        &self,
        request: generated::SubmitFeedback,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitFeedback {
                run_id: request.run_id,
                task_id: (!request.task_id.trim().is_empty()).then_some(request.task_id),
                subject_ref: (!request.subject_ref.trim().is_empty())
                    .then_some(request.subject_ref),
                signal: request.signal,
                correction: (!request.correction.trim().is_empty()).then_some(request.correction),
                rejection_reason: (!request.rejection_reason.trim().is_empty())
                    .then_some(request.rejection_reason),
                outcome: (!request.outcome.trim().is_empty()).then_some(request.outcome),
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_feedback(
        &self,
        request: generated::ListFeedback,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListFeedback {
                run_id: request.run_id,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    /// План 01.5: bounded projection состава контекста.
    async fn dispatch_index_workspace(
        &self,
        request: generated::IndexWorkspace,
        rebuild: bool,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| {
            if rebuild {
                CoreCommand::RebuildIndex {
                    workspace_path: request.workspace_path.clone(),
                    enable_embeddings: request.enable_embeddings,
                    reply,
                }
            } else {
                CoreCommand::IndexWorkspace {
                    workspace_path: request.workspace_path.clone(),
                    enable_embeddings: request.enable_embeddings,
                    reply,
                }
            }
        })
        .await
    }

    async fn dispatch_rebuild_index(
        &self,
        request: generated::RebuildIndex,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::RebuildIndex {
            workspace_path: request.workspace_path.clone(),
            enable_embeddings: request.enable_embeddings,
            reply,
        })
        .await
    }

    async fn dispatch_search_workspace_knowledge(
        &self,
        request: generated::SearchWorkspaceKnowledge,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::SearchWorkspaceKnowledge {
            workspace_path: request.workspace_path.clone(),
            query: request.query.clone(),
            path_filter: (!request.path_filter.trim().is_empty())
                .then(|| request.path_filter.clone()),
            language_filter: (!request.language_filter.trim().is_empty())
                .then(|| request.language_filter.clone()),
            hybrid: request.hybrid,
            reply,
        })
        .await
    }

    async fn dispatch_get_index_status(
        &self,
        request: generated::GetIndexStatus,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::GetIndexStatus {
            workspace_path: request.workspace_path.clone(),
            reply,
        })
        .await
    }

    async fn dispatch_cancel_workspace_index(
        &self,
        request: generated::CancelWorkspaceIndex,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::CancelWorkspaceIndex {
            workspace_path: request.workspace_path.clone(),
            reply,
        })
        .await
    }

    async fn dispatch_get_context_ledger(
        &self,
        request: generated::GetContextLedger,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::GetContextLedger {
            task_id: request.task_id.clone(),
            limit: request.limit,
            reply,
        })
        .await
    }

    async fn dispatch_list_task_scratchpad(
        &self,
        request: generated::ListTaskScratchpad,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ListTaskScratchpad {
            task_id: request.task_id.clone(),
            category: (!request.category.trim().is_empty()).then(|| request.category.clone()),
            status: (!request.status.trim().is_empty()).then(|| request.status.clone()),
            limit: request.limit,
            reply,
        })
        .await
    }

    async fn dispatch_clear_task_scratchpad(
        &self,
        request: generated::ClearTaskScratchpad,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ClearTaskScratchpad {
            task_id: request.task_id.clone(),
            reply,
        })
        .await
    }

    async fn dispatch_summarize_context_now(
        &self,
        request: generated::SummarizeContextNow,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::SummarizeContextNow {
            task_id: request.task_id.clone(),
            reply,
        })
        .await
    }

    async fn dispatch_pin_context_item(
        &self,
        request: generated::PinContextItem,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::PinContextItem {
            task_id: request.task_id.clone(),
            item_id: request.item_id.clone(),
            pinned: request.pinned,
            reply,
        })
        .await
    }

    async fn dispatch_read_context_artifact(
        &self,
        request: generated::ReadContextArtifact,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ReadContextArtifact {
            task_id: request.task_id.clone(),
            locator: request.locator.clone(),
            reply,
        })
        .await
    }

    /// Общая отправка команды контекста в очередь Core.
    async fn dispatch_context<F>(&self, build: F) -> Result<Vec<u8>, IpcBridgeError>
    where
        F: FnOnce(oneshot::Sender<Result<Vec<u8>, String>>) -> CoreCommand,
    {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(build(reply))
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    /// Configuration as the shell should see it: the route's own model unless
    /// the shell selected another one for the next request.
    fn current_model_config(&self) -> Option<ModelConfigSnapshot> {
        let config = self.model_config.as_ref()?;
        let Some(model) = self.selected_model.get() else {
            return Some(config.clone());
        };
        Some(ModelConfigSnapshot {
            model,
            ..config.clone()
        })
    }

    /// Builds a Core Doctor provider probe from already-loaded, secret-free
    /// gateway configuration. Never exposes an API key value, only whether
    /// one is present.
    fn provider_probe(&self) -> crate::doctor::ProviderProbe {
        let (provider_id, model_id, configured) = match &self.model_config {
            Some(config) => (
                config.provider.clone(),
                config.model.clone(),
                config.configured,
            ),
            None => (String::new(), String::new(), false),
        };
        let key_present = self
            .gateway_config
            .as_ref()
            .and_then(|config| config.routes.get(&config.default_route))
            .map(|route| route.configured())
            .unwrap_or(false);
        let metadata_valid = !provider_id.is_empty() && !model_id.is_empty();
        crate::doctor::ProviderProbe {
            provider_id,
            model_id,
            configured,
            key_present,
            metadata_valid,
        }
    }

    async fn dispatch_git_read(
        &self,
        workspace_path: String,
        tool_name: &str,
        input: serde_json::Value,
        requested_max_bytes: u32,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        const DEFAULT_MAX_BYTES: usize = 512 * 1024;
        let max_bytes = if requested_max_bytes == 0 {
            DEFAULT_MAX_BYTES
        } else {
            (requested_max_bytes as usize).min(DEFAULT_MAX_BYTES)
        };
        let tools = self
            .tools
            .as_ref()
            .ok_or_else(|| FrameError::Io("Git tools are not configured".into()))?;
        let context = ToolContext {
            workspace_root: std::path::PathBuf::from(workspace_path),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let result = tools
            .execute(&context, tool_name, input)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let bytes = result.output.as_bytes();
        let truncated = bytes.len() > max_bytes;
        let output = if truncated {
            String::from_utf8_lossy(&bytes[..max_bytes]).into_owned()
        } else {
            result.output
        };
        serde_json::to_vec(&serde_json::json!({
            "output": output,
            "structured": result.structured,
            "truncated": truncated,
            "max_bytes": max_bytes,
        }))
        .map_err(|error| FrameError::Io(error.to_string()).into())
    }

    fn terminal_capability_snapshot(
        action_id: uuid::Uuid,
        context: &ToolContext,
        scope: &str,
    ) -> Result<evohime_receipts::capability::CapabilitySnapshotV1, String> {
        use evohime_receipts::capability::{CapabilityLimits, CapabilitySnapshotV1};
        let task_id = context.task_id.to_string();
        let session_id = context
            .session_id
            .map_or_else(|| "session:anonymous".to_owned(), |id| id.to_string());
        CapabilitySnapshotV1 {
            snapshot_id: format!("snapshot:{action_id}"),
            run_id: format!("run:{task_id}"),
            session_id,
            task_id: format!("task:{task_id}"),
            parent_snapshot_hash: None,
            policy_id: "policy:terminal-v1".into(),
            policy_version: 1,
            policy_hash: evohime_receipts::sha256_hex(b"policy:terminal-v1"),
            manifest_hash: evohime_receipts::sha256_hex(b"builtin:shell.execute:v1"),
            workspace_anchors: vec![context.workspace_root.to_string_lossy().into_owned()],
            operation_scopes: vec![scope.to_owned()],
            permissions: vec!["shell_execute".into()],
            tool_identities: vec!["shell.execute".into()],
            network_routes: vec![],
            adapter_scopes: vec![],
            secret_refs: vec![],
            limits: CapabilityLimits {
                timeout_ms: 30_000,
                input_bytes: 64 * 1024,
                output_bytes: 512 * 1024,
                concurrency: 1,
                tool_calls: 1,
                token_budget: 0,
                cost_micros: 0,
            },
            snapshot_hash: String::new(),
        }
        .finalize()
        .map_err(|error| error.to_string())
    }

    async fn execute_terminal_with_receipt(
        &self,
        context: &ToolContext,
        input: serde_json::Value,
        cancellation: CancellationToken,
    ) -> Result<evohime_tool_runtime::ToolResult, evohime_tool_runtime::ToolError> {
        match self
            .tools
            .as_ref()
            .ok_or_else(|| {
                evohime_tool_runtime::ToolError::Execution(
                    "Terminal tools are not configured".into(),
                )
            })?
            .preflight(context, "shell.execute", &input)
            .await?
        {
            evohime_tool_runtime::ToolPreflightDecision::Allowed { scope, preview } => {
                let scope = self
                    .tools
                    .as_ref()
                    .unwrap()
                    .permissions()
                    .normalize_scope(&scope)
                    .map_err(evohime_tool_runtime::ToolError::Execution)?;
                let request = evohime_receipts::runtime::ActionRequest {
                    action_id: uuid::Uuid::now_v7(),
                    task_id: context.task_id.to_string(),
                    run_id: context.task_id.to_string(),
                    tool_name: "shell.execute".into(),
                    policy_id: "permission:ShellExecute".into(),
                    normalized_scope: scope.clone(),
                    input: input.clone(),
                    policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                    approval_id: None,
                    parent_approval_ref: None,
                    preview: serde_json::to_string(&preview).unwrap_or_else(|_| "terminal".into()),
                };
                let capability =
                    Self::terminal_capability_snapshot(request.action_id, context, &scope)
                        .map_err(evohime_tool_runtime::ToolError::Execution)?;
                let gate = super::policy_gate::PolicyGate::new(capability.clone()).map_err(
                    |decision| evohime_tool_runtime::ToolError::Execution(decision.reason_code),
                )?;
                let binding = gate
                    .preflight(
                        &request.action_id.to_string(),
                        &request.tool_name,
                        &request.normalized_scope,
                        &request.input,
                        evohime_receipts::capability::PolicyOutcome::Allowed,
                    )
                    .map_err(|decision| {
                        evohime_tool_runtime::ToolError::Execution(decision.reason_code)
                    })?;
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                    database.connection_mut(),
                    &signer,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                let prepared = match runtime.prepare(request.clone()) {
                    Ok(value) => value,
                    Err(error) => {
                        let marker = if error.to_string().contains("signer_unavailable") {
                            "signer_unavailable"
                        } else {
                            "storage_key_unavailable"
                        };
                        let _ = runtime.store_unsigned_runtime_marker(request.action_id, marker);
                        return Err(evohime_tool_runtime::ToolError::Execution(
                            error.to_string(),
                        ));
                    }
                };
                if !matches!(
                    prepared,
                    evohime_receipts::runtime::PrepareOutcome::Prepared { .. }
                ) {
                    return Err(evohime_tool_runtime::ToolError::Execution(
                        "receipt.precondition_failed".into(),
                    ));
                }
                evohime_receipts::runtime::bind_capability_to_action(
                    database.connection(),
                    request.action_id,
                    &capability,
                    1,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                let decision = evohime_receipts::capability::PolicyDecision::new(
                    evohime_receipts::capability::PolicyOutcome::Allowed,
                    "preflight_allowed",
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                evohime_receipts::runtime::persist_policy_decision(
                    database.connection(),
                    request.action_id,
                    Some(&capability.snapshot_hash),
                    &decision,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                    database.connection_mut(),
                    &signer,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                runtime
                    .mark_started(request.action_id)
                    .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                record_ledger_tool_call(
                    &database,
                    &request,
                    context.session_id.map(|id| id.to_string()),
                );
                drop(database);
                gate.recheck_before_effect(
                    &binding,
                    &request.tool_name,
                    &request.normalized_scope,
                    &request.input,
                    evohime_receipts::capability::PolicyOutcome::Allowed,
                )
                .map_err(|decision| {
                    evohime_tool_runtime::ToolError::Execution(decision.reason_code)
                })?;
                let result = self
                    .tools
                    .as_ref()
                    .unwrap()
                    .execute_with_cancellation(context, "shell.execute", input, cancellation)
                    .await;
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                    database.connection_mut(),
                    &signer,
                )
                .map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                match &result {
                    Ok(value) => {
                        runtime.mark_returned(request.action_id).map_err(|e| {
                            evohime_tool_runtime::ToolError::Execution(e.to_string())
                        })?;
                        let digest = evohime_receipts::sha256_hex(value.output.as_bytes());
                        let receipt_hash = runtime
                            .complete(&request, "succeeded", &digest, None)
                            .map_err(|e| {
                            evohime_tool_runtime::ToolError::Execution(e.to_string())
                        })?;
                        // План 08-4: the "observation" link of "action →
                        // tool call → observation → receipt" — a bounded
                        // content-addressed marker of the tool's output,
                        // published right before the terminal receipt so a
                        // reader sees the result was observed before it was
                        // signed. `runtime`'s borrow of `database` has to
                        // end (last use above) before this can borrow it.
                        record_ledger_tool_outcome(
                            &database,
                            &request,
                            context.session_id.map(|id| id.to_string()),
                            execution_ledger::ActionState::Running,
                            execution_ledger::ExecutionEventBody::Observation {
                                summary_digest: digest.clone(),
                                artifact_refs: Vec::new(),
                            },
                        );
                        record_ledger_tool_outcome(
                            &database,
                            &request,
                            context.session_id.map(|id| id.to_string()),
                            execution_ledger::ActionState::Succeeded,
                            execution_ledger::ExecutionEventBody::ToolReceipt {
                                receipt_action_id: request.action_id.to_string(),
                                receipt_hash,
                            },
                        );
                    }
                    Err(error) => {
                        runtime.mark_returned(request.action_id).map_err(|e| {
                            evohime_tool_runtime::ToolError::Execution(e.to_string())
                        })?;
                        let failed_digest = evohime_receipts::sha256_hex(b"tool_error");
                        if runtime
                            .complete(&request, "failed", &failed_digest, Some("tool_error"))
                            .is_ok()
                        {
                            // Ledger observability gets the specific bounded
                            // error code (e.g. "timed_out") even though the
                            // receipt's own error_category stays the coarser
                            // "tool_error" it already used — changing that
                            // category is a receipts-crate decision, out of
                            // scope here.
                            record_ledger_tool_outcome(
                                &database,
                                &request,
                                context.session_id.map(|id| id.to_string()),
                                execution_ledger::ActionState::Failed,
                                execution_ledger::ExecutionEventBody::TypedFailure {
                                    error_class: bounded_tool_error_code(error).to_string(),
                                    provider_error_id: None,
                                },
                            );
                            return result;
                        }
                        let mut recovery_code = "signature_failed";
                        let pre_hash = runtime
                            .action(request.action_id)
                            .ok()
                            .flatten()
                            .and_then(|row| row.pre_receipt_hash)
                            .unwrap_or_default();
                        let key_id = match self.receipt_keys.storage_key_id() {
                            Ok(value) => value,
                            Err(_) => {
                                recovery_code = "storage_key_unavailable";
                                "unavailable".to_owned()
                            }
                        };
                        let row = ProtectedActionRow {
                            schema_version: 1,
                            action_id: request.action_id.to_string(),
                            pre_receipt_hash: pre_hash,
                            tool_args_hash: evohime_receipts::runtime::canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input).unwrap_or_default(),
                            result_status: "failed".into(),
                            result_hash: evohime_receipts::result_hash(&serde_json::json!({"status":"failed","error_category":"tool_error"})).unwrap_or_else(|_| evohime_receipts::sha256_hex(b"tool_error")),
                            recovery_code: recovery_code.into(),
                            created_at_ms: SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis() as i64).unwrap_or_default(),
                            key_id,
                        };
                        if let Ok(plain) = serde_json::to_vec(&row) {
                            if let Ok(envelope) = self.receipt_keys.protect_storage(&plain) {
                                if runtime.store_protected_envelope(&row, envelope).is_err() {
                                    recovery_code = "storage_key_unavailable";
                                }
                            } else {
                                recovery_code = "storage_key_unavailable";
                            }
                        } else {
                            recovery_code = "storage_key_unavailable";
                        }
                        if recovery_code == "storage_key_unavailable" {
                            let _ = runtime.store_unsigned_runtime_marker(
                                request.action_id,
                                "storage_key_unavailable",
                            );
                        }
                        let _ = runtime.mark_pending_recovery(request.action_id, recovery_code);
                    }
                }
                result
            }
            evohime_tool_runtime::ToolPreflightDecision::Denied(permission) => Err(
                evohime_tool_runtime::ToolError::PermissionDenied(permission),
            ),
            evohime_tool_runtime::ToolPreflightDecision::ApprovalRequired { .. } => {
                // Preflight is a hard boundary. The ordinary execute path
                // creates the approval request and returns NeedsApproval;
                // dispatching the implementation here would bypass policy.
                self.tools
                    .as_ref()
                    .unwrap()
                    .execute_with_cancellation(context, "shell.execute", input, cancellation)
                    .await
            }
        }
    }

    async fn dispatch_terminal_execute<W: AsyncWrite + Unpin>(
        &self,
        request: generated::TerminalExecute,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        const DEFAULT_TIMEOUT_MS: u32 = 30_000;
        const MAX_OUTPUT_BYTES: usize = 512 * 1024;
        let tools = self
            .tools
            .as_ref()
            .ok_or_else(|| FrameError::Io("Terminal tools are not configured".into()))?;
        let task_id = uuid::Uuid::parse_str(&request.task_id)
            .map_err(|error| FrameError::Io(format!("invalid terminal task id: {error}")))?;
        let workspace_root = std::path::PathBuf::from(request.workspace_path);
        let input = serde_json::json!({
            "program": request.program,
            "args": request.args,
            "cwd": (!request.cwd.is_empty()).then_some(request.cwd),
            "timeout_ms": if request.timeout_ms == 0 { DEFAULT_TIMEOUT_MS } else { request.timeout_ms.min(DEFAULT_TIMEOUT_MS) },
        });
        let context = ToolContext {
            workspace_root,
            task_id,
            session_id: None,
            progress_tx: None,
        };
        let cancellation = tokio_util::sync::CancellationToken::new();
        let result = if request.approval_id.is_empty() {
            match self
                .execute_terminal_with_receipt(&context, input.clone(), cancellation.clone())
                .await
            {
                Ok(result) => result,
                Err(evohime_tool_runtime::ToolError::NeedsApproval(details)) => {
                    let evohime_tool_runtime::ApprovalRequired {
                        tool,
                        permission,
                        scope,
                        approval_id,
                        input,
                        preview,
                    } = *details;
                    let durable_action_id = uuid::Uuid::now_v7();
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id: durable_action_id,
                        task_id: task_id.to_string(),
                        run_id: task_id.to_string(),
                        tool_name: tool.clone(),
                        policy_id: format!("permission:{permission:?}"),
                        normalized_scope: scope.clone(),
                        input: input.clone(),
                        policy_decision:
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired,
                        approval_id: Some(approval_id),
                        parent_approval_ref: None,
                        preview: serde_json::to_string(&preview)
                            .unwrap_or_else(|_| "approval".into()),
                    };
                    let capability =
                        Self::terminal_capability_snapshot(durable_action_id, &context, &scope)
                            .map_err(|e| FrameError::Io(e.to_string()))?;
                    {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        runtime
                            .prepare_existing_approval(receipt_request)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                        evohime_receipts::runtime::bind_capability_to_action(
                            database.connection(),
                            durable_action_id,
                            &capability,
                            1,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        let decision = evohime_receipts::capability::PolicyDecision::new(
                            evohime_receipts::capability::PolicyOutcome::ApprovalRequired,
                            "approval_required",
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        evohime_receipts::runtime::persist_policy_decision(
                            database.connection(),
                            durable_action_id,
                            Some(&capability.snapshot_hash),
                            &decision,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                    self.write_response(
                        writer,
                        "approval.required",
                        serde_json::to_vec(&serde_json::json!({
                            "task_id": task_id.to_string(),
                            "approval_id": approval_id.to_string(),
                            "tool_name": tool,
                            "permission": format!("{permission:?}"),
                            "scope": scope,
                            "preview": preview,
                        }))?,
                    )
                    .await?;
                    return Ok(());
                }
                Err(error) => {
                    return self
                        .write_response(
                            writer,
                            "terminal.result",
                            serde_json::to_vec(&serde_json::json!({
                                "task_id": task_id.to_string(),
                                "ok": false,
                                "error_code": bounded_tool_error_code(&error),
                            }))?,
                        )
                        .await;
                }
            }
        } else {
            let approval_id = uuid::Uuid::parse_str(&request.approval_id).map_err(|error| {
                FrameError::Io(format!("invalid terminal approval id: {error}"))
            })?;
            let (action_id, receipt_request) = {
                let database = self.journal.database().lock().await;
                let (action_id, receipt_scope): (String, String) = database.connection().query_row(
                    "SELECT action_id,normalized_scope FROM receipt_approval_intents WHERE approval_id=?1",
                    [approval_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                let action_id = uuid::Uuid::parse_str(&action_id)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                (
                    action_id,
                    evohime_receipts::runtime::ActionRequest {
                        action_id,
                        task_id: task_id.to_string(),
                        run_id: task_id.to_string(),
                        tool_name: "shell.execute".into(),
                        policy_id: "permission:ShellExecute".into(),
                        normalized_scope: receipt_scope,
                        input: input.clone(),
                        policy_decision:
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired,
                        approval_id: Some(approval_id),
                        parent_approval_ref: None,
                        preview: "terminal approval".into(),
                    },
                )
            };
            let capability = Self::terminal_capability_snapshot(
                action_id,
                &context,
                &receipt_request.normalized_scope,
            )
            .map_err(|error| FrameError::Io(error.to_string()))?;
            {
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                    database.connection_mut(),
                    &signer,
                )
                .map_err(|error| FrameError::Io(error.to_string()))?;
                if let Err(error) = runtime.grant_approval(approval_id) {
                    // План 08-4 acceptance: the third arm of "approval
                    // approve/reject/expiry" — the approval window closed
                    // before the client claimed it. `grant_approval` is the
                    // one place `evohime-receipts` actually detects this
                    // (deadline check against the intent's own boot/wall
                    // clock), so it is the only honest place to observe it.
                    if matches!(
                        error,
                        evohime_receipts::runtime::RuntimeError::Code("approval_expired")
                    ) {
                        record_ledger_tool_outcome(
                            &database,
                            &receipt_request,
                            None,
                            execution_ledger::ActionState::TimedOut,
                            execution_ledger::ExecutionEventBody::ApprovalDecision {
                                approval_intent_id: approval_id.to_string(),
                                decision: execution_ledger::ApprovalOutcome::Expired,
                                snapshot_hash: None,
                            },
                        );
                    }
                    if matches!(
                        error,
                        evohime_receipts::runtime::RuntimeError::Code("approval_denied")
                    ) {
                        self.write_response(
                            writer,
                            "terminal.result",
                            serde_json::to_vec(&serde_json::json!({
                                "task_id": task_id.to_string(),
                                "ok": false,
                                "error_code": "approval_denied",
                            }))?,
                        )
                        .await?;
                        return Ok(());
                    }
                    return Err(FrameError::Io(error.to_string()).into());
                }
                runtime
                    .claim_approval_checked_with_binding(
                        &receipt_request,
                        approval_id,
                        &capability.session_id,
                        &capability.snapshot_hash,
                        capability.policy_version,
                        |_| true,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                runtime
                    .mark_started(action_id)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                record_ledger_tool_call(&database, &receipt_request, None);
            }
            match tools
                .execute_after_durable_approval(&context, "shell.execute", input, cancellation)
                .await
            {
                Ok(result) => {
                    let output_digest = evohime_receipts::sha256_hex(result.output.as_bytes());
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    runtime
                        .mark_returned(action_id)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let receipt_hash = runtime
                        .complete(&receipt_request, "succeeded", &output_digest, None)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    // See execute_terminal_with_receipt: the "observation"
                    // link of "action → tool call → observation → receipt".
                    record_ledger_tool_outcome(
                        &database,
                        &receipt_request,
                        None,
                        execution_ledger::ActionState::Running,
                        execution_ledger::ExecutionEventBody::Observation {
                            summary_digest: output_digest.clone(),
                            artifact_refs: Vec::new(),
                        },
                    );
                    record_ledger_tool_outcome(
                        &database,
                        &receipt_request,
                        None,
                        execution_ledger::ActionState::Succeeded,
                        execution_ledger::ExecutionEventBody::ToolReceipt {
                            receipt_action_id: action_id.to_string(),
                            receipt_hash,
                        },
                    );
                    result
                }
                Err(error) => {
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    ) {
                        let _ = runtime.mark_pending_recovery(action_id, "external_error");
                    }
                    // `mark_pending_recovery` (not a clean failure) leaves the
                    // dispatch marker open with an ambiguous outcome, so the
                    // ledger records this as `unknown_outcome`, not `failed` —
                    // the same distinction plan 08-2's startup reconciliation
                    // makes between "known failure" and "needs review".
                    record_ledger_tool_outcome(
                        &database,
                        &receipt_request,
                        None,
                        execution_ledger::ActionState::UnknownOutcome,
                        execution_ledger::ExecutionEventBody::TypedFailure {
                            error_class: "external_error".into(),
                            provider_error_id: None,
                        },
                    );
                    return self
                        .write_response(
                            writer,
                            "terminal.result",
                            serde_json::to_vec(&serde_json::json!({
                                "task_id": task_id.to_string(),
                                "ok": false,
                                "error_code": bounded_tool_error_code(&error),
                            }))?,
                        )
                        .await;
                }
            }
        };
        let bytes = result.output.as_bytes();
        let truncated = bytes.len() > MAX_OUTPUT_BYTES;
        let output = if truncated {
            String::from_utf8_lossy(&bytes[..MAX_OUTPUT_BYTES]).into_owned()
        } else {
            result.output
        };
        self.write_response(
            writer,
            "terminal.result",
            serde_json::to_vec(&serde_json::json!({
                "task_id": task_id.to_string(),
                "ok": true,
                "output": output,
                "structured": result.structured,
                "truncated": truncated,
                "max_bytes": MAX_OUTPUT_BYTES,
            }))?,
        )
        .await
    }

    /// Builds a control envelope (challenge, ready, protocol error) that the
    /// transport layer sends outside the command/response loop. Sequence 0
    /// keeps these events out of the replayable event stream.
    pub fn control_event(
        &self,
        event_type: &str,
        event: Option<generated::event_envelope::Event>,
        payload: Vec<u8>,
    ) -> generated::EventEnvelope {
        generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: event_type.into(),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event,
        }
    }

    /// The `core.ready` envelope this bridge answers a verified handshake with.
    pub fn ready_event(&self) -> generated::EventEnvelope {
        self.control_event(
            "core.ready",
            Some(generated::event_envelope::Event::Ready(generated::Ready {
                protocol: Some(protocol()),
                core_version: env!("CARGO_PKG_VERSION").into(),
                core_info: Some(core_info()),
            })),
            Vec::new(),
        )
    }

    async fn start_plan_review<W: AsyncWrite + Unpin>(
        &self,
        request: generated::StartPlanReview,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        if !request.file_name.to_ascii_lowercase().ends_with(".md") {
            return Err(FrameError::Io("review accepts Markdown files only".into()).into());
        }
        let context_documents =
            crate::plan_context::read_linked_plans(&request.source_paths, &request.source_markdown)
                .await;
        let review = crate::plan_review::ReviewRequest {
            review_id: request.review_id,
            file_name: request.file_name,
            file_names: request.file_names,
            source_markdown: request.source_markdown,
            reviewer_models: request.reviewer_models,
            synthesis_model: request.synthesis_model,
            context_documents,
        };
        review
            .validate()
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let gateway_config = self
            .gateway_config
            .clone()
            .ok_or_else(|| FrameError::Io("provider is not configured".into()))?;
        let gateway = evohime_model_gateway::ModelGateway::from_config(&gateway_config)
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let route = gateway_config
            .routes
            .get(&gateway_config.default_route)
            .ok_or_else(|| FrameError::Io("default provider route is missing".into()))?;
        let available = evohime_model_gateway::fetch_model_catalog(route)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        if review
            .reviewer_models
            .iter()
            .chain(std::iter::once(&review.synthesis_model))
            .any(|model| !available.iter().any(|entry| entry.id == *model))
        {
            return Err(FrameError::Io(
                "review model was not returned by the configured provider".into(),
            )
            .into());
        }
        let cancellation = CancellationToken::new();
        let review_id = review.review_id.clone();
        self.review_tasks
            .lock()
            .await
            .insert(review_id.clone(), cancellation.clone());
        let tasks = Arc::clone(&self.review_tasks);
        let results = Arc::clone(&self.review_results);
        let journal = self.journal.clone();
        let coordinator = self.coordinator.clone();
        let task_review_id = review_id.clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = Arc::new(move |progress: crate::plan_review::ReviewProgress| {
            let _ = progress_tx.send(progress);
        });
        tokio::spawn(async move {
            let progress_journal = journal.clone();
            let progress_coordinator = coordinator.clone();
            let progress_writer = tokio::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    publish_review_event(
                        &progress_coordinator,
                        &progress_journal,
                        CoreEvent::ReviewProgress {
                            review_id: progress.review_id,
                            stage: progress.stage,
                            status: progress.status,
                            model: progress.model,
                            completed: progress.completed,
                            total: progress.total,
                        },
                    )
                    .await;
                }
            });
            let event = match crate::plan_review::run_review_with_progress(
                Arc::new(gateway),
                review,
                cancellation,
                progress,
            )
            .await
            {
                Ok(result) => {
                    let payload = serde_json::to_string(&result).unwrap_or_default();
                    results
                        .lock()
                        .await
                        .insert(result.review_id.clone(), result.clone());
                    CoreEvent::TaskCompleted {
                        task_id: result.review_id,
                        final_message: payload,
                    }
                }
                Err(crate::plan_review::ReviewError::Cancelled) => CoreEvent::TaskStopped {
                    task_id: task_review_id.clone(),
                },
                Err(error) => CoreEvent::TaskFailed {
                    task_id: task_review_id.clone(),
                    error: error.to_string(),
                },
            };
            let _ = progress_writer.await;
            let terminal_progress = match &event {
                CoreEvent::TaskCompleted { .. } => Some(CoreEvent::ReviewProgress {
                    review_id: task_review_id.clone(),
                    stage: "completed".into(),
                    status: "completed".into(),
                    model: None,
                    completed: 1,
                    total: 1,
                }),
                CoreEvent::TaskFailed { .. } => Some(CoreEvent::ReviewProgress {
                    review_id: task_review_id.clone(),
                    stage: "failed".into(),
                    status: "failed".into(),
                    model: None,
                    completed: 0,
                    total: 1,
                }),
                _ => None,
            };
            if let Some(progress) = terminal_progress {
                publish_review_event(&coordinator, &journal, progress).await;
            }
            publish_review_event(&coordinator, &journal, event).await;
            tasks.lock().await.remove(&task_review_id);
        });
        self.write_response(
            writer,
            "review.started",
            serde_json::to_vec(&serde_json::json!({
                "review_id": review_id,
                "accepted": true,
            }))
            .unwrap_or_default(),
        )
        .await
    }

    /// Rewrites the plan a finished review was made for.
    ///
    /// The review text comes from Core's own cache or journal rather than from
    /// the shell: the shell may have been restarted, and a review the user did
    /// not actually run must never be passed off as one.
    async fn revise_plan<W: AsyncWrite + Unpin>(
        &self,
        request: generated::RevisePlan,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        if !request.file_name.to_ascii_lowercase().ends_with(".md") {
            return Err(FrameError::Io("revision accepts Markdown files only".into()).into());
        }
        let mut review = self
            .review_results
            .lock()
            .await
            .get(&request.review_id)
            .cloned();
        if review.is_none() {
            if let Ok(events) = self.journal.task_history(&request.review_id, 10).await {
                review = events
                    .iter()
                    .rev()
                    .find_map(|event| review_result_from_event(&event.payload));
            }
        }
        let review = review.ok_or_else(|| FrameError::Io("review not found".into()))?;
        let context_documents = crate::plan_context::read_linked_plans(
            std::slice::from_ref(&request.source_path),
            &request.source_markdown,
        )
        .await;
        let revision = crate::plan_review::RevisionRequest {
            revision_id: request.revision_id,
            review_id: request.review_id,
            file_name: request.file_name,
            source_markdown: request.source_markdown,
            review_markdown: strip_review_header(&review.final_markdown),
            model: request.model,
            context_documents,
        };
        revision
            .validate()
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let gateway_config = self
            .gateway_config
            .clone()
            .ok_or_else(|| FrameError::Io("provider is not configured".into()))?;
        let gateway = evohime_model_gateway::ModelGateway::from_config(&gateway_config)
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let route = gateway_config
            .routes
            .get(&gateway_config.default_route)
            .ok_or_else(|| FrameError::Io("default provider route is missing".into()))?;
        let available = evohime_model_gateway::fetch_model_catalog(route)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        if !available.iter().any(|entry| entry.id == revision.model) {
            return Err(FrameError::Io(
                "revision model was not returned by the configured provider".into(),
            )
            .into());
        }
        let cancellation = CancellationToken::new();
        let revision_id = revision.revision_id.clone();
        self.revision_tasks
            .lock()
            .await
            .insert(revision_id.clone(), cancellation.clone());
        let tasks = Arc::clone(&self.revision_tasks);
        let results = Arc::clone(&self.revision_results);
        let journal = self.journal.clone();
        let coordinator = self.coordinator.clone();
        let task_revision_id = revision_id.clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = Arc::new(move |progress: crate::plan_review::RevisionProgress| {
            let _ = progress_tx.send(progress);
        });
        tokio::spawn(async move {
            let progress_journal = journal.clone();
            let progress_coordinator = coordinator.clone();
            let progress_writer = tokio::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    publish_review_event(
                        &progress_coordinator,
                        &progress_journal,
                        CoreEvent::RevisionProgress {
                            revision_id: progress.revision_id,
                            status: progress.status,
                            model: progress.model,
                        },
                    )
                    .await;
                }
            });
            let event = match crate::plan_review::run_revision(
                Arc::new(gateway),
                revision,
                cancellation,
                progress,
            )
            .await
            {
                Ok(result) => {
                    let payload = serde_json::to_string(&result).unwrap_or_default();
                    results
                        .lock()
                        .await
                        .insert(result.revision_id.clone(), result.clone());
                    CoreEvent::TaskCompleted {
                        task_id: result.revision_id,
                        final_message: payload,
                    }
                }
                Err(crate::plan_review::ReviewError::Cancelled) => CoreEvent::TaskStopped {
                    task_id: task_revision_id.clone(),
                },
                Err(error) => CoreEvent::TaskFailed {
                    task_id: task_revision_id.clone(),
                    error: error.to_string(),
                },
            };
            let _ = progress_writer.await;
            publish_review_event(&coordinator, &journal, event).await;
            tasks.lock().await.remove(&task_revision_id);
        });
        self.write_response(
            writer,
            "revision.started",
            serde_json::to_vec(&serde_json::json!({
                "revision_id": revision_id,
                "accepted": true,
            }))
            .unwrap_or_default(),
        )
        .await
    }

    async fn dispatch_list_refinement_candidates(
        &self,
        request: generated::ListRefinementCandidates,
    ) -> generated::RefinementListProjection {
        let database = self.journal.database().lock().await;
        let store =
            evohime_local_storage::refinement_store::RefinementStore::new(database.connection());
        match store.list(&request.owner_scope, request.limit) {
            Ok(rows) => generated::RefinementListProjection {
                schema_version: crate::refinement::CONTRACT_VERSION,
                candidates: rows.into_iter().map(refinement_projection).collect(),
                truncated: request.limit > 0 && request.limit < 128,
                error_code: String::new(),
            },
            Err(_) => generated::RefinementListProjection {
                schema_version: crate::refinement::CONTRACT_VERSION,
                candidates: Vec::new(),
                truncated: false,
                error_code: "storage_failed".into(),
            },
        }
    }

    async fn dispatch_get_refinement_candidate(
        &self,
        request: generated::GetRefinementCandidate,
    ) -> generated::RefinementProjection {
        let database = self.journal.database().lock().await;
        let store =
            evohime_local_storage::refinement_store::RefinementStore::new(database.connection());
        store
            .get(&request.candidate_id, request.revision as i64)
            .ok()
            .flatten()
            .map(refinement_projection)
            .unwrap_or_else(|| refinement_projection_error("not_found"))
    }

    async fn dispatch_refinement_action(
        &self,
        request: generated::RefinementAction,
    ) -> generated::RefinementActionResult {
        let database = self.journal.database().lock().await;
        let store =
            evohime_local_storage::refinement_store::RefinementStore::new(database.connection());
        let Some(current) = store
            .get(&request.candidate_id, request.revision as i64)
            .ok()
            .flatten()
        else {
            return refinement_action_error(&request, "not_found");
        };
        if request.idempotency_key.trim().is_empty() {
            return refinement_action_error(&request, "missing_idempotency_key");
        }
        let request_hash = crate::refinement::content_hash(
            &serde_json::json!({
                "candidate_id": request.candidate_id,
                "revision": request.revision,
                "expected_version": request.expected_version,
                "action": request.action,
                "approval_token": request.approval_token,
            })
            .to_string(),
        );
        match store.replay_idempotency(
            &current.owner_scope,
            &request.idempotency_key,
            &request_hash,
        ) {
            Ok(Some(row)) => {
                return generated::RefinementActionResult {
                    schema_version: crate::refinement::CONTRACT_VERSION,
                    candidate_id: row.id,
                    revision: row.revision as u64,
                    action: request.action,
                    applied: true,
                    deduplicated: true,
                    version: row.version as u64,
                    status: row.status,
                    error_code: String::new(),
                };
            }
            Err(
                evohime_local_storage::refinement_store::RefinementStoreError::IdempotencyConflict,
            ) => return refinement_action_error(&request, "idempotency_conflict"),
            Ok(None) => {}
            Err(_) => return refinement_action_error(&request, "storage_failed"),
        }
        if request.action == "activate" && current.kind != "memory" {
            return refinement_action_error(&request, "unavailable");
        }
        if request.action == "activate"
            && current.owner_scope == "global"
            && request.approval_token.is_empty()
        {
            return refinement_action_error(&request, "approval_required");
        }
        let status = match request.action.as_str() {
            "approve" => "approved",
            "reject" => "rejected",
            "activate" => "active",
            "rollback" => "rolled_back",
            _ => return refinement_action_error(&request, "invalid_action"),
        };
        match store.transition_with_idempotency(
            &request.candidate_id,
            request.revision as i64,
            request.expected_version as i64,
            status,
            None,
            crate::task_memory::now_millis() as i64,
            Some((&request.idempotency_key, &request_hash)),
        ) {
            Ok(row) => generated::RefinementActionResult {
                schema_version: crate::refinement::CONTRACT_VERSION,
                candidate_id: row.id,
                revision: row.revision as u64,
                action: request.action,
                applied: true,
                deduplicated: false,
                version: row.version as u64,
                status: row.status,
                error_code: String::new(),
            },
            Err(
                evohime_local_storage::refinement_store::RefinementStoreError::VersionConflict {
                    ..
                },
            ) => refinement_action_error(&request, "stale_version"),
            Err(_) => refinement_action_error(&request, "storage_failed"),
        }
    }

    async fn dispatch_create_analysis_kernel(
        &self,
        request: generated::CreateAnalysisKernel,
    ) -> generated::AnalysisKernelProjection {
        let limits = if request.limits_json.is_empty() {
            crate::analysis_kernel::KernelLimitsV1::default()
        } else {
            match serde_json::from_slice(&request.limits_json) {
                Ok(limits) => limits,
                Err(_) => return analysis_kernel_projection_error("invalid_limits"),
            }
        };
        let now = crate::task_memory::now_millis() as i64;
        let session = crate::analysis_kernel::AnalysisKernelSessionV1 {
            schema_version: crate::analysis_kernel::ANALYSIS_KERNEL_SCHEMA_VERSION,
            id: format!("kernel-{}", uuid::Uuid::new_v4()),
            task_id: request.task_id,
            workspace_id: request.workspace_id,
            runtime_version: request.runtime_version,
            package_manifest_hash: request.package_manifest_hash,
            policy_hash: request.policy_hash,
            status: crate::analysis_kernel::KernelStatus::Created,
            revision: 0,
            limits,
            created_at_ms: now,
            updated_at_ms: now,
        };
        if session.validate().is_err() {
            return analysis_kernel_projection_error("invalid_argument");
        }
        #[cfg(windows)]
        if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some() {
            let launch = crate::analysis_kernel::supervisor_command(serde_json::json!({
                "op": "kernel_launch",
                "kernel_id": session.id,
                "package_manifest_hash": session.package_manifest_hash,
            }))
            .await;
            if !matches!(launch, Ok(value) if value.get("accepted") == Some(&serde_json::Value::Bool(true)))
            {
                return analysis_kernel_projection_error("worker_unavailable");
            }
        }
        let database = self.journal.database().lock().await;
        let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
        if store.create_session(&session).is_err() {
            return analysis_kernel_projection_error("storage_failed");
        }
        if store
            .set_status(
                &session.id,
                session.revision,
                crate::analysis_kernel::KernelStatus::Running,
                now,
            )
            .is_err()
        {
            return analysis_kernel_projection_error("runtime_unavailable");
        }
        let mut session = session;
        session.status = crate::analysis_kernel::KernelStatus::Running;
        session.revision = session.revision.saturating_add(1);
        let mut runtime = match crate::analysis_kernel::KernelRuntime::new(session.clone()) {
            Ok(runtime) => runtime,
            Err(_) => return analysis_kernel_projection_error("invalid_argument"),
        };
        if runtime.start(std::time::Instant::now()).is_err() {
            return analysis_kernel_projection_error("runtime_unavailable");
        }
        self.analysis_kernels
            .lock()
            .await
            .insert(session.id.clone(), runtime);
        analysis_kernel_projection(&session, 0, "")
    }

    async fn dispatch_get_analysis_kernel(
        &self,
        request: generated::GetAnalysisKernel,
    ) -> generated::AnalysisKernelProjection {
        let database = self.journal.database().lock().await;
        let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
        let Ok(Some(session)) = store.get_session(&request.kernel_id) else {
            return analysis_kernel_projection_error("not_found");
        };
        let objects = store.list_objects(&session.id).unwrap_or_default();
        analysis_kernel_projection(&session, objects.len(), "")
    }

    async fn dispatch_execute_analysis_kernel(
        &self,
        request: generated::ExecuteAnalysisKernel,
    ) -> generated::AnalysisKernelResult {
        let operation_name = request.operation.clone();
        if !request.idempotency_key.is_empty() {
            let database = self.journal.database().lock().await;
            let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
            if store
                .get_idempotency(
                    &request.kernel_id,
                    &request.idempotency_key,
                    &operation_name,
                )
                .ok()
                .flatten()
                .is_some()
            {
                return analysis_kernel_result_error(&request.request_id, "duplicate_request");
            }
        }
        let operation = match serde_json::from_str(&format!("\"{}\"", request.operation)) {
            Ok(crate::analysis_kernel::KernelOperation::JsonParse) => {
                crate::analysis_kernel::KernelOperation::JsonParse
            }
            Ok(crate::analysis_kernel::KernelOperation::JsonSelect) => {
                crate::analysis_kernel::KernelOperation::JsonSelect
            }
            Ok(crate::analysis_kernel::KernelOperation::CsvSummary) => {
                crate::analysis_kernel::KernelOperation::CsvSummary
            }
            Ok(crate::analysis_kernel::KernelOperation::ObjectPut) => {
                crate::analysis_kernel::KernelOperation::ObjectPut
            }
            Ok(crate::analysis_kernel::KernelOperation::ArtifactRead) => {
                crate::analysis_kernel::KernelOperation::ArtifactRead
            }
            Ok(crate::analysis_kernel::KernelOperation::ToolRequest) => {
                crate::analysis_kernel::KernelOperation::ToolRequest
            }
            _ => return analysis_kernel_result_error(&request.request_id, "unsupported_operation"),
        };
        let host_request = crate::analysis_kernel::KernelHostRequestV1 {
            version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
            request_id: request.request_id,
            kernel_id: request.kernel_id.clone(),
            session_id: request.kernel_id.clone(),
            operation,
            args: request.args,
            requested_capability: (!request.requested_capability.is_empty())
                .then_some(request.requested_capability),
            context_refs: request.context_refs,
            correlation_id: request.correlation_id,
            idempotency_key: request.idempotency_key.clone(),
        };
        let mut kernels = self.analysis_kernels.lock().await;
        let Some(runtime) = kernels.get_mut(&request.kernel_id) else {
            return analysis_kernel_result_error(&host_request.request_id, "kernel_not_running");
        };
        let request_id = host_request.request_id.clone();
        #[cfg(windows)]
        if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some()
            && !matches!(
                &host_request.operation,
                crate::analysis_kernel::KernelOperation::ObjectPut
            )
        {
            let worker_args = match host_request.operation {
                crate::analysis_kernel::KernelOperation::CsvSummary => {
                    serde_json::Value::String(String::from_utf8_lossy(&host_request.args).into())
                }
                _ => serde_json::from_slice(&host_request.args).unwrap_or_else(|_| {
                    serde_json::Value::String(String::from_utf8_lossy(&host_request.args).into())
                }),
            };
            if let Err(error) = runtime.admit(&host_request, std::time::Instant::now()) {
                return analysis_kernel_result_error(&request_id, kernel_error_code(&error));
            }
            drop(kernels);
            let worker_response = crate::analysis_kernel::supervisor_command(serde_json::json!({
                "op": "kernel_execute",
                "kernel_id": request.kernel_id,
                "request": {
                    "request_id": host_request.request_id,
                    "operation": operation_name,
                    "args": worker_args,
                },
            }))
            .await;
            let response = match worker_response {
                Ok(value) if value.get("accepted") == Some(&serde_json::Value::Bool(true)) => value
                    .get("response")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
                Ok(value) => {
                    let mut kernels = self.analysis_kernels.lock().await;
                    if let Some(runtime) = kernels.get_mut(&request.kernel_id) {
                        runtime.mark_crashed();
                    }
                    return analysis_kernel_result_error(
                        &request_id,
                        value
                            .get("reason")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("worker_unavailable"),
                    );
                }
                Err(_) => {
                    let mut kernels = self.analysis_kernels.lock().await;
                    if let Some(runtime) = kernels.get_mut(&request.kernel_id) {
                        runtime.mark_crashed();
                    }
                    return analysis_kernel_result_error(&request_id, "worker_unavailable");
                }
            };
            let status = response
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("error");
            if status != "ok" {
                return analysis_kernel_result_error(
                    &request_id,
                    response
                        .get("error_class")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("worker_error"),
                );
            }
            let inline_result =
                serde_json::to_vec(response.get("result").unwrap_or(&serde_json::Value::Null))
                    .unwrap_or_default();
            let mut kernels = self.analysis_kernels.lock().await;
            if let Some(runtime) = kernels.get_mut(&request.kernel_id) {
                if let Err(error) = runtime.accept_output(inline_result.len()) {
                    return analysis_kernel_result_error(&request_id, kernel_error_code(&error));
                }
            }
            return generated::AnalysisKernelResult {
                schema_version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
                request_id,
                status: "ok".into(),
                inline_result,
                object_ref: None,
                sensitivity: crate::analysis_kernel::KernelSensitivity::Internal
                    .as_str()
                    .into(),
                provenance: "core:analysis-kernel-worker".into(),
                error_class: String::new(),
            };
        }
        match runtime.execute(host_request, std::time::Instant::now()) {
            Ok(response) => {
                let result = generated::AnalysisKernelResult {
                    schema_version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
                    request_id: response.request_id,
                    status: "ok".into(),
                    inline_result: response.inline_result.unwrap_or_default(),
                    object_ref: response.object_ref.as_ref().map(analysis_kernel_object_ref),
                    sensitivity: response.sensitivity.as_str().into(),
                    provenance: response.provenance,
                    error_class: String::new(),
                };
                let database = self.journal.database().lock().await;
                let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
                if let Some(object) = response.object_ref.as_ref() {
                    let _ = store.put_object(object);
                }
                let _ = store.put_idempotency(
                    &request.kernel_id,
                    &request.idempotency_key,
                    &operation_name,
                    b"{\"status\":\"ok\"}",
                    crate::task_memory::now_millis() as i64,
                );
                result
            }
            Err(error) => analysis_kernel_result_error(&request_id, kernel_error_code(&error)),
        }
    }

    async fn dispatch_reset_analysis_kernel(
        &self,
        request: generated::ResetAnalysisKernel,
    ) -> generated::AnalysisKernelResult {
        if !request.idempotency_key.is_empty() {
            let database = self.journal.database().lock().await;
            let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
            if store
                .get_idempotency(&request.kernel_id, &request.idempotency_key, "reset")
                .ok()
                .flatten()
                .is_some()
            {
                return analysis_kernel_result_error("", "duplicate_request");
            }
        }
        if !self
            .analysis_kernels
            .lock()
            .await
            .contains_key(&request.kernel_id)
        {
            return analysis_kernel_result_error("", "not_found");
        }
        #[cfg(windows)]
        if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some() {
            let stopped = crate::analysis_kernel::supervisor_command(serde_json::json!({
                "op": "kernel_stop",
                "kernel_id": request.kernel_id,
            }))
            .await;
            if !matches!(stopped, Ok(value) if value.get("accepted") == Some(&serde_json::Value::Bool(true)))
            {
                return analysis_kernel_result_error("", "worker_unavailable");
            }
        }
        let status_result = {
            let database = self.journal.database().lock().await;
            let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
            store.set_status(
                &request.kernel_id,
                request.expected_revision,
                crate::analysis_kernel::KernelStatus::Reset,
                crate::task_memory::now_millis() as i64,
            )
        };
        let result = match status_result {
            Ok(_) => {
                self.analysis_kernels
                    .lock()
                    .await
                    .get_mut(&request.kernel_id)
                    .expect("kernel existence checked before reset")
                    .reset();
                generated::AnalysisKernelResult {
                    schema_version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
                    request_id: String::new(),
                    status: "reset".into(),
                    inline_result: Vec::new(),
                    object_ref: None,
                    sensitivity: "internal".into(),
                    provenance: "core:analysis-kernel".into(),
                    error_class: String::new(),
                }
            }
            Err(error) => analysis_kernel_result_error("", kernel_storage_error_code(&error)),
        };
        if result.status == "reset" && !request.idempotency_key.is_empty() {
            let database = self.journal.database().lock().await;
            let store = crate::analysis_kernel::AnalysisKernelStore::new(database.connection());
            let _ = store.put_idempotency(
                &request.kernel_id,
                &request.idempotency_key,
                "reset",
                b"{\"status\":\"reset\"}",
                crate::task_memory::now_millis() as i64,
            );
        }
        result
    }

    async fn dispatch_get_task_checkpoint(
        &self,
        request: generated::GetTaskCheckpoint,
    ) -> generated::TaskCheckpointProjection {
        let task_id = request.task_id;
        let max_replay_events = if request.max_replay_events == 0 {
            TASK_CHECKPOINT_IPC_MAX_REPLAY_EVENTS
        } else {
            request.max_replay_events as usize
        };
        if !valid_checkpoint_token(&task_id, 128)
            || !valid_checkpoint_workspace(&request.workspace_path)
            || max_replay_events > TASK_CHECKPOINT_IPC_MAX_REPLAY_EVENTS
        {
            return task_checkpoint_projection_error(&task_id, "invalid_argument");
        }
        let runtime = crate::task_checkpoint::TaskCheckpointRuntime::new(self.journal.clone());
        match runtime
            .recover(&task_id, std::path::Path::new(&request.workspace_path))
            .await
        {
            Ok(recovery) => task_checkpoint_projection(&task_id, recovery, max_replay_events),
            Err(error) => task_checkpoint_projection_error(&task_id, checkpoint_error_code(&error)),
        }
    }

    async fn dispatch_resolve_task_checkpoint(
        &self,
        request: generated::ResolveTaskCheckpoint,
    ) -> Result<generated::TaskCheckpointActionResult, IpcBridgeError> {
        let task_id = request.task_id;
        let checkpoint_id = request.checkpoint_id;
        let action = request.action;
        let idempotency_key = request.idempotency_key;
        let invalid = !valid_checkpoint_token(&task_id, 128)
            || !valid_checkpoint_workspace(&request.workspace_path)
            || !valid_checkpoint_token(&checkpoint_id, 128)
            || request.expected_source_event_seq < 0
            || !matches!(action.as_str(), "acknowledge_recovery" | "request_resume")
            || !valid_checkpoint_token(&idempotency_key, 128);
        if invalid {
            return Ok(task_checkpoint_action_result(
                task_id,
                checkpoint_id,
                action,
                false,
                false,
                "invalid_argument",
                "Запрос действия checkpoint отклонён.",
            ));
        }

        let runtime = crate::task_checkpoint::TaskCheckpointRuntime::new(self.journal.clone());
        let recovery = match runtime
            .recover(&task_id, std::path::Path::new(&request.workspace_path))
            .await
        {
            Ok(recovery) => recovery,
            Err(error) => {
                return Ok(task_checkpoint_action_result(
                    task_id,
                    checkpoint_id,
                    action,
                    false,
                    false,
                    checkpoint_error_code(&error),
                    "Состояние checkpoint недоступно.",
                ));
            }
        };
        let (applied, error_code, error_message) = match recovery.checkpoint.as_ref() {
            None => (
                false,
                "checkpoint_not_found",
                "Checkpoint для задачи не найден.",
            ),
            Some(checkpoint)
                if checkpoint.id != checkpoint_id
                    || checkpoint.source_event_seq != request.expected_source_event_seq =>
            {
                (
                    false,
                    "stale_action",
                    "Состояние checkpoint уже изменилось; обнови проекцию.",
                )
            }
            Some(_) if action == "request_resume" => {
                if recovery.disposition == crate::task_checkpoint::RecoveryDisposition::Replayable {
                    (
                        true,
                        "",
                        "Запрос reconciliation записан; внешний effect автоматически не повторяется.",
                    )
                } else {
                    (
                        false,
                        "recovery_blocked",
                        "Продолжение заблокировано до явной reconciliation.",
                    )
                }
            }
            Some(_) => (true, "", "Состояние checkpoint подтверждено пользователем."),
        };
        let request_id = format!("{task_id}:{idempotency_key}");
        let command_hash = crate::research::sha256_hex(
            format!(
                "{task_id}|{checkpoint_id}|{}|{}",
                request.expected_source_event_seq, action
            )
            .as_bytes(),
        );
        let record = TaskCheckpointActionRecord {
            task_id: task_id.clone(),
            checkpoint_id: checkpoint_id.clone(),
            action: action.clone(),
            applied,
            deduplicated: false,
            error_code: error_code.into(),
            error_message: error_message.into(),
        };
        let result_payload = serde_json::to_vec(&record)?;
        let event_payload = serde_json::to_vec(&serde_json::json!({
            "checkpoint_id": checkpoint_id,
            "action": action,
            "expected_source_event_seq": request.expected_source_event_seq,
            "applied": applied,
            "error_code": error_code,
        }))?;
        let stored = match self
            .journal
            .record_task_checkpoint_action(
                &task_id,
                &request_id,
                &command_hash,
                &event_payload,
                &result_payload,
            )
            .await
        {
            Ok(stored) => stored,
            Err(StorageError::DeduplicationConflict { .. }) => {
                return Ok(task_checkpoint_action_result(
                    task_id,
                    checkpoint_id,
                    action,
                    false,
                    false,
                    "idempotency_conflict",
                    "Ключ idempotency уже использован для другого действия.",
                ));
            }
            Err(_) => {
                return Ok(task_checkpoint_action_result(
                    task_id,
                    checkpoint_id,
                    action,
                    false,
                    false,
                    "storage_failed",
                    "Действие checkpoint не удалось записать.",
                ));
            }
        };
        let deduplicated = stored.is_some();
        let mut record = match stored {
            Some(stored) => match serde_json::from_slice::<TaskCheckpointActionRecord>(&stored) {
                Ok(record) => record,
                Err(_) => {
                    return Ok(task_checkpoint_action_result(
                        task_id,
                        checkpoint_id,
                        action,
                        false,
                        true,
                        "storage_failed",
                        "Сохранённый результат действия checkpoint повреждён.",
                    ));
                }
            },
            None => record,
        };
        record.deduplicated = deduplicated;
        Ok(task_checkpoint_action_result_from_record(record))
    }

    async fn dispatch_create_goal(
        &self,
        request: generated::CreateGoal,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        let invalid = !valid_goal_token(&request.goal_id)
            || !valid_checkpoint_workspace(&request.workspace_path)
            || (!request.chat_id.is_empty() && !valid_goal_token(&request.chat_id))
            || request.objective.trim().is_empty()
            || request.success_criteria.len() > crate::goal::GOAL_MAX_CRITERIA
            || !valid_goal_token(&request.idempotency_key);
        if invalid {
            return goal_action_error(
                "",
                "create",
                "invalid_argument",
                "Параметры цели отклонены.",
            );
        }
        let criteria = match goal_criteria_from_request(&request.success_criteria) {
            Ok(criteria) if !criteria.is_empty() => criteria,
            _ => {
                return goal_action_error(
                    &request.goal_id,
                    "create",
                    "invalid_argument",
                    "Цель должна содержать хотя бы один критерий.",
                )
            }
        };
        let now = crate::goal::now_ms();
        let goal = crate::goal::GoalV1 {
            id: request.goal_id.clone(),
            version: 1,
            workspace_id: crate::goal::workspace_id_from_path(&request.workspace_path),
            chat_id: (!request.chat_id.is_empty()).then_some(request.chat_id),
            objective: request.objective,
            success_criteria: criteria,
            status: crate::goal::GoalStatus::Active,
            progress_summary: "Цель создана; доказательства ещё не подтверждены.".into(),
            completed_criteria: Vec::new(),
            remaining_criteria: Vec::new(),
            blockers: Vec::new(),
            next_action: Some("Выполнить критерии и подтвердить Core evidence.".into()),
            workflow_run_ids: Vec::new(),
            child_run_ids: Vec::new(),
            checkpoint_id: None,
            token_budget: (request.token_budget > 0).then_some(request.token_budget),
            cost_budget_micros: (request.cost_budget_micros > 0)
                .then_some(request.cost_budget_micros),
            continuation_budget: (request.continuation_budget > 0)
                .then_some(request.continuation_budget),
            created_at_ms: now,
            updated_at_ms: now,
            created_by: "shell".into(),
            updated_by: "shell".into(),
            content_hash: String::new(),
        };
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        match runtime
            .create(
                &goal,
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                "create",
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    async fn dispatch_get_goal(&self, request: generated::GetGoal) -> generated::GoalProjection {
        if !valid_goal_token(&request.goal_id) {
            return goal_projection_error("", "invalid_argument");
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        match runtime.get(&request.goal_id).await {
            Ok(Some(goal)) => goal_projection(&goal, ""),
            Ok(None) => goal_projection_error(&request.goal_id, "not_found"),
            Err(error) => goal_projection_error(&request.goal_id, goal_storage_error_code(&error)),
        }
    }

    async fn dispatch_list_goals(
        &self,
        request: generated::ListGoals,
    ) -> generated::GoalListProjection {
        let limit = if request.limit == 0 {
            crate::goal::GOAL_MAX_READ_LIMIT
        } else {
            request.limit as usize
        };
        if !valid_checkpoint_workspace(&request.workspace_path)
            || limit > crate::goal::GOAL_MAX_READ_LIMIT
        {
            return generated::GoalListProjection {
                schema_version: crate::goal::GOAL_SCHEMA_VERSION,
                error_code: "invalid_argument".into(),
                ..Default::default()
            };
        }
        let workspace_id = crate::goal::workspace_id_from_path(&request.workspace_path);
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        match (
            runtime.list(&workspace_id, limit).await,
            runtime.recovery(&workspace_id).await,
        ) {
            (Ok(goals), Ok(recovery)) => {
                let warnings = recovery
                    .into_iter()
                    .map(|item| (item.goal_id, item.warning))
                    .collect::<std::collections::HashMap<_, _>>();
                let mut projected_goals = Vec::new();
                let mut projected_bytes = 0usize;
                let mut truncated = false;
                for goal in &goals {
                    let projection = goal_projection(
                        goal,
                        warnings.get(&goal.id).map(String::as_str).unwrap_or(""),
                    );
                    let next_bytes = projected_bytes.saturating_add(projection.encoded_len());
                    if next_bytes > GOAL_LIST_MAX_PROJECTION_BYTES {
                        truncated = true;
                        break;
                    }
                    projected_bytes = next_bytes;
                    projected_goals.push(projection);
                }
                generated::GoalListProjection {
                    schema_version: crate::goal::GOAL_SCHEMA_VERSION,
                    goals: projected_goals,
                    error_code: if truncated {
                        "projection_truncated".into()
                    } else {
                        String::new()
                    },
                    truncated,
                }
            }
            (Err(error), _) | (_, Err(error)) => generated::GoalListProjection {
                schema_version: crate::goal::GOAL_SCHEMA_VERSION,
                error_code: goal_storage_error_code(&error).into(),
                ..Default::default()
            },
        }
    }

    async fn dispatch_goal_transition(
        &self,
        request: generated::GoalAction,
        status: crate::goal::GoalStatus,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        let action = match status {
            crate::goal::GoalStatus::Paused => "pause",
            crate::goal::GoalStatus::Active => "resume",
            crate::goal::GoalStatus::Cancelled => "cancel",
            _ => "transition",
        };
        if !valid_goal_action(
            &request.goal_id,
            request.expected_version,
            &request.idempotency_key,
        ) {
            return goal_action_error(
                &request.goal_id,
                action,
                "invalid_argument",
                "Действие цели отклонено.",
            );
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        match runtime
            .transition(
                &request.goal_id,
                request.expected_version,
                status,
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                action,
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    async fn dispatch_update_goal(
        &self,
        request: generated::UpdateGoal,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        if !valid_goal_action(
            &request.goal_id,
            request.expected_version,
            &request.idempotency_key,
        ) {
            return goal_action_error(
                &request.goal_id,
                "update",
                "invalid_argument",
                "Обновление цели отклонено.",
            );
        }
        let criteria = if request.success_criteria.is_empty() {
            None
        } else {
            match goal_criteria_from_request(&request.success_criteria) {
                Ok(criteria) => Some(criteria),
                Err(_) => {
                    return goal_action_error(
                        &request.goal_id,
                        "update",
                        "invalid_argument",
                        "Критерии цели отклонены.",
                    )
                }
            }
        };
        let objective = (!request.objective.trim().is_empty()).then_some(request.objective);
        if objective.is_none() && criteria.is_none() {
            return goal_action_error(
                &request.goal_id,
                "update",
                "invalid_argument",
                "Нет изменений для цели.",
            );
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        match runtime
            .update(
                &request.goal_id,
                request.expected_version,
                objective,
                criteria,
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                "update",
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    async fn dispatch_verify_goal_criterion(
        &self,
        request: generated::VerifyGoalCriterion,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        if !valid_goal_action(
            &request.goal_id,
            request.expected_version,
            &request.idempotency_key,
        ) || !valid_goal_token(&request.criterion_id)
        {
            return goal_action_error(
                &request.goal_id,
                "verify_criterion",
                "invalid_argument",
                "Evidence критерия отклонена.",
            );
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        let goal = match runtime.get(&request.goal_id).await {
            Ok(Some(goal)) => goal,
            Ok(None) => {
                return goal_action_error(
                    &request.goal_id,
                    "verify_criterion",
                    "not_found",
                    "Цель не найдена.",
                )
            }
            Err(error) => {
                return goal_action_error(
                    &request.goal_id,
                    "verify_criterion",
                    goal_storage_error_code(&error),
                    &goal_storage_error_message(&error),
                )
            }
        };
        let is_manual = goal
            .success_criteria
            .iter()
            .find(|criterion| criterion.id == request.criterion_id)
            .is_some_and(|criterion| criterion.kind == crate::goal::GoalCriterionKind::Manual);
        if !is_manual {
            return goal_action_error(
                &request.goal_id,
                "verify_criterion",
                "authority_denied",
                "Этот критерий подтверждается только Core runtime.",
            );
        }
        let evidence_digest = crate::research::sha256_hex(
            format!(
                "{}:{}:{}",
                request.goal_id, request.criterion_id, goal_command_hash
            )
            .as_bytes(),
        );
        let evidence_ref = format!("core:user-decision:{evidence_digest}");
        match runtime
            .verify_criterion(
                &request.goal_id,
                request.expected_version,
                crate::goal::GoalCriterionEvidence::new(
                    &request.criterion_id,
                    &evidence_ref,
                    "core.user-decision",
                    "goal-v1",
                ),
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                "verify_criterion",
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    async fn dispatch_link_goal_reference(
        &self,
        request: generated::LinkGoalReference,
        command_hash: &str,
    ) -> generated::GoalActionResult {
        if !valid_goal_action(
            &request.goal_id,
            request.expected_version,
            &request.idempotency_key,
        ) || !valid_goal_token(&request.kind)
            || !valid_goal_token(&request.reference_id)
        {
            return goal_action_error(
                &request.goal_id,
                "link_reference",
                "invalid_argument",
                "Ссылка цели отклонена.",
            );
        }
        let runtime = crate::goal::GoalRuntime::new(self.journal.clone());
        let goal_command_hash = crate::research::sha256_hex(command_hash.as_bytes());
        match runtime
            .link_reference(
                &request.goal_id,
                request.expected_version,
                &request.kind,
                &request.reference_id,
                crate::goal::GoalCommand::new(
                    "shell",
                    &request.idempotency_key,
                    &goal_command_hash,
                ),
            )
            .await
        {
            Ok(result) => {
                self.notify_goal_event(result.event_sequence);
                goal_action_result_from_mutation(result)
            }
            Err(error) => goal_action_error(
                &request.goal_id,
                "link_reference",
                goal_storage_error_code(&error),
                &goal_storage_error_message(&error),
            ),
        }
    }

    fn notify_goal_event(&self, sequence: i64) {
        if let Some(coordinator) = &self.coordinator {
            coordinator.notify_journalled(sequence.max(0) as u64);
        }
    }

    async fn dispatch_list_skills<W: AsyncWrite + Unpin>(
        &self,
        request: generated::ListSkills,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let workspace = match validate_skill_workspace(&request.workspace_path) {
            Ok(workspace) => workspace,
            Err(error) => {
                return self
                    .write_skill_catalog(
                        writer,
                        generated::SkillCatalogProjection {
                            schema_version: crate::skill_registry::SKILL_SCHEMA_VERSION,
                            diagnostics: vec![generated::SkillDiagnosticProjection {
                                code: error.code().into(),
                                message: "Каталог skills недоступен.".into(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        },
                    )
                    .await;
            }
        };
        let mut registry = crate::skill_registry::SkillRegistry::for_workspace(&workspace);
        let catalog = registry.catalog();
        let limit = if request.limit == 0 {
            crate::skill_registry::MAX_SKILLS
        } else {
            (request.limit as usize).min(crate::skill_registry::MAX_SKILLS)
        };
        let projection = generated::SkillCatalogProjection {
            schema_version: catalog.schema_version,
            skills: catalog
                .skills
                .into_iter()
                .take(limit)
                .map(skill_metadata_projection)
                .collect(),
            diagnostics: catalog
                .diagnostics
                .into_iter()
                .take(32)
                .map(skill_diagnostic_projection)
                .collect(),
        };
        self.write_skill_catalog(writer, projection).await
    }

    async fn dispatch_load_skill<W: AsyncWrite + Unpin>(
        &self,
        request: generated::LoadSkill,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let max_bytes = if request.max_bytes == 0 {
            crate::skill_registry::MAX_SKILL_BYTES
        } else {
            (request.max_bytes as usize).min(crate::skill_registry::MAX_SKILL_BYTES)
        };
        let result = match validate_skill_workspace(&request.workspace_path) {
            Ok(workspace) => {
                let mut registry = crate::skill_registry::SkillRegistry::for_workspace(&workspace);
                match registry.load(&request.skill_id) {
                    Ok(skill) if skill.content.len() <= max_bytes => {
                        generated::SkillContentResult {
                            schema_version: skill.metadata.schema_version,
                            skill_id: skill.metadata.skill_id,
                            version: skill.metadata.version,
                            content: skill.content,
                            content_hash: skill.metadata.content_hash,
                            source_ref: skill.metadata.source_ref,
                            cache_hit: skill.cache_hit,
                            ..Default::default()
                        }
                    }
                    Ok(_) => skill_content_error(&request.skill_id, "too_large"),
                    Err(error) => skill_content_error(&request.skill_id, error.code()),
                }
            }
            Err(error) => skill_content_error(&request.skill_id, error.code()),
        };
        if result.error_code.is_empty() {
            self.append_skill_trace(
                &result.skill_id,
                "skill.loaded",
                serde_json::json!({
                    "skill_id": result.skill_id,
                    "version": result.version,
                    "content_hash": result.content_hash,
                    "source_ref": result.source_ref,
                }),
            )
            .await;
        }
        self.write_skill_content(writer, result).await
    }

    async fn dispatch_load_skill_reference<W: AsyncWrite + Unpin>(
        &self,
        request: generated::LoadSkillReference,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let max_bytes = if request.max_bytes == 0 {
            crate::skill_registry::MAX_REFERENCE_BYTES
        } else {
            (request.max_bytes as usize).min(crate::skill_registry::MAX_REFERENCE_BYTES)
        };
        let result = match validate_skill_workspace(&request.workspace_path) {
            Ok(workspace) => {
                let mut registry = crate::skill_registry::SkillRegistry::for_workspace(&workspace);
                match registry.load_reference(&request.skill_id, &request.reference) {
                    Ok(reference) if reference.content.len() <= max_bytes => {
                        generated::SkillReferenceResult {
                            schema_version: crate::skill_registry::SKILL_SCHEMA_VERSION,
                            skill_id: request.skill_id.clone(),
                            reference: reference.name,
                            content: reference.content,
                            content_hash: reference.content_hash,
                            source_ref: reference.provenance.source_ref,
                            ..Default::default()
                        }
                    }
                    Ok(_) => {
                        skill_reference_error(&request.skill_id, &request.reference, "too_large")
                    }
                    Err(error) => {
                        skill_reference_error(&request.skill_id, &request.reference, error.code())
                    }
                }
            }
            Err(error) => {
                skill_reference_error(&request.skill_id, &request.reference, error.code())
            }
        };
        if result.error_code.is_empty() {
            self.append_skill_trace(
                &result.skill_id,
                "skill.reference.loaded",
                serde_json::json!({
                    "skill_id": result.skill_id,
                    "reference": result.reference,
                    "content_hash": result.content_hash,
                    "source_ref": result.source_ref,
                }),
            )
            .await;
        }
        self.write_skill_reference(writer, result).await
    }

    async fn append_skill_trace(
        &self,
        skill_id: &str,
        event_type: &str,
        payload: serde_json::Value,
    ) {
        let database = self.journal.database();
        let database = database.lock().await;
        if let Err(error) = database.append_event(
            &format!("skill:{skill_id}"),
            event_type,
            &serde_json::to_vec(&payload).unwrap_or_default(),
        ) {
            tracing::warn!(target = "skill.registry", %error, "skill trace could not be persisted");
        }
    }

    async fn write_skill_catalog<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        projection: generated::SkillCatalogProjection,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: "skill.catalog".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::SkillCatalog(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_skill_content<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::SkillContentResult,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.skill_id.clone(),
            event_type: "skill.loaded".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::SkillContent(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_skill_reference<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::SkillReferenceResult,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.skill_id.clone(),
            event_type: "skill.reference.loaded".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::SkillReference(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_task_checkpoint_projection<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        projection: generated::TaskCheckpointProjection,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: projection.task_id.clone(),
            event_type: "task.checkpoint".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::TaskCheckpoint(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_task_checkpoint_action_result<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::TaskCheckpointActionResult,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.task_id.clone(),
            event_type: "task.checkpoint.action".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::TaskCheckpointActionResult(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_goal_projection<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        projection: generated::GoalProjection,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: projection.goal_id.clone(),
            event_type: "goal.projection".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::Goal(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_goal_list_projection<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        projection: generated::GoalListProjection,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: "goal.list".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::GoalList(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_goal_action_result<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        result: generated::GoalActionResult,
    ) -> Result<(), IpcBridgeError> {
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.goal_id.clone(),
            event_type: "goal.action".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::GoalAction(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_continuation_projection<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value =
            serde_json::from_slice(&payload).map_err(|error| FrameError::Io(error.to_string()))?;
        let number = |name: &str| {
            value
                .get(name)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        let projection = generated::ContinuationProjection {
            schema_version: 1,
            run_id: value
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            owner_scope: value
                .get("owner_scope")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            policy_id: value
                .get("policy_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            policy_revision: number("policy_revision"),
            policy_hash: value
                .get("policy_hash")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            state: value
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            continuation_index: number("continuation_index"),
            max_continuations: number("max_continuations"),
            model_turns: number("used_model_turns"),
            max_model_turns: number("max_model_turns"),
            token_used: number("token_used"),
            cost_used_micros: number("cost_used_micros"),
            stop_reason: value
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            error_code: value
                .get("error_code")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            gates: value
                .get("gates")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .take(32)
                        .map(|item| generated::ContinuationGateProjection {
                            gate_id: item
                                .get("gate_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .into(),
                            kind: String::new(),
                            capability_ref: String::new(),
                            status: item
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .into(),
                            evidence_ref: item
                                .get("evidence_ref")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .into(),
                            error_code: item
                                .get("error_code")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .into(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: projection.run_id.clone(),
            event_type: "continuation.run".into(),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::Continuation(projection)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    async fn write_continuation_action<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value =
            serde_json::from_slice(&payload).map_err(|error| FrameError::Io(error.to_string()))?;
        let result = generated::ContinuationActionResult {
            schema_version: 1,
            run_id: value
                .get("run_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            action: value
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            applied: value
                .get("applied")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            deduplicated: value
                .get("deduplicated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            error_code: value
                .get("error_code")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
        };
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: result.run_id.clone(),
            event_type: "continuation.action".into(),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ContinuationAction(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }

    fn dispatch_integration_provider_sdk(
        &self,
        request: generated::IntegrationProviderSdkCommand,
    ) -> serde_json::Value {
        let operation = request.operation.as_str();
        if operation == "list_catalog" || operation == "get_provider" {
            return serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "status": "ok",
                "operation": operation,
                "providers": [crate::integration_provider_sdk::fixture_echo_manifest()],
                "error_code": "",
            });
        }
        if operation == "invoke_fixture" {
            let input = serde_json::from_slice(&request.payload).unwrap_or(serde_json::Value::Null);
            let result =
                crate::integration_provider_runtime::invoke_fixture("fixture.echo", "echo", input);
            return serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "status": "ok",
                "operation": operation,
                "result": result,
                "error_code": "",
            });
        }
        serde_json::json!({
            "schema_version": 1,
            "request_id": request.request_id,
            "status": "unavailable",
            "operation": operation,
            "error_code": "provider_adapter_unavailable",
        })
    }

    fn dispatch_event_trigger_runtime(
        &self,
        request: generated::EventTriggerRuntimeCommand,
    ) -> serde_json::Value {
        let operation = request.operation.as_str();
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":operation,"status":"rejected","error_code":"invalid_request"});
        }
        match operation {
            "list" | "get" => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "ok", "triggers": [], "mvp_sources": ["local_workspace_event", "system_event"],
                "provider_webhook": "unavailable", "error_code": ""
            }),
            "reconcile" | "pause" | "resume" => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "unavailable", "error_code": "no_trigger_configured"
            }),
            _ => serde_json::json!({
                "schema_version": 1, "request_id": request.request_id, "operation": operation,
                "status": "unavailable", "error_code": "unsupported_operation"
            }),
        }
    }

    async fn dispatch_invocation_preset(
        &self,
        request: generated::InvocationPresetCommand,
    ) -> serde_json::Value {
        if request.schema_version != 1
            || request.request_id.is_empty()
            || request.owner_scope.is_empty()
        {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"rejected","error_code":"invalid_request"});
        }
        let database = self.journal.database().lock().await;
        let connection = database.connection();
        match request.operation.as_str() {
            "list" => {
                let mut statement = match connection.prepare("SELECT id, revision, content_hash, state FROM invocation_presets WHERE owner_scope=?1 ORDER BY id, revision DESC LIMIT ?2") { Ok(statement) => statement, Err(_) => return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","error_code":"storage_error"}) };
                let limit = if request.expected_revision == 0 {
                    50
                } else {
                    request.expected_revision.min(100)
                };
                let rows = statement.query_map(rusqlite::params![request.owner_scope, limit as i64], |row| Ok(serde_json::json!({"id":row.get::<_,String>(0)?,"revision":row.get::<_,i64>(1)? as u64,"content_hash":row.get::<_,String>(2)?,"state":row.get::<_,String>(3)?}))).and_then(|rows| rows.collect::<Result<Vec<_>, _>>());
                return match rows {
                    Ok(presets) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"ok","presets":presets,"error_code":""})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"list","status":"error","presets":[],"error_code":"storage_error"})
                    }
                };
            }
            "create" | "save" => {
                let mut preset: crate::invocation_presets::InvocationPreset =
                    match serde_json::from_slice(&request.payload) {
                        Ok(value) => value,
                        Err(_) => {
                            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":"invalid_payload"})
                        }
                    };
                if preset.owner_scope != request.owner_scope {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":"owner_scope_mismatch"});
                }
                if let Err(error) = preset.validate() {
                    return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"rejected","error_code":error.to_string()});
                }
                preset.content_hash = preset.canonical_content_hash();
                let content = serde_json::to_string(&preset).unwrap_or_default();
                let state = serde_json::to_value(preset.state)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("ready")
                    .to_string();
                match evohime_local_storage::invocation_presets_store::save_revision(
                    connection,
                    &preset.owner_scope,
                    &preset.id,
                    preset.revision,
                    &content,
                    &preset.content_hash,
                    &state,
                    crate::task_memory::now_millis() as i64,
                ) {
                    Ok(true) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"saved","preset_id":preset.id,"revision":preset.revision,"content_hash":preset.content_hash,"error_code":""})
                    }
                    Ok(false) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"conflict","preset_id":preset.id,"revision":preset.revision,"error_code":"duplicate_revision"})
                    }
                    Err(_) => {
                        serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"error","error_code":"storage_error"})
                    }
                }
            }
            "sanitize" => match crate::invocation_presets::sanitize_completed_run(
                &serde_json::from_slice(&request.payload).unwrap_or_default(),
            ) {
                Ok(preview) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"sanitize","status":"preview","preview":preview,"error_code":""})
                }
                Err(error) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":"sanitize","status":"rejected","error_code":error.to_string()})
                }
            },
            _ => {
                serde_json::json!({"schema_version":1,"request_id":request.request_id,"operation":request.operation,"status":"unavailable","error_code":"unsupported_operation"})
            }
        }
    }

    async fn write_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        event_type: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: event_type.into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: None,
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }

    async fn write_package_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        operation: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        let value: serde_json::Value = serde_json::from_slice(&payload)?;
        let result = generated::WorkflowPackageResult {
            schema_version: 1,
            operation: operation.into(),
            status: value["status"].as_str().unwrap_or("unknown").into(),
            package_hash: value["package_hash"].as_str().unwrap_or_default().into(),
            import_id: value["import_id"].as_str().unwrap_or_default().into(),
            local_workflow_id: value["local_workflow_id"]
                .as_str()
                .unwrap_or_default()
                .into(),
            error_code: value["error_code"].as_str().unwrap_or_default().into(),
        };
        let event = generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: format!("workflow.package.{operation}"),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::WorkflowPackage(result)),
        };
        transport::write_frame(writer, &event.encode_to_vec()).await?;
        Ok(())
    }
}

fn validate_skill_workspace(
    value: &str,
) -> Result<std::path::PathBuf, crate::skill_registry::SkillRegistryError> {
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(crate::skill_registry::SkillRegistryError::UnsafePath(
            "workspace".into(),
        ));
    }
    let path = std::path::Path::new(value);
    if !path.is_absolute() || !path.is_dir() {
        return Err(crate::skill_registry::SkillRegistryError::UnsafePath(
            "workspace".into(),
        ));
    }
    path.canonicalize()
        .map_err(|error| crate::skill_registry::SkillRegistryError::Io(error.to_string()))
}

fn skill_metadata_projection(
    metadata: crate::skill_registry::SkillMetadataV1,
) -> generated::SkillMetadataProjection {
    generated::SkillMetadataProjection {
        schema_version: metadata.schema_version,
        skill_id: bounded_skill_field(&metadata.skill_id),
        name: bounded_skill_field(&metadata.name),
        description: bounded_skill_field(&metadata.description),
        version: bounded_skill_field(&metadata.version),
        scope: bounded_skill_field(&metadata.scope),
        source_kind: metadata.source_kind.as_str().into(),
        source_ref: bounded_skill_field(&metadata.source_ref),
        content_hash: bounded_skill_field(&metadata.content_hash),
        allowed_tools: bounded_skill_list(metadata.allowed_tools),
        required_capabilities: bounded_skill_list(metadata.required_capabilities),
        disable_model_invocation: metadata.disable_model_invocation,
        reference_count: metadata.reference_count.min(u32::MAX as usize) as u32,
        validation_status: serde_json::to_string(&metadata.validation_status)
            .unwrap_or_else(|_| "invalid".into())
            .trim_matches('"')
            .into(),
        validation_error_code: metadata.validation_error_code.unwrap_or_default(),
        warnings: metadata
            .warnings
            .into_iter()
            .take(16)
            .map(|warning| bounded_skill_field(&warning))
            .collect(),
    }
}

fn skill_diagnostic_projection(
    diagnostic: crate::skill_registry::SkillDiagnostic,
) -> generated::SkillDiagnosticProjection {
    generated::SkillDiagnosticProjection {
        code: bounded_skill_field(&diagnostic.code),
        skill_id: bounded_skill_field(&diagnostic.skill_id),
        source_kind: diagnostic.source_kind.as_str().into(),
        source_ref: bounded_skill_field(&diagnostic.source_ref),
        message: bounded_skill_field(&diagnostic.message),
    }
}

fn bounded_skill_field(value: &str) -> String {
    value.chars().take(512).collect()
}
fn bounded_skill_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .take(crate::skill_registry::MAX_LIST_ITEMS)
        .map(|value| bounded_skill_field(&value))
        .collect()
}
fn skill_content_error(skill_id: &str, code: &str) -> generated::SkillContentResult {
    generated::SkillContentResult {
        schema_version: crate::skill_registry::SKILL_SCHEMA_VERSION,
        skill_id: bounded_skill_field(skill_id),
        error_code: code.into(),
        error_message: "Skill не удалось загрузить; содержимое не выдано.".into(),
        ..Default::default()
    }
}
fn skill_reference_error(
    skill_id: &str,
    reference: &str,
    code: &str,
) -> generated::SkillReferenceResult {
    generated::SkillReferenceResult {
        schema_version: crate::skill_registry::SKILL_SCHEMA_VERSION,
        skill_id: bounded_skill_field(skill_id),
        reference: bounded_skill_field(reference),
        error_code: code.into(),
        error_message: "Reference не удалось загрузить; содержимое не выдано.".into(),
        ..Default::default()
    }
}

fn valid_goal_token(value: &str) -> bool {
    valid_checkpoint_token(value, crate::goal::GOAL_MAX_ID_CHARS)
}

fn valid_goal_action(goal_id: &str, expected_version: u64, idempotency_key: &str) -> bool {
    valid_goal_token(goal_id) && expected_version > 0 && valid_goal_token(idempotency_key)
}

fn goal_criteria_from_request(
    criteria: &[generated::GoalCriterionInput],
) -> Result<Vec<crate::goal::GoalCriterionV1>, ()> {
    criteria
        .iter()
        .map(|criterion| {
            let kind = match criterion.kind.as_str() {
                "manual" => crate::goal::GoalCriterionKind::Manual,
                "gate" => crate::goal::GoalCriterionKind::Gate,
                "workflow_evidence" => crate::goal::GoalCriterionKind::WorkflowEvidence,
                "artifact" => crate::goal::GoalCriterionKind::Artifact,
                _ => return Err(()),
            };
            if !valid_goal_token(&criterion.id)
                || criterion.statement.trim().is_empty()
                || criterion.statement.len() > crate::goal::GOAL_MAX_TEXT_CHARS
            {
                return Err(());
            }
            Ok(crate::goal::GoalCriterionV1::new(
                &criterion.id,
                kind,
                &criterion.statement,
            ))
        })
        .collect()
}

fn goal_storage_error_code(error: &StorageError) -> &'static str {
    match error {
        StorageError::Goal(error) => error.code(),
        StorageError::VersionConflict { .. } => "stale_version",
        StorageError::DeduplicationConflict { .. } => "idempotency_conflict",
        _ => "storage_failed",
    }
}

fn goal_storage_error_message(error: &StorageError) -> String {
    match error {
        StorageError::Goal(crate::goal::GoalError::NotFound(_)) => "Цель не найдена.".into(),
        StorageError::Goal(crate::goal::GoalError::ReferenceNotFound { .. }) => {
            "Связанный runtime-объект не найден или недоступен.".into()
        }
        StorageError::VersionConflict { .. } => {
            "Состояние цели уже изменилось; обнови проекцию.".into()
        }
        StorageError::DeduplicationConflict { .. } => {
            "Ключ idempotency уже использован для другой команды.".into()
        }
        StorageError::Goal(crate::goal::GoalError::CompletionEvidenceMissing) => {
            "Цель нельзя завершить без подтверждённых Core evidence.".into()
        }
        StorageError::Goal(crate::goal::GoalError::InvalidField { .. }) => {
            "Контракт цели нарушен.".into()
        }
        _ => "Операция с целью не записалась.".into(),
    }
}

fn goal_projection_error(goal_id: &str, error_code: &str) -> generated::GoalProjection {
    generated::GoalProjection {
        schema_version: crate::goal::GOAL_SCHEMA_VERSION,
        goal_id: if valid_goal_token(goal_id) {
            goal_id.to_owned()
        } else {
            String::new()
        },
        error_code: error_code.into(),
        recovery_warning: "Проекция цели недоступна; автоматическое продолжение запрещено.".into(),
        ..Default::default()
    }
}

fn goal_projection(
    goal: &crate::goal::GoalV1,
    recovery_warning: &str,
) -> generated::GoalProjection {
    generated::GoalProjection {
        schema_version: crate::goal::GOAL_SCHEMA_VERSION,
        goal_id: bounded_checkpoint_text(&goal.id),
        version: goal.version,
        workspace_id: bounded_checkpoint_text(&goal.workspace_id),
        chat_id: goal.chat_id.clone().unwrap_or_default(),
        objective: bounded_checkpoint_text(&goal.objective),
        success_criteria: goal
            .success_criteria
            .iter()
            .take(crate::goal::GOAL_MAX_CRITERIA)
            .map(goal_criterion_projection)
            .collect(),
        status: goal.status.as_str().into(),
        progress_summary: bounded_checkpoint_text(&goal.progress_summary),
        completed_criteria: goal
            .completed_criteria
            .iter()
            .take(crate::goal::GOAL_MAX_CRITERIA)
            .cloned()
            .collect(),
        remaining_criteria: goal
            .remaining_criteria
            .iter()
            .take(crate::goal::GOAL_MAX_CRITERIA)
            .cloned()
            .collect(),
        blockers: goal
            .blockers
            .iter()
            .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
            .map(|value| bounded_checkpoint_text(value))
            .collect(),
        next_action: goal.next_action.clone().unwrap_or_default(),
        workflow_run_ids: goal
            .workflow_run_ids
            .iter()
            .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
            .cloned()
            .collect(),
        child_run_ids: goal
            .child_run_ids
            .iter()
            .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
            .cloned()
            .collect(),
        checkpoint_id: goal.checkpoint_id.clone().unwrap_or_default(),
        token_budget: goal.token_budget.unwrap_or_default(),
        cost_budget_micros: goal.cost_budget_micros.unwrap_or_default(),
        continuation_budget: goal.continuation_budget.unwrap_or_default(),
        created_at_ms: goal.created_at_ms,
        updated_at_ms: goal.updated_at_ms,
        content_hash: bounded_checkpoint_text(&goal.content_hash),
        recovery_warning: bounded_checkpoint_text(recovery_warning),
        error_code: String::new(),
    }
}

fn goal_criterion_projection(
    criterion: &crate::goal::GoalCriterionV1,
) -> generated::GoalCriterionProjection {
    generated::GoalCriterionProjection {
        id: bounded_checkpoint_text(&criterion.id),
        kind: criterion.kind.as_str().into(),
        statement: bounded_checkpoint_text(&criterion.statement),
        status: criterion.status.as_str().into(),
        evidence_ref: criterion.evidence_ref.clone().unwrap_or_default(),
        verifier_id: criterion.verifier_id.clone().unwrap_or_default(),
        verifier_version: criterion.verifier_version.clone().unwrap_or_default(),
        verified_at_ms: criterion.verified_at_ms.unwrap_or_default(),
        provenance: match criterion.provenance {
            crate::goal::GoalProvenance::User => "user",
            crate::goal::GoalProvenance::Core => "core",
        }
        .into(),
    }
}

fn goal_action_result_from_mutation(
    result: crate::goal::GoalMutationResult,
) -> generated::GoalActionResult {
    generated::GoalActionResult {
        schema_version: crate::goal::GOAL_SCHEMA_VERSION,
        goal_id: bounded_checkpoint_text(&result.goal.id),
        action: bounded_checkpoint_text(&result.action),
        applied: result.applied,
        deduplicated: result.deduplicated,
        goal_version: result.goal.version,
        sequence_id: result.event_sequence,
        goal: Some(goal_projection(&result.goal, "")),
        ..Default::default()
    }
}

fn goal_action_error(
    goal_id: &str,
    action: &str,
    error_code: &str,
    error_message: &str,
) -> generated::GoalActionResult {
    generated::GoalActionResult {
        schema_version: crate::goal::GOAL_SCHEMA_VERSION,
        goal_id: if valid_goal_token(goal_id) {
            goal_id.to_owned()
        } else {
            String::new()
        },
        action: bounded_checkpoint_text(action),
        error_code: bounded_checkpoint_text(error_code),
        error_message: bounded_checkpoint_text(error_message),
        ..Default::default()
    }
}

fn valid_checkpoint_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
}

fn valid_checkpoint_workspace(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn analysis_kernel_projection_error(code: &str) -> generated::AnalysisKernelProjection {
    generated::AnalysisKernelProjection {
        schema_version: crate::analysis_kernel::ANALYSIS_KERNEL_SCHEMA_VERSION,
        error_code: code.into(),
        ..Default::default()
    }
}

fn refinement_projection(
    row: evohime_local_storage::refinement_store::CandidateRow,
) -> generated::RefinementProjection {
    generated::RefinementProjection {
        schema_version: crate::refinement::CONTRACT_VERSION,
        candidate_id: row.id,
        revision: row.revision as u64,
        owner_scope: row.owner_scope,
        kind: row.kind,
        target: row.target,
        status: row.status,
        pattern_key: row.pattern_key,
        title: row.title,
        evidence_count: row.evidence_count,
        conflict_count: row.conflict_count,
        confidence: row.confidence,
        content_hash: row.content_hash,
        policy_snapshot_hash: row.policy_snapshot_hash,
        version: row.version as u64,
        error_code: row.error_code.unwrap_or_default(),
        updated_at_ms: row.updated_at_ms,
    }
}

fn refinement_projection_error(code: &str) -> generated::RefinementProjection {
    generated::RefinementProjection {
        schema_version: crate::refinement::CONTRACT_VERSION,
        error_code: code.into(),
        ..Default::default()
    }
}

fn refinement_action_error(
    request: &generated::RefinementAction,
    code: &str,
) -> generated::RefinementActionResult {
    generated::RefinementActionResult {
        schema_version: crate::refinement::CONTRACT_VERSION,
        candidate_id: request.candidate_id.clone(),
        revision: request.revision,
        action: request.action.clone(),
        error_code: code.into(),
        ..Default::default()
    }
}

async fn write_refinement_projection<W: AsyncWrite + Unpin>(
    writer: &mut W,
    projection: generated::RefinementProjection,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: String::new(),
        event_type: "refinement.candidate".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::Refinement(projection)),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

async fn write_refinement_list_projection<W: AsyncWrite + Unpin>(
    writer: &mut W,
    projection: generated::RefinementListProjection,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: String::new(),
        event_type: "refinement.list".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::RefinementList(projection)),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

async fn write_refinement_action_result<W: AsyncWrite + Unpin>(
    writer: &mut W,
    result: generated::RefinementActionResult,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: String::new(),
        event_type: "refinement.action".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::RefinementAction(result)),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

fn analysis_kernel_projection(
    session: &crate::analysis_kernel::AnalysisKernelSessionV1,
    object_count: usize,
    error_code: &str,
) -> generated::AnalysisKernelProjection {
    generated::AnalysisKernelProjection {
        schema_version: session.schema_version,
        kernel_id: session.id.clone(),
        task_id: session.task_id.clone(),
        workspace_id: session.workspace_id.clone(),
        runtime_version: session.runtime_version.clone(),
        package_manifest_hash: session.package_manifest_hash.clone(),
        policy_hash: session.policy_hash.clone(),
        status: session.status.as_str().into(),
        revision: session.revision,
        limits_json: serde_json::to_vec(&session.limits).unwrap_or_default(),
        object_count: object_count as u32,
        truncated: object_count > 1024,
        error_code: error_code.into(),
    }
}

fn analysis_kernel_object_ref(
    object: &crate::analysis_kernel::KernelObjectRefV1,
) -> generated::AnalysisKernelObjectRef {
    generated::AnalysisKernelObjectRef {
        id: object.id.clone(),
        logical_name: object.logical_name.clone(),
        type_hint: object.type_hint.clone(),
        size: object.size,
        sensitivity: object.sensitivity.as_str().into(),
        persistence: object.persistence.as_str().into(),
        content_hash: object.content_hash.clone().unwrap_or_default(),
        artifact_locator: object.artifact_locator.clone().unwrap_or_default(),
        provenance: object.provenance.clone(),
    }
}

fn analysis_kernel_result_error(request_id: &str, code: &str) -> generated::AnalysisKernelResult {
    generated::AnalysisKernelResult {
        schema_version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
        request_id: request_id.into(),
        status: "error".into(),
        error_class: code.into(),
        sensitivity: "internal".into(),
        provenance: "core:analysis-kernel".into(),
        ..Default::default()
    }
}

fn kernel_error_code(error: &crate::analysis_kernel::KernelRuntimeError) -> &'static str {
    match error {
        crate::analysis_kernel::KernelRuntimeError::NotRunning => "kernel_not_running",
        crate::analysis_kernel::KernelRuntimeError::Denied(_) => "host_request_denied",
        crate::analysis_kernel::KernelRuntimeError::LimitExceeded(_) => "limit_exceeded",
        crate::analysis_kernel::KernelRuntimeError::Operation(_) => "operation_failed",
        crate::analysis_kernel::KernelRuntimeError::Contract(error) => match error {
            crate::analysis_kernel::AnalysisKernelError::ForbiddenOperation => {
                "forbidden_operation"
            }
            crate::analysis_kernel::AnalysisKernelError::ForbiddenCapability => {
                "forbidden_capability"
            }
            crate::analysis_kernel::AnalysisKernelError::RequestTooLarge(_) => "request_too_large",
            _ => "invalid_argument",
        },
    }
}

fn kernel_storage_error_code(error: &evohime_local_storage::StorageError) -> &'static str {
    match error {
        evohime_local_storage::StorageError::AnalysisKernel(
            crate::analysis_kernel::AnalysisKernelError::VersionConflict { .. },
        ) => "stale_revision",
        _ => "storage_failed",
    }
}

async fn write_analysis_kernel_projection<W: AsyncWrite + Unpin>(
    writer: &mut W,
    projection: generated::AnalysisKernelProjection,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: projection.task_id.clone(),
        event_type: "analysis_kernel.projection".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::AnalysisKernel(projection)),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

async fn write_analysis_kernel_result<W: AsyncWrite + Unpin>(
    writer: &mut W,
    result: generated::AnalysisKernelResult,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: String::new(),
        event_type: "analysis_kernel.result".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::AnalysisKernelResult(
            result,
        )),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

fn checkpoint_status_text(status: crate::task_checkpoint::CheckpointStatus) -> String {
    serde_json::to_string(&status)
        .unwrap_or_else(|_| "unknown".into())
        .trim_matches('"')
        .to_owned()
}

fn checkpoint_disposition_text(disposition: crate::task_checkpoint::RecoveryDisposition) -> String {
    serde_json::to_string(&disposition)
        .unwrap_or_else(|_| "blocked".into())
        .trim_matches('"')
        .to_owned()
}

fn bounded_checkpoint_text(value: &str) -> String {
    value
        .chars()
        .take(TASK_CHECKPOINT_IPC_MAX_TEXT_BYTES)
        .collect()
}

fn bounded_checkpoint_event_type(value: &str) -> String {
    if value.is_empty() || value.len() > 128 || value.bytes().any(|byte| byte.is_ascii_control()) {
        "unknown".into()
    } else {
        value.to_owned()
    }
}

fn checkpoint_error_code(error: &StorageError) -> &'static str {
    match error {
        StorageError::TaskCheckpoint(error) => error.code(),
        _ => "storage_failed",
    }
}

fn task_checkpoint_projection_error(
    task_id: &str,
    error_code: &str,
) -> generated::TaskCheckpointProjection {
    generated::TaskCheckpointProjection {
        schema_version: crate::task_checkpoint::TASK_CHECKPOINT_VERSION,
        task_id: if valid_checkpoint_token(task_id, 128) {
            task_id.to_owned()
        } else {
            String::new()
        },
        recovery_disposition: "blocked".into(),
        recovery_warning: "Проекция checkpoint недоступна; автоматическое продолжение запрещено."
            .into(),
        error_code: error_code.into(),
        ..Default::default()
    }
}

fn task_checkpoint_projection(
    task_id: &str,
    recovery: crate::task_checkpoint::TaskCheckpointRecovery,
    max_replay_events: usize,
) -> generated::TaskCheckpointProjection {
    let Some(checkpoint) = recovery.checkpoint else {
        return generated::TaskCheckpointProjection {
            schema_version: crate::task_checkpoint::TASK_CHECKPOINT_VERSION,
            task_id: task_id.to_owned(),
            recovery_disposition: "no_checkpoint".into(),
            recovery_warning: "Для задачи ещё нет сохранённого checkpoint.".into(),
            ..Default::default()
        };
    };
    let blockers = checkpoint
        .blockers
        .iter()
        .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
        .map(|item| bounded_checkpoint_text(&item.text))
        .collect();
    let mut refs = Vec::new();
    for reference in checkpoint
        .workflow_refs
        .iter()
        .chain(checkpoint.child_refs.iter())
        .chain(checkpoint.artifact_refs.iter())
        .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
    {
        refs.push(generated::TaskCheckpointRef {
            kind: bounded_checkpoint_text(&reference.kind),
            id: bounded_checkpoint_text(&reference.id),
            content_hash: reference.content_hash.clone().unwrap_or_default(),
            sensitivity: serde_json::to_string(&reference.sensitivity)
                .unwrap_or_else(|_| "internal".into())
                .trim_matches('"')
                .to_owned(),
        });
    }
    let policy_id = checkpoint
        .workflow_refs
        .iter()
        .find(|reference| reference.kind == "policy_snapshot")
        .map(|reference| bounded_checkpoint_text(&reference.id))
        .unwrap_or_default();
    let replayed_event_types = recovery
        .replayed_events
        .iter()
        .take(max_replay_events)
        .map(|event| bounded_checkpoint_event_type(&event.event_type))
        .collect();
    generated::TaskCheckpointProjection {
        schema_version: checkpoint.version,
        checkpoint_id: bounded_checkpoint_text(&checkpoint.id),
        task_id: task_id.to_owned(),
        workspace_id: bounded_checkpoint_text(&checkpoint.workspace_id),
        parent_checkpoint_id: checkpoint.parent_checkpoint_id.unwrap_or_default(),
        status: checkpoint_status_text(checkpoint.status),
        source_event_seq: checkpoint.source_event_seq,
        created_at: checkpoint.created_at,
        completed_count: checkpoint.completed_items.len().min(u32::MAX as usize) as u32,
        remaining_count: checkpoint.remaining_items.len().min(u32::MAX as usize) as u32,
        blocker_count: checkpoint.blockers.len().min(u32::MAX as usize) as u32,
        blockers,
        refs,
        recovery_disposition: checkpoint_disposition_text(recovery.disposition),
        recovery_warning: recovery
            .warning
            .as_deref()
            .map(bounded_checkpoint_text)
            .unwrap_or_default(),
        replayed_event_types,
        can_request_resume: recovery.disposition
            == crate::task_checkpoint::RecoveryDisposition::Replayable,
        replayed_event_count: recovery.replayed_events.len().min(u32::MAX as usize) as u32,
        policy_id,
        error_code: String::new(),
    }
}

fn task_checkpoint_action_result(
    task_id: String,
    checkpoint_id: String,
    action: String,
    applied: bool,
    deduplicated: bool,
    error_code: &str,
    error_message: &str,
) -> generated::TaskCheckpointActionResult {
    generated::TaskCheckpointActionResult {
        task_id: if valid_checkpoint_token(&task_id, 128) {
            task_id
        } else {
            String::new()
        },
        checkpoint_id: if valid_checkpoint_token(&checkpoint_id, 128) {
            checkpoint_id
        } else {
            String::new()
        },
        action: matches!(action.as_str(), "acknowledge_recovery" | "request_resume")
            .then_some(action)
            .unwrap_or_default(),
        applied,
        deduplicated,
        error_code: error_code.into(),
        error_message: bounded_checkpoint_text(error_message),
        sequence_id: 0,
    }
}

fn task_checkpoint_action_result_from_record(
    record: TaskCheckpointActionRecord,
) -> generated::TaskCheckpointActionResult {
    task_checkpoint_action_result(
        record.task_id,
        record.checkpoint_id,
        record.action,
        record.applied,
        record.deduplicated,
        &record.error_code,
        &record.error_message,
    )
}

/// Результат `SetAmbientListening` в одном месте: неизвестный код не
/// притворяется успехом, а пустая строка означает «ошибки не было».
fn listening_result(
    state: evohime_listener_contract::ListeningState,
    code: Option<evohime_listener_contract::AmbientErrorCode>,
) -> serde_json::Value {
    serde_json::json!({
        "state": state,
        "error_code": code.map(|code| code.as_str()).unwrap_or(""),
    })
}

/// Разбирает отметку времени ambient-строки в миллисекунды эпохи.
///
/// Формат задаёт `crate::ambient::timestamp_ms`; неразбираемое значение
/// становится нулём, а не «сейчас»: выдуманное время выглядело бы как
/// свежий эпизод.
fn parse_timestamp_ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp_millis())
        .unwrap_or(0)
}

/// Builds a stage 01.4 `ReceiptFilter` from bounded IPC request fields. Empty
/// strings mean "no filter" for that field; a non-empty value must be a valid
/// 01.1 typed identifier or RFC3339 timestamp, or the whole request is
/// rejected rather than silently ignored.
fn receipt_filter_from_request(
    task_id: &str,
    run_id: &str,
    action_id: &str,
    from_rfc3339: &str,
    to_rfc3339: &str,
) -> Result<evohime_receipts::export::ReceiptFilter, &'static str> {
    let parse_ms = |value: &str| -> Result<Option<i64>, &'static str> {
        if value.is_empty() {
            return Ok(None);
        }
        chrono::DateTime::parse_from_rfc3339(value)
            .map(|parsed| Some(parsed.timestamp_millis()))
            .map_err(|_| "receipts.invalid_filter")
    };
    Ok(evohime_receipts::export::ReceiptFilter {
        task_id: (!task_id.is_empty()).then(|| task_id.to_string()),
        run_id: (!run_id.is_empty()).then(|| run_id.to_string()),
        action_id: (!action_id.is_empty()).then(|| action_id.to_string()),
        from_ms: parse_ms(from_rfc3339)?,
        to_ms: parse_ms(to_rfc3339)?,
    })
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Publishes a review event on the path a connected shell actually reads.
///
/// The pipe server flushes its journal tail only on the coordinator's
/// `journalled` signal, and that signal is raised by the coordinator's own
/// journal writer. An event recorded straight into the journal is durable but
/// stays invisible until some later event wakes the pump, which left a running
/// review looking frozen in the UI. Recording directly is the fallback for a
/// bridge built without a coordinator.
async fn publish_review_event(
    coordinator: &Option<TaskCoordinator>,
    journal: &EventJournal,
    event: CoreEvent,
) {
    match coordinator {
        Some(coordinator) => coordinator.emit(event).await,
        None => {
            let _ = journal.record(&event).await;
        }
    }
}

/// Drops the "which models reviewed which files" preamble that
/// `format_review_markdown` prepends. It is provenance for the reader, and
/// feeding it to the editing model invites those model names into the plan.
fn strip_review_header(final_markdown: &str) -> String {
    match final_markdown.split_once("\n---\n\n") {
        Some((header, body)) if header.starts_with("<!-- Контекст EvoHime") => {
            body.to_string()
        }
        _ => final_markdown.to_string(),
    }
}

fn revision_result_from_event(payload: &[u8]) -> Option<crate::plan_review::RevisionResult> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let message = value
        .get("TaskCompleted")
        .and_then(|item| item.get("final_message"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .get("final_message")
                .and_then(serde_json::Value::as_str)
        })?;
    serde_json::from_str(message).ok()
}

fn review_result_from_event(payload: &[u8]) -> Option<crate::plan_review::ReviewResult> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let message = value
        .get("TaskCompleted")
        .and_then(|item| item.get("final_message"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .get("final_message")
                .and_then(serde_json::Value::as_str)
        })?;
    serde_json::from_str(message).ok()
}

fn protocol() -> generated::ProtocolVersion {
    generated::ProtocolVersion {
        major: PROTOCOL_MAJOR,
        minor: PROTOCOL_MINOR,
    }
}

fn core_info() -> generated::CoreInfo {
    generated::CoreInfo {
        protocol: Some(protocol()),
        core_version: env!("CARGO_PKG_VERSION").into(),
        build_revision: option_env!("EVOHIME_BUILD_REVISION")
            .unwrap_or("unknown")
            .into(),
        runtime_revision: "rust-core".into(),
        capabilities: vec![
            "replay".into(),
            "resync".into(),
            "task_checkpoint".into(),
            "skills".into(),
            "goals".into(),
            "workflow_builder".into(),
        ],
        feature_flags: vec!["authenticated-ipc".into()],
        max_frame_bytes: evohime_desktop_ipc::MAX_FRAME_BYTES as u32,
        max_replay_events: evohime_desktop_ipc::MAX_REPLAY_EVENTS as u32,
        max_snapshot_bytes: evohime_desktop_ipc::MAX_RESYNC_SNAPSHOT_BYTES as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreEvent;
    use tokio::io::duplex;

    fn sample_typed_ledger_event(
        event_id: &str,
        action_id: &str,
    ) -> execution_ledger::ExecutionEventV1 {
        execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: event_id.to_string(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::Standalone,
            run_id: "run-ipc-1".into(),
            session_id: Some("session-ipc-1".into()),
            task_id: "task-ipc".into(),
            created_at_ms: 1_700_000_000_000,
            state_after: Some(execution_ledger::ActionState::Running),
            action_id: Some(action_id.to_string()),
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
                tool_name: "shell".into(),
                tool_call_hash: "hash-1".into(),
                manifest_hash: None,
            },
            redaction: execution_ledger::RedactionMeta::default(),
        }
    }

    /// Typed ledger rows written by 08-2's `append_ledger_event` must reach
    /// the IPC replay path (план 08-3) as an additive `execution_event`
    /// projection, without disturbing the generic backward-compat fields.
    #[tokio::test]
    async fn push_journal_tail_projects_typed_ledger_row_into_execution_event() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-ledger-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let source_event = sample_typed_ledger_event("event-ipc-1", "action-ipc-1");
        {
            let database = journal.database().lock().await;
            database
                .append_ledger_event(&source_event)
                .expect("typed event appends");
        }
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, mut server) = duplex(16 * 1024);

        bridge
            .push_journal_tail(&mut server, 0)
            .await
            .expect("tail pushes");
        let frame = transport::read_frame(&mut client)
            .await
            .expect("frame reads");
        let envelope = generated::EventEnvelope::decode(frame.as_slice()).expect("frame decodes");

        assert_eq!(envelope.event_type, "ledger.tool_call");
        assert!(
            !envelope.payload.is_empty(),
            "generic payload stays populated"
        );
        let projected = match envelope.event {
            Some(generated::event_envelope::Event::ExecutionEvent(projected)) => projected,
            other => panic!("expected ExecutionEvent oneof, got {other:?}"),
        };
        assert_eq!(projected.event_id, "event-ipc-1");
        assert_eq!(projected.action_id, "action-ipc-1");
        assert_eq!(projected.run_scope, "standalone");
        assert_eq!(projected.state_after, "running");
        let body: execution_ledger::ExecutionEventBody =
            serde_json::from_slice(&projected.body_json).expect("body_json decodes");
        assert_eq!(body, source_event.body);
        let _ = std::fs::remove_file(&path);
    }

    /// План 08-4 acceptance: "reconnect во время каждой промежуточной
    /// фазы" — the typed IPC projection is generic over `state_after`, not
    /// special-cased to whatever phase happened to be tested elsewhere.
    /// Replays a run whose last known phase is `waiting_approval` and one
    /// whose last known phase is `cancelling` (the state this plan's own
    /// CHECK-rebuild migration exists to allow), proving both reconnect
    /// correctly rather than only the already-covered `running`/terminal
    /// cases.
    #[tokio::test]
    async fn reconnect_projects_every_intermediate_phase_not_just_running() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-reconnect-phases-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        {
            let database = journal.database().lock().await;
            let mut waiting_approval = sample_typed_ledger_event("event-phase-1", "action-phase-1");
            waiting_approval.state_after = Some(execution_ledger::ActionState::WaitingApproval);
            database
                .append_ledger_event(&waiting_approval)
                .expect("waiting_approval event appends");
            let mut cancelling = sample_typed_ledger_event("event-phase-2", "action-phase-2");
            cancelling.state_after = Some(execution_ledger::ActionState::Cancelling);
            database
                .append_ledger_event(&cancelling)
                .expect("cancelling event appends");
        }
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, mut server) = duplex(16 * 1024);

        bridge
            .push_journal_tail(&mut server, 0)
            .await
            .expect("tail pushes");

        let first = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("first frame reads")
                .as_slice(),
        )
        .expect("first frame decodes");
        let second = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("second frame reads")
                .as_slice(),
        )
        .expect("second frame decodes");

        for (envelope, expected_state) in [(first, "waiting_approval"), (second, "cancelling")] {
            let projected = match envelope.event {
                Some(generated::event_envelope::Event::ExecutionEvent(projected)) => projected,
                other => panic!("expected ExecutionEvent oneof, got {other:?}"),
            };
            assert_eq!(projected.state_after, expected_state);
        }
        let _ = std::fs::remove_file(&path);
    }

    /// Generic (non-`ledger.*`) rows keep flowing through the pre-08-3 path:
    /// `execution_event` stays unset and nothing else about the frame changes.
    #[tokio::test]
    async fn push_journal_tail_leaves_generic_rows_unprojected() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-generic-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-generic".into(),
                final_message: serde_json::json!({"ok": true}).to_string(),
            })
            .await
            .expect("generic event records");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, mut server) = duplex(16 * 1024);

        bridge
            .push_journal_tail(&mut server, 0)
            .await
            .expect("tail pushes");
        let frame = transport::read_frame(&mut client)
            .await
            .expect("frame reads");
        let envelope = generated::EventEnvelope::decode(frame.as_slice()).expect("frame decodes");

        assert!(
            envelope.event.is_none(),
            "generic row must not get a typed projection"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Regression: the clear marker was published on the coordinator broadcast,
    /// so the journal writer recorded it a moment later. The listing that the
    /// panel sends right after the response still read the old marker and kept
    /// showing the reviews that had just been cleared.
    #[tokio::test]
    async fn clearing_history_hides_reviews_from_the_next_listing() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-clear-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "review-old".into(),
                final_message: serde_json::json!({
                    "review_id": "review-old",
                    "file_name": "plan.md",
                    "synthesis_model": "main",
                    "reviewers": [],
                    "final_markdown": "# Итог"
                })
                .to_string(),
            })
            .await
            .expect("review records");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let list_frame = || {
            generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: "review-list".into(),
                client_id: "test-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(generated::command_envelope::Command::ListPlanReviews(
                    generated::ListPlanReviews { limit: 20 },
                )),
            }
            .encode_to_vec()
        };

        transport::write_frame(&mut client, &list_frame())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("list response")
                .as_slice(),
        )
        .expect("list decodes");
        let before: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert_eq!(before["reviews"].as_array().expect("reviews").len(), 1);

        let clear = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "review-clear".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(
                generated::command_envelope::Command::ClearPlanReviewHistory(
                    generated::ClearPlanReviewHistory {},
                ),
            ),
        };
        transport::write_frame(&mut client, &clear.encode_to_vec())
            .await
            .expect("clear writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("clear serves");
        let _ = transport::read_frame(&mut client)
            .await
            .expect("clear response");

        // The panel lists again as soon as the clear is acknowledged.
        transport::write_frame(&mut client, &list_frame())
            .await
            .expect("second list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("second list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("second list response")
                .as_slice(),
        )
        .expect("second list decodes");
        let after: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert!(
            after["reviews"].as_array().expect("reviews").is_empty(),
            "a cleared history must be empty in the very next listing"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Regression: журнал вырос, снапшот resync перестал влезать в кадр IPC,
    /// и Core обрывал соединение с оболочкой. Оболочка переподключалась без
    /// состояния и навсегда показывала «нет связи с процессом слушателя»,
    /// хотя слушатель работал. Превышение лимита обязано деградировать до
    /// поштучной отправки, а не рвать канал.
    #[tokio::test]
    async fn an_oversized_snapshot_degrades_instead_of_dropping_the_shell() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-snapshot-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        // Payload журнала уезжает в снапшот массивом чисел, поэтому байты
        // раздуваются в несколько раз: восьми записей хватает, чтобы перейти
        // границу кадра.
        for index in 0..8 {
            journal
                .record(&CoreEvent::TaskCompleted {
                    task_id: format!("task-{index}"),
                    final_message: "a".repeat(200 * 1024),
                })
                .await
                .expect("event records");
        }
        let bridge = IpcBridge::new(journal);
        let (client, server) = duplex(64 * 1024 * 1024);
        let (mut client_reader, mut client_writer) = tokio::io::split(client);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let request = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resync-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ResyncRequest(
                generated::ResyncRequest {
                    after_sequence: 0,
                    max_events: 0,
                    include_full_snapshot: true,
                },
            )),
        };
        transport::write_frame(&mut client_writer, &request.encode_to_vec())
            .await
            .expect("resync writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("оболочка не должна терять соединение из-за размера снапшота");

        let mut seen = Vec::new();
        loop {
            let frame = transport::read_frame(&mut client_reader)
                .await
                .expect("resync response");
            let event = generated::EventEnvelope::decode(frame.as_slice()).expect("event decodes");
            seen.push(event.event_type.clone());
            if event.event_type == "resync.end" {
                break;
            }
        }

        assert!(
            seen.iter().any(|event| event == "replay.snapshot_skipped"),
            "оболочку нужно предупредить о пропущенном снапшоте: {seen:?}"
        );
        assert!(
            !seen.iter().any(|event| event == "replay.full_snapshot"),
            "снапшот сверх лимита отправлять нельзя: {seen:?}"
        );
        assert_eq!(
            seen.len(),
            10,
            "вместо снапшота оболочка обязана получить те же события поштучно: {seen:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// A large backlog is paged `max_events` at a time (план про «нет связи»
    /// после большой сессии): `resync.end` must say when more history sits
    /// beyond the page it just sent, so the shell chains the next resync
    /// itself instead of racing a random live-event gap to notice.
    #[tokio::test]
    async fn resync_end_reports_more_available_across_a_bounded_page() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-more-available-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        for index in 0..3 {
            journal
                .record(&CoreEvent::TaskCompleted {
                    task_id: format!("task-{index}"),
                    final_message: "done".into(),
                })
                .await
                .expect("event records");
        }
        let bridge = IpcBridge::new(journal);
        let (client, server) = duplex(64 * 1024);
        let (mut client_reader, mut client_writer) = tokio::io::split(client);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let request = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resync-page-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ResyncRequest(
                generated::ResyncRequest {
                    after_sequence: 0,
                    max_events: 2,
                    include_full_snapshot: false,
                },
            )),
        };
        transport::write_frame(&mut client_writer, &request.encode_to_vec())
            .await
            .expect("resync writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resync succeeds");

        let end = loop {
            let frame = transport::read_frame(&mut client_reader)
                .await
                .expect("resync response");
            let event = generated::EventEnvelope::decode(frame.as_slice()).expect("event decodes");
            if event.event_type == "resync.end" {
                break event;
            }
        };
        assert_eq!(end.sequence_id, 2, "page stops at the requested max_events");
        let payload: serde_json::Value =
            serde_json::from_slice(&end.payload).expect("resync.end payload decodes as json");
        assert_eq!(
            payload["more_available"],
            serde_json::json!(true),
            "a third event sits beyond this page: {payload:?}"
        );
        assert_eq!(payload["latest_sequence"], serde_json::json!(3));

        let _ = std::fs::remove_file(&path);
    }

    /// The revised plan is written by Core, not by the shell, so the extension
    /// guard lives here: a shell bug must not be able to overwrite a `.rs` or a
    /// `.json` with Markdown.
    #[tokio::test]
    async fn saves_a_revised_plan_only_to_a_markdown_path() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-revision-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let bridge = IpcBridge::new(journal);
        bridge.revision_results.lock().await.insert(
            "revision-1".into(),
            crate::plan_review::RevisionResult {
                revision_id: "revision-1".into(),
                review_id: "review-1".into(),
                file_name: "plan.md".into(),
                model: "main".into(),
                revised_markdown: "# Исправленный план".into(),
                context_files: Vec::new(),
            },
        );
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let save = |destination: &str| {
            generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: "revision-save".into(),
                client_id: "test-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(generated::command_envelope::Command::SaveRevisedPlan(
                    generated::SaveRevisedPlan {
                        revision_id: "revision-1".into(),
                        destination_path: destination.into(),
                    },
                )),
            }
            .encode_to_vec()
        };

        let destination =
            std::env::temp_dir().join(format!("evohime-revised-{}.md", std::process::id()));
        let _ = std::fs::remove_file(&destination);
        transport::write_frame(&mut client, &save(&destination.to_string_lossy()))
            .await
            .expect("save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("save serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("save response")
                .as_slice(),
        )
        .expect("save decodes");
        assert_eq!(response.event_type, "plan.saved");
        assert_eq!(
            std::fs::read_to_string(&destination).expect("revised plan is on disk"),
            "# Исправленный план"
        );

        // Отказ приходит событием: ошибка кадра оборвала бы соединение с
        // оболочкой, и опечатка в имени файла читалась бы как падение ядра.
        let rejected = std::env::temp_dir().join("evohime-revised.txt");
        transport::write_frame(&mut client, &save(&rejected.to_string_lossy()))
            .await
            .expect("rejected save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("a refused save keeps the connection");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("refusal response")
                .as_slice(),
        )
        .expect("refusal decodes");
        assert_eq!(response.event_type, "plan.save_failed");
        assert!(!rejected.exists());

        let _ = std::fs::remove_file(&destination);
        let _ = std::fs::remove_file(&path);
    }

    /// Обновление Евы перезапускает Core, а нажать «сохранить» пользователь
    /// может и после этого: правка обязана находиться в журнале, когда кэш уже
    /// пуст.
    #[tokio::test]
    async fn saves_a_revised_plan_recovered_from_the_journal() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-revision-journal-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "revision-7".into(),
                final_message: serde_json::json!({
                    "revision_id": "revision-7",
                    "review_id": "review-1",
                    "file_name": "plan.md",
                    "model": "main",
                    "revised_markdown": "# Восстановленный план"
                })
                .to_string(),
            })
            .await
            .expect("revision records");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let destination =
            std::env::temp_dir().join(format!("evohime-revised-journal-{}.md", std::process::id()));
        let _ = std::fs::remove_file(&destination);
        transport::write_frame(
            &mut client,
            &generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: "revision-save".into(),
                client_id: "test-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(generated::command_envelope::Command::SaveRevisedPlan(
                    generated::SaveRevisedPlan {
                        revision_id: "revision-7".into(),
                        destination_path: destination.to_string_lossy().into(),
                    },
                )),
            }
            .encode_to_vec(),
        )
        .await
        .expect("save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("save serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("save response")
                .as_slice(),
        )
        .expect("save decodes");
        assert_eq!(response.event_type, "plan.saved");
        assert_eq!(
            std::fs::read_to_string(&destination).expect("revised plan is on disk"),
            "# Восстановленный план"
        );
        let _ = std::fs::remove_file(&destination);
        let _ = std::fs::remove_file(&path);
    }

    /// Revising a review the core has never seen would let the shell hand the
    /// editing model an arbitrary text and call it a review.
    #[tokio::test]
    async fn refuses_to_revise_an_unknown_review() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-revision-missing-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "revision-start".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RevisePlan(
                generated::RevisePlan {
                    revision_id: "revision-1".into(),
                    review_id: "review-missing".into(),
                    file_name: "plan.md".into(),
                    source_markdown: "# Plan".into(),
                    model: "main".into(),
                    source_path: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("revise writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn serves_replay_command_over_framed_transport() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-ipc".into(),
                final_message: "replayed".into(),
            })
            .await
            .expect("event records");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "request-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 0 },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("bridge serves replay");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.sequence_id, 1);
        assert_eq!(event.task_id, "task-ipc");
        assert_eq!(event.event_type, "task.completed");
        assert!(String::from_utf8(event.payload)
            .expect("payload utf8")
            .contains("replayed"));
        let _ = std::fs::remove_file(path);
    }

    /// План 08-4: `redaction.secrets_present` must reflect a real scan of
    /// the request, not just always be `false` — the same secret-shape
    /// markers `crate::audit::contains_secret` already redacts on.
    #[test]
    fn tool_request_redaction_flags_secret_shaped_input_and_clears_ordinary_input() {
        let secret_request = evohime_receipts::runtime::ActionRequest {
            action_id: uuid::Uuid::now_v7(),
            task_id: "task-1".into(),
            run_id: "task-1".into(),
            tool_name: "shell.execute".into(),
            policy_id: "permission:ShellExecute".into(),
            normalized_scope: "workspace".into(),
            input: serde_json::json!({"program": "curl", "args": ["-H", "Authorization: Bearer sk-abc123"]}),
            policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
            approval_id: None,
            parent_approval_ref: None,
            preview: "curl call".into(),
        };
        assert!(tool_request_redaction(&secret_request).secrets_present);

        let ordinary_request = evohime_receipts::runtime::ActionRequest {
            input: serde_json::json!({"program": "git", "args": ["status"]}),
            ..secret_request
        };
        assert!(!tool_request_redaction(&ordinary_request).secrets_present);
    }

    /// План 08-3: a client whose `CommandEnvelope` names a different
    /// generation than this process must get an honest typed `ReplayGap`
    /// with `reason = "stale_generation"` before the (still-served) replay,
    /// not just silently receive events stamped with a new identity.
    #[tokio::test]
    async fn stale_generation_produces_a_typed_replay_gap_before_replay() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-stale-generation-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-stale".into(),
                final_message: "stale".into(),
            })
            .await
            .expect("event records");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "request-stale".into(),
            client_id: "test-client".into(),
            core_instance_id: "a-generation-this-process-never-had".into(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 0 },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("bridge serves replay");

        let gap_frame = transport::read_frame(&mut client)
            .await
            .expect("gap frame reads");
        let gap_envelope =
            generated::EventEnvelope::decode(gap_frame.as_slice()).expect("gap decodes");
        let gap = match gap_envelope.event {
            Some(generated::event_envelope::Event::ReplayGap(gap)) => gap,
            other => panic!("expected typed ReplayGap, got {other:?}"),
        };
        assert_eq!(gap.reason, "stale_generation");
        assert_eq!(gap.requested_after_sequence, 0);

        let event_frame = transport::read_frame(&mut client)
            .await
            .expect("event frame reads");
        let event =
            generated::EventEnvelope::decode(event_frame.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "task.completed");
        let _ = std::fs::remove_file(path);
    }

    /// План 08-3 п.5: `FullSnapshot.snapshot_json` carries a bounded typed
    /// action projection (latest state per `action_id`), not just a raw
    /// event dump — a reconnecting client can rebuild action cards from the
    /// snapshot alone.
    #[tokio::test]
    async fn resync_snapshot_includes_typed_action_projection() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-snapshot-actions-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let ledger_event = execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: "event-snapshot-action-1".into(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::Standalone,
            run_id: "run-snapshot-1".into(),
            session_id: Some("session-snapshot-1".into()),
            task_id: "task-snapshot".into(),
            created_at_ms: 1_700_000_000_000,
            state_after: Some(execution_ledger::ActionState::Running),
            action_id: Some("action-snapshot-1".into()),
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
                tool_name: "shell".into(),
                tool_call_hash: "hash-1".into(),
                manifest_hash: None,
            },
            redaction: execution_ledger::RedactionMeta::default(),
        };
        {
            let database = journal.database().lock().await;
            database
                .append_ledger_event(&ledger_event)
                .expect("typed event appends");
        }
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resync-actions".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ResyncRequest(
                generated::ResyncRequest {
                    after_sequence: 0,
                    max_events: 0,
                    include_full_snapshot: true,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("resync writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resync serves");

        let frame = transport::read_frame(&mut client)
            .await
            .expect("snapshot frame reads");
        let envelope = generated::EventEnvelope::decode(frame.as_slice()).expect("frame decodes");
        let snapshot = match envelope.event {
            Some(generated::event_envelope::Event::FullSnapshot(snapshot)) => snapshot,
            other => panic!("expected FullSnapshot, got {other:?}"),
        };
        let payload: serde_json::Value =
            serde_json::from_slice(&snapshot.snapshot_json).expect("snapshot json decodes");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["snapshot_sequence_id"], snapshot.sequence_id);
        let actions = payload["actions"].as_array().expect("actions array");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["action_id"], "action-snapshot-1");
        assert_eq!(actions[0]["state_after"], "running");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_bounded_workspace_list_and_file_read_over_ipc() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-workspace-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("src directory");
        std::fs::write(root.join("README.md"), "hello from workspace").expect("readme");
        let journal_path =
            std::env::temp_dir().join(format!("evohime-ipc-workspace-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&journal_path);
        let bridge = IpcBridge::new(EventJournal::open(&journal_path).expect("journal opens"));
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let list = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "workspace-list".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ListWorkspace(
                generated::ListWorkspace {
                    workspace_path: root.display().to_string(),
                    relative_path: ".".into(),
                    max_entries: 10,
                },
            )),
        };
        transport::write_frame(&mut client, &list.encode_to_vec())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("list reads")
                .as_slice(),
        )
        .expect("list event decodes");
        assert_eq!(response.event_type, "workspace.list");
        let listing: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert_eq!(listing["entries"][0]["name"], "src");

        let read = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "workspace-read".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ReadWorkspaceFile(
                generated::ReadWorkspaceFile {
                    workspace_path: root.display().to_string(),
                    relative_path: "README.md".into(),
                    max_bytes: 100,
                },
            )),
        };
        transport::write_frame(&mut client, &read.encode_to_vec())
            .await
            .expect("read writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("read serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("read response")
                .as_slice(),
        )
        .expect("read event decodes");
        assert_eq!(response.event_type, "workspace.file");
        let file: serde_json::Value = serde_json::from_slice(&response.payload).expect("file json");
        assert_eq!(file["content"], "hello from workspace");
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(journal_path);
    }

    #[tokio::test]
    async fn terminal_requires_approval_and_denied_retry_does_not_execute() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-terminal-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("terminal root");
        let data_root =
            std::env::temp_dir().join(format!("evohime-ipc-terminal-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("terminal data root");
        let journal_path = data_root.join("events.db");
        let _ = std::fs::remove_file(&journal_path);
        let receipt_keys = ReceiptKeyManager::new(&data_root);
        receipt_keys.initialize().expect("receipt keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal,
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4().to_string();
        let make_terminal = |approval_id: String| generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "terminal-request".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::TerminalExecute(
                generated::TerminalExecute {
                    task_id: task_id.clone(),
                    workspace_path: root.display().to_string(),
                    program: "git".into(),
                    args: vec!["status".into()],
                    cwd: String::new(),
                    timeout_ms: 5_000,
                    approval_id,
                },
            )),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
            .await
            .expect("terminal writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("approval serves");
        let approval = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("approval reads")
                .as_slice(),
        )
        .expect("approval decodes");
        assert_eq!(approval.event_type, "approval.required");
        let approval_json =
            serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
        assert_eq!(approval_json["preview"]["kind"], "command");
        assert_eq!(approval_json["preview"]["command"], "git status");
        let approval_id = approval_json["approval_id"]
            .as_str()
            .expect("approval id")
            .to_string();

        let resolve = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resolve-terminal".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ResolveApproval(
                generated::ResolveApproval {
                    approval_id: approval_id.clone(),
                    granted: false,
                    idempotency_key: String::new(),
                    rejection_reason: String::new(),
                    cancel: false,
                },
            )),
        };
        transport::write_frame(&mut client, &resolve.encode_to_vec())
            .await
            .expect("resolve writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resolve serves");

        // План 08-4 acceptance: a denied approval publishes a typed
        // ApprovalDecision/Denied ledger event linked to the receipts
        // approval intent's own action_id — this is the "reject" arm of
        // "approval approve/reject/expiry".
        {
            let journal_handle = bridge.journal();
            let database = journal_handle.database().lock().await;
            let (decision_state, body_payload): (String, Vec<u8>) = database
                .connection()
                .query_row(
                    "SELECT state_after, payload FROM events
                       WHERE event_type = 'ledger.approval_decision'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("ledger.approval_decision row exists");
            assert_eq!(decision_state, "denied");
            let decision_event: execution_ledger::ExecutionEventV1 =
                serde_json::from_slice(&body_payload).expect("decision event decodes");
            let execution_ledger::ExecutionEventBody::ApprovalDecision {
                approval_intent_id,
                decision,
                ..
            } = decision_event.body
            else {
                panic!(
                    "expected ApprovalDecision body, got {:?}",
                    decision_event.body
                );
            };
            assert_eq!(approval_intent_id, approval_id);
            assert_eq!(decision, execution_ledger::ApprovalOutcome::Rejected);
        }

        transport::write_frame(&mut client, &make_terminal(approval_id).encode_to_vec())
            .await
            .expect("retry writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("retry serves");
        let result = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("result reads")
                .as_slice(),
        )
        .expect("result decodes");
        assert_eq!(result.event_type, "terminal.result");
        let result_json: serde_json::Value =
            serde_json::from_slice(&result.payload).expect("result json");
        assert_eq!(result_json["ok"], false);
        assert_eq!(result_json["error_code"], "approval_denied");
        assert!(result_json.get("error").is_none());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(data_root);
    }

    /// План 08-4 acceptance: the third arm of "approval approve/reject/
    /// expiry". A retry that arrives after the approval window closed must
    /// be refused by `grant_approval`'s own deadline check (not by a new
    /// check invented here) and publish a typed `ApprovalDecision/Expired`
    /// ledger event before the error propagates. The deadline is forced
    /// into the past directly in `receipt_approval_intents` — waiting out
    /// the real 10-minute TTL is not a workable test.
    #[tokio::test]
    async fn expired_approval_publishes_ledger_decision_and_refuses_the_retry() {
        let root = std::env::temp_dir().join(format!(
            "evohime-ipc-terminal-expiry-root-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("terminal root");
        let data_root = std::env::temp_dir().join(format!(
            "evohime-ipc-terminal-expiry-data-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("terminal data root");
        let journal_path = data_root.join("events.db");
        let _ = std::fs::remove_file(&journal_path);
        let receipt_keys = ReceiptKeyManager::new(&data_root);
        receipt_keys.initialize().expect("receipt keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal.clone(),
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4().to_string();
        let make_terminal = |approval_id: String| generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "terminal-request".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::TerminalExecute(
                generated::TerminalExecute {
                    task_id: task_id.clone(),
                    workspace_path: root.display().to_string(),
                    program: "git".into(),
                    args: vec!["status".into()],
                    cwd: String::new(),
                    timeout_ms: 5_000,
                    approval_id,
                },
            )),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
            .await
            .expect("terminal writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("approval serves");
        let approval = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("approval reads")
                .as_slice(),
        )
        .expect("approval decodes");
        let approval_json =
            serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
        let approval_id = approval_json["approval_id"]
            .as_str()
            .expect("approval id")
            .to_string();

        // Force the approval window into the past — same-process retries
        // hit the monotonic-clock branch of `grant_approval`'s deadline
        // check, so backdating `deadline_monotonic_ms` is what actually
        // exercises it (backdating `expires_at_ms` alone would not, since
        // the boot id matches).
        {
            let database = journal.database().lock().await;
            let changed = database
                .connection()
                .execute(
                    "UPDATE receipt_approval_intents SET deadline_monotonic_ms = 0 WHERE approval_id = ?1",
                    [&approval_id],
                )
                .expect("deadline backdates");
            assert_eq!(changed, 1, "the approval intent row must exist");
        }

        transport::write_frame(
            &mut client,
            &make_terminal(approval_id.clone()).encode_to_vec(),
        )
        .await
        .expect("retry writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect_err("an expired approval must refuse the retry");

        let journal_handle = bridge.journal();
        let database = journal_handle.database().lock().await;
        let (decision_state, body_payload): (String, Vec<u8>) = database
            .connection()
            .query_row(
                "SELECT state_after, payload FROM events
                   WHERE event_type = 'ledger.approval_decision'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ledger.approval_decision row exists");
        assert_eq!(decision_state, "timed_out");
        let decision_event: execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&body_payload).expect("decision event decodes");
        let execution_ledger::ExecutionEventBody::ApprovalDecision {
            approval_intent_id,
            decision,
            ..
        } = decision_event.body
        else {
            panic!(
                "expected ApprovalDecision body, got {:?}",
                decision_event.body
            );
        };
        assert_eq!(approval_intent_id, approval_id);
        assert_eq!(decision, execution_ledger::ApprovalOutcome::Expired);
        drop(database);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(data_root);
    }

    /// План 08-4 acceptance: "action → tool call → observation → successful
    /// typed receipt linked to signed receipts_v1". A real terminal
    /// execution, approved and run through `dispatch_terminal_execute`, must
    /// leave a typed `ledger.tool_call` (Running) followed by a typed
    /// `ledger.tool_receipt` (Succeeded) under the same `action_id` — and
    /// that receipt event's `receipt_hash` must resolve to an actual signed
    /// row in `receipt_records`, not just a plausible-looking string.
    #[tokio::test]
    async fn approved_terminal_execute_links_ledger_receipt_to_signed_receipts_v1() {
        let root = std::env::temp_dir().join(format!(
            "evohime-ipc-terminal-linkage-root-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("terminal root");
        std::process::Command::new("git")
            .arg("init")
            .arg(&root)
            .output()
            .expect("git init runs");
        let data_root = std::env::temp_dir().join(format!(
            "evohime-ipc-terminal-linkage-data-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("terminal data root");
        let journal_path = data_root.join("events.db");
        let _ = std::fs::remove_file(&journal_path);
        let receipt_keys = ReceiptKeyManager::new(&data_root);
        receipt_keys.initialize().expect("receipt keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal.clone(),
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4().to_string();
        let make_terminal = |approval_id: String| generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "terminal-request".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::TerminalExecute(
                generated::TerminalExecute {
                    task_id: task_id.clone(),
                    workspace_path: root.display().to_string(),
                    program: "git".into(),
                    args: vec!["status".into()],
                    cwd: String::new(),
                    timeout_ms: 5_000,
                    approval_id,
                },
            )),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
            .await
            .expect("terminal writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("approval serves");
        let approval = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("approval reads")
                .as_slice(),
        )
        .expect("approval decodes");
        let approval_json =
            serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
        let approval_id = approval_json["approval_id"]
            .as_str()
            .expect("approval id")
            .to_string();

        let resolve = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resolve-terminal".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ResolveApproval(
                generated::ResolveApproval {
                    approval_id: approval_id.clone(),
                    granted: true,
                    idempotency_key: String::new(),
                    rejection_reason: String::new(),
                    cancel: false,
                },
            )),
        };
        transport::write_frame(&mut client, &resolve.encode_to_vec())
            .await
            .expect("resolve writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resolve serves");

        // План 08-4 acceptance: a granted approval publishes a typed
        // ApprovalDecision/Approved ledger event — the "approve" arm of
        // "approval approve/reject/expiry" — before the retried execution
        // publishes its own ToolCall/ToolReceipt pair below.
        {
            let database = journal.database().lock().await;
            let (decision_state, body_payload): (String, Vec<u8>) = database
                .connection()
                .query_row(
                    "SELECT state_after, payload FROM events
                       WHERE event_type = 'ledger.approval_decision'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("ledger.approval_decision row exists");
            assert_eq!(decision_state, "running");
            let decision_event: execution_ledger::ExecutionEventV1 =
                serde_json::from_slice(&body_payload).expect("decision event decodes");
            let execution_ledger::ExecutionEventBody::ApprovalDecision {
                approval_intent_id,
                decision,
                ..
            } = decision_event.body
            else {
                panic!(
                    "expected ApprovalDecision body, got {:?}",
                    decision_event.body
                );
            };
            assert_eq!(approval_intent_id, approval_id);
            assert_eq!(decision, execution_ledger::ApprovalOutcome::Approved);
        }

        transport::write_frame(&mut client, &make_terminal(approval_id).encode_to_vec())
            .await
            .expect("retry writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("retry serves");
        let result = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("result reads")
                .as_slice(),
        )
        .expect("result decodes");
        assert_eq!(result.event_type, "terminal.result");
        let result_json: serde_json::Value =
            serde_json::from_slice(&result.payload).expect("result json");
        assert_eq!(
            result_json["ok"], true,
            "git status in a real repo must succeed: {result_json}"
        );

        let database = journal.database().lock().await;
        let (tool_call_action_id, tool_call_state): (String, String) = database
            .connection()
            .query_row(
                "SELECT action_id, state_after FROM events WHERE event_type = 'ledger.tool_call'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ledger.tool_call row exists");
        assert_eq!(tool_call_state, "running");

        // The "observation" link of "action → tool call → observation →
        // receipt" — must exist under the same action_id, between the call
        // and the receipt.
        let (observation_action_id, observation_payload): (String, Vec<u8>) = database
            .connection()
            .query_row(
                "SELECT action_id, payload FROM events WHERE event_type = 'ledger.observation'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("ledger.observation row exists");
        assert_eq!(observation_action_id, tool_call_action_id);
        let observation_event: execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&observation_payload).expect("observation event decodes");
        assert!(matches!(
            observation_event.body,
            execution_ledger::ExecutionEventBody::Observation { .. }
        ));

        let (receipt_action_id, receipt_state, receipt_payload): (String, String, Vec<u8>) =
            database
                .connection()
                .query_row(
                    "SELECT action_id, state_after, payload FROM events
                       WHERE event_type = 'ledger.tool_receipt'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .expect("ledger.tool_receipt row exists");
        assert_eq!(receipt_state, "succeeded");
        assert_eq!(
            receipt_action_id, tool_call_action_id,
            "tool_call and tool_receipt must share the same action_id"
        );
        let receipt_event: execution_ledger::ExecutionEventV1 =
            serde_json::from_slice(&receipt_payload).expect("receipt event decodes");
        let execution_ledger::ExecutionEventBody::ToolReceipt {
            receipt_action_id: body_action_id,
            receipt_hash,
        } = receipt_event.body
        else {
            panic!("expected ToolReceipt body, got {:?}", receipt_event.body);
        };
        assert_eq!(body_action_id, receipt_action_id);

        // The linkage is only real if that hash resolves to an actual signed
        // row — not merely a string that looks like one.
        let signed_action_id: String = database
            .connection()
            .query_row(
                "SELECT action_id FROM receipt_records WHERE receipt_hash = ?1",
                [&receipt_hash],
                |row| row.get(0),
            )
            .expect("receipt_hash resolves to a real receipt_records row");
        assert_eq!(signed_action_id, receipt_action_id);
        drop(database);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(data_root);
    }

    #[tokio::test]
    async fn reconciliation_command_executes_only_new_read_only_action() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-reconcile-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("reconcile root");
        std::fs::write(root.join("observed.txt"), "observed state\n").expect("observed file");
        let data_root =
            std::env::temp_dir().join(format!("evohime-ipc-reconcile-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("reconcile data root");
        let journal_path = data_root.join("events.db");
        let keys = ReceiptKeyManager::new(&data_root);
        keys.initialize().expect("keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal.clone(),
            coordinator,
            ApprovalCoordinator::default(),
            Arc::new(ToolRegistry::bootstrap()),
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4();
        let old_action_id = uuid::Uuid::now_v7();
        {
            let mut database = journal.database().lock().await;
            let signer = crate::CoreReceiptSigner(Arc::new(keys));
            let mut runtime =
                evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                    .unwrap();
            let old_request = evohime_receipts::runtime::ActionRequest {
                action_id: old_action_id,
                task_id: task_id.to_string(),
                run_id: task_id.to_string(),
                tool_name: "shell.execute".into(),
                policy_id: "permission:ShellExecute".into(),
                normalized_scope: "workspace".into(),
                input: serde_json::json!({"program":"echo","args":[]}),
                policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                approval_id: None,
                parent_approval_ref: None,
                preview: "old mutation".into(),
            };
            runtime.prepare(old_request).unwrap();
            runtime.mark_started(old_action_id).unwrap();
            runtime.mark_returned(old_action_id).unwrap();
            runtime
                .mark_pending_recovery(old_action_id, "unknown")
                .unwrap();
        }
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "reconcile-read-only".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(
                generated::command_envelope::Command::ReconcilePendingReceiptAction(
                    generated::ReconcilePendingReceiptAction {
                        old_action_id: old_action_id.to_string(),
                        tool_name: "filesystem.read".into(),
                        input_json: r#"{"path":"observed.txt"}"#.into(),
                        workspace_path: root.display().to_string(),
                    },
                ),
            ),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .unwrap();
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .unwrap();
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client).await.unwrap().as_slice(),
        )
        .unwrap();
        assert_eq!(response.event_type, "receipt.reconciliation");
        let payload: serde_json::Value = serde_json::from_slice(&response.payload).unwrap();
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["status"], "succeeded");
        assert_ne!(payload["action_id"], old_action_id.to_string());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&data_root);
    }

    #[tokio::test]
    async fn serves_bounded_git_status_and_diff_through_core_tools() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-git-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("git root");
        let status = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&root)
            .status()
            .expect("git init starts");
        assert!(status.success());
        std::fs::write(root.join("notes.txt"), "hello\n").expect("notes");
        let status = std::process::Command::new("git")
            .args(["add", "notes.txt"])
            .current_dir(&root)
            .status()
            .expect("git add starts");
        assert!(status.success());
        std::fs::write(root.join("notes.txt"), "hello\nworld\n").expect("changed notes");
        let journal_path =
            std::env::temp_dir().join(format!("evohime-ipc-git-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&journal_path);
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal,
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );

        let status_payload = bridge
            .dispatch_git_read(
                root.display().to_string(),
                "git.status",
                serde_json::Value::Null,
                128,
            )
            .await
            .expect("git status reads");
        let status_json: serde_json::Value =
            serde_json::from_slice(&status_payload).expect("status json");
        assert!(status_json["output"]
            .as_str()
            .unwrap()
            .contains("notes.txt"));
        assert_eq!(status_json["truncated"], false);

        let diff_payload = bridge
            .dispatch_git_read(
                root.display().to_string(),
                "git.diff",
                serde_json::json!({"path": "notes.txt"}),
                8,
            )
            .await
            .expect("git diff reads");
        let diff_json: serde_json::Value =
            serde_json::from_slice(&diff_payload).expect("diff json");
        assert_eq!(diff_json["max_bytes"], 8);
        assert_eq!(diff_json["truncated"], true);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(journal_path);
    }

    #[tokio::test]
    async fn handshake_exposes_runtime_identity() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-handshake-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "handshake".into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 9,
            command: Some(generated::command_envelope::Command::Handshake(
                generated::Handshake {
                    protocol: Some(protocol()),
                    client_id: "client".into(),
                    session_id: "session".into(),
                    session_epoch: 9,
                    last_event_sequence: 0,
                    capabilities: vec!["task.crud".into()],
                    client_role: "shell".into(),
                    nonce: String::new(),
                    proof: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("handshake writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("handshake serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert!(!event.core_instance_id.is_empty());
        assert!(event.session_epoch > 0);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn malformed_command_is_rejected_without_crashing_bridge() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-malformed-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
        let (mut client, server) = duplex(1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &[0xff, 0x00, 0x01])
            .await
            .expect("malformed frame writes");
        assert!(matches!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await,
            Err(IpcBridgeError::Protobuf(_))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn reconnect_replays_only_events_after_last_sequence() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-reconnect-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let first = journal
            .record(&CoreEvent::TaskStarted {
                task_id: "task-reconnect".into(),
                prompt: "one".into(),
            })
            .await
            .expect("first event");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-reconnect".into(),
                final_message: "two".into(),
            })
            .await
            .expect("second event");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "reconnect".into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 0,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents {
                    after_sequence: first as u64,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("reconnect writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("reconnect serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("event reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "task.completed");
        assert_eq!(event.sequence_id, first as u64 + 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_task_crud_and_replays_deduplicated_create() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-task-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "create-project-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::CreateProject(
                generated::CreateProject {
                    project_id: "project-1".into(),
                    title: "Demo".into(),
                    workspace_path: "C:\\Projects\\demo".into(),
                    source_ref: "plan:0a".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("project creates");
        let first = transport::read_frame(&mut client)
            .await
            .expect("first response");

        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("duplicate writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("duplicate replays");
        let second = transport::read_frame(&mut client)
            .await
            .expect("second response");
        assert_eq!(first, second);

        let mut conflict = command.clone();
        if let Some(generated::command_envelope::Command::CreateProject(project)) =
            &mut conflict.command
        {
            project.title = "Different".into();
        }
        transport::write_frame(&mut client, &conflict.encode_to_vec())
            .await
            .expect("conflicting writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn imports_prd_without_touching_workspace_and_rejects_duplicate_import() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-prd-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server) = duplex(32 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let project = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "project-prd".into(),
            client_id: "prd-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::CreateProject(
                generated::CreateProject {
                    project_id: "project-prd".into(),
                    title: "PRD".into(),
                    workspace_path: "C:\\Projects\\prd".into(),
                    source_ref: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &project.encode_to_vec())
            .await
            .expect("project writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("project creates");
        let _ = transport::read_frame(&mut client)
            .await
            .expect("project response");

        let import = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "import-prd-1".into(),
            client_id: "prd-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ImportPrd(
                generated::ImportPrd {
                    import_id: "import-1".into(),
                    project_id: "project-prd".into(),
                    origin: "prd.md".into(),
                    version: "v1".into(),
                    source_text: "# Plan\n\n## Task\n- [ ] Pass\n".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &import.encode_to_vec())
            .await
            .expect("import writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("import succeeds");
        let response = transport::read_frame(&mut client)
            .await
            .expect("import response");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "prd.imported");
        assert_eq!(
            journal
                .list_task_graph("project-prd")
                .await
                .unwrap()
                .0
                .len(),
            1
        );

        let mut duplicate = import;
        if let Some(generated::command_envelope::Command::ImportPrd(request)) =
            &mut duplicate.command
        {
            request.source_text.push_str("\n## Another");
            duplicate.request_id = "import-prd-2".into();
        }
        transport::write_frame(&mut client, &duplicate.encode_to_vec())
            .await
            .expect("duplicate writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        assert_eq!(
            journal
                .list_task_graph("project-prd")
                .await
                .unwrap()
                .0
                .len(),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_run_doctor_with_real_storage_and_pipe_state() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-doctor-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "doctor-1".into(),
            client_id: "doctor-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RunDoctor(
                generated::RunDoctor {
                    project_id: String::new(),
                    detail_level: 1,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("doctor serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "doctor.report");
        let report: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("doctor report is valid json");
        assert_eq!(report["bounded"], serde_json::json!(true));
        let checks = report["checks"].as_array().expect("checks array");
        assert_eq!(checks.len(), 7);
        let storage_check = checks
            .iter()
            .find(|check| check["id"] == "storage")
            .expect("storage check present");
        // A freshly-opened journal exists, is writable, and is on the
        // current schema version, so this reflects real (not fabricated)
        // storage state.
        assert_eq!(storage_check["status"], serde_json::json!("OK"));
        let permissions_check = checks
            .iter()
            .find(|check| check["id"] == "permissions")
            .expect("permissions check present");
        // No project_id was supplied, so the permissions probe is honestly
        // fail-closed rather than fabricated as healthy.
        assert_ne!(permissions_check["status"], serde_json::json!("OK"));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn saves_and_lists_research_evidence_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-research-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let save = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "research-save-1".into(),
            client_id: "research-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SaveResearchEvidence(
                generated::SaveResearchEvidence {
                    work_item_id: "task-42".into(),
                    source_kind: "url".into(),
                    source_ref: "https://example.test/article".into(),
                    title: "Example Article".into(),
                    publisher: "Example Org".into(),
                    content_type: "text/html".into(),
                    raw_excerpt: "Useful finding sk-secret alice@example.test".into(),
                    retrieved_at_ms: 1_700_000_000_000,
                    ttl_ms: 3_600_000,
                },
            )),
        };
        transport::write_frame(&mut client, &save.encode_to_vec())
            .await
            .expect("save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("save serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("save response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.evidence.saved");
        let saved: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("save payload is valid json");
        assert_eq!(saved["work_item_id"], serde_json::json!("task-42"));
        let evidence_id = saved["id"].as_str().expect("id present").to_owned();
        assert_eq!(
            saved["evidence"]["excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );

        let list = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "research-list-1".into(),
            client_id: "research-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ListResearchEvidence(
                generated::ListResearchEvidence {
                    work_item_id: "task-42".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &list.encode_to_vec())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("list response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.evidence.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("list payload is valid json");
        let records = listed["records"].as_array().expect("records array");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id"], serde_json::json!(evidence_id));
        assert_eq!(records[0]["source_kind"], serde_json::json!("url"));
        assert_eq!(
            records[0]["redacted_excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );
        assert_eq!(records[0]["provenance_link"], serde_json::json!("task-42"));

        let _ = std::fs::remove_file(path);
    }

    fn run_research_fetch_command(
        work_item_id: &str,
        url: String,
        allowed_domains: Vec<String>,
        max_bytes: u64,
    ) -> generated::CommandEnvelope {
        generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: format!("research-fetch-{work_item_id}"),
            client_id: "research-fetch-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RunResearchFetch(
                generated::RunResearchFetch {
                    work_item_id: work_item_id.into(),
                    url,
                    title: "Example Article".into(),
                    allowed_domains,
                    max_bytes,
                    max_latency_ms: 5_000,
                    max_cost_micros: 0,
                    ttl_ms: 3_600_000,
                },
            )),
        }
    }

    #[tokio::test]
    async fn run_research_fetch_persists_real_evidence_from_a_live_http_get() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/article"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string("Useful finding sk-secret alice@example.test")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));
        let domain = reqwest::Url::parse(&server.uri())
            .expect("mock uri parses")
            .host_str()
            .expect("mock uri has host")
            .to_ascii_lowercase();

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-ok-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-ok",
            format!("{}/article", server.uri()),
            vec![domain],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("fetch serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("fetch response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.fetch.completed");
        let payload: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("fetch payload is valid json");
        assert_eq!(payload["state"], serde_json::json!("completed"));
        assert_eq!(
            payload["evidence"]["excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );
        let evidence_id = payload["id"].as_str().expect("id present").to_owned();

        let records = journal
            .list_research_evidence("task-fetch-ok")
            .await
            .expect("evidence lists from real storage");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, evidence_id);
        assert_eq!(
            records[0].redacted_excerpt,
            "Useful finding [REDACTED] [REDACTED]"
        );

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_denies_domain_outside_allowlist_and_persists_nothing() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/article"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("should not fetch"))
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-denied-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-denied",
            format!("{}/article", server.uri()),
            vec!["not-the-mock-domain.example".into()],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            outcome.is_err(),
            "domain-denied fetch must fail the command"
        );
        assert_eq!(
            server
                .received_requests()
                .await
                .expect("requests tracked")
                .len(),
            0,
            "no network call should happen for a denied domain"
        );

        let records = journal
            .list_research_evidence("task-fetch-denied")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_blocks_ssrf_targets_and_persists_nothing() {
        let _private = evohime_tool_runtime::lock_private_override(Some(false));

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-ssrf-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-ssrf",
            "http://127.0.0.1:9/".into(),
            vec!["127.0.0.1".into()],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(outcome.is_err(), "ssrf-blocked fetch must fail the command");

        let records = journal
            .list_research_evidence("task-fetch-ssrf")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_rejects_oversized_response_and_persists_nothing() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/big"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("x".repeat(4_096)))
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));
        let domain = reqwest::Url::parse(&server.uri())
            .expect("mock uri parses")
            .host_str()
            .expect("mock uri has host")
            .to_ascii_lowercase();

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-oversized-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-oversized",
            format!("{}/big", server.uri()),
            vec![domain],
            16,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(outcome.is_err(), "oversized response must fail the command");

        let records = journal
            .list_research_evidence("task-fetch-oversized")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn memory_create_list_search_archive_forget_round_trip_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-memory-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "memory-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Create two memories in the same task scope, one containing a
        // secret that must come back redacted.
        let create_one = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-create-1",
            generated::command_envelope::Command::CreateMemory(generated::CreateMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                title: "Rust build notes".into(),
                content: "Rust build cache lives in target/".into(),
                provenance_kind: "event".into(),
                provenance_id: "evt-1".into(),
                provenance_locator: String::new(),
                privacy: "internal".into(),
                ttl_ms: 3_600_000,
            }),
        )
        .await;
        assert_eq!(create_one.event_type, "memory.created");
        let created_one: serde_json::Value =
            serde_json::from_slice(&create_one.payload).expect("create payload is valid json");
        let memory_one_id = created_one["record"]["id"]
            .as_str()
            .expect("id present")
            .to_owned();

        let create_two = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-create-2",
            generated::command_envelope::Command::CreateMemory(generated::CreateMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                title: "Deployment secret".into(),
                content: "Token is sk-secret, keep it safe".into(),
                provenance_kind: "event".into(),
                provenance_id: "evt-2".into(),
                provenance_locator: String::new(),
                privacy: "internal".into(),
                ttl_ms: 3_600_000,
            }),
        )
        .await;
        assert_eq!(create_two.event_type, "memory.created");
        let created_two: serde_json::Value =
            serde_json::from_slice(&create_two.payload).expect("create payload is valid json");
        assert_eq!(
            created_two["record"]["content"],
            serde_json::json!("Token is [REDACTED] keep it safe")
        );
        let memory_two_id = created_two["record"]["id"]
            .as_str()
            .expect("id present")
            .to_owned();

        // List returns both, newest first.
        let list = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-1",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: false,
                limit: 10,
            }),
        )
        .await;
        assert_eq!(list.event_type, "memory.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&list.payload).expect("list payload is valid json");
        let records = listed["records"].as_array().expect("records array");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["id"], serde_json::json!(memory_two_id));
        assert_eq!(records[1]["id"], serde_json::json!(memory_one_id));
        assert_eq!(records[0]["project_id"], serde_json::json!("proj-1"));
        assert_eq!(records[0]["secondary_id"], serde_json::json!("task-1"));
        // ListMemory is metadata-only: no statement, no provenance body.
        for record in records {
            assert!(
                record.get("statement").is_none(),
                "list must not carry body"
            );
            assert!(
                record.get("provenance").is_none(),
                "list must not carry provenance body"
            );
            assert_eq!(record["confirmation_state"], serde_json::json!("confirmed"));
            assert_eq!(record["kind"], serde_json::json!("entity"));
        }

        // The body is reachable only through an explicit GetMemory.
        let fetched = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-get-1",
            generated::command_envelope::Command::GetMemory(generated::GetMemory {
                id: memory_one_id.clone(),
            }),
        )
        .await;
        assert_eq!(fetched.event_type, "memory.record");
        let body: serde_json::Value =
            serde_json::from_slice(&fetched.payload).expect("get payload is valid json");
        assert_eq!(body["record"]["body_redacted"], serde_json::json!(false));
        assert_eq!(
            body["record"]["statement"],
            serde_json::json!("Rust build cache lives in target/")
        );
        assert_eq!(
            body["supersession_chain"],
            serde_json::json!([memory_one_id])
        );

        // Search only matches the record with "rust" in it.
        let search = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-search-1",
            generated::command_envelope::Command::SearchMemory(generated::SearchMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                query: "rust".into(),
                limit: 10,
            }),
        )
        .await;
        assert_eq!(search.event_type, "memory.search");
        let searched: serde_json::Value =
            serde_json::from_slice(&search.payload).expect("search payload is valid json");
        let hits = searched["records"].as_array().expect("records array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["id"], serde_json::json!(memory_one_id));

        // Archive without an approval token is rejected.
        let archive_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-archive-denied".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ArchiveMemory(
                generated::ArchiveMemory {
                    id: memory_one_id.clone(),
                    approval_id: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &archive_envelope.encode_to_vec())
            .await
            .expect("archive request writes");
        let denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(denied.is_err(), "archive without approval must fail");

        // Archive with an approval token succeeds and is audited.
        let archive = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-archive-1",
            generated::command_envelope::Command::ArchiveMemory(generated::ArchiveMemory {
                id: memory_one_id.clone(),
                approval_id: "approval-1".into(),
            }),
        )
        .await;
        assert_eq!(archive.event_type, "memory.archived");
        let archived: serde_json::Value =
            serde_json::from_slice(&archive.payload).expect("archive payload is valid json");
        assert_eq!(archived["archived"], serde_json::json!(true));

        // Archived record is hidden from default listing.
        let list_after_archive = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-2",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: false,
                limit: 10,
            }),
        )
        .await;
        let listed_after: serde_json::Value = serde_json::from_slice(&list_after_archive.payload)
            .expect("list payload is valid json");
        let records_after = listed_after["records"].as_array().expect("records array");
        assert_eq!(records_after.len(), 1);
        assert_eq!(records_after[0]["id"], serde_json::json!(memory_two_id));

        // Forget with an approval token erases title/content.
        let forget = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-forget-1",
            generated::command_envelope::Command::ForgetMemory(generated::ForgetMemory {
                id: memory_two_id.clone(),
                approval_id: "approval-2".into(),
            }),
        )
        .await;
        assert_eq!(forget.event_type, "memory.forgotten");
        let forgotten: serde_json::Value =
            serde_json::from_slice(&forget.payload).expect("forget payload is valid json");
        assert_eq!(forgotten["forgotten"], serde_json::json!(true));

        let list_after_forget = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-3",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: true,
                limit: 10,
            }),
        )
        .await;
        let listed_final: serde_json::Value =
            serde_json::from_slice(&list_after_forget.payload).expect("list payload is valid json");
        let records_final = listed_final["records"].as_array().expect("records array");
        // Forgotten records are excluded even with include_archived=true.
        assert!(records_final
            .iter()
            .all(|record| record["id"] != serde_json::json!(memory_two_id)));
        assert_eq!(forgotten["forgotten"], serde_json::json!(true));
        assert!(
            forgotten["tombstone_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "forget must produce a tombstone id"
        );

        // A forgotten record still answers GetMemory, but only with metadata.
        let forgotten_body = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-get-forgotten",
            generated::command_envelope::Command::GetMemory(generated::GetMemory {
                id: memory_two_id.clone(),
            }),
        )
        .await;
        let forgotten_json: serde_json::Value =
            serde_json::from_slice(&forgotten_body.payload).expect("payload is valid json");
        assert_eq!(
            forgotten_json["record"]["body_redacted"],
            serde_json::json!(true)
        );
        assert!(forgotten_json["record"].get("statement").is_none());

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn memory_pending_conflict_confirm_reject_supersede_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-memory-pending-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "memory-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Seed the store directly: extraction candidates are produced by
        // Core's policy gate, not by an IPC caller, so the IPC surface only
        // has to prove that pending records can be reviewed and resolved.
        let seed = |id: &str, state: &str, statement: &str| {
            let mut record = evohime_local_storage::memory_store::MemoryRecord::new(
                id,
                evohime_local_storage::memory_store::MemoryScope::Project,
                "proj-1",
                "Язык интерфейса",
                statement,
                "{\"message_id\":\"msg-1\"}",
                evohime_local_storage::memory_store::MemoryPrivacy::Internal,
                "1000",
                Some("99999999999999".to_owned()),
            )
            .expect("record builds");
            record.extraction = evohime_local_storage::memory_store::MemoryExtractionFields {
                kind: "preference".to_owned(),
                canonical_subject: Some("язык интерфейса".to_owned()),
                confirmation_state: state.to_owned(),
                model_confidence: 0.9,
                verification_confidence: 0.0,
                extractor_version: "extractor-v1".to_owned(),
                policy_version: "extraction-policy-v1".to_owned(),
                ..Default::default()
            };
            record
        };
        journal
            .save_memory(&seed("active-1", "confirmed", "UI на русском языке"))
            .await
            .expect("active memory saves");
        journal
            .save_memory(&seed(
                "pending-1",
                "pending_confirmation",
                "UI на английском языке",
            ))
            .await
            .expect("pending memory saves");
        journal
            .save_memory(&seed(
                "pending-2",
                "pending_confirmation",
                "UI на русском языке",
            ))
            .await
            .expect("duplicate pending saves");

        let pending = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-pending-1",
            generated::command_envelope::Command::ListMemoryPending(generated::ListMemoryPending {
                scope_kind: "project".into(),
                project_id: "proj-1".into(),
                secondary_id: String::new(),
                limit: 10,
                workspace_path: String::new(),
            }),
        )
        .await;
        assert_eq!(pending.event_type, "memory.pending");
        let pending_json: serde_json::Value =
            serde_json::from_slice(&pending.payload).expect("pending payload is valid json");
        assert_eq!(
            pending_json["counts"]["pending_confirmation"],
            serde_json::json!(2)
        );
        assert_eq!(pending_json["counts"]["confirmed"], serde_json::json!(1));
        for record in pending_json["records"].as_array().expect("records array") {
            assert!(
                record.get("statement").is_none(),
                "queue must stay metadata-only"
            );
        }

        // Only the incompatible statement is a conflict; the duplicate is not.
        let conflicts = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-conflicts-1",
            generated::command_envelope::Command::GetMemoryConflicts(
                generated::GetMemoryConflicts {
                    scope_kind: "project".into(),
                    project_id: "proj-1".into(),
                    secondary_id: String::new(),
                    limit: 10,
                    workspace_path: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(conflicts.event_type, "memory.conflicts");
        let conflicts_json: serde_json::Value =
            serde_json::from_slice(&conflicts.payload).expect("conflicts payload is valid json");
        let conflict_list = conflicts_json["conflicts"].as_array().expect("conflicts");
        assert_eq!(conflict_list.len(), 1);
        assert_eq!(
            conflict_list[0]["pending"]["id"],
            serde_json::json!("pending-1")
        );
        assert_eq!(
            conflict_list[0]["active"]["id"],
            serde_json::json!("active-1")
        );

        // "Изменить": the user rewrites the statement before deciding. The
        // record becomes a user assertion but stays pending.
        let revised = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-revise-1",
            generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-1".into(),
                    statement: "UI строго на английском языке".into(),
                    session_only: false,
                    session_id: String::new(),
                    approval_id: "approval-revise".into(),
                    idempotency_key: "key-revise".into(),
                },
            ),
        )
        .await;
        assert_eq!(revised.event_type, "memory.revised");
        let revised_json: serde_json::Value =
            serde_json::from_slice(&revised.payload).expect("payload is valid json");
        assert_eq!(
            revised_json["record"]["confirmation_state"],
            serde_json::json!("pending_confirmation")
        );
        assert_eq!(
            revised_json["record"]["source_trust"],
            serde_json::json!("user")
        );
        assert_eq!(
            revised_json["record"]["extractor_version"],
            serde_json::json!("user_edited")
        );
        // Even the revision response stays metadata-only.
        assert!(revised_json["record"].get("statement").is_none());

        // "Только на эту сессию": no persistent memory survives.
        journal
            .save_memory(&seed(
                "pending-3",
                "pending_confirmation",
                "временное правило",
            ))
            .await
            .expect("third pending saves");
        let session_only = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-session-only-1",
            generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-3".into(),
                    statement: String::new(),
                    session_only: true,
                    session_id: "session-1".into(),
                    approval_id: "approval-session".into(),
                    idempotency_key: "key-session".into(),
                },
            ),
        )
        .await;
        let session_json: serde_json::Value =
            serde_json::from_slice(&session_only.payload).expect("payload is valid json");
        assert_eq!(session_json["session_only"], serde_json::json!(true));
        assert_eq!(session_json["state"], serde_json::json!("rejected"));
        let notes = journal
            .list_memory_session_notes("session-1", &0.to_string())
            .await
            .expect("session notes read");
        assert_eq!(notes.len(), 1, "the statement lives only as a session note");

        // A session-only note without a session id is refused.
        let no_session = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-session-only-bad".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-2".into(),
                    statement: String::new(),
                    session_only: true,
                    session_id: String::new(),
                    approval_id: "approval-session-2".into(),
                    idempotency_key: "key-session-2".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &no_session.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "a session-only note needs a session id"
        );

        // Confirm without approval is rejected.
        let denied_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-confirm-denied".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ConfirmMemory(
                generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: String::new(),
                    idempotency_key: "key-1".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &denied_envelope.encode_to_vec())
            .await
            .expect("denied request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "confirm without approval must fail"
        );

        // Confirm without an idempotency key is rejected too.
        let no_key = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-confirm-no-key".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ConfirmMemory(
                generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: "approval-1".into(),
                    idempotency_key: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &no_key.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "confirm without an idempotency key must fail"
        );

        // Approved confirm applies, and repeating it is safe.
        for request_id in ["memory-confirm-1", "memory-confirm-1-replay"] {
            let confirmed = send(
                &bridge,
                &mut client,
                &mut server_reader,
                &mut server_writer,
                request_id,
                generated::command_envelope::Command::ConfirmMemory(generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: "approval-1".into(),
                    idempotency_key: "key-1".into(),
                }),
            )
            .await;
            assert_eq!(confirmed.event_type, "memory.confirmed");
            let json: serde_json::Value =
                serde_json::from_slice(&confirmed.payload).expect("payload is valid json");
            assert_eq!(json["results"][0]["state"], serde_json::json!("confirmed"));
        }

        // Batch reject is terminal: a later confirm reports the real state.
        let rejected = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-reject-1",
            generated::command_envelope::Command::RejectMemory(generated::RejectMemory {
                ids: vec!["pending-2".into()],
                approval_id: "approval-2".into(),
                idempotency_key: "key-2".into(),
            }),
        )
        .await;
        assert_eq!(rejected.event_type, "memory.rejected");
        let rejected_json: serde_json::Value =
            serde_json::from_slice(&rejected.payload).expect("payload is valid json");
        assert_eq!(
            rejected_json["results"][0]["state"],
            serde_json::json!("rejected")
        );

        let reopen = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-confirm-2",
            generated::command_envelope::Command::ConfirmMemory(generated::ConfirmMemory {
                ids: vec!["pending-2".into()],
                approval_id: "approval-3".into(),
                idempotency_key: "key-3".into(),
            }),
        )
        .await;
        let reopen_json: serde_json::Value =
            serde_json::from_slice(&reopen.payload).expect("payload is valid json");
        assert_eq!(
            reopen_json["results"][0]["state"],
            serde_json::json!("rejected")
        );
        assert_eq!(
            reopen_json["results"][0]["applied"],
            serde_json::json!(false)
        );

        // The conflict is resolved only by an explicit supersede.
        let superseded = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-supersede-1",
            generated::command_envelope::Command::SupersedeMemory(generated::SupersedeMemory {
                old_id: "active-1".into(),
                new_id: "pending-1".into(),
                reason: "user_choice".into(),
                approval_id: "approval-4".into(),
                idempotency_key: "key-4".into(),
            }),
        )
        .await;
        assert_eq!(superseded.event_type, "memory.superseded");
        let superseded_json: serde_json::Value =
            serde_json::from_slice(&superseded.payload).expect("payload is valid json");
        assert_eq!(
            superseded_json["supersession_chain"],
            serde_json::json!(["active-1", "pending-1"])
        );

        // An unsupported reason is refused rather than stored as free text.
        let bad_reason = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-supersede-bad".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SupersedeMemory(
                generated::SupersedeMemory {
                    old_id: "pending-1".into(),
                    new_id: "pending-2".into(),
                    reason: "because".into(),
                    approval_id: "approval-5".into(),
                    idempotency_key: "key-5".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_reason.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "an unsupported supersession reason must fail"
        );

        // After resolution only the winning record is retrievable.
        let search = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-search-final",
            generated::command_envelope::Command::SearchMemory(generated::SearchMemory {
                scope_kind: "project".into(),
                project_id: "proj-1".into(),
                secondary_id: String::new(),
                query: "ui".into(),
                limit: 10,
            }),
        )
        .await;
        let search_json: serde_json::Value =
            serde_json::from_slice(&search.payload).expect("payload is valid json");
        let hits = search_json["records"].as_array().expect("records array");
        assert_eq!(
            hits.iter()
                .map(|hit| hit["id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            ["pending-1"]
        );

        let _ = std::fs::remove_file(path);
    }

    fn capability_manifest_json(name: &str, version: &str, risk_class: &str) -> String {
        let content_hash = "0123456789abcdef0123456789abcdef";
        let signature =
            crate::capability_registry::test_sign_with_trusted_key(name, version, content_hash);
        serde_json::json!({
            "name": name,
            "version": version,
            "content_hash": content_hash,
            "signature": signature,
            "signing_key_id": "evohime-dev-1",
            "roles": [{
                "name": "reviewer",
                "version": "1",
                "content_hash": "abcdef0123456789abcdef0123456789"
            }],
            "skills": [],
            "allowed_tools": ["filesystem.read", "git.diff"],
            "allowed_domains": ["docs.example.com"],
            "protected_paths": ["src"],
            "risk_class": risk_class,
            "install": {
                "source": "local_archive",
                "allow_install_scripts": false,
                "allow_update": true,
                "rollback_on_failure": true
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn capability_install_list_match_remove_round_trip_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-capability-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "capability-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // HTTPS installation requires a real URL and a trusted hash. A
        // request without those inputs must still be rejected before storage.
        let https_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-https".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("reviewer", "1.0.0", "medium"),
                    install_source: "https_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &https_envelope.encode_to_vec())
            .await
            .expect("https install request writes");
        let https_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            https_denied.is_err(),
            "https_archive install source must be rejected in this pass"
        );

        // Installing a manifest with a malformed content_hash must be
        // rejected via the real RegistryError::InvalidHash path.
        let bad_hash_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-bad-hash".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("bad-hash", "1.0.0", "medium")
                        .replace("0123456789abcdef0123456789abcdef", "not-a-hex-hash"),
                    install_source: "local_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_hash_envelope.encode_to_vec())
            .await
            .expect("bad hash install request writes");
        let bad_hash_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            bad_hash_denied.is_err(),
            "manifest with a malformed content_hash must be rejected"
        );

        // Installing a manifest with an invalid risk_class must be rejected
        // before it ever reaches storage.
        let bad_risk_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-bad-risk".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("bad-risk", "1.0.0", "extreme"),
                    install_source: "local_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_risk_envelope.encode_to_vec())
            .await
            .expect("bad risk install request writes");
        let bad_risk_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            bad_risk_denied.is_err(),
            "manifest with an invalid risk_class must be rejected"
        );

        // A valid local-archive manifest installs successfully.
        let install = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-install-1",
            generated::command_envelope::Command::InstallCapability(generated::InstallCapability {
                manifest_json: capability_manifest_json("reviewer", "1.0.0", "medium"),
                install_source: "local_archive".into(),
                source_path: "C:/archives/reviewer.zip".into(),
                expected_content_hash: String::new(),
            }),
        )
        .await;
        assert_eq!(install.event_type, "capability.installed");
        let installed: serde_json::Value =
            serde_json::from_slice(&install.payload).expect("install payload is valid json");
        assert_eq!(installed["manifest"]["name"], serde_json::json!("reviewer"));

        // List returns the installed manifest.
        let list = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-list-1",
            generated::command_envelope::Command::ListCapabilities(generated::ListCapabilities {
                limit: 10,
            }),
        )
        .await;
        assert_eq!(list.event_type, "capability.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&list.payload).expect("list payload is valid json");
        let manifests = listed["manifests"].as_array().expect("manifests array");
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0]["name"], serde_json::json!("reviewer"));

        // Match selects the installed manifest for a fitting query.
        let matched = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-match-1",
            generated::command_envelope::Command::MatchCapabilities(generated::MatchCapabilities {
                intent: "review reviewer".into(),
                required_tools: vec!["git.diff".into()],
                required_domains: vec!["docs.example.com".into()],
                requested_risk: "low".into(),
            }),
        )
        .await;
        assert_eq!(matched.event_type, "capability.match");
        let matches: serde_json::Value =
            serde_json::from_slice(&matched.payload).expect("match payload is valid json");
        let hits = matches["matches"].as_array().expect("matches array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["manifest_name"], serde_json::json!("reviewer"));

        // Remove deletes the manifest.
        let removed = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-remove-1",
            generated::command_envelope::Command::RemoveCapability(generated::RemoveCapability {
                id: "reviewer".into(),
            }),
        )
        .await;
        assert_eq!(removed.event_type, "capability.removed");
        let removed_payload: serde_json::Value =
            serde_json::from_slice(&removed.payload).expect("remove payload is valid json");
        assert_eq!(removed_payload["removed"], serde_json::json!(true));

        // Removing again is rejected: the manifest is already gone.
        let remove_again_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-remove-2".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RemoveCapability(
                generated::RemoveCapability {
                    id: "reviewer".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &remove_again_envelope.encode_to_vec())
            .await
            .expect("second remove request writes");
        let remove_again = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            remove_again.is_err(),
            "removing a manifest that no longer exists must fail"
        );

        let list_after_remove = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-list-2",
            generated::command_envelope::Command::ListCapabilities(generated::ListCapabilities {
                limit: 10,
            }),
        )
        .await;
        let listed_after: serde_json::Value =
            serde_json::from_slice(&list_after_remove.payload).expect("list payload is valid json");
        assert!(listed_after["manifests"]
            .as_array()
            .expect("manifests array")
            .is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn capability_selection_get_pin_replace_round_trip_against_real_storage() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-capability-selection-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "capability-selection-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Install two candidate manifests so replace() has a real
        // alternative to switch to.
        for name in ["reviewer", "planner"] {
            let install = send(
                &bridge,
                &mut client,
                &mut server_reader,
                &mut server_writer,
                &format!("capability-selection-install-{name}"),
                generated::command_envelope::Command::InstallCapability(
                    generated::InstallCapability {
                        manifest_json: capability_manifest_json(name, "1.0.0", "medium"),
                        install_source: "local_archive".into(),
                        source_path: format!("C:/archives/{name}.zip"),
                        expected_content_hash: String::new(),
                    },
                ),
            )
            .await;
            assert_eq!(install.event_type, "capability.installed");
        }

        let query_fields = || generated::GetCapabilitySelection {
            task_id: "task-1".into(),
            intent: "review reviewer".into(),
            required_tools: vec!["git.diff".into()],
            required_domains: vec!["docs.example.com".into()],
            requested_risk: "low".into(),
        };

        // First GetCapabilitySelection: no prior state, so the matcher's
        // top-scoring manifest is auto-selected and persisted.
        let selected = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-1",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        assert_eq!(selected.event_type, "capability.selection");
        let selected_json: serde_json::Value =
            serde_json::from_slice(&selected.payload).expect("selection payload is valid json");
        assert_eq!(
            selected_json["selection"]["manifest_name"],
            serde_json::json!("reviewer")
        );
        assert_eq!(selected_json["origin"], serde_json::json!("auto"));

        // Pinning persists origin=pinned for the same task_id.
        let pinned = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-pin-1",
            generated::command_envelope::Command::PinCapabilitySelection(
                generated::PinCapabilitySelection {
                    task_id: "task-1".into(),
                },
            ),
        )
        .await;
        assert_eq!(pinned.event_type, "capability.selection.pinned");
        let pinned_json: serde_json::Value =
            serde_json::from_slice(&pinned.payload).expect("pin payload is valid json");
        assert_eq!(pinned_json["origin"], serde_json::json!("pinned"));
        assert!(pinned_json["selection"]["pinned"].as_bool().unwrap());

        // A subsequent GetCapabilitySelection must not silently override the
        // pin, even though the matcher would still pick "reviewer" here --
        // the persisted origin stays "pinned".
        let reconciled = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-2",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        let reconciled_json: serde_json::Value =
            serde_json::from_slice(&reconciled.payload).expect("selection payload is valid json");
        assert_eq!(reconciled_json["origin"], serde_json::json!("pinned"));

        // Explicitly replacing switches the persisted selection to
        // "planner" and marks origin=replaced.
        let replaced = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-replace-1",
            generated::command_envelope::Command::ReplaceCapabilitySelection(
                generated::ReplaceCapabilitySelection {
                    task_id: "task-1".into(),
                    manifest_name: "planner".into(),
                    intent: "review reviewer".into(),
                    required_tools: vec!["git.diff".into()],
                    required_domains: vec!["docs.example.com".into()],
                    requested_risk: "low".into(),
                },
            ),
        )
        .await;
        assert_eq!(replaced.event_type, "capability.selection.replaced");
        let replaced_json: serde_json::Value =
            serde_json::from_slice(&replaced.payload).expect("replace payload is valid json");
        assert_eq!(
            replaced_json["selection"]["manifest_name"],
            serde_json::json!("planner")
        );
        assert_eq!(replaced_json["origin"], serde_json::json!("replaced"));

        // A fresh GetCapabilitySelection still returns the replaced choice
        // -- proving persistence survives a new request (simulated
        // reconnect), matching the store's own round-trip contract.
        let after_replace = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-3",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        let after_replace_json: serde_json::Value = serde_json::from_slice(&after_replace.payload)
            .expect("selection payload is valid json");
        assert_eq!(
            after_replace_json["selection"]["manifest_name"],
            serde_json::json!("planner")
        );
        assert_eq!(after_replace_json["origin"], serde_json::json!("replaced"));

        // Pinning for a task_id with no persisted selection must fail.
        let pin_missing_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-selection-pin-missing".into(),
            client_id: "capability-selection-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(
                generated::command_envelope::Command::PinCapabilitySelection(
                    generated::PinCapabilitySelection {
                        task_id: "task-never-selected".into(),
                    },
                ),
            ),
        };
        transport::write_frame(&mut client, &pin_missing_envelope.encode_to_vec())
            .await
            .expect("pin-missing request writes");
        let pin_missing = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            pin_missing.is_err(),
            "pinning a task with no persisted selection must fail"
        );

        let _ = std::fs::remove_file(path);
    }

    /// Proves the read-only child delegation boundary holds end-to-end
    /// through the real IPC command path, not just at the pure-function
    /// level (`child_runtime::ChildTaskRequest::validate` /
    /// `child_runtime::accept_report` unit tests): a request naming a
    /// non-read-only capability is rejected, a nested-child request is
    /// rejected, a report with secret-like content is rejected, and a
    /// valid read-only request plus matching valid report round-trips
    /// through save -> submit -> list successfully. This test does not
    /// spawn or execute any child agent; it only proves the
    /// request/report validation and persistence boundary.
    #[tokio::test]
    async fn child_handoff_request_report_security_boundary_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-child-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "child-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // (a) A request naming a non-read-only capability (workspace.write)
        // must be rejected end-to-end, not just by the pure function.
        let write_capability_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-request-write-capability".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-write".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.write".into()],
                    parent_is_child: false,
                },
            )),
        };
        transport::write_frame(&mut client, &write_capability_envelope.encode_to_vec())
            .await
            .expect("write-capability request writes");
        let write_capability_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            write_capability_denied.is_err(),
            "a request naming a non-read-only capability must be rejected"
        );

        // (b) A nested child (parent_is_child = true) must be rejected.
        let nested_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-request-nested".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-nested".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.read".into()],
                    parent_is_child: true,
                },
            )),
        };
        transport::write_frame(&mut client, &nested_envelope.encode_to_vec())
            .await
            .expect("nested request writes");
        let nested_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            nested_denied.is_err(),
            "a nested child request (parent_is_child = true) must be rejected"
        );

        // A valid read-only request submits successfully.
        let submitted = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-request-valid",
            generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-1".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.read".into(), "git.diff".into()],
                    parent_is_child: false,
                },
            ),
        )
        .await;
        assert_eq!(submitted.event_type, "child.request.submitted");
        let submitted_payload: serde_json::Value =
            serde_json::from_slice(&submitted.payload).expect("submit payload is valid json");
        assert_eq!(
            submitted_payload["request"]["child_task_id"],
            serde_json::json!("child-1")
        );

        // (c) A report containing secret-like content must be rejected,
        // even though it matches a valid, already-persisted request.
        let secret_report_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-report-secret".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildReport(
                generated::SubmitChildReport {
                    child_task_id: "child-1".into(),
                    status: "complete".into(),
                    summary: "api_key=do-not-leak".into(),
                    findings: vec!["module is bounded".into()],
                    sources: vec!["src/lib.rs:10".into()],
                    confidence_percent: 90,
                },
            )),
        };
        transport::write_frame(&mut client, &secret_report_envelope.encode_to_vec())
            .await
            .expect("secret report writes");
        let secret_report_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            secret_report_denied.is_err(),
            "a report containing secret-like content must be rejected"
        );

        // (d) A matching, valid report round-trips through
        // save -> submit -> list successfully.
        let accepted = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-report-valid",
            generated::command_envelope::Command::SubmitChildReport(generated::SubmitChildReport {
                child_task_id: "child-1".into(),
                status: "complete".into(),
                summary: "found one relevant module".into(),
                findings: vec!["module is bounded".into()],
                sources: vec!["src/lib.rs:10".into()],
                confidence_percent: 90,
            }),
        )
        .await;
        assert_eq!(accepted.event_type, "child.report.accepted");
        let accepted_payload: serde_json::Value =
            serde_json::from_slice(&accepted.payload).expect("report payload is valid json");
        assert_eq!(
            accepted_payload["report"]["child_task_id"],
            serde_json::json!("child-1")
        );

        // A separately requested handoff persists and lists back through
        // the real command path too.
        let handoff = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-handoff-valid",
            generated::command_envelope::Command::RequestChildHandoff(
                generated::RequestChildHandoff {
                    handoff_id: "handoff-1".into(),
                    task_id: "task-1".into(),
                    kind: "delegate".into(),
                    from_role: "coordinator".into(),
                    from_name: String::new(),
                    to_role: "researcher".into(),
                    to_name: String::new(),
                    purpose: "investigate module bounds".into(),
                    payload: std::collections::HashMap::new(),
                    sequence: 1,
                },
            ),
        )
        .await;
        assert_eq!(handoff.event_type, "child.handoff.requested");

        let listed_handoffs = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-handoff-list",
            generated::command_envelope::Command::ListChildHandoffs(generated::ListChildHandoffs {
                task_id: "task-1".into(),
                limit: 10,
            }),
        )
        .await;
        assert_eq!(listed_handoffs.event_type, "child.handoff.list");
        let listed_handoffs_payload: serde_json::Value =
            serde_json::from_slice(&listed_handoffs.payload)
                .expect("handoff list payload is valid json");
        let handoffs = listed_handoffs_payload["handoffs"]
            .as_array()
            .expect("handoffs array");
        assert_eq!(handoffs.len(), 1);
        assert_eq!(handoffs[0]["handoff_id"], serde_json::json!("handoff-1"));

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn list_verify_and_export_receipts_over_ipc() {
        let data_root =
            std::env::temp_dir().join(format!("evohime-ipc-receipts-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("data root");
        let journal_path = data_root.join("events.db");
        let keys = ReceiptKeyManager::new(&data_root);
        keys.initialize().expect("keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        {
            let mut database = journal.database().lock().await;
            let signer = crate::CoreReceiptSigner(Arc::new(ReceiptKeyManager::new(&data_root)));
            let mut runtime =
                evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                    .unwrap();
            let action_id = uuid::Uuid::now_v7();
            let request = evohime_receipts::runtime::ActionRequest {
                action_id,
                task_id: "receipts-task".into(),
                run_id: "receipts-run".into(),
                tool_name: "filesystem.read".into(),
                policy_id: "permission:FilesystemRead".into(),
                normalized_scope: "workspace".into(),
                input: serde_json::json!({"path":"a.txt"}),
                policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                approval_id: None,
                parent_approval_ref: None,
                preview: "read a.txt".into(),
            };
            runtime.prepare(request.clone()).unwrap();
            runtime.mark_started(action_id).unwrap();
            runtime
                .complete(&request, "succeeded", &"a".repeat(64), None)
                .unwrap();
        }
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(64 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "receipts-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        let listed = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "list-1",
            generated::command_envelope::Command::ListReceipts(generated::ListReceipts {
                task_id: "receipts-task".into(),
                ..Default::default()
            }),
        )
        .await;
        assert_eq!(listed.event_type, "receipts.listed");
        let listed_payload: serde_json::Value = serde_json::from_slice(&listed.payload).unwrap();
        assert_eq!(listed_payload["ok"], true);
        assert_eq!(listed_payload["rows"].as_array().unwrap().len(), 2);

        let verified = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "verify-1",
            generated::command_envelope::Command::VerifyReceipts(generated::VerifyReceipts {
                task_id: "receipts-task".into(),
                ..Default::default()
            }),
        )
        .await;
        assert_eq!(verified.event_type, "receipts.verified");
        let verified_payload: serde_json::Value =
            serde_json::from_slice(&verified.payload).unwrap();
        assert_eq!(verified_payload["ok"], true);
        assert_eq!(verified_payload["status"], "verified");
        assert_eq!(verified_payload["actual_verified_count"], 2);

        let destination = data_root.join("export-bundle");
        let exported = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "export-1",
            generated::command_envelope::Command::ExportReceipts(generated::ExportReceipts {
                destination_path: destination.display().to_string(),
                task_id: "receipts-task".into(),
                limit: 1000,
                ..Default::default()
            }),
        )
        .await;
        assert_eq!(exported.event_type, "receipts.exported");
        let exported_payload: serde_json::Value =
            serde_json::from_slice(&exported.payload).unwrap();
        assert_eq!(exported_payload["ok"], true, "{exported_payload:?}");
        assert_eq!(exported_payload["actual_exported_count"], 2);
        assert!(destination.join("manifest.json").exists());
        assert!(destination.join("receipts.jsonl").exists());

        let _ = std::fs::remove_dir_all(&data_root);
    }

    // ------------------------------------------------------------------
    // Постоянное слушание (план 04.5): девять команд и их коды ошибок.
    // ------------------------------------------------------------------

    /// Мост поверх временной базы и временного каталога данных.
    ///
    /// Каталог берётся полем, а не переменной окружения: подмена окружения
    /// сделала бы соседние тесты зависимыми от порядка запуска.
    fn ambient_bridge(name: &str) -> (IpcBridge, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal =
            EventJournal::open(directory.path().join(format!("{name}.db"))).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator)
            .with_ambient_data_dir(directory.path().to_path_buf());
        (bridge, directory)
    }

    async fn ambient_call(
        bridge: &IpcBridge,
        command: generated::command_envelope::Command,
    ) -> (String, serde_json::Value) {
        let (mut client, server) = duplex(256 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "ambient-request".into(),
            client_id: "ambient-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(command),
        };
        transport::write_frame(&mut client, &envelope.encode_to_vec())
            .await
            .expect("request writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("request serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        let payload = serde_json::from_slice(&event.payload).unwrap_or(serde_json::Value::Null);
        (event.event_type, payload)
    }

    async fn typed_checkpoint_call(
        bridge: &IpcBridge,
        command: generated::command_envelope::Command,
    ) -> generated::EventEnvelope {
        let (mut client, server) = duplex(256 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: uuid::Uuid::now_v7().to_string(),
            client_id: "checkpoint-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(command),
        };
        transport::write_frame(&mut client, &envelope.encode_to_vec())
            .await
            .expect("typed checkpoint request writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("typed checkpoint request serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("typed checkpoint response reads");
        generated::EventEnvelope::decode(response.as_slice()).expect("typed checkpoint decodes")
    }

    async fn typed_goal_call(
        bridge: &IpcBridge,
        request_id: &str,
        command: generated::command_envelope::Command,
    ) -> generated::EventEnvelope {
        let (mut client, server) = duplex(256 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: request_id.into(),
            client_id: "goal-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(command),
        };
        transport::write_frame(&mut client, &envelope.encode_to_vec())
            .await
            .expect("typed goal request writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("typed goal request serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("typed goal response reads");
        generated::EventEnvelope::decode(response.as_slice()).expect("typed goal decodes")
    }

    #[tokio::test]
    async fn persistent_goal_ipc_is_typed_bounded_and_recoverable() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-goal-{}.db", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let bridge = IpcBridge::new(journal);
        let workspace = std::env::temp_dir().join("evohime-goal-workspace");
        std::fs::create_dir_all(&workspace).expect("goal workspace creates");
        let create = generated::CreateGoal {
            goal_id: "goal-ipc-1".into(),
            workspace_path: workspace.to_string_lossy().into_owned(),
            chat_id: "chat-1".into(),
            objective: "Проверить typed Goal".into(),
            success_criteria: vec![generated::GoalCriterionInput {
                id: "criterion-1".into(),
                kind: "manual".into(),
                statement: "Core evidence сохранено".into(),
            }],
            idempotency_key: "goal-create-1".into(),
            ..Default::default()
        };
        let created = typed_goal_call(
            &bridge,
            "goal-create-request",
            generated::command_envelope::Command::CreateGoal(create.clone()),
        )
        .await;
        let created = match created.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed GoalAction, got {other:?}"),
        };
        assert!(
            created.applied,
            "create error={} message={}",
            created.error_code, created.error_message
        );
        assert_eq!(created.goal_version, 1);
        let projection = created.goal.expect("create carries projection");
        assert_eq!(projection.status, "active");
        assert_eq!(projection.remaining_criteria, vec!["criterion-1"]);
        assert!(!projection.workspace_id.contains("evohime-goal-workspace"));

        let replay = typed_goal_call(
            &bridge,
            "goal-create-request",
            generated::command_envelope::Command::CreateGoal(create),
        )
        .await;
        let replay = match replay.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed replay GoalAction, got {other:?}"),
        };
        assert!(replay.deduplicated);
        assert_eq!(replay.goal_version, 1);

        let listed = typed_goal_call(
            &bridge,
            "goal-list-request",
            generated::command_envelope::Command::ListGoals(generated::ListGoals {
                workspace_path: workspace.to_string_lossy().into_owned(),
                limit: 16,
            }),
        )
        .await;
        let listed = match listed.event {
            Some(generated::event_envelope::Event::GoalList(result)) => result,
            other => panic!("expected typed GoalList, got {other:?}"),
        };
        assert_eq!(listed.goals.len(), 1);
        assert_eq!(listed.goals[0].objective, "Проверить typed Goal");

        let fetched = typed_goal_call(
            &bridge,
            "goal-get-request",
            generated::command_envelope::Command::GetGoal(generated::GetGoal {
                goal_id: "goal-ipc-1".into(),
            }),
        )
        .await;
        let fetched = match fetched.event {
            Some(generated::event_envelope::Event::Goal(goal)) => goal,
            other => panic!("expected typed Goal projection, got {other:?}"),
        };
        assert_eq!(fetched.objective, "Проверить typed Goal");

        let updated = typed_goal_call(
            &bridge,
            "goal-update-request",
            generated::command_envelope::Command::UpdateGoal(generated::UpdateGoal {
                goal_id: "goal-ipc-1".into(),
                expected_version: 1,
                objective: "Проверить typed Goal и историю".into(),
                idempotency_key: "goal-update-key".into(),
                ..Default::default()
            }),
        )
        .await;
        let updated = match updated.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed update result, got {other:?}"),
        };
        assert!(updated.applied);
        assert_eq!(updated.goal_version, 2);

        let paused = typed_goal_call(
            &bridge,
            "goal-pause-request",
            generated::command_envelope::Command::PauseGoal(generated::GoalAction {
                goal_id: "goal-ipc-1".into(),
                expected_version: 2,
                idempotency_key: "goal-pause-key".into(),
            }),
        )
        .await;
        let paused = match paused.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed pause result, got {other:?}"),
        };
        assert_eq!(
            paused.goal.as_ref().map(|goal| goal.status.as_str()),
            Some("paused")
        );

        let resumed = typed_goal_call(
            &bridge,
            "goal-resume-request",
            generated::command_envelope::Command::ResumeGoal(generated::GoalAction {
                goal_id: "goal-ipc-1".into(),
                expected_version: 3,
                idempotency_key: "goal-resume-key".into(),
            }),
        )
        .await;
        let resumed = match resumed.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed resume result, got {other:?}"),
        };
        assert_eq!(
            resumed.goal.as_ref().map(|goal| goal.status.as_str()),
            Some("active")
        );

        let checkpoint = crate::task_checkpoint::TaskCheckpointRuntime::new(bridge.journal.clone())
            .capture(
                "goal-checkpoint-task",
                &workspace,
                crate::task_checkpoint::CheckpointStatus::Blocked,
                crate::task_checkpoint::CheckpointCaptureReason::RecoveryBlocked,
                None,
            )
            .await
            .expect("goal checkpoint persists");
        let linked = typed_goal_call(
            &bridge,
            "goal-link-checkpoint-request",
            generated::command_envelope::Command::LinkGoalReference(generated::LinkGoalReference {
                goal_id: "goal-ipc-1".into(),
                expected_version: 4,
                kind: "checkpoint".into(),
                reference_id: checkpoint.id,
                idempotency_key: "goal-link-checkpoint-key".into(),
            }),
        )
        .await;
        let linked = match linked.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed checkpoint link result, got {other:?}"),
        };
        assert!(linked.applied);
        assert_eq!(linked.goal_version, 5);

        let missing_link = typed_goal_call(
            &bridge,
            "goal-link-missing-request",
            generated::command_envelope::Command::LinkGoalReference(generated::LinkGoalReference {
                goal_id: "goal-ipc-1".into(),
                expected_version: 5,
                kind: "workflow".into(),
                reference_id: "missing-workflow".into(),
                idempotency_key: "goal-link-missing-key".into(),
            }),
        )
        .await;
        let missing_link = match missing_link.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed link result, got {other:?}"),
        };
        assert_eq!(missing_link.error_code, "reference_not_found");

        let stale = typed_goal_call(
            &bridge,
            "goal-pause-stale",
            generated::command_envelope::Command::PauseGoal(generated::GoalAction {
                goal_id: "goal-ipc-1".into(),
                expected_version: 99,
                idempotency_key: "goal-pause-stale-key".into(),
            }),
        )
        .await;
        let stale = match stale.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed stale GoalAction, got {other:?}"),
        };
        assert_eq!(stale.error_code, "stale_version");

        let verified = typed_goal_call(
            &bridge,
            "goal-verify-request",
            generated::command_envelope::Command::VerifyGoalCriterion(
                generated::VerifyGoalCriterion {
                    goal_id: "goal-ipc-1".into(),
                    expected_version: 5,
                    criterion_id: "criterion-1".into(),
                    idempotency_key: "goal-verify-key".into(),
                },
            ),
        )
        .await;
        let verified = match verified.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed verified GoalAction, got {other:?}"),
        };
        assert!(verified.applied);
        assert_eq!(
            verified.goal.as_ref().map(|goal| goal.status.as_str()),
            Some("completed")
        );
        let verified_criterion = &verified
            .goal
            .as_ref()
            .expect("verified projection")
            .success_criteria[0];
        assert_eq!(verified_criterion.provenance, "core");
        assert!(verified_criterion
            .evidence_ref
            .starts_with("core:user-decision:"));

        let cancelled = typed_goal_call(
            &bridge,
            "goal-cancel-completed",
            generated::command_envelope::Command::CancelGoal(generated::GoalAction {
                goal_id: "goal-ipc-1".into(),
                expected_version: 6,
                idempotency_key: "goal-cancel-completed-key".into(),
            }),
        )
        .await;
        let cancelled = match cancelled.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected typed cancel result, got {other:?}"),
        };
        assert_eq!(cancelled.error_code, "invalid_state_transition");

        let cancel_target = typed_goal_call(
            &bridge,
            "goal-create-cancel-target",
            generated::command_envelope::Command::CreateGoal(generated::CreateGoal {
                goal_id: "goal-ipc-cancel-target".into(),
                workspace_path: workspace.to_string_lossy().into_owned(),
                objective: "Отменяемая цель".into(),
                success_criteria: vec![generated::GoalCriterionInput {
                    id: "criterion-1".into(),
                    kind: "manual".into(),
                    statement: "Не требуется подтверждение".into(),
                }],
                idempotency_key: "goal-create-cancel-target-key".into(),
                ..Default::default()
            }),
        )
        .await;
        let cancel_target = match cancel_target.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected cancel target creation, got {other:?}"),
        };
        assert!(cancel_target.applied);
        let cancelled = typed_goal_call(
            &bridge,
            "goal-cancel-active",
            generated::command_envelope::Command::CancelGoal(generated::GoalAction {
                goal_id: "goal-ipc-cancel-target".into(),
                expected_version: 1,
                idempotency_key: "goal-cancel-active-key".into(),
            }),
        )
        .await;
        let cancelled = match cancelled.event {
            Some(generated::event_envelope::Event::GoalAction(result)) => result,
            other => panic!("expected successful cancel result, got {other:?}"),
        };
        assert_eq!(
            cancelled.goal.as_ref().map(|goal| goal.status.as_str()),
            Some("cancelled")
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[tokio::test]
    async fn task_checkpoint_ipc_is_typed_bounded_and_idempotent() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal =
            EventJournal::open(directory.path().join("checkpoint-ipc.db")).expect("journal opens");
        let runtime = crate::task_checkpoint::TaskCheckpointRuntime::new(journal.clone());
        let checkpoint = runtime
            .capture(
                "task-1",
                directory.path(),
                crate::task_checkpoint::CheckpointStatus::Blocked,
                crate::task_checkpoint::CheckpointCaptureReason::RecoveryBlocked,
                None,
            )
            .await
            .expect("checkpoint persists");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);

        let projection_event = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::GetTaskCheckpoint(generated::GetTaskCheckpoint {
                task_id: "task-1".into(),
                workspace_path: directory.path().to_string_lossy().into_owned(),
                max_replay_events: 64,
            }),
        )
        .await;
        assert!(projection_event.payload.is_empty());
        let Some(generated::event_envelope::Event::TaskCheckpoint(projection)) =
            projection_event.event
        else {
            panic!("expected typed checkpoint projection");
        };
        assert_eq!(projection.checkpoint_id, checkpoint.id);
        assert_eq!(projection.recovery_disposition, "blocked");
        assert!(projection
            .refs
            .iter()
            .all(|reference| reference.content_hash.len() <= 128));

        let action = generated::ResolveTaskCheckpoint {
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().into_owned(),
            checkpoint_id: checkpoint.id.clone(),
            expected_source_event_seq: checkpoint.source_event_seq,
            action: "acknowledge_recovery".into(),
            idempotency_key: "ack-1".into(),
        };
        let first_action = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::ResolveTaskCheckpoint(action.clone()),
        )
        .await;
        let Some(generated::event_envelope::Event::TaskCheckpointActionResult(first_result)) =
            first_action.event
        else {
            panic!("expected typed checkpoint action result");
        };
        assert!(first_result.applied);
        assert!(!first_result.deduplicated);
        assert!(first_action.payload.is_empty());

        let repeated_action = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::ResolveTaskCheckpoint(action),
        )
        .await;
        let Some(generated::event_envelope::Event::TaskCheckpointActionResult(repeated_result)) =
            repeated_action.event
        else {
            panic!("expected deduplicated checkpoint action result");
        };
        assert!(repeated_result.applied);
        assert!(repeated_result.deduplicated);
        let action_events = journal
            .task_history("task-1", 32)
            .await
            .expect("checkpoint history reads")
            .into_iter()
            .filter(|event| event.event_type == "task.checkpoint.action")
            .count();
        assert_eq!(action_events, 1);
    }

    #[tokio::test]
    async fn agent_skills_ipc_is_typed_metadata_first_and_non_durable() {
        let directory = tempfile::tempdir().expect("temp dir");
        let skill_dir = directory.path().join(".agents/skills/reviewer");
        std::fs::create_dir_all(skill_dir.join("references")).expect("skill dir creates");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: reviewer\ndescription: bounded review\nversion: 1.0.0\n---\nsecretly never persisted\n",
        )
        .expect("skill writes");
        std::fs::write(skill_dir.join("references/guide.md"), "bounded guide")
            .expect("reference writes");
        let journal =
            EventJournal::open(directory.path().join("skills-ipc.db")).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let workspace = directory.path().to_string_lossy().into_owned();

        let catalog_event = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::ListSkills(generated::ListSkills {
                workspace_path: workspace.clone(),
                limit: 10,
            }),
        )
        .await;
        assert!(catalog_event.payload.is_empty());
        let Some(generated::event_envelope::Event::SkillCatalog(catalog)) = catalog_event.event
        else {
            panic!("expected typed skill catalog");
        };
        assert_eq!(catalog.skills.len(), 1);
        assert_eq!(catalog.skills[0].skill_id, "reviewer");
        assert!(catalog.skills[0].content_hash.len() <= 128);

        let content_event = typed_checkpoint_call(
            &bridge,
            generated::command_envelope::Command::LoadSkill(generated::LoadSkill {
                workspace_path: workspace,
                skill_id: "reviewer".into(),
                max_bytes: 4096,
            }),
        )
        .await;
        let Some(generated::event_envelope::Event::SkillContent(content)) = content_event.event
        else {
            panic!("expected typed skill content");
        };
        assert_eq!(content.error_code, "");
        assert!(content.content.contains("secretly never persisted"));
        assert!(content_event.payload.is_empty());
        let history = journal
            .task_history("skill:reviewer", 16)
            .await
            .expect("skill trace reads");
        assert_eq!(history.len(), 1);
        assert!(!String::from_utf8_lossy(&history[0].payload).contains("secretly never persisted"));
    }

    #[tokio::test]
    async fn a_voice_command_card_appears_and_is_declined_without_launching_anything() {
        let (bridge, _directory) = ambient_bridge("ambient-voice");
        let policy = evohime_listener_contract::AmbientPolicy::default();
        let now_ms = crate::task_memory::now_millis();
        let decision = crate::voice_command::decide(
            &bridge.voice_commands(),
            &policy,
            "Ева, открой блокнот",
            now_ms,
            "voice-1".to_owned(),
        );
        let crate::voice_command::Decision::Confirm(command) = decision else {
            panic!("услышанное обязано ждать клика");
        };
        assert_eq!(command.app_id, "notepad");

        let (event_type, listed) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListVoiceCommands(generated::ListVoiceCommands {
                limit: 10,
            }),
        )
        .await;
        assert_eq!(event_type, "ambient.voice_commands");
        assert_eq!(listed["requires_confirmation"], true);
        assert_eq!(listed["commands"][0]["command_id"], "voice-1");
        assert_eq!(listed["commands"][0]["title"], "Блокнот");

        let (event_type, declined) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ResolveVoiceCommand(
                generated::ResolveVoiceCommand {
                    command_id: "voice-1".into(),
                    accepted: false,
                },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.voice_command_resolved");
        assert_eq!(declined["launched"], false);
        assert_eq!(declined["state"], "declined");

        // Второй клик по решённой карточке ничего не запускает: её больше нет.
        let (_, again) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ResolveVoiceCommand(
                generated::ResolveVoiceCommand {
                    command_id: "voice-1".into(),
                    accepted: true,
                },
            ),
        )
        .await;
        assert_eq!(again["launched"], false);
        assert_eq!(again["error_code"], "not_found");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn saving_a_policy_without_the_voice_fields_keeps_the_stored_value() {
        let (bridge, _directory) = ambient_bridge("ambient-voice-policy");
        let _control = attach_fake_listener(&bridge);
        let (_, saved) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: Vec::new(),
                    blocklist_patterns: Vec::new(),
                    retention_days: 7,
                    window_title_blocklist: Vec::new(),
                    voice_commands: Some(false),
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(saved["applied"], true);
        let (_, policy) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
        )
        .await;
        assert_eq!(policy["voice_commands"], false);
        assert_eq!(policy["voice_commands_autorun"], false);

        // Старый клиент не шлёт новых полей — и не выключает их своим молчанием.
        let (_, saved) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: Vec::new(),
                    blocklist_patterns: vec!["zoom*.exe".into()],
                    retention_days: 7,
                    window_title_blocklist: Vec::new(),
                    voice_commands: None,
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(saved["applied"], true);
        let (_, policy) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
        )
        .await;
        assert_eq!(policy["voice_commands"], false);
    }

    /// Подключает фиктивный листенер: команда уезжает в канал и остаётся там.
    fn attach_fake_listener(
        bridge: &IpcBridge,
    ) -> tokio::sync::mpsc::Receiver<crate::ambient::ListenerControl> {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let registry = bridge.ambient();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(registry.attach_control(tx))
        });
        rx
    }

    /// Без листенера включение не притворяется успехом: намерение сохранено,
    /// но состояние честно называется недоступным.
    #[tokio::test]
    async fn enabling_without_a_listener_reports_that_the_listener_is_missing() {
        let (bridge, directory) = ambient_bridge("ambient-no-listener");
        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.listening");
        assert_eq!(payload["error_code"], "LISTENER_UNAVAILABLE");
        assert_eq!(payload["state"], "engine_unavailable");
        // Намерение всё равно сохранено: следующее подключение листенера его
        // применит, а не начнёт с выключенного микрофона.
        assert!(crate::ambient::load_control(directory.path()).enabled);
    }

    /// Движок не готов — включение отвечает `ENGINE_NOT_READY`, а не молчит.
    #[tokio::test(flavor = "multi_thread")]
    async fn enabling_without_an_engine_reports_engine_not_ready() {
        let (bridge, _directory) = ambient_bridge("ambient-engine");
        let mut control = attach_fake_listener(&bridge);
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "ENGINE_NOT_READY");
        assert_eq!(payload["state"], "starting");
        assert!(matches!(
            control.try_recv(),
            Ok(crate::ambient::ListenerControl::Policy(_))
        ));
    }

    /// Занятое устройство называется своим кодом и не превращается в
    /// «запускаюсь».
    #[tokio::test(flavor = "multi_thread")]
    async fn a_busy_device_reports_a_conflict() {
        let (bridge, _directory) = ambient_bridge("ambient-conflict");
        let _control = attach_fake_listener(&bridge);
        bridge
            .ambient()
            .set_state(
                ListeningState::DeviceConflict,
                ListeningReason::DeviceConflict,
                None,
            )
            .await;
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "DEVICE_CONFLICT");
        assert_eq!(payload["state"], "device_conflict");
    }

    /// Неизвестное устройство не выбирается: подмена на умолчание означала бы
    /// слушать не тем микрофоном, который выбрал пользователь.
    #[tokio::test(flavor = "multi_thread")]
    async fn selecting_a_missing_device_is_refused() {
        let (bridge, _directory) = ambient_bridge("ambient-device");
        let _control = attach_fake_listener(&bridge);
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: "mic-that-left".into(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "DEVICE_DISCONNECTED");
    }

    /// Фраза в поле идентификатора устройства — это попытка протащить текст
    /// через метаданные, и она отбивается контрактом 04.1.
    #[tokio::test]
    async fn a_phrase_in_a_device_id_is_refused() {
        let (bridge, _directory) = ambient_bridge("ambient-device-id");
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: false,
                    device_id: "позвони маме завтра".into(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "INVALID_ARGUMENT");
    }

    /// Снимок статуса отвечает всегда: панель открывается, не дожидаясь
    /// события.
    #[tokio::test]
    async fn status_answers_before_any_event_arrives() {
        let (bridge, _directory) = ambient_bridge("ambient-status");
        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientStatus(generated::GetAmbientStatus {}),
        )
        .await;
        assert_eq!(event_type, "ambient.status");
        assert_eq!(payload["state"], "engine_unavailable");
        assert_eq!(payload["engine_ready"], false);
        assert!(payload["devices"].as_array().expect("devices").is_empty());
    }

    /// Список эпизодов не несёт текста; текст отдаётся только явным запросом
    /// одного эпизода.
    #[tokio::test]
    async fn text_is_absent_from_the_listing_and_present_only_on_demand() {
        let (bridge, _directory) = ambient_bridge("ambient-episodes");
        let journal = bridge.journal();
        journal
            .open_ambient_episode(
                "ep-1",
                "whisper-base-q5_1",
                "whisper-base-q5_1",
                evohime_listener_contract::ExtractionState::Disabled,
                1_700_000_000_000,
            )
            .await
            .expect("episode opens");
        journal
            .insert_ambient_utterance(
                &crate::ambient::AmbientUtteranceInput {
                    utterance_id: "ep-1-0".into(),
                    episode_id: "ep-1".into(),
                    sequence: 0,
                    started_at_ms: 1_700_000_000_000,
                    duration_ms: 1_200,
                    text: "надо купить хлеб".into(),
                    language: "ru".into(),
                    avg_logprob: -0.2,
                    redacted: false,
                },
                7,
                2_000,
            )
            .await
            .expect("utterance stored");

        let (event_type, listing) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListAmbientEpisodes(
                generated::ListAmbientEpisodes {
                    since_ms: 0,
                    limit: 10,
                    cursor: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.episodes");
        let serialized = listing.to_string();
        assert!(
            !serialized.contains("надо купить хлеб"),
            "listing leaked transcript text"
        );
        assert_eq!(listing["episodes"][0]["episode_id"], "ep-1");
        assert_eq!(listing["episodes"][0]["utterance_count"], 1);

        let (event_type, detail) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientEpisode(generated::GetAmbientEpisode {
                episode_id: "ep-1".into(),
            }),
        )
        .await;
        assert_eq!(event_type, "ambient.episode");
        assert_eq!(detail["utterances"][0]["text"], "надо купить хлеб");
    }

    /// Неподтверждённое удаление отвергается ядром, а не только модальным
    /// окном оболочки: обход UI не даёт больше прав.
    #[tokio::test]
    async fn deleting_without_confirmation_is_refused_by_core() {
        let (bridge, _directory) = ambient_bridge("ambient-delete");
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::DeleteAmbientTranscripts(
                generated::DeleteAmbientTranscripts {
                    episode_ids: vec!["ep-1".into()],
                    all: false,
                    confirmed: false,
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "CONFIRMATION_REQUIRED");
        assert_eq!(payload["deleted_count"], 0);

        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ForgetAmbientWindow(
                generated::ForgetAmbientWindow {
                    window_ms: 5 * 60 * 1000,
                    confirmed: false,
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "CONFIRMATION_REQUIRED");
    }

    /// Удаление действительно удаляет текст и вычищает ambient-строки
    /// журнала: событие об эпизоде не переживает сам эпизод.
    #[tokio::test]
    async fn deleting_removes_the_text_and_its_journal_rows() {
        let (bridge, _directory) = ambient_bridge("ambient-delete-real");
        let journal = bridge.journal();
        journal
            .open_ambient_episode(
                "ep-2",
                "whisper-base-q5_1",
                "whisper-base-q5_1",
                evohime_listener_contract::ExtractionState::Disabled,
                1_700_000_000_000,
            )
            .await
            .expect("episode opens");
        journal
            .insert_ambient_utterance(
                &crate::ambient::AmbientUtteranceInput {
                    utterance_id: "ep-2-0".into(),
                    episode_id: "ep-2".into(),
                    sequence: 0,
                    started_at_ms: 1_700_000_000_000,
                    duration_ms: 900,
                    text: "это надо забыть".into(),
                    language: "ru".into(),
                    avg_logprob: -0.1,
                    redacted: false,
                },
                7,
                2_000,
            )
            .await
            .expect("utterance stored");
        bridge
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Transcript {
                episode_id: evohime_listener_contract::EpisodeId::new("ep-2").unwrap(),
                started_at_ms: 1_700_000_000_000,
                utterance_count: 1,
                extraction_state: evohime_listener_contract::ExtractionState::Disabled,
            })
            .await
            .expect("transcript event published");

        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::DeleteAmbientTranscripts(
                generated::DeleteAmbientTranscripts {
                    episode_ids: vec!["ep-2".into()],
                    all: false,
                    confirmed: true,
                },
            ),
        )
        .await;
        assert_eq!(payload["deleted_count"], 1);
        assert!(journal
            .list_ambient_utterances("ep-2", 10)
            .await
            .expect("utterances read")
            .is_empty());
        let replay = journal
            .replay_bounded(0, 256)
            .await
            .expect("journal replays");
        assert!(
            !replay
                .events
                .iter()
                .any(|event| event.task_id == "ep-2" && event.event_type == "ambient.transcript"),
            "episode journal rows outlived the episode"
        );
    }

    /// Ни одно ambient-событие не несёт ни текста, ни его хеша.
    #[tokio::test(flavor = "multi_thread")]
    async fn ambient_events_never_carry_text_or_its_hash() {
        let (bridge, _directory) = ambient_bridge("ambient-events");
        let _control = attach_fake_listener(&bridge);
        let _ = ambient_call(
            &bridge,
            generated::command_envelope::Command::SetAmbientListening(
                generated::SetAmbientListening {
                    enabled: true,
                    paused: true,
                    device_id: String::new(),
                },
            ),
        )
        .await;
        let replay = bridge
            .journal()
            .replay_bounded(0, 256)
            .await
            .expect("journal replays");
        let ambient_rows: Vec<_> = replay
            .events
            .iter()
            .filter(|event| event.event_type.starts_with("ambient."))
            .collect();
        assert!(!ambient_rows.is_empty(), "no ambient event was published");
        for event in ambient_rows {
            let payload: serde_json::Value =
                serde_json::from_slice(&event.payload).expect("ambient payload is json");
            let object = payload.as_object().expect("ambient payload is an object");
            for forbidden in ["text", "text_hash", "transcript", "utterance"] {
                assert!(
                    !object.contains_key(forbidden),
                    "{} leaked {forbidden}",
                    event.event_type
                );
            }
        }
    }

    /// Политика применяется целиком или не применяется вовсе.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_invalid_policy_is_refused_whole() {
        let (bridge, directory) = ambient_bridge("ambient-policy");
        let mut control = attach_fake_listener(&bridge);

        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: vec![generated::QuietHours {
                        start_minute: 23 * 60,
                        end_minute: 7 * 60,
                    }],
                    blocklist_patterns: vec!["zoom*.exe".into()],
                    retention_days: 14,
                    window_title_blocklist: vec!["*банк*".into()],
                    voice_commands: None,
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(event_type, "ambient.policy_saved");
        assert_eq!(payload["applied"], true);
        assert!(matches!(
            control.try_recv(),
            Ok(crate::ambient::ListenerControl::Policy(_))
        ));

        let (_, refused) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: Vec::new(),
                    blocklist_patterns: vec!["^bank.*$".into()],
                    retention_days: 14,
                    window_title_blocklist: Vec::new(),
                    voice_commands: None,
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(refused["applied"], false);
        assert_eq!(refused["error_code"], "INVALID_ARGUMENT");

        let (_, over_retention) = ambient_call(
            &bridge,
            generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
                policy: Some(generated::AmbientPolicy {
                    quiet_hours: Vec::new(),
                    blocklist_patterns: Vec::new(),
                    retention_days: 365,
                    window_title_blocklist: Vec::new(),
                    voice_commands: None,
                    voice_commands_autorun: None,
                }),
            }),
        )
        .await;
        assert_eq!(over_retention["error_code"], "POLICY_INVALID");

        // Отвергнутая политика не затёрла сохранённую.
        let stored = crate::ambient::load_policy(directory.path());
        assert_eq!(stored.retention_days, 14);
        assert_eq!(stored.process_blocklist, vec!["zoom*.exe".to_string()]);

        let (event_type, read_back) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
        )
        .await;
        assert_eq!(event_type, "ambient.policy");
        assert_eq!(read_back["retention_days"], 14);
        assert_eq!(read_back["quiet_hours"][0]["start_minute"], 23 * 60);
    }

    /// Кладёт готовое предложение в базу моста.
    async fn seed_proposal(
        bridge: &IpcBridge,
        proposal_id: &str,
        kind: evohime_listener_contract::ProposalKind,
        subject: &str,
        episode_id: Option<&str>,
        now_ms: u64,
    ) {
        use crate::ambient_proactivity as proactivity;
        let subject_key = proactivity::subject_key(subject);
        let record = crate::ambient::proposal_record(
            proposal_id,
            &proactivity::proposal_key(kind, &subject_key, now_ms),
            &proactivity::mute_key(kind, &subject_key),
            kind,
            &subject_key,
            subject,
            "Напомнить купить хлеб",
            episode_id,
            now_ms,
        );
        bridge
            .journal()
            .record_ambient_proposal(&record)
            .await
            .expect("предложение записывается");
    }

    fn resolve_command(
        proposal_id: &str,
        accepted: bool,
        mute: bool,
        idempotency_key: &str,
    ) -> generated::command_envelope::Command {
        generated::command_envelope::Command::ResolveAmbientProposal(
            generated::ResolveAmbientProposal {
                proposal_id: proposal_id.into(),
                accepted,
                idempotency_key: idempotency_key.into(),
                mute,
            },
        )
    }

    /// Решения по несуществующему предложению не бывает: команда честно
    /// отвечает «не применено», а не выдумывает успех. Пустой ключ
    /// идемпотентности отвергается там же.
    #[tokio::test]
    async fn resolving_an_unknown_proposal_is_not_applied() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-unknown");
        let (event_type, payload) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
        assert_eq!(event_type, "ambient.proposal_resolved");
        assert_eq!(payload["applied"], false);
        assert_eq!(payload["error_code"], "INVALID_ARGUMENT");

        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Reminder,
            "хлеб",
            None,
            crate::task_memory::now_millis(),
        )
        .await;
        let (_, without_key) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "   ")).await;
        assert_eq!(
            without_key["applied"], false,
            "принятие без ключа идемпотентности не проходит"
        );
        assert_eq!(without_key["error_code"], "INVALID_ARGUMENT");
    }

    /// Повторный клик по карточке возвращает первое решение и не создаёт
    /// вторую задачу.
    #[tokio::test]
    async fn a_repeated_resolve_with_the_same_key_creates_no_second_task() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-idempotent");
        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Suggestion,
            "отчёт",
            None,
            crate::task_memory::now_millis(),
        )
        .await;
        let (_, first) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
        assert_eq!(first["applied"], true);
        assert_eq!(first["state"], "accepted");
        let task_id = first["task_id"]
            .as_str()
            .expect("задача создана")
            .to_owned();
        assert!(!task_id.is_empty());

        let (_, second) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
        assert_eq!(second["applied"], true, "повтор отвечает первым решением");
        assert_eq!(second["task_id"], task_id);

        let tasks = bridge
            .journal()
            .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
            .await
            .expect("задачи читаются");
        assert_eq!(tasks.len(), 1, "двойной клик не породил вторую задачу");
        assert_eq!(tasks[0].status, "backlog", "принятое не запускается само");
    }

    /// Принятое напоминание — неисполняемая запись: это записано в данных, а
    /// не подразумевается. Провенанс ведёт к эпизоду-источнику.
    #[tokio::test]
    async fn an_accepted_reminder_is_a_non_executable_row_with_provenance() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-reminder");
        let now_ms = crate::task_memory::now_millis();
        bridge
            .journal()
            .open_ambient_episode(
                "ep-1",
                "whisper-base-q5_1",
                "base-q5_1",
                evohime_listener_contract::ExtractionState::Done,
                now_ms,
            )
            .await
            .expect("эпизод открывается");
        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Reminder,
            "хлеб",
            Some("ep-1"),
            now_ms,
        )
        .await;
        let (_, payload) =
            ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
        assert_eq!(payload["applied"], true);
        let tasks = bridge
            .journal()
            .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
            .await
            .expect("задачи читаются");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].non_goals, AMBIENT_REMINDER_NON_GOAL);
        assert_eq!(tasks[0].source_ref.as_deref(), Some("ep-1"));
    }

    /// Отклонение задачу не создаёт, а mute переживает рестарт Core: он живёт
    /// строкой таблицы, а не полем реестра в памяти процесса.
    #[tokio::test]
    async fn muting_a_subject_survives_a_core_restart() {
        let directory = tempfile::tempdir().expect("temp dir");
        let database = directory.path().join("ambient-proposal-mute.db");
        let now_ms = crate::task_memory::now_millis();
        {
            let journal = EventJournal::open(&database).expect("journal opens");
            let (coordinator, _events) =
                TaskCoordinator::new_with_journal(8, None, journal.clone());
            let bridge = IpcBridge::with_coordinator(journal, coordinator)
                .with_ambient_data_dir(directory.path().to_path_buf());
            seed_proposal(
                &bridge,
                "prop-1",
                evohime_listener_contract::ProposalKind::Reminder,
                "хлеб",
                None,
                now_ms,
            )
            .await;
            let (_, payload) =
                ambient_call(&bridge, resolve_command("prop-1", false, true, "idem-1")).await;
            assert_eq!(payload["applied"], true);
            assert_eq!(payload["state"], "muted");
            assert_eq!(payload["task_id"], "", "заглушённое задач не создаёт");
            assert!(bridge
                .journal()
                .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
                .await
                .expect("задачи читаются")
                .is_empty());
        }
        // Новый процесс: реестр пуст, единственный источник истины — база.
        let journal = EventJournal::open(&database).expect("journal reopens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator)
            .with_ambient_data_dir(directory.path().to_path_buf());
        let subject_key = crate::ambient_proactivity::subject_key("хлеб");
        let mute_key = crate::ambient_proactivity::mute_key(
            evohime_listener_contract::ProposalKind::Reminder,
            &subject_key,
        );
        assert!(
            bridge.proactivity().is_muted(&journal, &mute_key).await,
            "mute обязан пережить рестарт"
        );
        // И он глушит предложение из другой временной корзины — то есть с
        // другим `proposal_key`.
        let later = crate::ambient::proposal_record(
            "prop-2",
            &crate::ambient_proactivity::proposal_key(
                evohime_listener_contract::ProposalKind::Reminder,
                &subject_key,
                now_ms + 5 * 60 * 60 * 1000,
            ),
            &mute_key,
            evohime_listener_contract::ProposalKind::Reminder,
            &subject_key,
            "хлеб",
            "Напомнить купить хлеб",
            None,
            now_ms + 5 * 60 * 60 * 1000,
        );
        assert_eq!(
            journal.record_ambient_proposal(&later).await,
            Ok(evohime_local_storage::ambient_store::ProposalInsert::Muted)
        );
    }

    /// Список карточек — единственный путь для человекочитаемого текста, и он
    /// не показывает просроченное как ждущее ответа.
    #[tokio::test]
    async fn the_proposal_list_carries_the_card_text_and_hides_expired_cards() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-list");
        let now_ms = crate::task_memory::now_millis();
        seed_proposal(
            &bridge,
            "prop-fresh",
            evohime_listener_contract::ProposalKind::Reminder,
            "хлеб",
            None,
            now_ms,
        )
        .await;
        seed_proposal(
            &bridge,
            "prop-stale",
            evohime_listener_contract::ProposalKind::Suggestion,
            "отчёт",
            None,
            now_ms - 2 * crate::ambient_proactivity::PROPOSAL_LIFETIME_MS,
        )
        .await;
        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListAmbientProposals(
                generated::ListAmbientProposals { limit: 50 },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.proposals");
        let rows = payload["proposals"].as_array().expect("список карточек");
        assert_eq!(rows.len(), 1, "просроченная карточка снята со списка");
        assert_eq!(rows[0]["proposal_id"], "prop-fresh");
        assert_eq!(rows[0]["title"], "Напомнить купить хлеб");
        assert_eq!(payload["max_per_hour"], 3);
        assert_eq!(payload["max_per_day"], 10);
        assert_eq!(payload["min_interval_ms"], 600_000);
    }

    /// Ни при каких входных данных `ambient.proposal` в журнале не несёт ни
    /// текста карточки, ни темы человеческими словами.
    #[tokio::test]
    async fn the_journalled_proposal_event_carries_no_card_text() {
        let (bridge, _directory) = ambient_bridge("ambient-proposal-privacy");
        let now_ms = crate::task_memory::now_millis();
        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Reminder,
            "секретный пароль от банка",
            None,
            now_ms,
        )
        .await;
        let (_, payload) =
            ambient_call(&bridge, resolve_command("prop-1", false, false, "idem-1")).await;
        assert_eq!(payload["applied"], true);
        assert_eq!(payload["state"], "declined");

        let journal = bridge.journal();
        let database = journal.database().lock().await;
        let events = database.read_events_after(0, 100).expect("журнал читается");
        let proposal_events: Vec<_> = events
            .into_iter()
            .filter(|event| event.event_type == "ambient.proposal")
            .collect();
        assert_eq!(proposal_events.len(), 1);
        for event in proposal_events {
            let body = String::from_utf8(event.payload).expect("payload is JSON");
            assert!(!body.contains("секретный"), "{body} несёт тему словами");
            assert!(
                !body.contains("Напомнить купить хлеб"),
                "{body} несёт текст карточки"
            );
            let value: serde_json::Value = serde_json::from_str(&body).expect("payload parses");
            for key in value.as_object().expect("object").keys() {
                assert!(
                    !matches!(
                        key.as_str(),
                        "title" | "subject" | "canonical_subject" | "text"
                    ),
                    "ambient.proposal раскрывает {key}"
                );
            }
        }
    }

    /// «Забыть последние 5 минут» удаляет то, что попало в окно, и оставляет
    /// то, что в него не попало.
    #[tokio::test]
    async fn forgetting_a_window_removes_only_that_window() {
        let (bridge, _directory) = ambient_bridge("ambient-forget");
        let journal = bridge.journal();
        let now_ms = crate::task_memory::now_millis();
        journal
            .open_ambient_episode(
                "ep-3",
                "whisper-base-q5_1",
                "whisper-base-q5_1",
                evohime_listener_contract::ExtractionState::Disabled,
                now_ms - 60 * 60 * 1000,
            )
            .await
            .expect("episode opens");
        for (sequence, offset_ms) in [(0i64, 60 * 60 * 1000u64), (1, 60 * 1000)] {
            journal
                .insert_ambient_utterance(
                    &crate::ambient::AmbientUtteranceInput {
                        utterance_id: format!("ep-3-{sequence}"),
                        episode_id: "ep-3".into(),
                        sequence,
                        started_at_ms: now_ms - offset_ms,
                        duration_ms: 800,
                        text: format!("фраза {sequence}"),
                        language: "ru".into(),
                        avg_logprob: -0.1,
                        redacted: false,
                    },
                    7,
                    2_000,
                )
                .await
                .expect("utterance stored");
        }

        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ForgetAmbientWindow(
                generated::ForgetAmbientWindow {
                    window_ms: 5 * 60 * 1000,
                    confirmed: true,
                },
            ),
        )
        .await;
        assert_eq!(event_type, "ambient.forgotten");
        assert_eq!(payload["deleted_count"], 1);
        let left = journal
            .list_ambient_utterances("ep-3", 10)
            .await
            .expect("utterances read");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].sequence, 0);
    }

    // ------------------------------------------------------------------
    // Workflow orchestration (план 06.3).
    // ------------------------------------------------------------------

    fn workflow_bridge(name: &str) -> (IpcBridge, tempfile::TempDir) {
        ambient_bridge(name)
    }

    /// Каталог отдаёт версии, входы и пригодность к расписанию, но не граф
    /// целиком: renderer не должен получать материал для собственного
    /// планирования.
    #[tokio::test]
    async fn the_template_catalog_is_bounded_and_versioned() {
        let (bridge, _directory) = workflow_bridge("workflow-templates");
        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListWorkflowTemplates(
                generated::ListWorkflowTemplates {},
            ),
        )
        .await;
        assert_eq!(event_type, "workflow.templates");
        let templates = payload["templates"].as_array().expect("список шаблонов");
        assert_eq!(templates.len(), 3);
        let ids: Vec<&str> = templates
            .iter()
            .map(|item| item["template_id"].as_str().unwrap_or_default())
            .collect();
        assert!(ids.contains(&"repository-research"));
        assert!(ids.contains(&"plan-implement-review"));
        assert!(ids.contains(&"parallel-security-review"));
        for template in templates {
            assert!(template["version"].as_u64().unwrap_or_default() >= 1);
            assert!(!template["schedule_eligibility"]
                .as_str()
                .unwrap_or_default()
                .is_empty());
            assert!(template.get("graph").is_none(), "граф целиком не уходит");
        }
        let approval_bearing = templates
            .iter()
            .find(|item| item["template_id"] == "plan-implement-review")
            .expect("шаблон с подтверждением");
        assert_eq!(approval_bearing["schedule_eligibility"], "unavailable");
    }

    /// Неизвестный шаблон получает typed-код, а не пустой успешный ответ.
    #[tokio::test]
    async fn an_unknown_template_definition_is_named_not_faked() {
        let (bridge, _directory) = workflow_bridge("workflow-definition");
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetWorkflowDefinition(
                generated::GetWorkflowDefinition {
                    template_id: "does-not-exist".into(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "unknown_template");
        assert!(payload["nodes"].as_array().expect("узлы").is_empty());

        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetWorkflowDefinition(
                generated::GetWorkflowDefinition {
                    template_id: "parallel-security-review".into(),
                },
            ),
        )
        .await;
        assert_eq!(payload["error_code"], "");
        assert_eq!(payload["nodes"].as_array().expect("узлы").len(), 4);
        assert_eq!(payload["graph_hash"].as_str().unwrap_or_default().len(), 64);
    }

    /// Пропущенный обязательный вход не запускает граф.
    #[tokio::test]
    async fn a_template_input_contract_violation_never_starts_a_run() {
        let (bridge, directory) = workflow_bridge("workflow-start-invalid");
        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
                template_id: "repository-research".into(),
                task_id: "task-1".into(),
                workspace_path: directory.path().to_string_lossy().to_string(),
                inputs: vec![],
                idempotency_key: "key-1".into(),
            }),
        )
        .await;
        assert_eq!(event_type, "workflow.started");
        assert_eq!(payload["error_code"], "missing_input");
        assert_eq!(payload["run_id"], "");
        assert!(bridge
            .journal()
            .list_workflow_runs(10)
            .await
            .expect("список запусков")
            .is_empty());
    }

    /// Один и тот же ключ идемпотентности возвращает первый запуск.
    #[tokio::test]
    async fn the_same_idempotency_key_returns_the_first_run() {
        let (bridge, directory) = workflow_bridge("workflow-idempotency");
        let command = || {
            generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
                template_id: "parallel-security-review".into(),
                task_id: "task-1".into(),
                workspace_path: directory.path().to_string_lossy().to_string(),
                inputs: vec![generated::WorkflowInput {
                    name: "scope".into(),
                    value: "crates/evohime-core".into(),
                }],
                idempotency_key: "key-1".into(),
            })
        };
        let (_, first) = ambient_call(&bridge, command()).await;
        assert_eq!(first["error_code"], "");
        let run_id = first["run_id"].as_str().expect("идентификатор").to_string();
        assert!(!run_id.is_empty());
        assert_eq!(first["deduplicated"], false);

        let (_, second) = ambient_call(&bridge, command()).await;
        assert_eq!(second["run_id"], run_id);
        assert_eq!(second["deduplicated"], true);
        assert_eq!(
            bridge
                .journal()
                .list_workflow_runs(10)
                .await
                .expect("список запусков")
                .len(),
            1
        );
    }

    /// Проекция запуска несёт состояния и роли, но не цель child, не prompt и
    /// не сырой вывод.
    #[tokio::test]
    async fn a_run_projection_carries_no_prompt_goal_or_raw_output() {
        let (bridge, directory) = workflow_bridge("workflow-projection");
        let (_, started) = ambient_call(
            &bridge,
            generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
                template_id: "repository-research".into(),
                task_id: "task-1".into(),
                workspace_path: directory.path().to_string_lossy().to_string(),
                inputs: vec![generated::WorkflowInput {
                    name: "question".into(),
                    value: "секретная формулировка вопроса".into(),
                }],
                idempotency_key: "key-1".into(),
            }),
        )
        .await;
        let run_id = started["run_id"]
            .as_str()
            .expect("идентификатор")
            .to_string();

        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetWorkflowRun(generated::GetWorkflowRun {
                run_id: run_id.clone(),
            }),
        )
        .await;
        assert_eq!(event_type, "workflow.run");
        assert_eq!(payload["error_code"], "");
        assert_eq!(payload["run_id"], run_id);
        let rendered = payload.to_string();
        assert!(
            !rendered.contains("секретная формулировка вопроса"),
            "цель узла не должна доходить до renderer: {rendered}"
        );
        let nodes = payload["nodes"].as_array().expect("узлы");
        assert_eq!(nodes.len(), 4);
        for node in nodes {
            assert!(node.get("node_id").is_some());
            assert!(node.get("state").is_some());
            assert!(node.get("output").is_none(), "сырой вывод наружу не уходит");
        }
    }

    /// Неизвестный запуск даёт `unknown_state`, а не выдуманный успех.
    #[tokio::test]
    async fn an_unknown_run_is_reported_as_unknown_state() {
        let (bridge, _directory) = workflow_bridge("workflow-unknown-run");
        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::GetWorkflowRun(generated::GetWorkflowRun {
                run_id: "missing".into(),
            }),
        )
        .await;
        assert_eq!(payload["error_code"], "unknown_run");
        assert_eq!(payload["state"], "unknown_state");

        let (_, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::CancelWorkflow(generated::CancelWorkflow {
                run_id: "missing".into(),
            }),
        )
        .await;
        assert_eq!(payload["cancelled"], false);
        assert_eq!(payload["error_code"], "not_cancellable");
    }

    /// События запуска durable, монотонны и доступны для replay с любой точки.
    #[tokio::test]
    async fn run_events_replay_from_any_sequence() {
        let (bridge, directory) = workflow_bridge("workflow-events");
        let (_, started) = ambient_call(
            &bridge,
            generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
                template_id: "parallel-security-review".into(),
                task_id: "task-1".into(),
                workspace_path: directory.path().to_string_lossy().to_string(),
                inputs: vec![generated::WorkflowInput {
                    name: "scope".into(),
                    value: "crates".into(),
                }],
                idempotency_key: "key-1".into(),
            }),
        )
        .await;
        let run_id = started["run_id"]
            .as_str()
            .expect("идентификатор")
            .to_string();

        let (event_type, payload) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListWorkflowEvents(
                generated::ListWorkflowEvents {
                    run_id: run_id.clone(),
                    after_sequence: -1,
                    limit: 100,
                },
            ),
        )
        .await;
        assert_eq!(event_type, "workflow.events");
        let events = payload["events"].as_array().expect("события");
        assert!(!events.is_empty());
        assert_eq!(events[0]["event_type"], "workflow.run_started");
        let sequences: Vec<i64> = events
            .iter()
            .map(|event| event["sequence"].as_i64().unwrap_or_default())
            .collect();
        let mut sorted = sequences.clone();
        sorted.sort();
        assert_eq!(sequences, sorted);

        let (_, tail) = ambient_call(
            &bridge,
            generated::command_envelope::Command::ListWorkflowEvents(
                generated::ListWorkflowEvents {
                    run_id,
                    after_sequence: 0,
                    limit: 100,
                },
            ),
        )
        .await;
        let tail_events = tail["events"].as_array().expect("хвост");
        assert!(tail_events
            .iter()
            .all(|event| event["sequence"].as_i64().unwrap_or_default() > 0));
    }

    #[tokio::test]
    async fn analysis_kernel_ipc_is_bounded_idempotent_and_version_checked() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal =
            EventJournal::open(directory.path().join("kernel-ipc.db")).expect("journal opens");
        let bridge = IpcBridge::new(journal);
        let created = bridge
            .dispatch_create_analysis_kernel(generated::CreateAnalysisKernel {
                task_id: "task-kernel-ipc".into(),
                workspace_id: "workspace-kernel-ipc".into(),
                runtime_version: "trusted-local-1".into(),
                package_manifest_hash: "a".repeat(64),
                policy_hash: "b".repeat(64),
                ..Default::default()
            })
            .await;
        assert_eq!(created.status, "running");
        assert_eq!(created.revision, 1);

        let put = generated::ExecuteAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            request_id: "object-put-request".into(),
            operation: "object_put".into(),
            args: br#"{"logical_name":"rows","type_hint":"json","value":[1,2,3],"sensitivity":"internal"}"#.to_vec(),
            correlation_id: "object-put-correlation".into(),
            idempotency_key: "object-put-idem".into(),
            ..Default::default()
        };
        let result = bridge.dispatch_execute_analysis_kernel(put.clone()).await;
        assert_eq!(result.status, "ok", "error={}", result.error_class);
        assert!(result.inline_result.is_empty());
        let object = result.object_ref.expect("metadata object ref");
        assert_eq!(object.logical_name, "rows");
        assert!(object.artifact_locator.is_empty());
        let duplicate = bridge.dispatch_execute_analysis_kernel(put).await;
        assert_eq!(duplicate.error_class, "duplicate_request");

        let denied = bridge
            .dispatch_execute_analysis_kernel(generated::ExecuteAnalysisKernel {
                kernel_id: created.kernel_id.clone(),
                request_id: "artifact-read-request".into(),
                operation: "artifact_read".into(),
                args: br#"{"locator":"artifact://missing"}"#.to_vec(),
                correlation_id: "artifact-read-correlation".into(),
                idempotency_key: "artifact-read-idem".into(),
                ..Default::default()
            })
            .await;
        assert_eq!(denied.error_class, "forbidden_capability");

        let stale = bridge
            .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
                kernel_id: created.kernel_id.clone(),
                expected_revision: 0,
                idempotency_key: "reset-idem".into(),
            })
            .await;
        assert_eq!(stale.error_class, "stale_revision");
        let still_running = bridge
            .dispatch_get_analysis_kernel(generated::GetAnalysisKernel {
                kernel_id: created.kernel_id.clone(),
                ..Default::default()
            })
            .await;
        assert_eq!(still_running.status, "running");
        assert_eq!(still_running.object_count, 1);

        let reset = bridge
            .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
                kernel_id: created.kernel_id.clone(),
                expected_revision: 1,
                idempotency_key: "reset-idem".into(),
            })
            .await;
        assert_eq!(reset.status, "reset");
        let duplicate_reset = bridge
            .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
                kernel_id: created.kernel_id,
                expected_revision: 1,
                idempotency_key: "reset-idem".into(),
            })
            .await;
        assert_eq!(duplicate_reset.error_class, "duplicate_request");
    }
}
