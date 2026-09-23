//! Durable-хранилище запусков workflow (план 06.2).
//!
//! Схема ставится идемпотентно через [`crate::workflow_store::install_schema`] — тем же способом, что
//! receipts и model provenance, поэтому существующая база получает таблицы без
//! отдельной ветки миграции и без потери данных.
//!
//! Модуль хранит только уже проверенные записи: контракт графа, allowlists и
//! policy проверяет `evohime_core::workflow`, а сюда попадает immutable
//! snapshot и его состояние. Две вещи здесь принципиальны:
//!
//! * dispatch marker пишется **до** эффекта и закрывается **после** него,
//!   поэтому падение между ними остаётся видимым как попытка с неизвестным
//!   исходом, а не как «можно повторить»;
//! * последовательность событий запуска монотонна и выдаётся транзакцией,
//!   поэтому replay не может увидеть дырку или переставленные события.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Потолки полей. Они совпадают по духу с bounded-лимитами контракта: запись,
/// которая не помещается, отклоняется, а не обрезается молча.
/// Maximum encoded byte length of workflow and node identifiers.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum serialized workflow graph size accepted by the store.
pub const MAX_GRAPH_JSON_BYTES: usize = 512 * 1024;
/// Maximum stored workflow input JSON size.
pub const MAX_INPUT_JSON_BYTES: usize = 32 * 1024;
/// Maximum stored node output JSON size.
pub const MAX_OUTPUT_JSON_BYTES: usize = 32 * 1024;
/// Maximum serialized workflow event payload size.
pub const MAX_EVENT_PAYLOAD_BYTES: usize = 8 * 1024;
/// Maximum stored error message size.
pub const MAX_ERROR_BYTES: usize = 2 * 1024;
/// Maximum number of rows returned by a list operation.
pub const MAX_LIST_LIMIT: usize = 500;

/// Validation and SQLite errors raised by workflow persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowStoreError {
    /// A required workflow field is empty.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the required field.
        field: &'static str,
    },
    /// A value exceeds its storage bound.
    #[error("{field} exceeds {max} bytes")]
    Limit {
        /// Name of the field that exceeded its bound.
        field: &'static str,
        /// Maximum permitted byte length.
        max: usize,
    },
    /// The requested workflow run does not exist.
    #[error("workflow run {0} is not stored")]
    UnknownRun(String),
    /// The requested node does not exist in the specified workflow run.
    #[error("workflow node {node_id} of run {run_id} is not stored")]
    UnknownNode {
        /// Workflow run containing the requested node.
        run_id: String,
        /// Identifier of the missing node.
        node_id: String,
    },
    /// SQLite rejected a persistence or query operation.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl PartialEq for WorkflowStoreError {
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
            (Self::UnknownRun(left), Self::UnknownRun(right)) => left == right,
            (
                Self::UnknownNode {
                    run_id: left_run,
                    node_id: left_node,
                },
                Self::UnknownNode {
                    run_id: right_run,
                    node_id: right_node,
                },
            ) => left_run == right_run && left_node == right_node,
            (Self::Sqlite(_), Self::Sqlite(_)) => true,
            _ => false,
        }
    }
}

fn bounded(
    field: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), WorkflowStoreError> {
    if required && value.trim().is_empty() {
        return Err(WorkflowStoreError::Empty { field });
    }
    if value.len() > max {
        return Err(WorkflowStoreError::Limit { field, max });
    }
    Ok(())
}

/// Состояние запуска. `Interrupted` и `Degraded` — самостоятельные состояния,
/// а не разновидность успеха: первое означает потерю определённости после
/// перезапуска Core, второе — завершение с частично недоступными источниками.
/// Lifecycle state of a persisted workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// The run is persisted but has not started execution.
    Pending,
    /// At least one node is actively executing.
    Running,
    /// Execution is paused pending human approval.
    WaitingApproval,
    /// All required workflow work completed successfully.
    Completed,
    /// The workflow ended with a failure.
    Failed,
    /// The workflow was cancelled by an explicit request.
    Cancelled,
    /// The workflow completed with degraded or unavailable sources.
    Degraded,
    /// Core restart interrupted the run before its outcome was known.
    Interrupted,
}

impl RunState {
    /// Returns the stable snake-case database representation.
    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Pending => "pending",
            RunState::Running => "running",
            RunState::WaitingApproval => "waiting_approval",
            RunState::Completed => "completed",
            RunState::Failed => "failed",
            RunState::Cancelled => "cancelled",
            RunState::Degraded => "degraded",
            RunState::Interrupted => "interrupted",
        }
    }

    /// Parses a stable database representation, returning `None` if unknown.
    ///
    /// ```
    /// use evohime_local_storage::workflow_store::RunState;
    /// assert_eq!(RunState::parse("waiting_approval"), Some(RunState::WaitingApproval));
    /// assert_eq!(RunState::parse("unknown"), None);
    /// ```
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => RunState::Pending,
            "running" => RunState::Running,
            "waiting_approval" => RunState::WaitingApproval,
            "completed" => RunState::Completed,
            "failed" => RunState::Failed,
            "cancelled" => RunState::Cancelled,
            "degraded" => RunState::Degraded,
            "interrupted" => RunState::Interrupted,
            _ => return None,
        })
    }

    /// Returns whether no further execution is expected for this run.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunState::Completed | RunState::Failed | RunState::Cancelled | RunState::Degraded
        )
    }
}

/// Lifecycle state of one node within a workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    /// The node is persisted but not yet eligible to run.
    Pending,
    /// Dependencies are satisfied and the node may be dispatched.
    Ready,
    /// The node's effect is currently executing.
    Running,
    /// The node is paused pending approval.
    WaitingApproval,
    /// The node completed successfully.
    Succeeded,
    /// The node ended with an execution failure.
    Failed,
    /// The node exceeded its configured execution deadline.
    TimedOut,
    /// The node was cancelled.
    Cancelled,
    /// A dependency or policy prevents execution.
    Blocked,
    /// Approval for the node was denied.
    Denied,
    /// The node was intentionally not executed.
    Skipped,
    /// The node completed with a degraded result.
    Degraded,
    /// Core упал после dispatch marker: исход эффекта неизвестен, слепой
    /// повтор запрещён.
    UnknownOutcome,
    /// Repeatedly failed or uncertain work was parked for operator review.
    DeadLetter,
}

impl NodeState {
    /// Returns the stable snake-case database representation.
    pub fn as_str(self) -> &'static str {
        match self {
            NodeState::Pending => "pending",
            NodeState::Ready => "ready",
            NodeState::Running => "running",
            NodeState::WaitingApproval => "waiting_approval",
            NodeState::Succeeded => "succeeded",
            NodeState::Failed => "failed",
            NodeState::TimedOut => "timed_out",
            NodeState::Cancelled => "cancelled",
            NodeState::Blocked => "blocked",
            NodeState::Denied => "denied",
            NodeState::Skipped => "skipped",
            NodeState::Degraded => "degraded",
            NodeState::UnknownOutcome => "unknown_outcome",
            NodeState::DeadLetter => "dead_letter",
        }
    }

    /// Parses a stable database representation, returning `None` if unknown.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => NodeState::Pending,
            "ready" => NodeState::Ready,
            "running" => NodeState::Running,
            "waiting_approval" => NodeState::WaitingApproval,
            "succeeded" => NodeState::Succeeded,
            "failed" => NodeState::Failed,
            "timed_out" => NodeState::TimedOut,
            "cancelled" => NodeState::Cancelled,
            "blocked" => NodeState::Blocked,
            "denied" => NodeState::Denied,
            "skipped" => NodeState::Skipped,
            "degraded" => NodeState::Degraded,
            "unknown_outcome" => NodeState::UnknownOutcome,
            "dead_letter" => NodeState::DeadLetter,
            _ => return None,
        })
    }

    /// Returns whether the node is no longer expected to execute.
    pub fn is_terminal(self) -> bool {
        !matches!(
            self,
            NodeState::Pending | NodeState::Ready | NodeState::Running | NodeState::WaitingApproval
        )
    }
}

/// Immutable graph and lifecycle metadata for one workflow execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRunRecord {
    /// Stable identifier of this execution.
    pub run_id: String,
    /// Task that owns this workflow run.
    pub task_id: String,
    /// Identifier of the workflow template used to create the run.
    pub template_id: String,
    /// Version of the workflow template.
    pub template_version: u32,
    /// Identifier of the persisted workflow graph.
    pub graph_id: String,
    /// Version of the graph snapshot used by this run.
    pub graph_version: u64,
    /// Digest of the canonical graph snapshot.
    pub graph_hash: String,
    /// Canonical JSON representation of the graph snapshot.
    pub graph_json: String,
    /// Canonical JSON input supplied to the run.
    pub inputs_json: String,
    /// Serialized policy snapshot used for this run.
    pub policy_json: String,
    /// Current lifecycle state.
    pub state: RunState,
    /// Run creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Time of the most recent run state update in Unix milliseconds.
    pub updated_at_ms: i64,
    /// Explanation recorded when the run reaches a terminal state.
    pub terminal_reason: String,
    /// Whether cancellation has been requested.
    pub cancel_requested: bool,
    /// Identity currently holding the run lease.
    pub lease_owner: String,
    /// Lease expiry time in Unix milliseconds.
    pub lease_expires_at_ms: i64,
}

/// Current execution state and result metadata for one workflow node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowNodeRecord {
    /// Workflow run containing this node.
    pub run_id: String,
    /// Identifier of the node in the workflow graph.
    pub node_id: String,
    /// Kind of action dispatched for the node.
    pub action_kind: String,
    /// Current execution state.
    pub state: NodeState,
    /// Number of dispatch attempts recorded for this node.
    pub attempts: u32,
    /// Serialized output produced by the node, if any.
    pub output_json: String,
    /// Stable error category from the latest attempt.
    pub error_code: String,
    /// Human-readable error detail from the latest attempt.
    pub error_message: String,
    /// Approval identifier associated with the node, if any.
    pub approval_id: String,
    /// Time of the latest node update in Unix milliseconds.
    pub updated_at_ms: i64,
}

/// Durable record of one dispatch attempt for a workflow node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowAttemptRecord {
    /// Stable identifier of this attempt.
    pub attempt_id: String,
    /// Workflow run containing this attempt.
    pub run_id: String,
    /// Node whose action was dispatched.
    pub node_id: String,
    /// One-based attempt number for the node.
    pub attempt: u32,
    /// Graph snapshot digest used for the attempt.
    pub graph_hash: String,
    /// Digest of the canonical action input.
    pub input_hash: String,
    /// Time the dispatch marker was persisted in Unix milliseconds.
    pub dispatched_at_ms: i64,
    /// Time the attempt was closed, or `None` while its outcome is unknown.
    pub completed_at_ms: Option<i64>,
    /// Пусто, пока попытка не закрыта. Именно эта пустота отличает
    /// «неизвестный исход» от «известной ошибки».
    pub outcome: String,
    /// Stable failure category, if the attempt failed.
    pub error_code: String,
}

/// Ordered event emitted during one workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEventRecord {
    /// Workflow run that owns the event.
    pub run_id: String,
    /// Monotonic sequence number within the run.
    pub run_sequence: i64,
    /// Node associated with the event, when applicable.
    pub node_id: String,
    /// Dispatch attempt associated with the event, when applicable.
    pub attempt_id: String,
    /// Stable event category.
    pub event_type: String,
    /// Serialized event-specific payload.
    pub payload_json: String,
    /// Event creation time in Unix milliseconds.
    pub created_at_ms: i64,
}

/// Ставит схему идемпотентно. Вызывается при каждом открытии базы.
pub fn install_schema(connection: &Connection) -> Result<(), WorkflowStoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workflow_runs (
            run_id TEXT PRIMARY KEY NOT NULL,
            task_id TEXT NOT NULL,
            template_id TEXT NOT NULL,
            template_version INTEGER NOT NULL,
            graph_id TEXT NOT NULL,
            graph_version INTEGER NOT NULL,
            graph_hash TEXT NOT NULL,
            graph_json TEXT NOT NULL,
            inputs_json TEXT NOT NULL,
            policy_json TEXT NOT NULL,
            state TEXT NOT NULL CHECK(state IN
                ('pending','running','waiting_approval','completed','failed',
                 'cancelled','degraded','interrupted')),
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            terminal_reason TEXT NOT NULL DEFAULT '',
            cancel_requested INTEGER NOT NULL DEFAULT 0,
            lease_owner TEXT NOT NULL DEFAULT '',
            lease_expires_at_ms INTEGER NOT NULL DEFAULT 0,
            next_sequence INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_state
            ON workflow_runs(state, updated_at_ms);
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_task
            ON workflow_runs(task_id, created_at_ms);
        CREATE TABLE IF NOT EXISTS workflow_run_nodes (
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
        CREATE TABLE IF NOT EXISTS workflow_node_attempts (
            attempt_id TEXT PRIMARY KEY NOT NULL,
            run_id TEXT NOT NULL REFERENCES workflow_runs(run_id) ON DELETE CASCADE,
            node_id TEXT NOT NULL,
            attempt INTEGER NOT NULL,
            graph_hash TEXT NOT NULL,
            input_hash TEXT NOT NULL DEFAULT '',
            dispatched_at_ms INTEGER NOT NULL,
            completed_at_ms INTEGER,
            outcome TEXT NOT NULL DEFAULT '',
            error_code TEXT NOT NULL DEFAULT '',
            UNIQUE(run_id, node_id, attempt)
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_attempts_open
            ON workflow_node_attempts(run_id, completed_at_ms);
        CREATE TABLE IF NOT EXISTS workflow_run_events (
            sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL REFERENCES workflow_runs(run_id) ON DELETE CASCADE,
            run_sequence INTEGER NOT NULL,
            node_id TEXT NOT NULL DEFAULT '',
            attempt_id TEXT NOT NULL DEFAULT '',
            event_type TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            UNIQUE(run_id, run_sequence)
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_events_run
            ON workflow_run_events(run_id, run_sequence);",
    )?;
    // План 08-2/08-4: nullable linkage back to the global execution ledger
    // row a workflow event corresponds to. Idempotent ALTER (same trick as
    // `model_provenance::install_schema`) because `workflow_run_events` may
    // already exist without these columns on a base-schema-29 database;
    // owned here, not by `execution_ledger`, so this table's full shape
    // stays defined in one place regardless of installer call order.
    for column in ["ledger_sequence_id INTEGER", "ledger_event_id TEXT"] {
        let _ = connection.execute(
            &format!("ALTER TABLE workflow_run_events ADD COLUMN {column}"),
            [],
        );
    }
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_workflow_run_events_ledger
             ON workflow_run_events(ledger_event_id) WHERE ledger_event_id IS NOT NULL;",
    )?;
    Ok(())
}

/// Вставляет запуск вместе с его узлами одной транзакцией: запуск без узлов
/// был бы неотличим от повреждённой записи.
pub fn insert_run(
    connection: &Connection,
    run: &WorkflowRunRecord,
    nodes: &[WorkflowNodeRecord],
) -> Result<(), WorkflowStoreError> {
    bounded("run_id", &run.run_id, MAX_ID_BYTES, true)?;
    bounded("task_id", &run.task_id, MAX_ID_BYTES, true)?;
    bounded("template_id", &run.template_id, MAX_ID_BYTES, true)?;
    bounded("graph_id", &run.graph_id, MAX_ID_BYTES, true)?;
    bounded("graph_hash", &run.graph_hash, MAX_ID_BYTES, true)?;
    bounded("graph_json", &run.graph_json, MAX_GRAPH_JSON_BYTES, true)?;
    bounded("inputs_json", &run.inputs_json, MAX_INPUT_JSON_BYTES, false)?;
    bounded("policy_json", &run.policy_json, MAX_INPUT_JSON_BYTES, false)?;
    for node in nodes {
        bounded("node_id", &node.node_id, MAX_ID_BYTES, true)?;
        bounded("action_kind", &node.action_kind, MAX_ID_BYTES, true)?;
    }

    let transaction = connection.unchecked_transaction()?;
    transaction.execute(
        "INSERT INTO workflow_runs (
            run_id, task_id, template_id, template_version, graph_id, graph_version,
            graph_hash, graph_json, inputs_json, policy_json, state, created_at_ms,
            updated_at_ms, terminal_reason, cancel_requested, lease_owner,
            lease_expires_at_ms, next_sequence
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, 0)",
        params![
            run.run_id,
            run.task_id,
            run.template_id,
            run.template_version,
            run.graph_id,
            run.graph_version as i64,
            run.graph_hash,
            run.graph_json,
            run.inputs_json,
            run.policy_json,
            run.state.as_str(),
            run.created_at_ms,
            run.updated_at_ms,
            run.terminal_reason,
            i64::from(run.cancel_requested),
            run.lease_owner,
            run.lease_expires_at_ms,
        ],
    )?;
    let mut insert_node = transaction.prepare_cached(
        "INSERT INTO workflow_run_nodes (
            run_id, node_id, action_kind, state, attempts, output_json,
            error_code, error_message, approval_id, updated_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    for node in nodes {
        insert_node.execute(params![
            node.run_id,
            node.node_id,
            node.action_kind,
            node.state.as_str(),
            node.attempts,
            node.output_json,
            node.error_code,
            node.error_message,
            node.approval_id,
            node.updated_at_ms,
        ])?;
    }
    drop(insert_node);
    transaction.commit()?;
    Ok(())
}

/// Loads one workflow run by identifier.
pub fn get_run(
    connection: &Connection,
    run_id: &str,
) -> Result<Option<WorkflowRunRecord>, WorkflowStoreError> {
    let record = connection
        .query_row(
            "SELECT run_id, task_id, template_id, template_version, graph_id, graph_version,
                    graph_hash, graph_json, inputs_json, policy_json, state, created_at_ms,
                    updated_at_ms, terminal_reason, cancel_requested, lease_owner,
                    lease_expires_at_ms
             FROM workflow_runs WHERE run_id = ?1",
            params![run_id],
            map_run,
        )
        .optional()?;
    Ok(record)
}

/// Lists workflow runs newest first, limiting the result to [`MAX_LIST_LIMIT`].
pub fn list_runs(
    connection: &Connection,
    limit: usize,
) -> Result<Vec<WorkflowRunRecord>, WorkflowStoreError> {
    let limit = limit.clamp(1, MAX_LIST_LIMIT);
    let mut statement = connection.prepare(
        "SELECT run_id, task_id, template_id, template_version, graph_id, graph_version,
                graph_hash, graph_json, inputs_json, policy_json, state, created_at_ms,
                updated_at_ms, terminal_reason, cancel_requested, lease_owner,
                lease_expires_at_ms
         FROM workflow_runs ORDER BY created_at_ms DESC, run_id DESC LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit as i64], map_run)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRunRecord> {
    let state: String = row.get(10)?;
    let cancel: i64 = row.get(14)?;
    let graph_version: i64 = row.get(5)?;
    Ok(WorkflowRunRecord {
        run_id: row.get(0)?,
        task_id: row.get(1)?,
        template_id: row.get(2)?,
        template_version: row.get(3)?,
        graph_id: row.get(4)?,
        graph_version: graph_version as u64,
        graph_hash: row.get(6)?,
        graph_json: row.get(7)?,
        inputs_json: row.get(8)?,
        policy_json: row.get(9)?,
        state: RunState::parse(&state).unwrap_or(RunState::Interrupted),
        created_at_ms: row.get(11)?,
        updated_at_ms: row.get(12)?,
        terminal_reason: row.get(13)?,
        cancel_requested: cancel != 0,
        lease_owner: row.get(15)?,
        lease_expires_at_ms: row.get(16)?,
    })
}

fn map_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowNodeRecord> {
    let state: String = row.get(3)?;
    Ok(WorkflowNodeRecord {
        run_id: row.get(0)?,
        node_id: row.get(1)?,
        action_kind: row.get(2)?,
        state: NodeState::parse(&state).unwrap_or(NodeState::UnknownOutcome),
        attempts: row.get(4)?,
        output_json: row.get(5)?,
        error_code: row.get(6)?,
        error_message: row.get(7)?,
        approval_id: row.get(8)?,
        updated_at_ms: row.get(9)?,
    })
}

/// Lists a run's nodes in stable node-identifier order.
pub fn list_nodes(
    connection: &Connection,
    run_id: &str,
) -> Result<Vec<WorkflowNodeRecord>, WorkflowStoreError> {
    let mut statement = connection.prepare(
        "SELECT run_id, node_id, action_kind, state, attempts, output_json,
                error_code, error_message, approval_id, updated_at_ms
         FROM workflow_run_nodes WHERE run_id = ?1 ORDER BY node_id",
    )?;
    let rows = statement.query_map(params![run_id], map_node)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

/// Values used to transition a non-terminal node and store its latest result.
pub struct UpdateNodeStateInput<'a> {
    /// Workflow run containing the node.
    pub run_id: &'a str,
    /// Node whose state should change.
    pub node_id: &'a str,
    /// New lifecycle state.
    pub state: NodeState,
    /// Number of attempts recorded after this transition.
    pub attempts: u32,
    /// Serialized output to store with the node.
    pub output_json: &'a str,
    /// Stable error category, or an empty string on success.
    pub error_code: &'a str,
    /// Human-readable error detail, or an empty string on success.
    pub error_message: &'a str,
    /// Transition time in Unix milliseconds.
    pub now_ms: i64,
}

/// Обновляет состояние узла. Терминальный узел не переводится обратно в
/// рабочее состояние: это запрещено на уровне SQL-условия, а не проверки в
/// вызывающем коде.
pub fn update_node_state(
    connection: &Connection,
    input: UpdateNodeStateInput<'_>,
) -> Result<(), WorkflowStoreError> {
    bounded(
        "output_json",
        input.output_json,
        MAX_OUTPUT_JSON_BYTES,
        false,
    )?;
    bounded("error_message", input.error_message, MAX_ERROR_BYTES, false)?;
    let changed = connection.execute(
        "UPDATE workflow_run_nodes
            SET state = ?3, attempts = ?4, output_json = ?5, error_code = ?6,
                error_message = ?7, updated_at_ms = ?8
          WHERE run_id = ?1 AND node_id = ?2
            AND state IN ('pending','ready','running','waiting_approval')",
        params![
            input.run_id,
            input.node_id,
            input.state.as_str(),
            input.attempts,
            input.output_json,
            input.error_code,
            input.error_message,
            input.now_ms
        ],
    )?;
    if changed == 0 {
        return Err(WorkflowStoreError::UnknownNode {
            run_id: input.run_id.to_string(),
            node_id: input.node_id.to_string(),
        });
    }
    Ok(())
}

/// Associates an approval with a node and moves it to `waiting_approval`.
pub fn set_node_approval(
    connection: &Connection,
    run_id: &str,
    node_id: &str,
    approval_id: &str,
    now_ms: i64,
) -> Result<(), WorkflowStoreError> {
    bounded("approval_id", approval_id, MAX_ID_BYTES, false)?;
    connection.execute(
        "UPDATE workflow_run_nodes SET approval_id = ?3, state = 'waiting_approval',
                updated_at_ms = ?4
         WHERE run_id = ?1 AND node_id = ?2 AND state IN ('pending','ready','running')",
        params![run_id, node_id, approval_id, now_ms],
    )?;
    Ok(())
}

/// Updates a run that has not yet reached a terminal state.
pub fn update_run_state(
    connection: &Connection,
    run_id: &str,
    state: RunState,
    terminal_reason: &str,
    now_ms: i64,
) -> Result<(), WorkflowStoreError> {
    bounded("terminal_reason", terminal_reason, MAX_ERROR_BYTES, false)?;
    let changed = connection.execute(
        "UPDATE workflow_runs SET state = ?2, terminal_reason = ?3, updated_at_ms = ?4
         WHERE run_id = ?1 AND state NOT IN ('completed','failed','cancelled','degraded')",
        params![run_id, state.as_str(), terminal_reason, now_ms],
    )?;
    if changed == 0 {
        return Err(WorkflowStoreError::UnknownRun(run_id.to_string()));
    }
    Ok(())
}

/// Отмечает запрос отмены. Сам переход в `cancelled` делает runtime, когда
/// снимет узлы: иначе отменённый запуск выглядел бы завершённым, пока в нём
/// ещё живёт попытка.
pub fn request_cancel(
    connection: &Connection,
    run_id: &str,
    now_ms: i64,
) -> Result<bool, WorkflowStoreError> {
    let changed = connection.execute(
        "UPDATE workflow_runs SET cancel_requested = 1, updated_at_ms = ?2
         WHERE run_id = ?1 AND state NOT IN ('completed','failed','cancelled','degraded')",
        params![run_id, now_ms],
    )?;
    Ok(changed > 0)
}

/// Claims a run lease if it is unowned, already owned by the caller, or expired.
pub fn acquire_lease(
    connection: &Connection,
    run_id: &str,
    owner: &str,
    expires_at_ms: i64,
    now_ms: i64,
) -> Result<bool, WorkflowStoreError> {
    bounded("lease_owner", owner, MAX_ID_BYTES, true)?;
    let changed = connection.execute(
        "UPDATE workflow_runs SET lease_owner = ?2, lease_expires_at_ms = ?3, updated_at_ms = ?4
         WHERE run_id = ?1 AND (lease_owner = '' OR lease_owner = ?2 OR lease_expires_at_ms < ?4)",
        params![run_id, owner, expires_at_ms, now_ms],
    )?;
    Ok(changed > 0)
}

/// Releases a run lease only when `owner` currently holds it.
pub fn release_lease(
    connection: &Connection,
    run_id: &str,
    owner: &str,
    now_ms: i64,
) -> Result<(), WorkflowStoreError> {
    connection.execute(
        "UPDATE workflow_runs SET lease_owner = '', lease_expires_at_ms = 0, updated_at_ms = ?3
         WHERE run_id = ?1 AND lease_owner = ?2",
        params![run_id, owner, now_ms],
    )?;
    Ok(())
}

/// Пишет dispatch marker до эффекта.
pub fn begin_attempt(
    connection: &Connection,
    attempt: &WorkflowAttemptRecord,
) -> Result<(), WorkflowStoreError> {
    bounded("attempt_id", &attempt.attempt_id, MAX_ID_BYTES, true)?;
    bounded("graph_hash", &attempt.graph_hash, MAX_ID_BYTES, true)?;
    let transaction = connection.unchecked_transaction()?;
    transaction.execute(
        "INSERT INTO workflow_node_attempts (
            attempt_id, run_id, node_id, attempt, graph_hash, input_hash,
            dispatched_at_ms, completed_at_ms, outcome, error_code
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, '', '')",
        params![
            attempt.attempt_id,
            attempt.run_id,
            attempt.node_id,
            attempt.attempt,
            attempt.graph_hash,
            attempt.input_hash,
            attempt.dispatched_at_ms,
        ],
    )?;
    transaction.execute(
        "UPDATE workflow_run_nodes SET state = 'running', attempts = ?3, updated_at_ms = ?4
         WHERE run_id = ?1 AND node_id = ?2",
        params![
            attempt.run_id,
            attempt.node_id,
            attempt.attempt,
            attempt.dispatched_at_ms
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

/// Закрывает dispatch marker после эффекта.
pub fn finish_attempt(
    connection: &Connection,
    attempt_id: &str,
    outcome: &str,
    error_code: &str,
    now_ms: i64,
) -> Result<(), WorkflowStoreError> {
    bounded("outcome", outcome, MAX_ID_BYTES, true)?;
    connection.execute(
        "UPDATE workflow_node_attempts
            SET completed_at_ms = ?4, outcome = ?2, error_code = ?3
          WHERE attempt_id = ?1 AND completed_at_ms IS NULL",
        params![attempt_id, outcome, error_code, now_ms],
    )?;
    Ok(())
}

/// Lists dispatch attempts for one run, grouped by node and attempt number.
pub fn list_attempts(
    connection: &Connection,
    run_id: &str,
) -> Result<Vec<WorkflowAttemptRecord>, WorkflowStoreError> {
    let mut statement = connection.prepare(
        "SELECT attempt_id, run_id, node_id, attempt, graph_hash, input_hash,
                dispatched_at_ms, completed_at_ms, outcome, error_code
         FROM workflow_node_attempts WHERE run_id = ?1
         ORDER BY node_id, attempt",
    )?;
    let rows = statement.query_map(params![run_id], |row| {
        Ok(WorkflowAttemptRecord {
            attempt_id: row.get(0)?,
            run_id: row.get(1)?,
            node_id: row.get(2)?,
            attempt: row.get(3)?,
            graph_hash: row.get(4)?,
            input_hash: row.get(5)?,
            dispatched_at_ms: row.get(6)?,
            completed_at_ms: row.get(7)?,
            outcome: row.get(8)?,
            error_code: row.get(9)?,
        })
    })?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

/// Input for appending a workflow event with an optional ledger link.
pub struct AppendEventRowInput<'a> {
    /// Workflow run receiving the event.
    pub run_id: &'a str,
    /// Associated node identifier, or empty when run-scoped.
    pub node_id: &'a str,
    /// Associated dispatch attempt identifier, or empty when not applicable.
    pub attempt_id: &'a str,
    /// Stable event category.
    pub event_type: &'a str,
    /// Serialized event data bounded by [`MAX_EVENT_PAYLOAD_BYTES`].
    pub payload_json: &'a str,
    /// Event creation time in Unix milliseconds.
    pub now_ms: i64,
    /// Optional global execution-ledger sequence number.
    pub ledger_sequence_id: Option<i64>,
    /// Optional global execution-ledger event identifier.
    pub ledger_event_id: Option<&'a str>,
}

/// Input for atomically linking a workflow event to a ledger event.
pub struct AppendEventLinkedInput<'a> {
    /// Workflow run receiving the event.
    pub run_id: &'a str,
    /// Associated node identifier, or empty when run-scoped.
    pub node_id: &'a str,
    /// Associated dispatch attempt identifier, or empty when not applicable.
    pub attempt_id: &'a str,
    /// Stable event category.
    pub event_type: &'a str,
    /// Serialized event data bounded by [`MAX_EVENT_PAYLOAD_BYTES`].
    pub payload_json: &'a str,
    /// Event creation time in Unix milliseconds.
    pub now_ms: i64,
    /// Global execution-ledger sequence number.
    pub ledger_sequence_id: i64,
    /// Global execution-ledger event identifier.
    pub ledger_event_id: &'a str,
}

/// Shared row-insert: assigns the next `run_sequence` and appends one
/// `workflow_run_events` row, optionally linked back to a global execution
/// ledger row (план 08-2/08-4 `run_sequence` <-> `sequence_id`/`event_id`
/// linkage). Takes `&Connection` so a caller already inside its own
/// transaction (e.g. `LocalDatabase::append_ledger_event_with_node_transition`)
/// can compose this atomically instead of nesting a second `BEGIN`.
fn append_event_row(
    connection: &Connection,
    input: AppendEventRowInput<'_>,
) -> Result<i64, WorkflowStoreError> {
    bounded("event_type", input.event_type, MAX_ID_BYTES, true)?;
    bounded(
        "payload_json",
        input.payload_json,
        MAX_EVENT_PAYLOAD_BYTES,
        false,
    )?;
    let next: Option<i64> = connection
        .query_row(
            "SELECT next_sequence FROM workflow_runs WHERE run_id = ?1",
            params![input.run_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(next) = next else {
        return Err(WorkflowStoreError::UnknownRun(input.run_id.to_string()));
    };
    connection.execute(
        "INSERT INTO workflow_run_events (
            run_id, run_sequence, node_id, attempt_id, event_type, payload_json, created_at_ms,
            ledger_sequence_id, ledger_event_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            input.run_id,
            next,
            input.node_id,
            input.attempt_id,
            input.event_type,
            input.payload_json,
            input.now_ms,
            input.ledger_sequence_id,
            input.ledger_event_id
        ],
    )?;
    connection.execute(
        "UPDATE workflow_runs SET next_sequence = ?2 WHERE run_id = ?1",
        params![input.run_id, next + 1],
    )?;
    Ok(next)
}

/// Добавляет событие и выдаёт монотонный номер внутри запуска.
pub fn append_event(
    connection: &Connection,
    run_id: &str,
    node_id: &str,
    attempt_id: &str,
    event_type: &str,
    payload_json: &str,
    now_ms: i64,
) -> Result<i64, WorkflowStoreError> {
    let transaction = connection.unchecked_transaction()?;
    let next = append_event_row(
        &transaction,
        AppendEventRowInput {
            run_id,
            node_id,
            attempt_id,
            event_type,
            payload_json,
            now_ms,
            ledger_sequence_id: None,
            ledger_event_id: None,
        },
    )?;
    transaction.commit()?;
    Ok(next)
}

/// Same as [`append_event`], but stamps the `workflow_run_events` row with
/// the global execution-ledger `sequence_id`/`event_id` it corresponds to.
/// `connection` must already be inside the caller's transaction — this
/// function never opens or commits one of its own, so the ledger event
/// insert and this linkage row land atomically together.
pub(crate) fn append_event_linked(
    connection: &Connection,
    input: AppendEventLinkedInput<'_>,
) -> Result<i64, WorkflowStoreError> {
    append_event_row(
        connection,
        AppendEventRowInput {
            run_id: input.run_id,
            node_id: input.node_id,
            attempt_id: input.attempt_id,
            event_type: input.event_type,
            payload_json: input.payload_json,
            now_ms: input.now_ms,
            ledger_sequence_id: Some(input.ledger_sequence_id),
            ledger_event_id: Some(input.ledger_event_id),
        },
    )
}

/// Lists events after a sequence cursor, preserving their monotonic run order.
pub fn list_events(
    connection: &Connection,
    run_id: &str,
    after_sequence: i64,
    limit: usize,
) -> Result<Vec<WorkflowEventRecord>, WorkflowStoreError> {
    let limit = limit.clamp(1, MAX_LIST_LIMIT);
    let mut statement = connection.prepare(
        "SELECT run_id, run_sequence, node_id, attempt_id, event_type, payload_json, created_at_ms
         FROM workflow_run_events WHERE run_id = ?1 AND run_sequence > ?2
         ORDER BY run_sequence LIMIT ?3",
    )?;
    let rows = statement.query_map(params![run_id, after_sequence, limit as i64], |row| {
        Ok(WorkflowEventRecord {
            run_id: row.get(0)?,
            run_sequence: row.get(1)?,
            node_id: row.get(2)?,
            attempt_id: row.get(3)?,
            event_type: row.get(4)?,
            payload_json: row.get(5)?,
            created_at_ms: row.get(6)?,
        })
    })?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

/// Итог восстановления после перезапуска Core.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RecoveryOutcome {
    /// Попытки, которые остались открытыми: исход эффекта неизвестен.
    pub unknown_attempts: Vec<String>,
    /// Запуски, переведённые в `interrupted`.
    pub interrupted_runs: Vec<String>,
}

/// Восстановление: незакрытая попытка становится `unknown_outcome`, а её
/// запуск — `interrupted`. Слепой повтор здесь невозможен по построению: узел
/// уходит в терминальное состояние, из которого `update_node_state` уже не
/// переводит его обратно в работу.
pub fn recover_after_restart(
    connection: &Connection,
    now_ms: i64,
) -> Result<RecoveryOutcome, WorkflowStoreError> {
    let transaction = connection.unchecked_transaction()?;
    let mut outcome = RecoveryOutcome::default();
    let mut complete_attempt = transaction.prepare_cached(
        "UPDATE workflow_node_attempts
            SET completed_at_ms = ?2, outcome = 'unknown', error_code = 'core_restart'
          WHERE attempt_id = ?1",
    )?;
    let mut update_node = transaction.prepare_cached(
        "UPDATE workflow_run_nodes SET state = 'unknown_outcome',
                error_code = 'core_restart', updated_at_ms = ?3
          WHERE run_id = ?1 AND node_id = ?2",
    )?;
    {
        let mut statement = transaction.prepare(
            "SELECT attempt_id, run_id, node_id FROM workflow_node_attempts
             WHERE completed_at_ms IS NULL ORDER BY run_id, node_id, attempt",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (attempt_id, run_id, node_id) = row?;
            complete_attempt.execute(params![attempt_id, now_ms])?;
            update_node.execute(params![run_id, node_id, now_ms])?;
            outcome.unknown_attempts.push(attempt_id);
            if !outcome.interrupted_runs.contains(&run_id) {
                outcome.interrupted_runs.push(run_id);
            }
        }
    }
    drop(update_node);
    drop(complete_attempt);
    let mut interrupt_run = transaction.prepare_cached(
        "UPDATE workflow_runs SET state = 'interrupted', lease_owner = '',
                lease_expires_at_ms = 0, updated_at_ms = ?2
          WHERE run_id = ?1 AND state NOT IN ('completed','failed','cancelled','degraded')",
    )?;
    for run_id in &outcome.interrupted_runs {
        interrupt_run.execute(params![run_id, now_ms])?;
    }
    drop(interrupt_run);
    // Запуски без открытых попыток, но оставшиеся в running, тоже теряют
    // lease: их продолжит новый владелец, а не прежний процесс.
    transaction.execute(
        "UPDATE workflow_runs SET lease_owner = '', lease_expires_at_ms = 0
          WHERE state IN ('pending','running','waiting_approval')",
        [],
    )?;
    transaction.commit()?;
    Ok(outcome)
}

#[cfg(test)]
#[path = "workflow_store_tests.rs"]
mod tests;
