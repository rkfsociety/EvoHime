//! Core-facing TaskCheckpoint contract (plan 23.1).
//!
//! The durable contract lives next to its SQLite representation so storage
//! can validate records independently. Core re-exports it as the public
//! authority boundary used by later runtime and IPC stages.

pub use evohime_local_storage::task_checkpoint::{
    install_schema, ApprovalState, CheckpointDecision, CheckpointItem, CheckpointRef,
    CheckpointSensitivity, CheckpointStatus, FileChange, FileChangeKind, FileReadRef, GateEvidence,
    GateStatus, InsertOutcome, PendingApproval, Provenance, TaskCheckpointError,
    TaskCheckpointStore, TaskCheckpointV1, TestEvidence, TestStatus, TASK_CHECKPOINT_MAX_BYTES,
    TASK_CHECKPOINT_MAX_ITEMS, TASK_CHECKPOINT_MAX_REFS, TASK_CHECKPOINT_VERSION,
};
