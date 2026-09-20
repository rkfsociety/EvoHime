use crate::{
    analysis_kernel, conversation_event_log_store, execution_ledger, goal, memory_store,
    plan_artifact, task_checkpoint, workflow_store,
};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported database schema version {0}")]
    UnsupportedSchema(u32),
    #[error(
        "optimistic version conflict for {entity} {id}: expected {expected}, current {current}"
    )]
    VersionConflict {
        entity: &'static str,
        id: String,
        expected: i64,
        current: i64,
    },
    #[error("request {client_id}/{request_id} was already used with another command")]
    DeduplicationConflict {
        client_id: String,
        request_id: String,
    },
    #[error("adding dependency {from_id} -> {to_id} would create a cycle")]
    DependencyCycle { from_id: String, to_id: String },
    #[error("invalid run effect transition: {0}")]
    InvalidRunEffect(String),
    #[error("invalid recovery transition: {0}")]
    InvalidRecovery(String),
    #[error("invalid storage input: {0}")]
    InvalidInput(String),
    #[error("backup operation failed: {0}")]
    Backup(String),
    #[error("backup format is invalid: {0}")]
    BackupFormat(String),
    #[error("backup checksum mismatch: expected {expected}, got {actual}")]
    BackupChecksumMismatch { expected: String, actual: String },
    #[error("backup schema mismatch: expected {expected}, got {actual}")]
    BackupSchemaMismatch { expected: u32, actual: u32 },
    #[error("backup is too large: {0} bytes")]
    BackupTooLarge(u64),
    #[error("backup destination already exists: {0}")]
    BackupDestinationExists(String),
    #[error("backup operation was cancelled")]
    BackupCancelled,
    /// Нарушение контракта плана 01: scratchpad или artifact store.
    #[error("context operation failed: {0}")]
    Context(String),
    #[error("task checkpoint contract violation: {0}")]
    TaskCheckpoint(#[from] task_checkpoint::TaskCheckpointError),
    #[error("conversation event log contract violation: {0}")]
    ConversationEventLog(#[from] conversation_event_log_store::ConversationStoreError),
    #[error("analysis kernel contract violation: {0}")]
    AnalysisKernel(#[from] analysis_kernel::AnalysisKernelError),
    #[error("goal contract violation: {0}")]
    Goal(#[from] goal::GoalError),
    #[error("plan artifact contract violation: {0}")]
    PlanArtifact(#[from] plan_artifact::PlanArtifactError),
    /// Нарушение контракта execution ledger (план 08-1/08-2).
    #[error("execution ledger contract violation: {0}")]
    LedgerContract(#[from] execution_ledger::LedgerContractError),
    /// Ошибка workflow-store слоя, всплывшая при atomic ledger<->workflow
    /// linkage (план 08-2/08-4).
    #[error("workflow store operation failed: {0}")]
    WorkflowStore(#[from] workflow_store::WorkflowStoreError),
    #[error("memory store operation failed: {0}")]
    MemoryStore(#[from] memory_store::MemoryStoreError),
    /// Атомарный переход action+projection не применился: узел уже не в
    /// ожидаемом исходном состоянии (гонка или устаревший вызов).
    #[error("ledger node transition conflict for run {run_id} node {node_id}")]
    LedgerNodeTransitionConflict { run_id: String, node_id: String },
}
