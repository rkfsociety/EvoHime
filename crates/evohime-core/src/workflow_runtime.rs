//! Durable runtime запусков workflow (план 06.2).
//!
//! Runtime — это связка чистого контракта (`workflow`, `workflow_runner`) с
//! реальным Core execution path и durable-состоянием
//! (`evohime_local_storage::workflow_store`). Он владеет только
//! планированием и состоянием; сами эффекты выполняет инъектируемый
//! [`NodeAdapter`], а approval — [`WorkflowApprovalGate`], который в продукте
//! ведёт к тому же approval registry, что и обычные инструменты.
//!
//! Инварианты, ради которых модуль существует:
//!
//! * узел не ставится в очередь, пока не получены и не проверены все
//!   обязательные входы;
//! * перед каждым эффектом заново сверяются graph hash, run policy, grants и
//!   разрешимость capability по реестру;
//! * dispatch marker пишется до эффекта, поэтому падение Core даёт
//!   `unknown_outcome`, а не слепой повтор;
//! * ошибка узла продолжает выполнение только по объявленной failure-ветви;
//! * события durable, монотонны и содержат только bounded projection.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::workflow::{
    ConcurrencyClass, EdgeChannel, JoinMode, NodeExecutionContext, NodeType, WorkflowGraph,
    WorkflowNode,
};
use crate::workflow_registry::{ParentCapabilities, WorkflowRegistry};
use crate::EventJournal;

use evohime_local_storage::workflow_store::{
    self as store, NodeState, RunState, WorkflowAttemptRecord, WorkflowEventRecord,
    WorkflowNodeRecord, WorkflowRunRecord,
};
use evohime_local_storage::StorageError;

/// Потолок текста ошибки в projection и событиях.
pub const MAX_EVENT_MESSAGE_CHARS: usize = 512;
/// Срок аренды запуска. После истечения запуск может продолжить другой Core.
pub const LEASE_MS: i64 = 60_000;
/// Максимум элементов batch-итерации по умолчанию.
pub const DEFAULT_BATCH_ITEMS: usize = 16;

// ---------------------------------------------------------------------------
// Контракт адаптера узла.
// ---------------------------------------------------------------------------

/// Успешный результат узла.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSuccess {
    pub output: Value,
    /// Число подтверждающих evidence. Проверяется по acceptance contract.
    pub evidence: u32,
    /// Статус узла (`accepted`, `degraded`, ...). Проверяется по allowed
    /// statuses, если они объявлены.
    pub status: String,
    /// Источник недоступен или устарел: результат честно помечается
    /// вырожденным, а не выдаётся за уверенный.
    pub degraded: bool,
}

impl NodeSuccess {
    pub fn new(output: Value) -> Self {
        Self {
            output,
            evidence: 0,
            status: "accepted".into(),
            degraded: false,
        }
    }

    pub fn with_evidence(mut self, evidence: u32) -> Self {
        self.evidence = evidence;
        self
    }

    pub fn degraded(mut self, status: &str) -> Self {
        self.degraded = true;
        self.status = status.into();
        self
    }
}

/// Ошибка узла. `retryable` — свойство ошибки, а не желание вызывающего:
/// повтор дополнительно ограничен acceptance contract узла.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl NodeError {
    pub fn permanent(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
        }
    }

    pub fn transient(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: true,
        }
    }
}

/// Всё, что узел получает на вход. Ни prompt родителя, ни произвольный
/// контекст запуска сюда не попадают: только объявленные входы и уже
/// проверенные capability.
pub struct NodeInvocation<'a> {
    pub context: NodeExecutionContext,
    pub node: &'a WorkflowNode,
    pub inputs: BTreeMap<String, Value>,
    pub parent: &'a ParentCapabilities,
    pub registry: &'a WorkflowRegistry,
}

pub type NodeFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<NodeSuccess, NodeError>> + Send + 'a>,
>;

/// Выполняет реальную работу узла: child request, tool call, `mcp.call`,
/// контекстный провайдер или deterministic-операция Core.
pub trait NodeAdapter: Send + Sync {
    fn execute<'a>(&'a self, invocation: NodeInvocation<'a>) -> NodeFuture<'a>;
}

/// Результат запроса подтверждения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved,
    Denied,
    /// Решение ещё не принято. `approval_id` — тот же идентификатор, который
    /// разрешает существующая команда `ResolveApproval`.
    Pending {
        approval_id: String,
    },
}

pub type ApprovalFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = ApprovalOutcome> + Send + 'a>>;

pub trait WorkflowApprovalGate: Send + Sync {
    fn decide<'a>(&'a self, run_id: &'a str, node: &'a WorkflowNode) -> ApprovalFuture<'a>;
}

/// Реестр подтверждений workflow.
///
/// Он не заводит второй механизм approval: идентификатор здесь тот же UUID,
/// который renderer разрешает существующей командой `ResolveApproval`, а
/// решение приходит из того же места, что и для обычных инструментов.
/// Собственная таблица нужна ровно для одного: отличить «ещё не решено» от
/// «отклонено», потому что запуск не должен выполнять узел ни в том, ни в
/// другом случае, но состояния это разные.
#[derive(Default)]
pub struct WorkflowApprovalRegistry {
    pending: std::sync::Mutex<BTreeMap<String, String>>,
    decisions: std::sync::Mutex<BTreeMap<String, bool>>,
}

impl WorkflowApprovalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Возвращает идентификатор подтверждения узла, создавая его при первом
    /// обращении. Повторный вызов даёт тот же идентификатор, поэтому
    /// перезапуск драйвера не плодит карточки.
    pub fn approval_id(&self, run_id: &str, node_id: &str) -> String {
        let key = format!("{run_id}:{node_id}");
        let mut pending = self.pending.lock().expect("approval registry");
        pending
            .entry(key)
            .or_insert_with(|| uuid::Uuid::new_v4().to_string())
            .clone()
    }

    pub fn resolve(&self, approval_id: &str, granted: bool) -> bool {
        let known = self
            .pending
            .lock()
            .expect("approval registry")
            .values()
            .any(|value| value == approval_id);
        if !known {
            return false;
        }
        self.decisions
            .lock()
            .expect("approval registry")
            .insert(approval_id.to_string(), granted);
        true
    }

    /// Запуск, которому принадлежит подтверждение. Нужен, чтобы после
    /// решения продолжить именно его.
    pub fn run_for(&self, approval_id: &str) -> Option<String> {
        self.pending
            .lock()
            .expect("approval registry")
            .iter()
            .find(|(_, value)| value.as_str() == approval_id)
            .and_then(|(key, _)| key.split(':').next().map(str::to_string))
    }

    pub fn decision(&self, approval_id: &str) -> Option<bool> {
        self.decisions
            .lock()
            .expect("approval registry")
            .get(approval_id)
            .copied()
    }
}

impl WorkflowApprovalGate for WorkflowApprovalRegistry {
    fn decide<'a>(&'a self, run_id: &'a str, node: &'a WorkflowNode) -> ApprovalFuture<'a> {
        let approval_id = self.approval_id(run_id, &node.id);
        let decision = self.decision(&approval_id);
        Box::pin(async move {
            match decision {
                Some(true) => ApprovalOutcome::Approved,
                Some(false) => ApprovalOutcome::Denied,
                None => ApprovalOutcome::Pending { approval_id },
            }
        })
    }
}

/// Approval-шлюз, который всегда отвечает «ожидание». Используется, когда
/// approval registry не подключён: запуск встаёт в `waiting_approval`, а не
/// выполняется без подтверждения.
pub struct AlwaysPendingApproval;

impl WorkflowApprovalGate for AlwaysPendingApproval {
    fn decide<'a>(&'a self, run_id: &'a str, node: &'a WorkflowNode) -> ApprovalFuture<'a> {
        let approval_id = format!("{run_id}:{}", node.id);
        Box::pin(async move { ApprovalOutcome::Pending { approval_id } })
    }
}

// ---------------------------------------------------------------------------
// Проекции и ошибки runtime.
// ---------------------------------------------------------------------------

/// Bounded projection события для timeline и renderer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEventProjection {
    pub run_id: String,
    pub sequence: i64,
    pub node_id: String,
    pub event_type: String,
    pub state: String,
    pub attempt: u32,
    pub error_code: String,
    pub message: String,
}

/// Bounded projection узла.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowNodeProjection {
    pub node_id: String,
    pub action_kind: String,
    pub role: String,
    pub state: String,
    pub attempts: u32,
    pub error_code: String,
    pub message: String,
    pub approval_id: String,
    pub dependencies: Vec<String>,
}

/// Bounded projection запуска. Ни prompt, ни сырой вывод child, ни секреты
/// здесь не появляются: только идентификаторы, состояния и коды.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunProjection {
    pub run_id: String,
    pub task_id: String,
    pub template_id: String,
    pub template_version: u32,
    pub graph_id: String,
    pub graph_version: u64,
    pub graph_hash: String,
    pub state: String,
    pub terminal_reason: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub nodes: Vec<WorkflowNodeProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    UnknownRun(String),
    InvalidGraph(String),
    /// Snapshot графа в базе не совпадает со своим hash: запись повреждена
    /// или подменена, запускать по ней нельзя.
    GraphHashMismatch {
        expected: String,
        actual: String,
    },
    CapabilityRejected(Vec<String>),
    Busy(String),
    Storage(String),
}

impl RuntimeError {
    pub fn code(&self) -> &'static str {
        match self {
            RuntimeError::UnknownRun(_) => "unknown_run",
            RuntimeError::InvalidGraph(_) => "invalid_graph",
            RuntimeError::GraphHashMismatch { .. } => "graph_hash_mismatch",
            RuntimeError::CapabilityRejected(_) => "capability_rejected",
            RuntimeError::Busy(_) => "run_busy",
            RuntimeError::Storage(_) => "storage_failed",
        }
    }
}

impl From<store::WorkflowStoreError> for RuntimeError {
    fn from(error: store::WorkflowStoreError) -> Self {
        RuntimeError::Storage(error.to_string())
    }
}

impl From<StorageError> for RuntimeError {
    fn from(error: StorageError) -> Self {
        RuntimeError::Storage(error.to_string())
    }
}

/// Запрос на запуск. `graph` — уже инстанцированный snapshot шаблона.
#[derive(Debug, Clone, PartialEq)]
pub struct StartWorkflowRequest {
    pub run_id: String,
    pub task_id: String,
    /// Рабочий каталог запуска. Он сохраняется в snapshot политики, потому
    /// что продолжение после approval или перезапуска обязано идти в том же
    /// каталоге, а не в том, который окажется открыт в оболочке позже.
    pub workspace_path: String,
    pub template_id: String,
    pub template_version: u32,
    pub inputs: BTreeMap<String, String>,
    pub graph: WorkflowGraph,
    pub parent: ParentCapabilities,
}

/// Итог одного прогона драйвера.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveOutcome {
    pub run_id: String,
    pub state: RunState,
    pub executed_nodes: Vec<String>,
    pub waiting_approval: Vec<String>,
}

// ---------------------------------------------------------------------------
// Хранилищный слой поверх EventJournal.
// ---------------------------------------------------------------------------

impl EventJournal {
    pub async fn insert_workflow_run(
        &self,
        run: &WorkflowRunRecord,
        nodes: &[WorkflowNodeRecord],
    ) -> Result<(), store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::insert_run(database.connection(), run, nodes)
    }

    /// Рабочий каталог запуска из снимка политики. Возврат пустой строки
    /// означает «каталог не был записан», а не «текущий каталог оболочки».
    pub async fn workflow_run_workspace(&self, run_id: &str) -> String {
        let Ok(Some(run)) = self.workflow_run(run_id).await else {
            return String::new();
        };
        serde_json::from_str::<Value>(&run.policy_json)
            .ok()
            .and_then(|policy| {
                policy
                    .get("workspace_path")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

    pub async fn workflow_run(
        &self,
        run_id: &str,
    ) -> Result<Option<WorkflowRunRecord>, store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::get_run(database.connection(), run_id)
    }

    pub async fn list_workflow_runs(
        &self,
        limit: usize,
    ) -> Result<Vec<WorkflowRunRecord>, store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::list_runs(database.connection(), limit)
    }

    pub async fn workflow_run_nodes(
        &self,
        run_id: &str,
    ) -> Result<Vec<WorkflowNodeRecord>, store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::list_nodes(database.connection(), run_id)
    }

    pub async fn workflow_run_attempts(
        &self,
        run_id: &str,
    ) -> Result<Vec<WorkflowAttemptRecord>, store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::list_attempts(database.connection(), run_id)
    }

    pub async fn list_workflow_events(
        &self,
        run_id: &str,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<WorkflowEventRecord>, store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::list_events(database.connection(), run_id, after_sequence, limit)
    }

    pub async fn request_workflow_cancel(
        &self,
        run_id: &str,
        now_ms: i64,
    ) -> Result<bool, store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::request_cancel(database.connection(), run_id, now_ms)
    }

    pub async fn recover_workflow_runs(
        &self,
        now_ms: i64,
    ) -> Result<store::RecoveryOutcome, store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::recover_after_restart(database.connection(), now_ms)
    }

    async fn append_workflow_event(
        &self,
        run_id: &str,
        node_id: &str,
        attempt_id: &str,
        event_type: &str,
        payload: &Value,
    ) -> Result<i64, store::WorkflowStoreError> {
        let database = self.database.lock().await;
        store::append_event(
            database.connection(),
            run_id,
            node_id,
            attempt_id,
            event_type,
            &payload.to_string(),
            crate::task_memory::now_millis() as i64,
        )
    }
}

// ---------------------------------------------------------------------------
// Runtime.
// ---------------------------------------------------------------------------

pub struct WorkflowRuntime {
    journal: EventJournal,
    registry: Arc<WorkflowRegistry>,
    adapter: Arc<dyn NodeAdapter>,
    approvals: Arc<dyn WorkflowApprovalGate>,
    owner_id: String,
    events: Option<tokio::sync::broadcast::Sender<crate::CoreEvent>>,
}

impl WorkflowRuntime {
    pub fn new(
        journal: EventJournal,
        registry: Arc<WorkflowRegistry>,
        adapter: Arc<dyn NodeAdapter>,
        approvals: Arc<dyn WorkflowApprovalGate>,
        owner_id: impl Into<String>,
    ) -> Self {
        Self {
            journal,
            registry,
            adapter,
            approvals,
            owner_id: owner_id.into(),
            events: None,
        }
    }

    pub fn with_events(mut self, events: tokio::sync::broadcast::Sender<crate::CoreEvent>) -> Self {
        self.events = Some(events);
        self
    }

    pub fn registry(&self) -> &WorkflowRegistry {
        &self.registry
    }

    /// Регистрирует запуск: проверяет контракт, реестр и родительские
    /// возможности, затем сохраняет immutable snapshot.
    pub async fn start(&self, request: StartWorkflowRequest) -> Result<String, RuntimeError> {
        request
            .graph
            .validate()
            .map_err(|errors| RuntimeError::InvalidGraph(format!("{errors:?}")))?;
        let graph = self
            .registry
            .expand_subgraphs(&request.graph)
            .map_err(|errors| {
                RuntimeError::CapabilityRejected(
                    errors
                        .iter()
                        .map(|error| error.code().to_string())
                        .collect(),
                )
            })?;
        graph
            .validate()
            .map_err(|errors| RuntimeError::InvalidGraph(format!("{errors:?}")))?;
        self.registry
            .validate_bindings(&graph, &request.parent)
            .map_err(|errors| {
                RuntimeError::CapabilityRejected(
                    errors
                        .iter()
                        .map(|error| error.code().to_string())
                        .collect(),
                )
            })?;

        let now_ms = crate::task_memory::now_millis() as i64;
        let graph_json = serde_json::to_string(&graph)
            .map_err(|error| RuntimeError::InvalidGraph(error.to_string()))?;
        let inputs_json = serde_json::to_string(&request.inputs)
            .map_err(|error| RuntimeError::InvalidGraph(error.to_string()))?;
        let policy_json = serde_json::to_string(&json!({
            "grants": request.parent.grants,
            "budget": request.parent.budget,
            "context_allowlist": request.parent.context_allowlist,
            "max_parallel_nodes": graph.budget.max_parallel_nodes,
            "max_tokens": graph.budget.max_tokens,
            "max_tool_calls": graph.budget.max_tool_calls,
            "max_wall_clock_ms": graph.budget.max_wall_clock_ms,
            "workspace_path": request.workspace_path,
        }))
        .map_err(|error| RuntimeError::InvalidGraph(error.to_string()))?;

        let record = WorkflowRunRecord {
            run_id: request.run_id.clone(),
            task_id: request.task_id.clone(),
            template_id: request.template_id.clone(),
            template_version: request.template_version,
            graph_id: graph.graph_id.clone(),
            graph_version: graph.version,
            graph_hash: graph.canonical_hash(),
            graph_json,
            inputs_json,
            policy_json,
            state: RunState::Pending,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            terminal_reason: String::new(),
            cancel_requested: false,
            lease_owner: String::new(),
            lease_expires_at_ms: 0,
        };
        let nodes: Vec<WorkflowNodeRecord> = graph
            .nodes
            .iter()
            .map(|node| WorkflowNodeRecord {
                run_id: request.run_id.clone(),
                node_id: node.id.clone(),
                action_kind: node.node_type.action_kind().to_string(),
                state: NodeState::Pending,
                attempts: 0,
                output_json: String::new(),
                error_code: String::new(),
                error_message: String::new(),
                approval_id: String::new(),
                updated_at_ms: now_ms,
            })
            .collect();
        self.journal.insert_workflow_run(&record, &nodes).await?;
        self.emit(
            &request.run_id,
            "",
            "",
            "workflow.run_started",
            json!({
                "template_id": request.template_id,
                "template_version": request.template_version,
                "graph_hash": record.graph_hash,
                "nodes": nodes.len(),
            }),
        )
        .await;
        Ok(request.run_id)
    }

    /// Прогоняет запуск настолько далеко, насколько позволяют входы, approval
    /// и отмена. Метод идемпотентен: повторный вызов после approval
    /// продолжает с того же места.
    pub async fn drive(&self, run_id: &str) -> Result<DriveOutcome, RuntimeError> {
        let run = self
            .journal
            .workflow_run(run_id)
            .await?
            .ok_or_else(|| RuntimeError::UnknownRun(run_id.to_string()))?;
        if run.state.is_terminal() {
            return Ok(DriveOutcome {
                run_id: run_id.to_string(),
                state: run.state,
                executed_nodes: Vec::new(),
                waiting_approval: Vec::new(),
            });
        }

        let graph: WorkflowGraph = serde_json::from_str(&run.graph_json)
            .map_err(|error| RuntimeError::InvalidGraph(error.to_string()))?;
        let actual_hash = graph.canonical_hash();
        if actual_hash != run.graph_hash {
            return Err(RuntimeError::GraphHashMismatch {
                expected: run.graph_hash.clone(),
                actual: actual_hash,
            });
        }
        let parent = parent_from_policy(&run.policy_json);

        let now_ms = crate::task_memory::now_millis() as i64;
        let acquired = {
            let database = self.journal.database.lock().await;
            store::acquire_lease(
                database.connection(),
                run_id,
                &self.owner_id,
                now_ms + LEASE_MS,
                now_ms,
            )?
        };
        if !acquired {
            return Err(RuntimeError::Busy(run_id.to_string()));
        }

        let outcome = self.drive_locked(&run, &graph, &parent).await;

        {
            let now_ms = crate::task_memory::now_millis() as i64;
            let database = self.journal.database.lock().await;
            store::release_lease(database.connection(), run_id, &self.owner_id, now_ms)?;
        }
        outcome
    }

    async fn drive_locked(
        &self,
        run: &WorkflowRunRecord,
        graph: &WorkflowGraph,
        parent: &ParentCapabilities,
    ) -> Result<DriveOutcome, RuntimeError> {
        let run_id = run.run_id.as_str();
        let mut executed = Vec::new();
        let mut waiting = Vec::new();

        loop {
            let current = self
                .journal
                .workflow_run(run_id)
                .await?
                .ok_or_else(|| RuntimeError::UnknownRun(run_id.to_string()))?;
            if current.state.is_terminal() {
                return Ok(DriveOutcome {
                    run_id: run_id.to_string(),
                    state: current.state,
                    executed_nodes: executed,
                    waiting_approval: waiting,
                });
            }
            let nodes = self.journal.workflow_run_nodes(run_id).await?;
            let states: BTreeMap<String, WorkflowNodeRecord> = nodes
                .iter()
                .map(|node| (node.node_id.clone(), node.clone()))
                .collect();

            if current.cancel_requested {
                self.cancel_open_nodes(run_id, &states).await?;
                self.finish_run(run_id, RunState::Cancelled, "cancelled")
                    .await?;
                return Ok(DriveOutcome {
                    run_id: run_id.to_string(),
                    state: RunState::Cancelled,
                    executed_nodes: executed,
                    waiting_approval: waiting,
                });
            }

            // Узлы, потерявшие возможность выполниться, помечаются до выбора
            // готовых: иначе запуск завис бы, ожидая вход, которого уже не
            // будет.
            let blocked = self.mark_blocked_nodes(run_id, graph, &states).await?;
            if blocked {
                continue;
            }

            let ready = ready_nodes(graph, &states);
            if ready.is_empty() {
                let terminal = terminal_state(graph, &states);
                if let Some((state, reason)) = terminal {
                    self.finish_run(run_id, state, &reason).await?;
                    return Ok(DriveOutcome {
                        run_id: run_id.to_string(),
                        state,
                        executed_nodes: executed,
                        waiting_approval: waiting,
                    });
                }
                // Единственная нетерминальная причина отсутствия готовых
                // узлов — ожидание approval.
                let state = if states
                    .values()
                    .any(|node| node.state == NodeState::WaitingApproval)
                {
                    RunState::WaitingApproval
                } else {
                    RunState::Interrupted
                };
                self.set_run_state(run_id, state, "").await?;
                return Ok(DriveOutcome {
                    run_id: run_id.to_string(),
                    state,
                    executed_nodes: executed,
                    waiting_approval: waiting,
                });
            }

            self.set_run_state(run_id, RunState::Running, "").await?;

            // Bounded fan-out: параллельно выполняются только узлы, явно
            // объявившие безопасную семантику; всё с побочным эффектом или
            // состоянием идёт по одному.
            let batch = select_batch(graph, &ready, graph.budget.max_parallel_nodes as usize);
            let mut progressed = false;
            for node_id in &batch {
                let node = graph.node(node_id).expect("validated node");
                match self.execute_node(run, graph, node, parent, &states).await? {
                    NodeStep::Executed => {
                        executed.push(node_id.clone());
                        progressed = true;
                    }
                    NodeStep::WaitingApproval => {
                        waiting.push(node_id.clone());
                    }
                    NodeStep::Cancelled => {
                        progressed = true;
                    }
                }
            }
            if !progressed {
                let state = if waiting.is_empty() {
                    RunState::Interrupted
                } else {
                    RunState::WaitingApproval
                };
                self.set_run_state(run_id, state, "").await?;
                return Ok(DriveOutcome {
                    run_id: run_id.to_string(),
                    state,
                    executed_nodes: executed,
                    waiting_approval: waiting,
                });
            }
        }
    }

    async fn execute_node(
        &self,
        run: &WorkflowRunRecord,
        graph: &WorkflowGraph,
        node: &WorkflowNode,
        parent: &ParentCapabilities,
        states: &BTreeMap<String, WorkflowNodeRecord>,
    ) -> Result<NodeStep, RuntimeError> {
        let run_id = run.run_id.as_str();

        // Повторная проверка перед эффектом: hash графа, разрешимость
        // capability и родительские возможности. Между постановкой в очередь и
        // запуском могли измениться и реестр, и окружение.
        if graph.canonical_hash() != run.graph_hash {
            return Err(RuntimeError::GraphHashMismatch {
                expected: run.graph_hash.clone(),
                actual: graph.canonical_hash(),
            });
        }
        let single = single_node_graph(graph, node);
        if let Err(errors) = self.registry.validate_bindings(&single, parent) {
            let codes: Vec<String> = errors.iter().map(|e| e.code().to_string()).collect();
            self.fail_node(
                run_id,
                node,
                0,
                &codes.join(","),
                "capability rejected before dispatch",
                NodeState::Failed,
            )
            .await?;
            return Ok(NodeStep::Executed);
        }

        if node.execution.approval.required {
            match self.approvals.decide(run_id, node).await {
                ApprovalOutcome::Approved => {}
                ApprovalOutcome::Denied => {
                    self.terminate_node(
                        run_id,
                        node,
                        0,
                        NodeState::Denied,
                        "approval_denied",
                        "подтверждение отклонено",
                    )
                    .await?;
                    return Ok(NodeStep::Executed);
                }
                ApprovalOutcome::Pending { approval_id } => {
                    let now_ms = crate::task_memory::now_millis() as i64;
                    {
                        let database = self.journal.database.lock().await;
                        store::set_node_approval(
                            database.connection(),
                            run_id,
                            &node.id,
                            &approval_id,
                            now_ms,
                        )?;
                    }
                    self.emit(
                        run_id,
                        &node.id,
                        "",
                        "workflow.node_waiting_approval",
                        json!({
                            "approval_id": approval_id,
                            "reason": node
                                .execution
                                .approval
                                .reason
                                .clone()
                                .unwrap_or_default(),
                        }),
                    )
                    .await;
                    return Ok(NodeStep::WaitingApproval);
                }
            }
        }

        let inputs = match collect_inputs(graph, node, states) {
            Ok(inputs) => inputs,
            Err(error) => {
                self.fail_node(
                    run_id,
                    node,
                    0,
                    &error.code,
                    &error.message,
                    NodeState::Failed,
                )
                .await?;
                return Ok(NodeStep::Executed);
            }
        };

        let max_attempts = node.execution.retry.max_attempts.max(1);
        let timeout = Duration::from_millis(node.execution.timeout_ms.max(1));
        let input_hash = hash_value(&json!(inputs));
        let mut attempt_number = 0u32;
        let mut last_error: Option<NodeError> = None;
        let mut retried = false;

        while attempt_number < max_attempts {
            attempt_number += 1;
            // Отмена проверяется перед каждой попыткой, а не только перед
            // узлом: длинный retry не должен переживать остановку запуска.
            if self.cancel_requested(run_id).await? {
                self.terminate_node(
                    run_id,
                    node,
                    attempt_number.saturating_sub(1),
                    NodeState::Cancelled,
                    "cancelled",
                    "запуск отменён",
                )
                .await?;
                return Ok(NodeStep::Cancelled);
            }

            let attempt_id = format!("{run_id}:{}:{attempt_number}", node.id);
            let now_ms = crate::task_memory::now_millis() as i64;
            {
                let database = self.journal.database.lock().await;
                store::begin_attempt(
                    database.connection(),
                    &WorkflowAttemptRecord {
                        attempt_id: attempt_id.clone(),
                        run_id: run_id.to_string(),
                        node_id: node.id.clone(),
                        attempt: attempt_number,
                        graph_hash: run.graph_hash.clone(),
                        input_hash: input_hash.clone(),
                        dispatched_at_ms: now_ms,
                        completed_at_ms: None,
                        outcome: String::new(),
                        error_code: String::new(),
                    },
                )?;
            }
            self.emit(
                run_id,
                &node.id,
                &attempt_id,
                "workflow.node_started",
                json!({
                    "action_kind": node.node_type.action_kind(),
                    "attempt": attempt_number,
                    "retry": retried,
                }),
            )
            .await;

            let invocation = NodeInvocation {
                context: NodeExecutionContext {
                    workflow_run_id: run_id.to_string(),
                    node_id: node.id.clone(),
                    attempt_id: attempt_id.clone(),
                    graph_hash: run.graph_hash.clone(),
                },
                node,
                inputs: inputs.clone(),
                parent,
                registry: &self.registry,
            };
            let result = match tokio::time::timeout(timeout, self.adapter.execute(invocation)).await
            {
                Ok(result) => result,
                Err(_) => Err(NodeError {
                    code: "timeout".into(),
                    message: "узел не уложился в timeout".into(),
                    retryable: true,
                }),
            };

            match result {
                Ok(success) => {
                    if let Err(error) = check_acceptance(node, &success) {
                        self.close_attempt(&attempt_id, "failed", &error.code)
                            .await?;
                        last_error = Some(error);
                        break;
                    }
                    self.close_attempt(&attempt_id, "succeeded", "").await?;
                    let state = if success.degraded {
                        NodeState::Degraded
                    } else {
                        NodeState::Succeeded
                    };
                    let output = serde_json::to_string(&success.output).unwrap_or_default();
                    let now_ms = crate::task_memory::now_millis() as i64;
                    {
                        let database = self.journal.database.lock().await;
                        store::update_node_state(
                            database.connection(),
                            store::UpdateNodeStateInput {
                                run_id,
                                node_id: &node.id,
                                state,
                                attempts: attempt_number,
                                output_json: &output,
                                error_code: "",
                                error_message: "",
                                now_ms,
                            },
                        )?;
                    }
                    self.emit(
                        run_id,
                        &node.id,
                        &attempt_id,
                        if success.degraded {
                            "workflow.node_degraded"
                        } else {
                            "workflow.node_succeeded"
                        },
                        json!({
                            "attempt": attempt_number,
                            "status": bounded_text(&success.status),
                            "evidence": success.evidence,
                        }),
                    )
                    .await;
                    return Ok(NodeStep::Executed);
                }
                Err(error) => {
                    self.close_attempt(&attempt_id, "failed", &error.code)
                        .await?;
                    let retryable = is_retryable(node, &error) && attempt_number < max_attempts;
                    last_error = Some(error);
                    if !retryable {
                        break;
                    }
                    retried = true;
                    let backoff = node
                        .execution
                        .retry
                        .backoff_ms
                        .saturating_mul(u64::from(attempt_number))
                        .min(node.execution.timeout_ms);
                    if backoff > 0 {
                        tokio::time::sleep(Duration::from_millis(backoff)).await;
                    }
                }
            }
        }

        let error = last_error.unwrap_or_else(|| NodeError::permanent("unknown", "нет результата"));
        // Исчерпанный retry по повторяемой ошибке — это dead letter; ошибка,
        // которую повторять нельзя, — обычный отказ. Смешивать их нельзя:
        // первая говорит «внешняя система не отвечает», вторая — «так делать
        // нельзя».
        let state = if error.retryable && attempt_number >= max_attempts && max_attempts > 1 {
            NodeState::DeadLetter
        } else if error.code == "timeout" {
            NodeState::TimedOut
        } else {
            NodeState::Failed
        };
        self.fail_node(
            run_id,
            node,
            attempt_number,
            &error.code,
            &error.message,
            state,
        )
        .await?;
        Ok(NodeStep::Executed)
    }

    async fn cancel_requested(&self, run_id: &str) -> Result<bool, RuntimeError> {
        Ok(self
            .journal
            .workflow_run(run_id)
            .await?
            .map(|run| run.cancel_requested)
            .unwrap_or(false))
    }

    async fn close_attempt(
        &self,
        attempt_id: &str,
        outcome: &str,
        error_code: &str,
    ) -> Result<(), RuntimeError> {
        let now_ms = crate::task_memory::now_millis() as i64;
        let database = self.journal.database.lock().await;
        store::finish_attempt(
            database.connection(),
            attempt_id,
            outcome,
            error_code,
            now_ms,
        )?;
        Ok(())
    }

    async fn fail_node(
        &self,
        run_id: &str,
        node: &WorkflowNode,
        attempts: u32,
        error_code: &str,
        message: &str,
        state: NodeState,
    ) -> Result<(), RuntimeError> {
        self.terminate_node(run_id, node, attempts, state, error_code, message)
            .await
    }

    async fn terminate_node(
        &self,
        run_id: &str,
        node: &WorkflowNode,
        attempts: u32,
        state: NodeState,
        error_code: &str,
        message: &str,
    ) -> Result<(), RuntimeError> {
        let now_ms = crate::task_memory::now_millis() as i64;
        {
            let database = self.journal.database.lock().await;
            store::update_node_state(
                database.connection(),
                store::UpdateNodeStateInput {
                    run_id,
                    node_id: &node.id,
                    state,
                    attempts,
                    output_json: "",
                    error_code,
                    error_message: &bounded_text(message),
                    now_ms,
                },
            )?;
        }
        let event_type = match state {
            NodeState::Cancelled => "workflow.node_cancelled",
            NodeState::Denied => "workflow.node_denied",
            NodeState::DeadLetter => "workflow.node_dead_letter",
            NodeState::TimedOut => "workflow.node_timed_out",
            _ => "workflow.node_failed",
        };
        self.emit(
            run_id,
            &node.id,
            "",
            event_type,
            json!({
                "attempt": attempts,
                "error_code": error_code,
                "message": bounded_text(message),
            }),
        )
        .await;
        Ok(())
    }

    /// Помечает узлы, чьи обязательные входы уже недостижимы. Возвращает
    /// `true`, если состояние изменилось.
    async fn mark_blocked_nodes(
        &self,
        run_id: &str,
        graph: &WorkflowGraph,
        states: &BTreeMap<String, WorkflowNodeRecord>,
    ) -> Result<bool, RuntimeError> {
        let mut changed = false;
        for node in &graph.nodes {
            let Some(record) = states.get(&node.id) else {
                continue;
            };
            if record.state.is_terminal() {
                continue;
            }
            if let Some(blocking) = blocking_dependency(graph, node, states) {
                let now_ms = crate::task_memory::now_millis() as i64;
                {
                    let database = self.journal.database.lock().await;
                    store::update_node_state(
                        database.connection(),
                        store::UpdateNodeStateInput {
                            run_id,
                            node_id: &node.id,
                            state: NodeState::Blocked,
                            attempts: record.attempts,
                            output_json: "",
                            error_code: "dependency_failed",
                            error_message: &bounded_text(&format!(
                                "зависимость {blocking} не выполнена"
                            )),
                            now_ms,
                        },
                    )?;
                }
                self.emit(
                    run_id,
                    &node.id,
                    "",
                    "workflow.node_blocked",
                    json!({ "blocking_dependency": blocking }),
                )
                .await;
                changed = true;
            }
        }
        Ok(changed)
    }

    async fn cancel_open_nodes(
        &self,
        run_id: &str,
        states: &BTreeMap<String, WorkflowNodeRecord>,
    ) -> Result<(), RuntimeError> {
        for record in states.values() {
            if record.state.is_terminal() {
                continue;
            }
            let now_ms = crate::task_memory::now_millis() as i64;
            {
                let database = self.journal.database.lock().await;
                store::update_node_state(
                    database.connection(),
                    store::UpdateNodeStateInput {
                        run_id,
                        node_id: &record.node_id,
                        state: NodeState::Cancelled,
                        attempts: record.attempts,
                        output_json: "",
                        error_code: "cancelled",
                        error_message: "",
                        now_ms,
                    },
                )?;
            }
            self.emit(
                run_id,
                &record.node_id,
                "",
                "workflow.node_cancelled",
                json!({ "attempt": record.attempts }),
            )
            .await;
        }
        Ok(())
    }

    async fn set_run_state(
        &self,
        run_id: &str,
        state: RunState,
        reason: &str,
    ) -> Result<(), RuntimeError> {
        let now_ms = crate::task_memory::now_millis() as i64;
        let database = self.journal.database.lock().await;
        store::update_run_state(database.connection(), run_id, state, reason, now_ms)?;
        Ok(())
    }

    async fn finish_run(
        &self,
        run_id: &str,
        state: RunState,
        reason: &str,
    ) -> Result<(), RuntimeError> {
        self.set_run_state(run_id, state, reason).await?;
        let event_type = match state {
            RunState::Completed => "workflow.run_completed",
            RunState::Cancelled => "workflow.run_cancelled",
            RunState::Degraded => "workflow.run_degraded",
            _ => "workflow.run_failed",
        };
        self.emit(
            run_id,
            "",
            "",
            event_type,
            json!({ "reason": bounded_text(reason) }),
        )
        .await;
        Ok(())
    }

    async fn emit(
        &self,
        run_id: &str,
        node_id: &str,
        attempt_id: &str,
        event_type: &str,
        payload: Value,
    ) {
        let sequence = self
            .journal
            .append_workflow_event(run_id, node_id, attempt_id, event_type, &payload)
            .await
            .unwrap_or(-1);
        if let Some(events) = &self.events {
            let projection = WorkflowEventProjection {
                run_id: run_id.to_string(),
                sequence,
                node_id: node_id.to_string(),
                event_type: event_type.to_string(),
                state: payload
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                attempt: payload
                    .get("attempt")
                    .and_then(Value::as_u64)
                    .unwrap_or_default() as u32,
                error_code: payload
                    .get("error_code")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                message: payload
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            };
            let _ = events.send(crate::CoreEvent::WorkflowProgress {
                run_id: run_id.to_string(),
                projection: Box::new(projection),
            });
        }
    }

    /// Bounded projection запуска для IPC.
    pub async fn projection(
        &self,
        run_id: &str,
    ) -> Result<Option<WorkflowRunProjection>, RuntimeError> {
        let Some(run) = self.journal.workflow_run(run_id).await? else {
            return Ok(None);
        };
        let graph: WorkflowGraph = serde_json::from_str(&run.graph_json)
            .map_err(|error| RuntimeError::InvalidGraph(error.to_string()))?;
        let nodes = self.journal.workflow_run_nodes(run_id).await?;
        let projection = WorkflowRunProjection {
            run_id: run.run_id.clone(),
            task_id: run.task_id.clone(),
            template_id: run.template_id.clone(),
            template_version: run.template_version,
            graph_id: run.graph_id.clone(),
            graph_version: run.graph_version,
            graph_hash: run.graph_hash.clone(),
            state: run.state.as_str().to_string(),
            terminal_reason: bounded_text(&run.terminal_reason),
            created_at_ms: run.created_at_ms,
            updated_at_ms: run.updated_at_ms,
            nodes: nodes
                .iter()
                .map(|record| WorkflowNodeProjection {
                    node_id: record.node_id.clone(),
                    action_kind: record.action_kind.clone(),
                    role: graph
                        .node(&record.node_id)
                        .map(node_role)
                        .unwrap_or_default(),
                    state: record.state.as_str().to_string(),
                    attempts: record.attempts,
                    error_code: record.error_code.clone(),
                    message: bounded_text(&record.error_message),
                    approval_id: record.approval_id.clone(),
                    dependencies: dependencies(&graph, &record.node_id),
                })
                .collect(),
        };
        Ok(Some(projection))
    }
}

enum NodeStep {
    Executed,
    WaitingApproval,
    Cancelled,
}

/// Роль узла в терминах продукта: она безопасна для renderer, в отличие от
/// цели и контекста child.
fn node_role(node: &WorkflowNode) -> String {
    match &node.node_type {
        NodeType::Child { child } => child.role.clone(),
        NodeType::ContextProvider { provider } => provider.provider_id.clone(),
        NodeType::McpTool { mcp } => format!("{}:{}", mcp.server_id, mcp.tool_name),
        NodeType::Tool { tool } => tool.tool_name.clone(),
        other => other.action_kind().to_string(),
    }
}

fn dependencies(graph: &WorkflowGraph, node_id: &str) -> Vec<String> {
    let mut result: BTreeSet<String> = BTreeSet::new();
    for edge in &graph.edges {
        if edge.to_node == node_id {
            result.insert(edge.from_node.clone());
        }
    }
    result.into_iter().collect()
}

/// Граф из одного узла: используется для повторной проверки capability прямо
/// перед эффектом, без повторной валидации всего графа.
fn single_node_graph(graph: &WorkflowGraph, node: &WorkflowNode) -> WorkflowGraph {
    WorkflowGraph {
        contract: graph.contract.clone(),
        graph_id: graph.graph_id.clone(),
        version: graph.version,
        entry_node: node.id.clone(),
        nodes: vec![node.clone()],
        edges: Vec::new(),
        budget: graph.budget,
    }
}

fn parent_from_policy(policy_json: &str) -> ParentCapabilities {
    #[derive(Deserialize)]
    struct Policy {
        #[serde(default)]
        grants: BTreeSet<String>,
        #[serde(default)]
        budget: Option<crate::workflow::NodeBudget>,
        #[serde(default)]
        context_allowlist: BTreeSet<String>,
    }
    let parsed: Policy = serde_json::from_str(policy_json).unwrap_or(Policy {
        grants: BTreeSet::new(),
        budget: None,
        context_allowlist: BTreeSet::new(),
    });
    ParentCapabilities {
        grants: parsed.grants,
        budget: parsed.budget.unwrap_or(crate::workflow::NodeBudget {
            max_tokens: u64::MAX,
            max_seconds: u64::MAX,
            max_tool_calls: u64::MAX,
        }),
        context_allowlist: parsed.context_allowlist,
    }
}

/// Зависимость, из-за которой узел больше не может выполниться.
fn blocking_dependency(
    graph: &WorkflowGraph,
    node: &WorkflowNode,
    states: &BTreeMap<String, WorkflowNodeRecord>,
) -> Option<String> {
    let incoming: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| edge.to_node == node.id)
        .collect();
    if incoming.is_empty() {
        return None;
    }
    let data_edges: Vec<_> = incoming
        .iter()
        .filter(|edge| edge.channel == EdgeChannel::Data)
        .collect();
    let failure_edges: Vec<_> = incoming
        .iter()
        .filter(|edge| edge.channel == EdgeChannel::Failure)
        .collect();

    // Failure-ветвь жива, пока её источник не завершился успехом.
    let failure_alive = failure_edges.iter().any(|edge| {
        states
            .get(&edge.from_node)
            .map(|record| {
                !record.state.is_terminal() || matches!(record.state, failed if is_failure(failed))
            })
            .unwrap_or(false)
    });
    if failure_alive {
        return None;
    }

    let mut satisfied = 0usize;
    let mut lost: Option<String> = None;
    for edge in &data_edges {
        match states.get(&edge.from_node).map(|record| record.state) {
            Some(state) if is_success(state) => satisfied += 1,
            Some(state) if state.is_terminal() => {
                lost.get_or_insert_with(|| edge.from_node.clone());
            }
            _ => return None,
        }
    }
    match node.join {
        JoinMode::All => lost,
        JoinMode::Any => {
            if satisfied == 0 && !data_edges.is_empty() {
                lost
            } else {
                None
            }
        }
    }
}

fn is_success(state: NodeState) -> bool {
    matches!(state, NodeState::Succeeded | NodeState::Degraded)
}

fn is_failure(state: NodeState) -> bool {
    matches!(
        state,
        NodeState::Failed | NodeState::TimedOut | NodeState::DeadLetter
    )
}

/// Узлы, у которых получены все обязательные входы.
/// Узлы, готовые к постановке в очередь. Функция чистая: она получает
/// снимок состояний и не обращается ни к базе, ни к часам, поэтому её же
/// используют deterministic evals.
pub fn ready_nodes(
    graph: &WorkflowGraph,
    states: &BTreeMap<String, WorkflowNodeRecord>,
) -> Vec<String> {
    let mut ready = Vec::new();
    for node in &graph.nodes {
        let Some(record) = states.get(&node.id) else {
            continue;
        };
        // Узел, ожидающий подтверждения, снова попадает в кандидаты: решение
        // могло прийти между прогонами драйвера, и шлюз перепроверит его.
        if !matches!(
            record.state,
            NodeState::Pending | NodeState::Ready | NodeState::WaitingApproval
        ) {
            continue;
        }
        let incoming: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| edge.to_node == node.id)
            .collect();
        if incoming.is_empty() {
            ready.push(node.id.clone());
            continue;
        }
        let data_ready = incoming
            .iter()
            .filter(|edge| edge.channel == EdgeChannel::Data)
            .map(|edge| states.get(&edge.from_node).map(|record| record.state))
            .collect::<Vec<_>>();
        let failure_ready = incoming
            .iter()
            .filter(|edge| edge.channel == EdgeChannel::Failure)
            .map(|edge| states.get(&edge.from_node).map(|record| record.state))
            .collect::<Vec<_>>();

        // Failure-ветвь становится готовой ровно тогда, когда её источник
        // действительно отказал, и только если источник объявил ветвление.
        if !failure_ready.is_empty()
            && failure_ready
                .iter()
                .all(|state| state.map(is_failure).unwrap_or(false))
        {
            ready.push(node.id.clone());
            continue;
        }
        if data_ready.is_empty() {
            continue;
        }
        let satisfied = data_ready
            .iter()
            .filter(|state| state.map(is_success).unwrap_or(false))
            .count();
        let all = satisfied == data_ready.len();
        let enough = match node.join {
            JoinMode::All => all,
            JoinMode::Any => satisfied > 0,
        };
        if enough {
            ready.push(node.id.clone());
        }
    }
    ready.sort();
    ready
}

/// Терминальное состояние запуска, когда готовых узлов больше нет.
/// Терминальное состояние запуска по снимку состояний узлов. Чистая
/// функция: тот же снимок всегда даёт тот же ответ.
pub fn terminal_state(
    graph: &WorkflowGraph,
    states: &BTreeMap<String, WorkflowNodeRecord>,
) -> Option<(RunState, String)> {
    let mut has_open = false;
    let mut has_failure = false;
    let mut has_degraded = false;
    let mut dead_letter: Option<String> = None;
    for node in &graph.nodes {
        let Some(record) = states.get(&node.id) else {
            continue;
        };
        match record.state {
            NodeState::Pending | NodeState::Ready | NodeState::Running => has_open = true,
            NodeState::WaitingApproval => return None,
            NodeState::Degraded => has_degraded = true,
            NodeState::DeadLetter => {
                has_failure = true;
                dead_letter.get_or_insert_with(|| record.node_id.clone());
            }
            NodeState::Failed
            | NodeState::TimedOut
            | NodeState::Denied
            | NodeState::Blocked
            | NodeState::UnknownOutcome => has_failure = true,
            NodeState::Cancelled => return Some((RunState::Cancelled, "cancelled".into())),
            NodeState::Succeeded | NodeState::Skipped => {}
        }
    }
    if has_open {
        return None;
    }
    if let Some(node_id) = dead_letter {
        return Some((RunState::Failed, format!("dead_letter:{node_id}")));
    }
    if has_failure {
        return Some((RunState::Failed, "node_failed".into()));
    }
    if has_degraded {
        return Some((RunState::Degraded, "degraded_source".into()));
    }
    Some((RunState::Completed, "completed".into()))
}

/// Bounded fan-out: параллельно выполняется только группа узлов, каждый из
/// которых объявил `ConcurrencyClass::Parallel` и не является stateful.
/// Выбор bounded-группы для параллельного исполнения. Чистая функция.
pub fn select_batch(graph: &WorkflowGraph, ready: &[String], max_parallel: usize) -> Vec<String> {
    let max_parallel = max_parallel.max(1);
    let mut batch = Vec::new();
    for node_id in ready {
        let Some(node) = graph.node(node_id) else {
            continue;
        };
        let parallel_safe =
            node.concurrency == ConcurrencyClass::Parallel && !node.execution.approval.required;
        if batch.is_empty() {
            batch.push(node_id.clone());
            if !parallel_safe {
                // Stateful-узел и узел с approval выполняются в одиночку.
                break;
            }
            continue;
        }
        if parallel_safe && batch.len() < max_parallel {
            batch.push(node_id.clone());
        } else {
            break;
        }
    }
    batch
}

/// Собирает входы узла из выходов уже выполненных зависимостей. Отсутствие
/// обязательного входа — ошибка, а не пустое значение.
/// Сбор входов узла из выходов зависимостей. Чистая функция.
pub fn collect_inputs(
    graph: &WorkflowGraph,
    node: &WorkflowNode,
    states: &BTreeMap<String, WorkflowNodeRecord>,
) -> Result<BTreeMap<String, Value>, NodeError> {
    let mut inputs = BTreeMap::new();
    for edge in graph.edges.iter().filter(|edge| edge.to_node == node.id) {
        let Some(record) = states.get(&edge.from_node) else {
            continue;
        };
        match edge.channel {
            EdgeChannel::Data => {
                if !is_success(record.state) {
                    continue;
                }
                let value: Value = serde_json::from_str(&record.output_json).unwrap_or(Value::Null);
                let value = value.get(&edge.from_port).cloned().unwrap_or(value);
                inputs.insert(edge.to_port.clone(), value);
            }
            EdgeChannel::Failure => {
                if !is_failure(record.state) {
                    continue;
                }
                inputs.insert(
                    edge.to_port.clone(),
                    json!({
                        "failed_node": record.node_id,
                        "error_code": record.error_code,
                    }),
                );
            }
        }
    }
    for port in node.inputs.iter().filter(|port| port.required) {
        if !inputs.contains_key(&port.name) {
            return Err(NodeError::permanent(
                "missing_required_input",
                format!("обязательный вход {} не получен", port.name),
            ));
        }
    }
    if let Some(batch) = node.batch {
        for value in inputs.values() {
            if let Some(items) = value.as_array() {
                if items.len() > batch.max_items as usize {
                    return Err(NodeError::permanent(
                        "batch_overflow",
                        format!(
                            "batch содержит {} элементов при пределе {}",
                            items.len(),
                            batch.max_items
                        ),
                    ));
                }
            }
        }
    }
    Ok(inputs)
}

/// Проверка acceptance contract до того, как результат станет входом
/// следующего узла.
/// Проверка acceptance contract узла. Чистая функция, вынесенная наружу для
/// deterministic evals.
pub fn check_acceptance(node: &WorkflowNode, success: &NodeSuccess) -> Result<(), NodeError> {
    let acceptance = &node.acceptance;
    if !acceptance.allowed_statuses.is_empty()
        && !acceptance
            .allowed_statuses
            .iter()
            .any(|status| status == &success.status)
    {
        return Err(NodeError::permanent(
            "status_not_allowed",
            format!("статус {} не разрешён контрактом узла", success.status),
        ));
    }
    if success.evidence < acceptance.required_evidence {
        return Err(NodeError::permanent(
            "insufficient_evidence",
            format!(
                "получено {} evidence при требуемых {}",
                success.evidence, acceptance.required_evidence
            ),
        ));
    }
    let schema = acceptance.output_schema.as_ref().or(match &node.node_type {
        NodeType::Child { child } => child.output_schema.as_ref(),
        _ => None,
    });
    if let Some(schema) = schema {
        let Ok(parsed) = serde_json::from_str::<Value>(schema) else {
            return Err(NodeError::permanent(
                "invalid_output_schema",
                "схема результата узла нечитаема",
            ));
        };
        let Ok(compiled) = jsonschema::JSONSchema::compile(&parsed) else {
            return Err(NodeError::permanent(
                "invalid_output_schema",
                "схема результата узла не компилируется",
            ));
        };
        if !compiled.is_valid(&success.output) {
            return Err(NodeError::permanent(
                "output_schema_violation",
                "результат узла не соответствует объявленной схеме",
            ));
        }
    }
    Ok(())
}

/// Повтор разрешён только для ошибок, объявленных повторяемыми, и только если
/// узел не сузил список классов ошибок.
/// Разрешён ли повтор попытки. Чистая функция.
pub fn is_retryable(node: &WorkflowNode, error: &NodeError) -> bool {
    if !error.retryable {
        return false;
    }
    let classes = &node.acceptance.retryable_error_classes;
    if !classes.is_empty() {
        return classes.iter().any(|class| class == &error.code);
    }
    let patterns = &node.execution.retry.retryable_errors;
    if patterns.is_empty() {
        return true;
    }
    patterns.iter().any(|pattern| {
        error.code.contains(pattern.as_str()) || error.message.contains(pattern.as_str())
    })
}

fn bounded_text(value: &str) -> String {
    value.chars().take(MAX_EVENT_MESSAGE_CHARS).collect()
}

fn hash_value(value: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.to_string().as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{
        ApprovalPolicy, CancellationPolicy, ExecutionPolicy, FailurePolicy, NodeAcceptance,
        PortType, RetryPolicy, WorkflowBudget, WorkflowEdge, WORKFLOW_CONTRACT_VERSION,
    };
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    fn journal() -> (EventJournal, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("core.db");
        let journal = EventJournal::open(&path).expect("journal");
        (journal, directory)
    }

    fn policy(max_attempts: u32, approval: bool) -> ExecutionPolicy {
        ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts,
                backoff_ms: if max_attempts > 1 { 1 } else { 0 },
                retryable_errors: vec![],
            },
            timeout_ms: 2_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: approval,
                reason: if approval {
                    Some("тест".into())
                } else {
                    None
                },
            },
        }
    }

    fn transform(id: &str) -> WorkflowNode {
        WorkflowNode::new(id, NodeType::Transform, policy(1, false))
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>, entry: &str) -> WorkflowGraph {
        WorkflowGraph {
            contract: WORKFLOW_CONTRACT_VERSION.into(),
            graph_id: "runtime.test".into(),
            version: 1,
            entry_node: entry.into(),
            nodes,
            edges,
            budget: WorkflowBudget::default(),
        }
    }

    #[derive(Default)]
    struct ScriptedAdapter {
        calls: Mutex<Vec<String>>,
        attempts: AtomicU32,
        fail_nodes: Vec<String>,
        fail_first_n: u32,
        degraded_nodes: Vec<String>,
    }

    impl NodeAdapter for ScriptedAdapter {
        fn execute<'a>(&'a self, invocation: NodeInvocation<'a>) -> NodeFuture<'a> {
            let node_id = invocation.node.id.clone();
            self.calls.lock().unwrap().push(node_id.clone());
            let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
            let fails = self.fail_nodes.contains(&node_id);
            let degraded = self.degraded_nodes.contains(&node_id);
            let fail_first_n = self.fail_first_n;
            Box::pin(async move {
                if fails && attempt <= fail_first_n.max(1) {
                    return Err(NodeError::transient("transient", "временный сбой"));
                }
                let mut success = NodeSuccess::new(json!({"out": node_id, "text": node_id}));
                if degraded {
                    success = success.degraded("degraded");
                }
                Ok(success)
            })
        }
    }

    struct ApprovedGate;
    impl WorkflowApprovalGate for ApprovedGate {
        fn decide<'a>(&'a self, _run_id: &'a str, _node: &'a WorkflowNode) -> ApprovalFuture<'a> {
            Box::pin(async { ApprovalOutcome::Approved })
        }
    }

    struct DeniedGate;
    impl WorkflowApprovalGate for DeniedGate {
        fn decide<'a>(&'a self, _run_id: &'a str, _node: &'a WorkflowNode) -> ApprovalFuture<'a> {
            Box::pin(async { ApprovalOutcome::Denied })
        }
    }

    fn runtime(
        journal: &EventJournal,
        adapter: Arc<dyn NodeAdapter>,
        approvals: Arc<dyn WorkflowApprovalGate>,
    ) -> WorkflowRuntime {
        WorkflowRuntime::new(
            journal.clone(),
            Arc::new(WorkflowRegistry::empty()),
            adapter,
            approvals,
            "core-test",
        )
    }

    fn request(run_id: &str, graph: WorkflowGraph) -> StartWorkflowRequest {
        StartWorkflowRequest {
            run_id: run_id.into(),
            task_id: "task-1".into(),
            workspace_path: String::new(),
            template_id: "test".into(),
            template_version: 1,
            inputs: BTreeMap::new(),
            graph,
            parent: ParentCapabilities::default(),
        }
    }

    #[tokio::test]
    async fn a_sequential_run_executes_in_dependency_order_and_completes() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Text),
                transform("b")
                    .with_input("in", PortType::Text, true)
                    .with_output("out", PortType::Text),
                transform("c").with_input("in", PortType::Text, true),
            ],
            vec![
                WorkflowEdge::data("a", "out", "b", "in"),
                WorkflowEdge::data("b", "out", "c", "in"),
            ],
            "a",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Completed);
        assert_eq!(*adapter.calls.lock().unwrap(), vec!["a", "b", "c"]);

        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.state, "completed");
        assert!(projection
            .nodes
            .iter()
            .all(|node| node.state == "succeeded"));
    }

    #[tokio::test]
    async fn a_diamond_fans_out_and_fans_in_deterministically() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let mut left = transform("left")
            .with_input("in", PortType::Text, true)
            .with_output("out", PortType::Text);
        left.concurrency = ConcurrencyClass::Parallel;
        let mut right = transform("right")
            .with_input("in", PortType::Text, true)
            .with_output("out", PortType::Text);
        right.concurrency = ConcurrencyClass::Parallel;
        let graph = graph(
            vec![
                transform("start").with_output("out", PortType::Text),
                left,
                right,
                transform("join")
                    .with_input("left", PortType::Text, true)
                    .with_input("right", PortType::Text, true),
            ],
            vec![
                WorkflowEdge::data("start", "out", "left", "in"),
                WorkflowEdge::data("start", "out", "right", "in"),
                WorkflowEdge::data("left", "out", "join", "left"),
                WorkflowEdge::data("right", "out", "join", "right"),
            ],
            "start",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Completed);
        let calls = adapter.calls.lock().unwrap().clone();
        assert_eq!(calls.first().map(String::as_str), Some("start"));
        assert_eq!(calls.last().map(String::as_str), Some("join"));
        assert!(calls.contains(&"left".to_string()) && calls.contains(&"right".to_string()));
    }

    #[tokio::test]
    async fn an_unconnected_failure_blocks_downstream_instead_of_succeeding() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter {
            fail_nodes: vec!["a".into()],
            fail_first_n: u32::MAX,
            ..Default::default()
        });
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Text),
                transform("b").with_input("in", PortType::Text, true),
            ],
            vec![WorkflowEdge::data("a", "out", "b", "in")],
            "a",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        assert_eq!(*adapter.calls.lock().unwrap(), vec!["a"]);
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        let downstream = projection
            .nodes
            .iter()
            .find(|node| node.node_id == "b")
            .expect("b");
        assert_eq!(downstream.state, "blocked");
    }

    #[tokio::test]
    async fn a_declared_failure_branch_continues_only_the_allowed_fallback() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter {
            fail_nodes: vec!["a".into()],
            fail_first_n: u32::MAX,
            ..Default::default()
        });
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let mut source = transform("a")
            .with_output("out", PortType::Text)
            .with_output("error", PortType::Json);
        source.on_failure = FailurePolicy::Branch;
        let graph = graph(
            vec![
                source,
                transform("happy").with_input("in", PortType::Text, true),
                transform("fallback").with_input("error", PortType::Json, true),
            ],
            vec![
                WorkflowEdge::data("a", "out", "happy", "in"),
                WorkflowEdge::failure("a", "error", "fallback", "error"),
            ],
            "a",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        runtime.drive("run-1").await.expect("drive");
        let calls = adapter.calls.lock().unwrap().clone();
        assert!(calls.contains(&"fallback".to_string()));
        assert!(!calls.contains(&"happy".to_string()));
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        let happy = projection
            .nodes
            .iter()
            .find(|node| node.node_id == "happy")
            .expect("happy");
        assert_eq!(happy.state, "blocked");
    }

    #[tokio::test]
    async fn retry_is_bounded_and_exhaustion_becomes_a_dead_letter() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter {
            fail_nodes: vec!["a".into()],
            fail_first_n: u32::MAX,
            ..Default::default()
        });
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let mut node = transform("a");
        node.execution = policy(3, false);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        assert_eq!(adapter.calls.lock().unwrap().len(), 3);
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].state, "dead_letter");
        assert!(projection.terminal_reason.starts_with("dead_letter:"));
    }

    #[tokio::test]
    async fn a_non_retryable_error_is_not_retried() {
        struct PermanentAdapter {
            calls: AtomicU32,
        }
        impl NodeAdapter for PermanentAdapter {
            fn execute<'a>(&'a self, _invocation: NodeInvocation<'a>) -> NodeFuture<'a> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    Err(NodeError::permanent("invalid_input", "нельзя повторять"))
                })
            }
        }
        let (journal, _dir) = journal();
        let adapter = Arc::new(PermanentAdapter {
            calls: AtomicU32::new(0),
        });
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let mut node = transform("a");
        node.execution = policy(5, false);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        runtime.drive("run-1").await.expect("drive");
        assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn approval_is_requested_before_the_effect_and_denial_stops_the_node() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(DeniedGate));
        let mut node = transform("a");
        node.execution = policy(1, true);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        assert!(adapter.calls.lock().unwrap().is_empty());
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].state, "denied");
    }

    #[tokio::test]
    async fn a_pending_approval_parks_the_run_without_executing_the_node() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(AlwaysPendingApproval));
        let mut node = transform("a");
        node.execution = policy(1, true);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::WaitingApproval);
        assert_eq!(outcome.waiting_approval, vec!["a".to_string()]);
        assert!(adapter.calls.lock().unwrap().is_empty());

        // Тот же approval-идентификатор виден в projection: renderer решает
        // его существующей командой ResolveApproval.
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].approval_id, "run-1:a");
        assert_eq!(projection.state, "waiting_approval");
    }

    #[tokio::test]
    async fn cancellation_stops_the_run_and_marks_open_nodes() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Text),
                transform("b").with_input("in", PortType::Text, true),
            ],
            vec![WorkflowEdge::data("a", "out", "b", "in")],
            "a",
        );
        runtime.start(request("run-1", graph)).await.expect("start");
        journal
            .request_workflow_cancel("run-1", crate::task_memory::now_millis() as i64)
            .await
            .expect("cancel");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Cancelled);
        assert!(adapter.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn output_schema_violation_is_not_accepted_as_a_result() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter, Arc::new(ApprovedGate));
        let mut node = transform("a");
        node.acceptance = NodeAcceptance {
            output_schema: Some(r#"{"type":"object","required":["required_field"]}"#.to_string()),
            ..Default::default()
        };
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].error_code, "output_schema_violation");
    }

    #[tokio::test]
    async fn insufficient_evidence_is_rejected_before_fan_in() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter, Arc::new(ApprovedGate));
        let mut node = transform("a");
        node.acceptance = NodeAcceptance {
            required_evidence: 2,
            ..Default::default()
        };
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");
        runtime.drive("run-1").await.expect("drive");
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].error_code, "insufficient_evidence");
    }

    #[tokio::test]
    async fn a_degraded_source_finishes_the_run_as_degraded_not_completed() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter {
            degraded_nodes: vec!["a".into()],
            ..Default::default()
        });
        let runtime = runtime(&journal, adapter, Arc::new(ApprovedGate));
        runtime
            .start(request("run-1", graph(vec![transform("a")], vec![], "a")))
            .await
            .expect("start");
        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Degraded);
    }

    #[tokio::test]
    async fn a_restart_between_dispatch_and_result_never_retries_blindly() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        runtime
            .start(request(
                "run-1",
                graph(
                    vec![transform("a").with_output("out", PortType::Text)],
                    vec![],
                    "a",
                ),
            ))
            .await
            .expect("start");
        // Падение сразу после dispatch marker.
        {
            let database = journal.database.lock().await;
            store::begin_attempt(
                database.connection(),
                &WorkflowAttemptRecord {
                    attempt_id: "run-1:a:1".into(),
                    run_id: "run-1".into(),
                    node_id: "a".into(),
                    attempt: 1,
                    graph_hash: "hash".into(),
                    input_hash: String::new(),
                    dispatched_at_ms: 1,
                    completed_at_ms: None,
                    outcome: String::new(),
                    error_code: String::new(),
                },
            )
            .expect("dispatch");
        }
        let recovery = journal
            .recover_workflow_runs(crate::task_memory::now_millis() as i64)
            .await
            .expect("recovery");
        assert_eq!(recovery.interrupted_runs, vec!["run-1".to_string()]);

        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::Failed);
        assert!(adapter.calls.lock().unwrap().is_empty());
        let projection = runtime
            .projection("run-1")
            .await
            .expect("projection")
            .expect("run");
        assert_eq!(projection.nodes[0].state, "unknown_outcome");
    }

    #[tokio::test]
    async fn a_tampered_graph_snapshot_is_refused_before_any_effect() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter.clone(), Arc::new(ApprovedGate));
        runtime
            .start(request("run-1", graph(vec![transform("a")], vec![], "a")))
            .await
            .expect("start");
        {
            let database = journal.database.lock().await;
            database
                .connection()
                .execute(
                    "UPDATE workflow_runs SET graph_hash = 'tampered' WHERE run_id = 'run-1'",
                    [],
                )
                .expect("tamper");
        }
        let error = runtime.drive("run-1").await.expect_err("hash mismatch");
        assert_eq!(error.code(), "graph_hash_mismatch");
        assert!(adapter.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn events_are_durable_monotonic_and_bounded() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let runtime = runtime(&journal, adapter, Arc::new(ApprovedGate));
        runtime
            .start(request("run-1", graph(vec![transform("a")], vec![], "a")))
            .await
            .expect("start");
        runtime.drive("run-1").await.expect("drive");
        let events = journal
            .list_workflow_events("run-1", -1, 100)
            .await
            .expect("events");
        assert!(events.len() >= 3);
        let sequences: Vec<i64> = events.iter().map(|event| event.run_sequence).collect();
        let mut sorted = sequences.clone();
        sorted.sort();
        assert_eq!(sequences, sorted);
        assert_eq!(events[0].event_type, "workflow.run_started");
        assert!(events
            .iter()
            .any(|event| event.event_type == "workflow.run_completed"));
        assert!(events
            .iter()
            .all(|event| event.payload_json.len() <= store::MAX_EVENT_PAYLOAD_BYTES));
    }

    #[tokio::test]
    async fn the_same_approval_id_resolves_the_parked_node_and_the_run_continues() {
        let (journal, _dir) = journal();
        let adapter = Arc::new(ScriptedAdapter::default());
        let approvals = Arc::new(WorkflowApprovalRegistry::new());
        let runtime = WorkflowRuntime::new(
            journal.clone(),
            Arc::new(WorkflowRegistry::empty()),
            adapter.clone(),
            approvals.clone(),
            "core-test",
        );
        let mut node = transform("a");
        node.execution = policy(1, true);
        runtime
            .start(request("run-1", graph(vec![node], vec![], "a")))
            .await
            .expect("start");

        let outcome = runtime.drive("run-1").await.expect("drive");
        assert_eq!(outcome.state, RunState::WaitingApproval);
        assert!(adapter.calls.lock().unwrap().is_empty());

        let approval_id = approvals.approval_id("run-1", "a");
        assert_eq!(approvals.run_for(&approval_id).as_deref(), Some("run-1"));
        assert!(approvals.resolve(&approval_id, true));
        assert!(!approvals.resolve("unknown-approval", true));

        let outcome = runtime.drive("run-1").await.expect("drive after approval");
        assert_eq!(outcome.state, RunState::Completed);
        assert_eq!(*adapter.calls.lock().unwrap(), vec!["a"]);
    }

    #[test]
    fn a_node_without_its_required_input_is_never_dispatched() {
        // Соединение есть, но зависимость ещё не дала значения: узел обязан
        // получить typed-ошибку до эффекта, а не пустой вход.
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Text),
                transform("b").with_input("in", PortType::Text, true),
            ],
            vec![WorkflowEdge::data("a", "out", "b", "in")],
            "a",
        );
        let states = BTreeMap::from([(
            "a".to_string(),
            WorkflowNodeRecord {
                run_id: "run-1".into(),
                node_id: "a".into(),
                action_kind: "transform".into(),
                state: NodeState::Failed,
                attempts: 1,
                output_json: String::new(),
                error_code: "boom".into(),
                error_message: String::new(),
                approval_id: String::new(),
                updated_at_ms: 0,
            },
        )]);
        let node = graph.node("b").expect("b");
        let error = collect_inputs(&graph, node, &states).expect_err("missing input");
        assert_eq!(error.code, "missing_required_input");
    }

    #[test]
    fn a_batch_input_above_its_bound_is_rejected_instead_of_multiplying_executions() {
        let mut node = transform("b").with_input("items", PortType::Json, true);
        node.batch = Some(crate::workflow::BatchPolicy { max_items: 2 });
        let graph = graph(
            vec![
                transform("a").with_output("out", PortType::Json),
                node.clone(),
            ],
            vec![WorkflowEdge::data("a", "out", "b", "items")],
            "a",
        );
        let states = BTreeMap::from([(
            "a".to_string(),
            WorkflowNodeRecord {
                run_id: "run-1".into(),
                node_id: "a".into(),
                action_kind: "transform".into(),
                state: NodeState::Succeeded,
                attempts: 1,
                output_json: json!({"out": [1, 2, 3]}).to_string(),
                error_code: String::new(),
                error_message: String::new(),
                approval_id: String::new(),
                updated_at_ms: 0,
            },
        )]);
        let error = collect_inputs(&graph, &node, &states).expect_err("batch bound");
        assert_eq!(error.code, "batch_overflow");
    }
}
