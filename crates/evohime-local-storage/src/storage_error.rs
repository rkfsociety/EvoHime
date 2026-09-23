use crate::{
    analysis_kernel, conversation_event_log_store, execution_ledger, goal, memory_store,
    plan_artifact, task_checkpoint, workflow_store,
};

/// Errors returned by the local storage API.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The operating-system storage operation failed.
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// A SQLite query, transaction, or schema operation failed.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Serialization or deserialization of stored JSON failed.
    #[error("JSON operation failed: {0}")]
    Json(#[from] serde_json::Error),
    /// The database schema version is not supported by this build.
    #[error("unsupported database schema version {0}")]
    UnsupportedSchema(u32),
    #[error(
        "optimistic version conflict for {entity} {id}: expected {expected}, current {current}"
    )]
    /// A version-checked write observed a different current version.
    VersionConflict {
        /// Entity type involved in the conflict.
        entity: &'static str,
        /// Identifier of the entity.
        id: String,
        /// Version supplied by the caller.
        expected: i64,
        /// Version currently stored, or `-1` when absent.
        current: i64,
    },
    /// A deduplication key was reused for a different command.
    #[error("request {client_id}/{request_id} was already used with another command")]
    DeduplicationConflict {
        /// Client that owns the request key.
        client_id: String,
        /// Reused request identifier.
        request_id: String,
    },
    /// A proposed dependency edge would create a cycle.
    #[error("adding dependency {from_id} -> {to_id} would create a cycle")]
    DependencyCycle {
        /// Work item that would depend on another item.
        from_id: String,
        /// Work item that would become the dependency.
        to_id: String,
    },
    /// A run effect transition did not satisfy its lifecycle preconditions.
    #[error("invalid run effect transition: {0}")]
    InvalidRunEffect(String),
    /// A recovery state transition did not satisfy its lifecycle preconditions.
    #[error("invalid recovery transition: {0}")]
    InvalidRecovery(String),
    /// Input failed storage-layer validation.
    #[error("invalid storage input: {0}")]
    InvalidInput(String),
    /// Backup creation, verification, or restoration failed.
    #[error("backup operation failed: {0}")]
    Backup(String),
    /// Backup contents do not match the supported format.
    #[error("backup format is invalid: {0}")]
    BackupFormat(String),
    /// Backup payload checksum differs from its recorded checksum.
    #[error("backup checksum mismatch: expected {expected}, got {actual}")]
    BackupChecksumMismatch {
        /// Checksum declared by the backup metadata.
        expected: String,
        /// Checksum computed from the backup payload.
        actual: String,
    },
    /// Backup schema does not match the schema expected by the restore operation.
    #[error("backup schema mismatch: expected {expected}, got {actual}")]
    BackupSchemaMismatch {
        /// Schema version required by the restore operation.
        expected: u32,
        /// Schema version recorded by the backup.
        actual: u32,
    },
    /// Backup size exceeded the configured safety limit.
    #[error("backup is too large: {0} bytes")]
    BackupTooLarge(u64),
    /// Backup creation refused to overwrite an existing destination.
    #[error("backup destination already exists: {0}")]
    BackupDestinationExists(String),
    /// The backup operation was cancelled before completion.
    #[error("backup operation was cancelled")]
    BackupCancelled,
    /// Нарушение контракта плана 01: scratchpad или artifact store.
    /// An artifact or context operation failed.
    #[error("context operation failed: {0}")]
    Context(String),
    /// A task checkpoint violated its storage contract.
    #[error("task checkpoint contract violation: {0}")]
    TaskCheckpoint(#[from] task_checkpoint::TaskCheckpointError),
    /// A conversation event operation violated its storage contract.
    #[error("conversation event log contract violation: {0}")]
    ConversationEventLog(#[from] conversation_event_log_store::ConversationStoreError),
    /// An analysis-kernel operation violated its storage contract.
    #[error("analysis kernel contract violation: {0}")]
    AnalysisKernel(#[from] analysis_kernel::AnalysisKernelError),
    /// A goal operation violated its storage contract.
    #[error("goal contract violation: {0}")]
    Goal(#[from] goal::GoalError),
    /// A plan-artifact operation violated its storage contract.
    #[error("plan artifact contract violation: {0}")]
    PlanArtifact(#[from] plan_artifact::PlanArtifactError),
    /// Нарушение контракта execution ledger (план 08-1/08-2).
    /// An execution-ledger operation violated its storage contract.
    #[error("execution ledger contract violation: {0}")]
    LedgerContract(#[from] execution_ledger::LedgerContractError),
    /// Ошибка workflow-store слоя, всплывшая при atomic ledger<->workflow
    /// linkage (план 08-2/08-4).
    /// A workflow-store operation failed during a linked ledger update.
    #[error("workflow store operation failed: {0}")]
    WorkflowStore(#[from] workflow_store::WorkflowStoreError),
    /// A memory-store operation failed.
    #[error("memory store operation failed: {0}")]
    MemoryStore(#[from] memory_store::MemoryStoreError),
    /// Атомарный переход action+projection не применился: узел уже не в
    /// ожидаемом исходном состоянии (гонка или устаревший вызов).
    /// An atomic ledger and projection transition lost its expected-state race.
    #[error("ledger node transition conflict for run {run_id} node {node_id}")]
    LedgerNodeTransitionConflict {
        /// Run containing the node.
        run_id: String,
        /// Node whose expected transition could not be applied.
        node_id: String,
    },
}
