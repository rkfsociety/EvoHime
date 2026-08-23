//! Versioned execution ledger contract (план 08-1).
//!
//! Этот модуль — чистый typed contract-слой: он не трогает SQLite и не
//! ставит схему. Он фиксирует машинно проверяемый формат `ExecutionEventV1`
//! и словарь состояний, которым должны пользоваться storage-слой (08-2) и
//! IPC-проекция (08-3), вместо того чтобы каждый слой изобретал свой формат.
//!
//! Состояния действий (`ActionState`) переиспользуют имена
//! [`crate::workflow_store::NodeState`] один в один (плюс новый нетерминальный
//! `Cancelling`, которого пока нет в CHECK-ограничении `workflow_run_nodes` —
//! он появится миграцией в 08-2). Это намеренно: план 08-1 запрещает вводить
//! синонимы словаря состояний, которым уже пользуется workflow runtime.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Потолки полей. Общие для storage (08-2) и IPC/Electron (08-3) — так что
/// значение, отклонённое здесь, не может тихо проскочить дальше по слоям.
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_DIGEST_BYTES: usize = 128;
pub const MAX_ERROR_CLASS_BYTES: usize = 256;
pub const MAX_SHORT_TEXT_BYTES: usize = 256;
pub const MAX_ARTIFACT_REFS: usize = 32;

/// `event_id` детерминированного legacy-mapping — hex SHA-256, всегда 64 байта.
pub const LEGACY_EVENT_ID_HEX_LEN: usize = 64;

const LEGACY_EVENT_ID_DOMAIN: &[u8] = b"evohime.legacy.event.v1";

#[derive(Debug, thiserror::Error)]
pub enum LedgerContractError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("{field} is not valid for run_scope {run_scope:?}")]
    ScopeMismatch {
        field: &'static str,
        run_scope: RunScope,
    },
    #[error("session_id is required outside system/legacy run_scope")]
    MissingSessionId,
    #[error("illegal state transition from {from:?} to {to:?}")]
    IllegalTransition { from: ActionState, to: ActionState },
    #[error("action {action_id} has more than one terminal outcome")]
    DuplicateTerminalOutcome { action_id: String },
    #[error("too many artifact refs: {0} exceeds {MAX_ARTIFACT_REFS}")]
    TooManyArtifactRefs(usize),
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl PartialEq for LedgerContractError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty { field: left }, Self::Empty { field: right }) => left == right,
            (
                Self::Limit {
                    field: left,
                    max: left_max,
                },
                Self::Limit {
                    field: right,
                    max: right_max,
                },
            ) => left == right && left_max == right_max,
            (
                Self::ScopeMismatch {
                    field: left,
                    run_scope: left_scope,
                },
                Self::ScopeMismatch {
                    field: right,
                    run_scope: right_scope,
                },
            ) => left == right && left_scope == right_scope,
            (Self::MissingSessionId, Self::MissingSessionId) => true,
            (
                Self::IllegalTransition {
                    from: left_from,
                    to: left_to,
                },
                Self::IllegalTransition {
                    from: right_from,
                    to: right_to,
                },
            ) => left_from == right_from && left_to == right_to,
            (
                Self::DuplicateTerminalOutcome { action_id: left },
                Self::DuplicateTerminalOutcome { action_id: right },
            ) => left == right,
            (Self::TooManyArtifactRefs(left), Self::TooManyArtifactRefs(right)) => left == right,
            (Self::Sqlite(_), Self::Sqlite(_)) => true,
            _ => false,
        }
    }
}

/// Владелец `run_id`. Пара `(run_scope, run_id)` однозначно указывает на
/// существующего владельца — `runs.id` (`work_item`) или
/// `workflow_runs.run_id` (`workflow`), а не на новое пространство имён.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunScope {
    Workflow,
    WorkItem,
    Standalone,
    System,
    Legacy,
}

impl RunScope {
    pub fn as_str(self) -> &'static str {
        match self {
            RunScope::Workflow => "workflow",
            RunScope::WorkItem => "work_item",
            RunScope::Standalone => "standalone",
            RunScope::System => "system",
            RunScope::Legacy => "legacy",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "workflow" => RunScope::Workflow,
            "work_item" => RunScope::WorkItem,
            "standalone" => RunScope::Standalone,
            "system" => RunScope::System,
            "legacy" => RunScope::Legacy,
            _ => return None,
        })
    }
}

/// Action-level состояние. Терминальность и словарь имён совпадают с
/// [`crate::workflow_store::NodeState`]; `Cancelling` — единственное
/// расширение, добавленное этим планом (нетерминальное, для in-flight
/// отмены до её терминального исхода).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionState {
    Pending,
    Ready,
    Running,
    WaitingApproval,
    Cancelling,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Blocked,
    Denied,
    Skipped,
    Degraded,
    UnknownOutcome,
    DeadLetter,
}

impl ActionState {
    pub const ALL: &'static [ActionState] = &[
        ActionState::Pending,
        ActionState::Ready,
        ActionState::Running,
        ActionState::WaitingApproval,
        ActionState::Cancelling,
        ActionState::Succeeded,
        ActionState::Failed,
        ActionState::TimedOut,
        ActionState::Cancelled,
        ActionState::Blocked,
        ActionState::Denied,
        ActionState::Skipped,
        ActionState::Degraded,
        ActionState::UnknownOutcome,
        ActionState::DeadLetter,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ActionState::Pending => "pending",
            ActionState::Ready => "ready",
            ActionState::Running => "running",
            ActionState::WaitingApproval => "waiting_approval",
            ActionState::Cancelling => "cancelling",
            ActionState::Succeeded => "succeeded",
            ActionState::Failed => "failed",
            ActionState::TimedOut => "timed_out",
            ActionState::Cancelled => "cancelled",
            ActionState::Blocked => "blocked",
            ActionState::Denied => "denied",
            ActionState::Skipped => "skipped",
            ActionState::Degraded => "degraded",
            ActionState::UnknownOutcome => "unknown_outcome",
            ActionState::DeadLetter => "dead_letter",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => ActionState::Pending,
            "ready" => ActionState::Ready,
            "running" => ActionState::Running,
            "waiting_approval" => ActionState::WaitingApproval,
            "cancelling" => ActionState::Cancelling,
            "succeeded" => ActionState::Succeeded,
            "failed" => ActionState::Failed,
            "timed_out" => ActionState::TimedOut,
            "cancelled" => ActionState::Cancelled,
            "blocked" => ActionState::Blocked,
            "denied" => ActionState::Denied,
            "skipped" => ActionState::Skipped,
            "degraded" => ActionState::Degraded,
            "unknown_outcome" => ActionState::UnknownOutcome,
            "dead_letter" => ActionState::DeadLetter,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        !matches!(
            self,
            ActionState::Pending
                | ActionState::Ready
                | ActionState::Running
                | ActionState::WaitingApproval
                | ActionState::Cancelling
        )
    }

    /// Допустимые следующие состояния. Терминальные состояния не имеют
    /// исходящих переходов — терминальный action нельзя представить
    /// одновременно с двумя исходами.
    pub fn allowed_transitions(self) -> &'static [ActionState] {
        use ActionState::*;
        match self {
            Pending => &[Ready, Cancelling, Cancelled, Blocked, Denied, Skipped],
            Ready => &[Running, Cancelling, Cancelled, Blocked, Denied, Skipped],
            Running => &[
                WaitingApproval,
                Cancelling,
                Succeeded,
                Failed,
                TimedOut,
                Cancelled,
                Degraded,
                UnknownOutcome,
            ],
            WaitingApproval => &[Running, Cancelling, Denied, Cancelled, TimedOut],
            Cancelling => &[Cancelled, Succeeded, Failed, UnknownOutcome],
            Succeeded | Failed | TimedOut | Cancelled | Blocked | Denied | Skipped | Degraded
            | UnknownOutcome | DeadLetter => &[],
        }
    }

    pub fn can_transition_to(self, to: ActionState) -> bool {
        self.allowed_transitions().contains(&to)
    }
}

impl From<crate::workflow_store::NodeState> for ActionState {
    fn from(value: crate::workflow_store::NodeState) -> Self {
        use crate::workflow_store::NodeState as N;
        match value {
            N::Pending => ActionState::Pending,
            N::Ready => ActionState::Ready,
            N::Running => ActionState::Running,
            N::WaitingApproval => ActionState::WaitingApproval,
            N::Succeeded => ActionState::Succeeded,
            N::Failed => ActionState::Failed,
            N::TimedOut => ActionState::TimedOut,
            N::Cancelled => ActionState::Cancelled,
            N::Blocked => ActionState::Blocked,
            N::Denied => ActionState::Denied,
            N::Skipped => ActionState::Skipped,
            N::Degraded => ActionState::Degraded,
            N::UnknownOutcome => ActionState::UnknownOutcome,
            N::DeadLetter => ActionState::DeadLetter,
        }
    }
}

impl TryFrom<ActionState> for crate::workflow_store::NodeState {
    type Error = LedgerContractError;

    fn try_from(value: ActionState) -> Result<Self, Self::Error> {
        use crate::workflow_store::NodeState as N;
        Ok(match value {
            ActionState::Pending => N::Pending,
            ActionState::Ready => N::Ready,
            ActionState::Running => N::Running,
            ActionState::WaitingApproval => N::WaitingApproval,
            ActionState::Succeeded => N::Succeeded,
            ActionState::Failed => N::Failed,
            ActionState::TimedOut => N::TimedOut,
            ActionState::Cancelled => N::Cancelled,
            ActionState::Blocked => N::Blocked,
            ActionState::Denied => N::Denied,
            ActionState::Skipped => N::Skipped,
            ActionState::Degraded => N::Degraded,
            ActionState::UnknownOutcome => N::UnknownOutcome,
            ActionState::DeadLetter => N::DeadLetter,
            ActionState::Cancelling => {
                return Err(LedgerContractError::ScopeMismatch {
                    field: "state_after",
                    run_scope: RunScope::Workflow,
                })
            }
        })
    }
}

/// Явный mapping action-level состояния в run-level `RunState`. Ledger не
/// смешивает эти два множества — только фиксирует направленное соответствие.
pub fn action_state_to_run_state(state: ActionState) -> crate::workflow_store::RunState {
    use crate::workflow_store::RunState as R;
    match state {
        ActionState::Pending | ActionState::Ready => R::Pending,
        ActionState::Running | ActionState::Cancelling => R::Running,
        ActionState::WaitingApproval => R::WaitingApproval,
        ActionState::Succeeded => R::Completed,
        ActionState::Failed | ActionState::TimedOut | ActionState::Denied => R::Failed,
        ActionState::Cancelled => R::Cancelled,
        ActionState::Blocked | ActionState::Skipped | ActionState::Degraded => R::Degraded,
        ActionState::UnknownOutcome | ActionState::DeadLetter => R::Interrupted,
    }
}

fn bounded(field: &'static str, value: &str, max: usize) -> Result<(), LedgerContractError> {
    if value.len() > max {
        return Err(LedgerContractError::Limit { field, max });
    }
    Ok(())
}

fn bounded_opt(
    field: &'static str,
    value: &Option<String>,
    max: usize,
) -> Result<(), LedgerContractError> {
    if let Some(value) = value {
        bounded(field, value, max)?;
    }
    Ok(())
}

/// Content-addressed ссылка на артефакт. Никакого пути или содержимого —
/// только хэш и род артефакта.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub content_hash: String,
    pub kind: String,
}

impl ArtifactRef {
    fn validate(&self) -> Result<(), LedgerContractError> {
        bounded(
            "artifact_ref.content_hash",
            &self.content_hash,
            MAX_DIGEST_BYTES,
        )?;
        bounded("artifact_ref.kind", &self.kind, MAX_SHORT_TEXT_BYTES)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    Approved,
    Rejected,
    Expired,
}

/// Redaction-метаданные. Хранят только присутствие/классификацию секрета и
/// keyed digest, пригодный для сравнения, но не для offline dictionary
/// lookup. Само значение секрета в типе физически отсутствует.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionMeta {
    pub secrets_present: bool,
    pub digest: Option<String>,
}

impl RedactionMeta {
    fn validate(&self) -> Result<(), LedgerContractError> {
        bounded_opt("redaction.digest", &self.digest, MAX_DIGEST_BYTES)
    }
}

/// Typed variants из плана 08-1 §3. Каждое поле — bounded, никаких raw
/// prompt/headers/tokens/credentials/raw tool output: их здесь физически
/// нет, а не «удалены редактором» на выходе.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionEventBody {
    ActionRequest {
        action_kind: String,
        requested_capability: String,
    },
    ToolCall {
        tool_name: String,
        tool_call_hash: String,
        /// Пусто до появления `tool/manifest/v1` (план 07-1).
        manifest_hash: Option<String>,
    },
    Observation {
        summary_digest: String,
        artifact_refs: Vec<ArtifactRef>,
    },
    ToolReceipt {
        receipt_action_id: String,
        receipt_hash: String,
    },
    TypedFailure {
        error_class: String,
        provider_error_id: Option<String>,
    },
    ApprovalDecision {
        approval_intent_id: String,
        decision: ApprovalOutcome,
        /// Пусто до capability snapshot (план 09-1).
        snapshot_hash: Option<String>,
    },
    Cancellation {
        reason_class: String,
    },
    RecoveryDecision {
        decision: String,
        evidence_digest: String,
    },
}

impl ExecutionEventBody {
    /// Стабильная строка варианта для колонки `events.event_type` — так
    /// storage-слой (08-2) остаётся читаемым тем же generic-путём, что и
    /// legacy-события, без повторной десериализации `payload`.
    pub fn kind_str(&self) -> &'static str {
        match self {
            ExecutionEventBody::ActionRequest { .. } => "ledger.action_request",
            ExecutionEventBody::ToolCall { .. } => "ledger.tool_call",
            ExecutionEventBody::Observation { .. } => "ledger.observation",
            ExecutionEventBody::ToolReceipt { .. } => "ledger.tool_receipt",
            ExecutionEventBody::TypedFailure { .. } => "ledger.typed_failure",
            ExecutionEventBody::ApprovalDecision { .. } => "ledger.approval_decision",
            ExecutionEventBody::Cancellation { .. } => "ledger.cancellation",
            ExecutionEventBody::RecoveryDecision { .. } => "ledger.recovery_decision",
        }
    }

    fn validate(&self) -> Result<(), LedgerContractError> {
        match self {
            ExecutionEventBody::ActionRequest {
                action_kind,
                requested_capability,
            } => {
                bounded("body.action_kind", action_kind, MAX_SHORT_TEXT_BYTES)?;
                bounded(
                    "body.requested_capability",
                    requested_capability,
                    MAX_SHORT_TEXT_BYTES,
                )?;
            }
            ExecutionEventBody::ToolCall {
                tool_name,
                tool_call_hash,
                manifest_hash,
            } => {
                bounded("body.tool_name", tool_name, MAX_ID_BYTES)?;
                bounded("body.tool_call_hash", tool_call_hash, MAX_DIGEST_BYTES)?;
                bounded_opt("body.manifest_hash", manifest_hash, MAX_DIGEST_BYTES)?;
            }
            ExecutionEventBody::Observation {
                summary_digest,
                artifact_refs,
            } => {
                bounded("body.summary_digest", summary_digest, MAX_DIGEST_BYTES)?;
                if artifact_refs.len() > MAX_ARTIFACT_REFS {
                    return Err(LedgerContractError::TooManyArtifactRefs(
                        artifact_refs.len(),
                    ));
                }
                for artifact_ref in artifact_refs {
                    artifact_ref.validate()?;
                }
            }
            ExecutionEventBody::ToolReceipt {
                receipt_action_id,
                receipt_hash,
            } => {
                bounded("body.receipt_action_id", receipt_action_id, MAX_ID_BYTES)?;
                bounded("body.receipt_hash", receipt_hash, MAX_DIGEST_BYTES)?;
            }
            ExecutionEventBody::TypedFailure {
                error_class,
                provider_error_id,
            } => {
                bounded("body.error_class", error_class, MAX_ERROR_CLASS_BYTES)?;
                bounded_opt("body.provider_error_id", provider_error_id, MAX_ID_BYTES)?;
            }
            ExecutionEventBody::ApprovalDecision {
                approval_intent_id,
                decision: _,
                snapshot_hash,
            } => {
                bounded("body.approval_intent_id", approval_intent_id, MAX_ID_BYTES)?;
                bounded_opt("body.snapshot_hash", snapshot_hash, MAX_DIGEST_BYTES)?;
            }
            ExecutionEventBody::Cancellation { reason_class } => {
                bounded("body.reason_class", reason_class, MAX_ERROR_CLASS_BYTES)?;
            }
            ExecutionEventBody::RecoveryDecision {
                decision,
                evidence_digest,
            } => {
                bounded("body.decision", decision, MAX_SHORT_TEXT_BYTES)?;
                bounded("body.evidence_digest", evidence_digest, MAX_DIGEST_BYTES)?;
            }
        }
        Ok(())
    }
}

/// Typed execution event — канонический versioned контракт плана 08-1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionEventV1 {
    pub schema_version: u32,
    pub event_id: String,
    /// Заполняется storage-слоем (08-2) при публикации; здесь просто поле.
    pub sequence_id: Option<i64>,
    pub run_scope: RunScope,
    pub run_id: String,
    /// Логическая execution session (не transport `session_epoch`).
    /// Обязателен вне `System`/`Legacy` scope.
    pub session_id: Option<String>,
    pub task_id: String,
    pub created_at_ms: i64,
    pub state_after: Option<ActionState>,
    pub action_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub observation_id: Option<String>,
    pub receipt_id: Option<String>,
    pub failure_id: Option<String>,
    pub workflow_run_id: Option<String>,
    pub node_id: Option<String>,
    pub attempt_id: Option<String>,
    pub effect_id: Option<String>,
    pub model_request_id: Option<String>,
    pub body: ExecutionEventBody,
    pub redaction: RedactionMeta,
}

impl ExecutionEventV1 {
    /// Self-consistency валидация одного события: длины полей, обязательность
    /// `session_id` вне system/legacy, соответствие scoped correlation-полей
    /// заявленному `run_scope`. Проверка «не ссылается на чужой run_id» живёт
    /// на уровне storage (08-2), где есть база для сверки.
    pub fn validate(&self) -> Result<(), LedgerContractError> {
        bounded(
            "event_id",
            &self.event_id,
            MAX_DIGEST_BYTES.max(LEGACY_EVENT_ID_HEX_LEN),
        )?;
        if self.event_id.trim().is_empty() {
            return Err(LedgerContractError::Empty { field: "event_id" });
        }
        bounded("run_id", &self.run_id, MAX_ID_BYTES)?;
        if self.run_scope != RunScope::System && self.run_id.trim().is_empty() {
            return Err(LedgerContractError::Empty { field: "run_id" });
        }
        bounded("task_id", &self.task_id, MAX_ID_BYTES)?;

        match (&self.run_scope, &self.session_id) {
            (RunScope::System, _) | (RunScope::Legacy, _) => {}
            (_, None) => return Err(LedgerContractError::MissingSessionId),
            (_, Some(session_id)) => bounded("session_id", session_id, MAX_ID_BYTES)?,
        }

        bounded_opt("action_id", &self.action_id, MAX_ID_BYTES)?;
        bounded_opt("tool_call_id", &self.tool_call_id, MAX_ID_BYTES)?;
        bounded_opt("observation_id", &self.observation_id, MAX_ID_BYTES)?;
        bounded_opt("receipt_id", &self.receipt_id, MAX_ID_BYTES)?;
        bounded_opt("failure_id", &self.failure_id, MAX_ID_BYTES)?;
        bounded_opt("workflow_run_id", &self.workflow_run_id, MAX_ID_BYTES)?;
        bounded_opt("node_id", &self.node_id, MAX_ID_BYTES)?;
        bounded_opt("attempt_id", &self.attempt_id, MAX_ID_BYTES)?;
        bounded_opt("effect_id", &self.effect_id, MAX_ID_BYTES)?;
        bounded_opt("model_request_id", &self.model_request_id, MAX_ID_BYTES)?;

        if self.run_scope != RunScope::Workflow
            && (self.workflow_run_id.is_some()
                || self.node_id.is_some()
                || self.attempt_id.is_some())
        {
            return Err(LedgerContractError::ScopeMismatch {
                field: "workflow_run_id/node_id/attempt_id",
                run_scope: self.run_scope,
            });
        }
        if self.run_scope == RunScope::System
            && (self.action_id.is_some() || self.tool_call_id.is_some())
        {
            return Err(LedgerContractError::ScopeMismatch {
                field: "action_id/tool_call_id",
                run_scope: self.run_scope,
            });
        }

        self.body.validate()?;
        self.redaction.validate()?;
        Ok(())
    }
}

/// Storage-слой (08-2) переиспользует этот хелпер при записи: два события с
/// одинаковым `action_id`, оба с терминальным `state_after`, — запрещённая
/// ситуация независимо от порядка записи.
pub fn assert_single_terminal(events: &[ExecutionEventV1]) -> Result<(), LedgerContractError> {
    use std::collections::HashSet;
    let mut seen_terminal: HashSet<&str> = HashSet::new();
    for event in events {
        let (Some(action_id), Some(state)) = (event.action_id.as_deref(), event.state_after) else {
            continue;
        };
        if state.is_terminal() {
            if !seen_terminal.insert(action_id) {
                return Err(LedgerContractError::DuplicateTerminalOutcome {
                    action_id: action_id.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Проверяет переход `from -> to` по общей transition-таблице
/// [`ActionState::allowed_transitions`].
pub fn validate_transition(from: ActionState, to: ActionState) -> Result<(), LedgerContractError> {
    if from.can_transition_to(to) {
        Ok(())
    } else {
        Err(LedgerContractError::IllegalTransition { from, to })
    }
}

/// Детерминированный `event_id` для legacy `events` rows: domain-separated
/// SHA-256 по исходным полям. Исходная payload не переписывается — только
/// используется как вход хэша.
pub fn legacy_event_id(
    sequence_id: i64,
    task_id: &str,
    event_type: &str,
    payload: &[u8],
    created_at: &str,
) -> String {
    use sha2::{Digest, Sha256};

    fn write_field(hasher: &mut Sha256, bytes: &[u8]) {
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }

    let mut hasher = Sha256::new();
    write_field(&mut hasher, LEGACY_EVENT_ID_DOMAIN);
    write_field(&mut hasher, &sequence_id.to_be_bytes());
    write_field(&mut hasher, task_id.as_bytes());
    write_field(&mut hasher, event_type.as_bytes());
    write_field(&mut hasher, payload);
    write_field(&mut hasher, created_at.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Ставит расширение схемы 08-2 идемпотентно: аддитивные колонки `events` и
/// rebuild CHECK `workflow_run_nodes` под `cancelling`. Вызывается при каждом
/// открытии базы, тем же способом, что `workflow_store::install_schema` и
/// соседи (`lib.rs::open_internal`) — без отдельной ветки `migrate()`,
/// потому что `migrate()` не выполняется для баз, уже стоящих на v26+.
pub fn install_schema(connection: &Connection) -> Result<(), LedgerContractError> {
    // В реальном потоке `events` существует всегда — она создаётся первым
    // же шагом `migrate()` (v0->v1) до того, как `open_internal` доходит до
    // идемпотентных installer'ов. Проверка присутствия таблицы здесь — не
    // подстраховка под гипотетический случай, а совместимость с тестовыми
    // fixture-базами (`lib.rs::tests::migrates_schema_*`), которые сеют
    // только `PRAGMA user_version` без промежуточных таблиц: для них
    // расширение `events` просто не нужно.
    let events_table_exists: bool = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='events'",
        [],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if events_table_exists {
        // `events` уже непустая таблица в любой существующей базе — SQLite
        // не позволяет добавить NOT NULL колонку без DEFAULT к непустой
        // таблице, поэтому все новые колонки nullable, а обязательность для
        // новых typed rows проверяется в `ExecutionEventV1::validate`, не
        // CHECK-ограничением.
        for column in [
            "event_id TEXT",
            "schema_version INTEGER",
            "run_scope TEXT",
            "run_id TEXT",
            "session_id TEXT",
            "action_id TEXT",
            "effect_id TEXT",
            "workflow_run_id TEXT",
            "state_after TEXT",
        ] {
            // Ошибка «duplicate column name» — ожидаемый путь идемпотентности,
            // тот же приём, что `model_provenance::install_schema`.
            let _ = connection.execute(&format!("ALTER TABLE events ADD COLUMN {column}"), []);
        }
        connection.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_events_event_id
                 ON events(event_id) WHERE event_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_events_run
                 ON events(run_scope, run_id, sequence_id) WHERE run_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_events_action
                 ON events(action_id) WHERE action_id IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_events_effect
                 ON events(effect_id) WHERE effect_id IS NOT NULL;",
        )?;
    }

    rebuild_workflow_run_nodes_check(connection)?;
    Ok(())
}

/// `workflow_run_nodes.state` CHECK не допускает `cancelling` до этой
/// миграции. SQLite не умеет менять CHECK на месте, поэтому таблица
/// пересоздаётся: rename -> create -> copy -> drop, одной транзакцией.
/// Проверено (план 08-2): ни одна таблица не ссылается на
/// `workflow_run_nodes` через FOREIGN KEY, и у неё нет вторичных индексов
/// кроме PRIMARY KEY — пересоздание безопасно без восстановления индексов.
fn rebuild_workflow_run_nodes_check(connection: &Connection) -> Result<(), LedgerContractError> {
    let current_sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='workflow_run_nodes'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let Some(current_sql) = current_sql else {
        // workflow_store::install_schema ещё не создал таблицу — нечего
        // пересобирать; следующий вызов install_schema (после того как
        // workflow_store поставит таблицу) сделает это.
        return Ok(());
    };
    if current_sql.contains("'cancelling'") {
        return Ok(());
    }
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "ALTER TABLE workflow_run_nodes RENAME TO workflow_run_nodes_legacy;
         CREATE TABLE workflow_run_nodes (
             run_id TEXT NOT NULL REFERENCES workflow_runs(run_id) ON DELETE CASCADE,
             node_id TEXT NOT NULL,
             action_kind TEXT NOT NULL,
             state TEXT NOT NULL CHECK(state IN
                 ('pending','ready','running','waiting_approval','cancelling','succeeded',
                  'failed','timed_out','cancelled','blocked','denied','skipped','degraded',
                  'unknown_outcome','dead_letter')),
             attempts INTEGER NOT NULL DEFAULT 0,
             output_json TEXT NOT NULL DEFAULT '',
             error_code TEXT NOT NULL DEFAULT '',
             error_message TEXT NOT NULL DEFAULT '',
             approval_id TEXT NOT NULL DEFAULT '',
             updated_at_ms INTEGER NOT NULL,
             PRIMARY KEY(run_id, node_id)
         );
         INSERT INTO workflow_run_nodes(
             run_id, node_id, action_kind, state, attempts, output_json,
             error_code, error_message, approval_id, updated_at_ms)
           SELECT run_id, node_id, action_kind, state, attempts, output_json,
                  error_code, error_message, approval_id, updated_at_ms
           FROM workflow_run_nodes_legacy;
         DROP TABLE workflow_run_nodes_legacy;",
    )?;
    transaction.commit()?;
    Ok(())
}

/// Присутствие и закрытость dispatch marker (`run_effects`) для одного
/// action при старте Core. Storage-слой читает это из SQL; классификация
/// ниже — чистая функция, тестируемая без базы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMarkerStatus {
    /// Эффект не диспатчился — pre-dispatch restart.
    Absent,
    /// `started_at` есть, `completed_at` — нет: исход неизвестен.
    StartedNotCompleted,
    /// Эффект уже закрыт.
    Completed,
}

/// Классификация одного нетерминального action при старте Core, по правилу
/// плана 08-2 §5: pre-dispatch (`Absent`) не переписывается — `pending`/
/// `ready` уже resumable по контракту, только run-level помечается
/// `interrupted` отдельно (не здесь — это ответственность workflow runtime);
/// marker открыт (`StartedNotCompleted`) -> `unknown_outcome`, блокирует
/// слепой повтор; `waiting_approval` не трогается (обычный TTL); уже
/// terminal action не трогается никогда. Возвращает `None`, когда
/// reconciliation-событие публиковать не нужно.
pub fn reconcile_action_state(
    previous: ActionState,
    marker: DispatchMarkerStatus,
) -> Option<ActionState> {
    if previous.is_terminal() || previous == ActionState::WaitingApproval {
        return None;
    }
    match marker {
        DispatchMarkerStatus::StartedNotCompleted => Some(ActionState::UnknownOutcome),
        DispatchMarkerStatus::Absent | DispatchMarkerStatus::Completed => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow_store::NodeState;

    fn sample_event() -> ExecutionEventV1 {
        ExecutionEventV1 {
            schema_version: 1,
            event_id: "a".repeat(64),
            sequence_id: None,
            run_scope: RunScope::Workflow,
            run_id: "run-1".into(),
            session_id: Some("session-1".into()),
            task_id: "task-1".into(),
            created_at_ms: 1_700_000_000_000,
            state_after: Some(ActionState::Running),
            action_id: Some("action-1".into()),
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: Some("wf-run-1".into()),
            node_id: Some("node-1".into()),
            attempt_id: Some("attempt-1".into()),
            effect_id: None,
            model_request_id: None,
            body: ExecutionEventBody::ToolCall {
                tool_name: "shell".into(),
                tool_call_hash: "b".repeat(32),
                manifest_hash: None,
            },
            redaction: RedactionMeta::default(),
        }
    }

    #[test]
    fn round_trip_serde_for_each_body_variant() {
        let bodies = vec![
            ExecutionEventBody::ActionRequest {
                action_kind: "shell".into(),
                requested_capability: "fs.write".into(),
            },
            ExecutionEventBody::ToolCall {
                tool_name: "shell".into(),
                tool_call_hash: "hash".into(),
                manifest_hash: Some("mhash".into()),
            },
            ExecutionEventBody::Observation {
                summary_digest: "digest".into(),
                artifact_refs: vec![ArtifactRef {
                    content_hash: "chash".into(),
                    kind: "file".into(),
                }],
            },
            ExecutionEventBody::ToolReceipt {
                receipt_action_id: "receipt-action".into(),
                receipt_hash: "rhash".into(),
            },
            ExecutionEventBody::TypedFailure {
                error_class: "timeout".into(),
                provider_error_id: Some("prov-1".into()),
            },
            ExecutionEventBody::ApprovalDecision {
                approval_intent_id: "approval-1".into(),
                decision: ApprovalOutcome::Approved,
                snapshot_hash: None,
            },
            ExecutionEventBody::Cancellation {
                reason_class: "user_requested".into(),
            },
            ExecutionEventBody::RecoveryDecision {
                decision: "resume".into(),
                evidence_digest: "evidence".into(),
            },
        ];
        for body in bodies {
            let mut event = sample_event();
            event.body = body;
            let json = serde_json::to_string(&event).expect("serialize");
            let round_tripped: ExecutionEventV1 = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(event, round_tripped);
            event.validate().expect("valid sample body");
        }
    }

    #[test]
    fn bound_rejects_oversized_field() {
        let mut event = sample_event();
        event.task_id = "x".repeat(MAX_ID_BYTES + 1);
        assert_eq!(
            event.validate(),
            Err(LedgerContractError::Limit {
                field: "task_id",
                max: MAX_ID_BYTES
            })
        );
    }

    #[test]
    fn redaction_meta_has_no_raw_secret_field() {
        let event = sample_event();
        let json = serde_json::to_value(&event).expect("serialize");
        let redaction = json.get("redaction").expect("redaction present");
        let keys: Vec<&str> = redaction
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"secrets_present"));
        assert!(keys.contains(&"digest"));
    }

    #[test]
    fn action_state_set_matches_node_state_modulo_cancelling() {
        let action_names: std::collections::BTreeSet<&str> = ActionState::ALL
            .iter()
            .filter(|state| **state != ActionState::Cancelling)
            .map(|state| state.as_str())
            .collect();
        let node_names: std::collections::BTreeSet<&str> = [
            NodeState::Pending,
            NodeState::Ready,
            NodeState::Running,
            NodeState::WaitingApproval,
            NodeState::Succeeded,
            NodeState::Failed,
            NodeState::TimedOut,
            NodeState::Cancelled,
            NodeState::Blocked,
            NodeState::Denied,
            NodeState::Skipped,
            NodeState::Degraded,
            NodeState::UnknownOutcome,
            NodeState::DeadLetter,
        ]
        .iter()
        .map(|state| state.as_str())
        .collect();
        assert_eq!(action_names, node_names);
    }

    #[test]
    fn action_state_to_node_state_round_trips_except_cancelling() {
        for state in ActionState::ALL {
            if *state == ActionState::Cancelling {
                assert!(crate::workflow_store::NodeState::try_from(*state).is_err());
                continue;
            }
            let node_state: NodeState = (*state).try_into().expect("convertible");
            let back: ActionState = node_state.into();
            assert_eq!(*state, back);
        }
    }

    #[test]
    fn action_state_to_run_state_is_exhaustive_and_covers_run_states() {
        use crate::workflow_store::RunState;
        let mut covered = std::collections::BTreeSet::new();
        for state in ActionState::ALL {
            covered.insert(action_state_to_run_state(*state).as_str());
        }
        for run_state in [
            RunState::Pending,
            RunState::Running,
            RunState::WaitingApproval,
            RunState::Completed,
            RunState::Failed,
            RunState::Cancelled,
            RunState::Degraded,
            RunState::Interrupted,
        ] {
            assert!(
                covered.contains(run_state.as_str()),
                "run_state {run_state:?} unreachable from any ActionState"
            );
        }
    }

    #[test]
    fn illegal_transition_is_rejected() {
        assert_eq!(
            validate_transition(ActionState::Succeeded, ActionState::Running),
            Err(LedgerContractError::IllegalTransition {
                from: ActionState::Succeeded,
                to: ActionState::Running,
            })
        );
        assert!(validate_transition(ActionState::Pending, ActionState::Ready).is_ok());
    }

    #[test]
    fn terminal_states_have_no_outgoing_transitions() {
        for state in ActionState::ALL {
            if state.is_terminal() {
                assert!(
                    state.allowed_transitions().is_empty(),
                    "{state:?} must be terminal-closed"
                );
            }
        }
    }

    #[test]
    fn duplicate_terminal_outcome_for_same_action_is_rejected() {
        let mut first = sample_event();
        first.action_id = Some("action-x".into());
        first.state_after = Some(ActionState::Succeeded);
        let mut second = sample_event();
        second.action_id = Some("action-x".into());
        second.state_after = Some(ActionState::Failed);

        assert_eq!(
            assert_single_terminal(&[first, second]),
            Err(LedgerContractError::DuplicateTerminalOutcome {
                action_id: "action-x".into()
            })
        );
    }

    #[test]
    fn single_terminal_outcome_is_accepted() {
        let mut running = sample_event();
        running.action_id = Some("action-y".into());
        running.state_after = Some(ActionState::Running);
        let mut terminal = sample_event();
        terminal.action_id = Some("action-y".into());
        terminal.state_after = Some(ActionState::Succeeded);

        assert!(assert_single_terminal(&[running, terminal]).is_ok());
    }

    #[test]
    fn run_id_with_incompatible_scope_is_rejected() {
        let mut event = sample_event();
        event.run_scope = RunScope::System;
        event.session_id = None;
        // System scope доступен без workflow correlation, но не с ним.
        assert_eq!(
            event.validate(),
            Err(LedgerContractError::ScopeMismatch {
                field: "workflow_run_id/node_id/attempt_id",
                run_scope: RunScope::System,
            })
        );
    }

    #[test]
    fn missing_session_id_outside_system_or_legacy_is_rejected() {
        let mut event = sample_event();
        event.session_id = None;
        assert_eq!(event.validate(), Err(LedgerContractError::MissingSessionId));
    }

    #[test]
    fn system_scope_without_correlation_is_valid_without_session_id() {
        let mut event = sample_event();
        event.run_scope = RunScope::System;
        event.session_id = None;
        event.run_id = String::new();
        event.action_id = None;
        event.tool_call_id = None;
        event.workflow_run_id = None;
        event.node_id = None;
        event.attempt_id = None;
        assert!(event.validate().is_ok());
    }

    #[test]
    fn too_many_artifact_refs_is_rejected() {
        let mut event = sample_event();
        event.body = ExecutionEventBody::Observation {
            summary_digest: "digest".into(),
            artifact_refs: (0..MAX_ARTIFACT_REFS + 1)
                .map(|i| ArtifactRef {
                    content_hash: format!("hash-{i}"),
                    kind: "file".into(),
                })
                .collect(),
        };
        assert_eq!(
            event.validate(),
            Err(LedgerContractError::TooManyArtifactRefs(
                MAX_ARTIFACT_REFS + 1
            ))
        );
    }

    #[test]
    fn unknown_body_tag_fails_to_deserialize_without_panicking() {
        let malformed = r#"{"kind":"not_a_real_variant"}"#;
        let result: Result<ExecutionEventBody, _> = serde_json::from_str(malformed);
        assert!(result.is_err());
    }

    #[test]
    fn legacy_event_id_is_deterministic_and_bounded() {
        let a = legacy_event_id(1, "task", "type", b"payload", "2024-01-01T00:00:00Z");
        let b = legacy_event_id(1, "task", "type", b"payload", "2024-01-01T00:00:00Z");
        assert_eq!(a, b);
        assert_eq!(a.len(), LEGACY_EVENT_ID_HEX_LEN);
        let different = legacy_event_id(2, "task", "type", b"payload", "2024-01-01T00:00:00Z");
        assert_ne!(a, different);
    }

    #[test]
    fn legacy_event_id_differs_on_payload_boundary_shift() {
        // "ab"+"c" и "a"+"bc" не должны совпасть — length-prefixing защищает
        // от конкатенационных коллизий на границе полей.
        let a = legacy_event_id(1, "ab", "c", b"", "");
        let b = legacy_event_id(1, "a", "bc", b"", "");
        assert_ne!(a, b);
    }

    fn open_test_connection() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory connection");
        connection
            .pragma_update(None, "foreign_keys", true)
            .expect("enable foreign keys");
        connection
    }

    #[test]
    fn install_schema_adds_event_columns_idempotently() {
        let connection = open_test_connection();
        connection
            .execute_batch(
                "CREATE TABLE events (
                    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                INSERT INTO events(task_id, event_type, payload) VALUES ('legacy-task', 'legacy.type', x'00');",
            )
            .expect("legacy events table with a row");

        install_schema(&connection).expect("first install");
        install_schema(&connection).expect("second install is a no-op");

        let (task_id, event_id): (String, Option<String>) = connection
            .query_row(
                "SELECT task_id, event_id FROM events WHERE task_id = 'legacy-task'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("legacy row survives");
        assert_eq!(task_id, "legacy-task");
        assert_eq!(event_id, None);
    }

    #[test]
    fn install_schema_rebuilds_workflow_run_nodes_check_and_keeps_rows() {
        let connection = open_test_connection();
        connection
            .execute_batch(
                "CREATE TABLE events (
                    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE workflow_runs (
                    run_id TEXT PRIMARY KEY NOT NULL,
                    state TEXT NOT NULL
                );
                CREATE TABLE workflow_run_nodes (
                    run_id TEXT NOT NULL REFERENCES workflow_runs(run_id) ON DELETE CASCADE,
                    node_id TEXT NOT NULL,
                    action_kind TEXT NOT NULL,
                    state TEXT NOT NULL CHECK(state IN
                        ('pending','ready','running','waiting_approval','succeeded','failed',
                         'timed_out','cancelled','blocked','denied','skipped','degraded',
                         'unknown_outcome','dead_letter')),
                    attempts INTEGER NOT NULL DEFAULT 0,
                    output_json TEXT NOT NULL DEFAULT '',
                    error_code TEXT NOT NULL DEFAULT '',
                    error_message TEXT NOT NULL DEFAULT '',
                    approval_id TEXT NOT NULL DEFAULT '',
                    updated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(run_id, node_id)
                );
                INSERT INTO workflow_runs(run_id, state) VALUES ('run-1', 'running');
                INSERT INTO workflow_run_nodes(run_id, node_id, action_kind, state, updated_at_ms)
                    VALUES ('run-1', 'node-1', 'shell', 'running', 1);",
            )
            .expect("legacy workflow tables with a row");

        install_schema(&connection).expect("first install rebuilds CHECK");
        install_schema(&connection).expect("second install is a no-op");

        let state: String = connection
            .query_row(
                "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
                [],
                |row| row.get(0),
            )
            .expect("existing row survives rebuild");
        assert_eq!(state, "running");

        connection
            .execute(
                "UPDATE workflow_run_nodes SET state = 'cancelling' WHERE run_id = 'run-1' AND node_id = 'node-1'",
                [],
            )
            .expect("CHECK now accepts cancelling");
    }

    #[test]
    fn reconcile_action_state_only_flags_open_dispatch_marker() {
        assert_eq!(
            reconcile_action_state(
                ActionState::Running,
                DispatchMarkerStatus::StartedNotCompleted
            ),
            Some(ActionState::UnknownOutcome)
        );
        assert_eq!(
            reconcile_action_state(ActionState::Running, DispatchMarkerStatus::Absent),
            None
        );
        assert_eq!(
            reconcile_action_state(ActionState::Pending, DispatchMarkerStatus::Absent),
            None
        );
        assert_eq!(
            reconcile_action_state(
                ActionState::WaitingApproval,
                DispatchMarkerStatus::StartedNotCompleted
            ),
            None
        );
        assert_eq!(
            reconcile_action_state(
                ActionState::Succeeded,
                DispatchMarkerStatus::StartedNotCompleted
            ),
            None
        );
    }
}
