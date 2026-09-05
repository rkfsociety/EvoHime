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

use std::path::Path;

use evohime_context_budget::ledger::ContextLedgerEntry;
use serde::Serialize;

/// Immutable policy identity captured in every runtime checkpoint. A later
/// stage may add richer policy snapshots, but it must keep this reference
/// stable so recovery never silently resumes under an unknown contract.
pub const TASK_CHECKPOINT_POLICY_ID: &str = "task-checkpoint-policy-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointCaptureReason {
    RunStarted,
    BeforeCompaction,
    ContextProjected,
    Completed,
    Failed,
    Paused,
    RecoveryBlocked,
}

impl CheckpointCaptureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunStarted => "run_started",
            Self::BeforeCompaction => "before_compaction",
            Self::ContextProjected => "context_projected",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Paused => "paused",
            Self::RecoveryBlocked => "recovery_blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDisposition {
    NoCheckpoint,
    Replayable,
    Terminal,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReplayedCheckpointEvent {
    pub sequence_id: i64,
    pub event_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskCheckpointRecovery {
    pub disposition: RecoveryDisposition,
    pub checkpoint: Option<TaskCheckpointV1>,
    pub replayed_events: Vec<ReplayedCheckpointEvent>,
    pub warning: Option<String>,
}

#[derive(Clone)]
pub struct TaskCheckpointRuntime {
    journal: crate::EventJournal,
}

impl TaskCheckpointRuntime {
    pub fn new(journal: crate::EventJournal) -> Self {
        Self { journal }
    }

    /// Captures a bounded Core-owned projection. The model never supplies the
    /// authority fields: policy and contract refs, sequence, status and
    /// storage identity are all derived here immediately before persistence.
    pub async fn capture(
        &self,
        task_id: &str,
        workspace_root: &Path,
        status: CheckpointStatus,
        reason: CheckpointCaptureReason,
        ledger: Option<&ContextLedgerEntry>,
    ) -> Result<TaskCheckpointV1, evohime_local_storage::StorageError> {
        self.capture_inner(task_id, workspace_root, status, reason, ledger, None)
            .await
    }

    /// Captures a checkpoint with explicitly selected, immutable analysis
    /// kernel refs. Values remain in the kernel/ArtifactStore; the checkpoint
    /// receives only the bounded metadata references.
    pub async fn capture_with_analysis_kernel(
        &self,
        input: AnalysisKernelCheckpointInput<'_>,
    ) -> Result<TaskCheckpointV1, evohime_local_storage::StorageError> {
        self.capture_inner(
            input.task_id,
            input.workspace_root,
            input.status,
            input.reason,
            input.ledger,
            Some((input.session, input.objects)),
        )
        .await
    }

    async fn capture_inner(
        &self,
        task_id: &str,
        workspace_root: &Path,
        status: CheckpointStatus,
        reason: CheckpointCaptureReason,
        ledger: Option<&ContextLedgerEntry>,
        kernel: Option<(
            &crate::analysis_kernel::AnalysisKernelSessionV1,
            &[crate::analysis_kernel::KernelObjectRefV1],
        )>,
    ) -> Result<TaskCheckpointV1, evohime_local_storage::StorageError> {
        let workspace_id = crate::task_memory::workspace_scope_id(workspace_root);
        let checkpoint = {
            let database = self.journal.database().lock().await;
            let store = TaskCheckpointStore::new(database.connection());
            let parent = store.latest_valid_for_chat(&workspace_id, task_id)?;
            let latest_event_sequence = database.latest_event_sequence()?;
            let source_event_seq = parent
                .as_ref()
                .map(|parent| parent.source_event_seq.saturating_add(1))
                .unwrap_or_default()
                .max(latest_event_sequence);
            let checkpoint = build_checkpoint(
                task_id,
                &workspace_id,
                source_event_seq,
                parent.as_ref().map(|parent| parent.id.clone()),
                status,
                reason,
                ledger,
            )?;
            match kernel {
                Some((session, objects)) => {
                    crate::analysis_kernel::attach_checkpoint_refs(checkpoint, session, objects)
                        .map_err(evohime_local_storage::StorageError::TaskCheckpoint)
                }
                None => Ok(checkpoint),
            }?
        };

        let payload = serde_json::to_vec(&serde_json::json!({
            "checkpoint_id": checkpoint.id,
            "content_hash": checkpoint.content_hash,
            "reason": reason.as_str(),
            "source_event_seq": checkpoint.source_event_seq,
            "status": checkpoint.status,
        }))?;
        let database = self.journal.database().lock().await;
        let store = TaskCheckpointStore::new(database.connection());
        store.insert(&checkpoint)?;
        database.append_event(task_id, "task.checkpoint.saved", &payload)?;
        Ok(checkpoint)
    }

    /// Replays only bounded event metadata after the latest valid checkpoint.
    /// Payloads stay in Core/storage; an incomplete or blocked replay is never
    /// treated as permission to retry an external effect.
    pub async fn recover(
        &self,
        task_id: &str,
        workspace_root: &Path,
    ) -> Result<TaskCheckpointRecovery, evohime_local_storage::StorageError> {
        const REPLAY_LIMIT: usize = 256;
        let workspace_id = crate::task_memory::workspace_scope_id(workspace_root);
        let (checkpoint, events) = {
            let database = self.journal.database().lock().await;
            let store = TaskCheckpointStore::new(database.connection());
            (
                store.latest_valid_for_chat(&workspace_id, task_id)?,
                database.read_task_events(task_id, REPLAY_LIMIT)?,
            )
        };
        let Some(checkpoint) = checkpoint else {
            return Ok(TaskCheckpointRecovery {
                disposition: RecoveryDisposition::NoCheckpoint,
                checkpoint: None,
                replayed_events: Vec::new(),
                warning: None,
            });
        };

        let replayed_events = events
            .iter()
            .filter(|event| event.sequence_id > checkpoint.source_event_seq)
            .map(|event| ReplayedCheckpointEvent {
                sequence_id: event.sequence_id,
                event_type: event.event_type.clone(),
            })
            .collect::<Vec<_>>();
        let replay_window_truncated = events.len() == REPLAY_LIMIT
            && events
                .first()
                .is_some_and(|event| event.sequence_id > checkpoint.source_event_seq);
        let unknown_outcome = replayed_events
            .iter()
            .any(|event| event.event_type == "run.recovery.blocked");
        let terminal = matches!(
            checkpoint.status,
            CheckpointStatus::Completed | CheckpointStatus::Failed
        );
        let blocked = replay_window_truncated
            || unknown_outcome
            || matches!(
                checkpoint.status,
                CheckpointStatus::Blocked | CheckpointStatus::Conflicted | CheckpointStatus::Stale
            );
        let warning = if replay_window_truncated {
            Some(
                "checkpoint replay window is truncated; explicit reconciliation is required".into(),
            )
        } else if unknown_outcome {
            Some("recovery contains an unknown external outcome; blind retry is forbidden".into())
        } else if terminal {
            Some("checkpoint is terminal; start a new task identity for new work".into())
        } else {
            None
        };
        Ok(TaskCheckpointRecovery {
            disposition: if blocked {
                RecoveryDisposition::Blocked
            } else if terminal {
                RecoveryDisposition::Terminal
            } else {
                RecoveryDisposition::Replayable
            },
            checkpoint: Some(checkpoint),
            replayed_events,
            warning,
        })
    }
}

/// Параметры checkpoint с явно выбранными analysis-kernel references.
pub struct AnalysisKernelCheckpointInput<'a> {
    pub task_id: &'a str,
    pub workspace_root: &'a Path,
    pub status: CheckpointStatus,
    pub reason: CheckpointCaptureReason,
    pub ledger: Option<&'a ContextLedgerEntry>,
    pub session: &'a crate::analysis_kernel::AnalysisKernelSessionV1,
    pub objects: &'a [crate::analysis_kernel::KernelObjectRefV1],
}

fn build_checkpoint(
    task_id: &str,
    workspace_id: &str,
    source_event_seq: i64,
    parent_checkpoint_id: Option<String>,
    status: CheckpointStatus,
    reason: CheckpointCaptureReason,
    ledger: Option<&ContextLedgerEntry>,
) -> Result<TaskCheckpointV1, TaskCheckpointError> {
    let task_ref_hash = crate::research::sha256_hex(task_id.as_bytes());
    let policy_hash = crate::research::sha256_hex(TASK_CHECKPOINT_POLICY_ID.as_bytes());
    let mut workflow_refs = vec![
        CheckpointRef {
            id: format!("task-scope-{task_ref_hash}"),
            kind: "task_scope".into(),
            content_hash: Some(task_ref_hash),
            sensitivity: CheckpointSensitivity::Internal,
            provenance: Provenance::core("core:task-runtime"),
        },
        CheckpointRef {
            id: TASK_CHECKPOINT_POLICY_ID.into(),
            kind: "policy_snapshot".into(),
            content_hash: Some(policy_hash),
            sensitivity: CheckpointSensitivity::Public,
            provenance: Provenance::core("core:task-runtime"),
        },
    ];
    if let Some(ledger) = ledger {
        workflow_refs.push(CheckpointRef {
            id: format!("context-ledger-{}", ledger.id),
            kind: "context_ledger".into(),
            content_hash: Some(ledger.context_ledger_hash.clone()),
            sensitivity: CheckpointSensitivity::Internal,
            provenance: Provenance::core("core:context-budget"),
        });
    }
    let is_completed = status == CheckpointStatus::Completed;
    let completed_items = is_completed
        .then(|| CheckpointItem::core("task completed", "core:task-runtime"))
        .into_iter()
        .collect();
    let blockers = matches!(
        status,
        CheckpointStatus::Blocked | CheckpointStatus::Conflicted | CheckpointStatus::Stale
    )
    .then(|| CheckpointItem::core(status_as_text(status), "core:recovery"))
    .into_iter()
    .collect();
    let remaining_items = (!is_completed)
        .then(|| CheckpointItem::model("continue from the durable checkpoint", "runtime:next"))
        .into_iter()
        .collect();
    let next_action = (!is_completed).then(|| {
        CheckpointItem::model(
            "replay durable state before the next effect",
            "runtime:next",
        )
    });
    let narrative_summary = Some(CheckpointItem::model(
        format!("checkpoint captured at {}", reason.as_str()),
        "runtime:checkpoint",
    ));
    TaskCheckpointV1 {
        id: format!("task-checkpoint-{}", uuid::Uuid::new_v4()),
        version: TASK_CHECKPOINT_VERSION,
        workspace_id: workspace_id.into(),
        chat_id: Some(task_id.into()),
        goal_id: None,
        parent_checkpoint_id,
        objective: "Task continuity checkpoint".into(),
        status,
        completed_items,
        remaining_items,
        decisions: vec![CheckpointDecision::core(
            "Core owns checkpoint authority and recovery",
            "core:task-runtime",
        )],
        blockers,
        files_read: Vec::new(),
        files_changed: Vec::new(),
        tests_passed: Vec::new(),
        tests_failed: Vec::new(),
        gates: Vec::new(),
        pending_approvals: Vec::new(),
        workflow_refs,
        child_refs: Vec::new(),
        artifact_refs: Vec::new(),
        open_questions: Vec::new(),
        next_action,
        narrative_summary,
        source_event_seq,
        created_at: crate::task_memory::now_millis() as i64,
        content_hash: String::new(),
    }
    .seal()
}

fn status_as_text(status: CheckpointStatus) -> String {
    match serde_json::to_string(&status) {
        Ok(value) => value.trim_matches('"').to_owned(),
        Err(error) => {
            tracing::warn!(%error, "checkpoint status serialization failed");
            "blocked".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temporary_database_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "evohime-task-checkpoint-{label}-{}.db",
            std::process::id()
        ))
    }

    #[test]
    fn capture_reasons_are_stable_wire_values() {
        assert_eq!(
            CheckpointCaptureReason::BeforeCompaction.as_str(),
            "before_compaction"
        );
        assert_eq!(
            serde_json::to_string(&RecoveryDisposition::Replayable).unwrap(),
            "\"replayable\""
        );
    }

    #[test]
    fn runtime_builder_keeps_authority_in_core() {
        let checkpoint = build_checkpoint(
            "task-1",
            "workspace-1",
            7,
            None,
            CheckpointStatus::InProgress,
            CheckpointCaptureReason::RunStarted,
            None,
        )
        .expect("checkpoint builds");
        assert!(checkpoint
            .workflow_refs
            .iter()
            .all(|reference| matches!(reference.provenance, Provenance::CoreDerived { .. })));
        assert_eq!(checkpoint.validate(), Ok(()));
    }

    #[tokio::test]
    async fn runtime_capture_replays_only_bounded_event_metadata() {
        let path = temporary_database_path("replay");
        let _ = std::fs::remove_file(&path);
        let journal = crate::EventJournal::open(&path).expect("journal opens");
        let runtime = TaskCheckpointRuntime::new(journal.clone());
        let workspace = Path::new("C:/workspace/evohime");
        runtime
            .capture(
                "task-1",
                workspace,
                CheckpointStatus::InProgress,
                CheckpointCaptureReason::RunStarted,
                None,
            )
            .await
            .expect("initial checkpoint persists");
        journal
            .database()
            .lock()
            .await
            .append_event("task-1", "tool.started", br#"{"raw":"secret"}"#)
            .expect("event persists");
        let latest = runtime
            .capture(
                "task-1",
                workspace,
                CheckpointStatus::InProgress,
                CheckpointCaptureReason::ContextProjected,
                None,
            )
            .await
            .expect("second checkpoint persists");
        journal
            .database()
            .lock()
            .await
            .append_event("task-1", "tool.started", br#"{"raw":"secret"}"#)
            .expect("post-checkpoint event persists");

        let recovery = runtime
            .recover("task-1", workspace)
            .await
            .expect("recovery reads");
        assert_eq!(recovery.disposition, RecoveryDisposition::Replayable);
        assert_eq!(recovery.checkpoint.as_ref().unwrap().id, latest.id);
        assert!(recovery
            .replayed_events
            .iter()
            .any(|event| event.event_type == "tool.started"));
        let serialized = serde_json::to_string(&recovery).unwrap();
        assert!(!serialized.contains("secret"));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn runtime_capture_persists_selected_analysis_kernel_refs() {
        let path = temporary_database_path("kernel-refs");
        let _ = std::fs::remove_file(&path);
        let journal = crate::EventJournal::open(&path).expect("journal opens");
        let runtime = TaskCheckpointRuntime::new(journal);
        let session = crate::analysis_kernel::AnalysisKernelSessionV1 {
            schema_version: 1,
            id: "kernel-1".into(),
            task_id: "task-kernel".into(),
            workspace_id: "workspace-kernel".into(),
            runtime_version: "trusted-local-1".into(),
            package_manifest_hash: "a".repeat(64),
            policy_hash: "b".repeat(64),
            status: crate::analysis_kernel::KernelStatus::Running,
            revision: 1,
            limits: crate::analysis_kernel::KernelLimitsV1::default(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let object = crate::analysis_kernel::KernelObjectRefV1 {
            id: "object-1".into(),
            kernel_id: session.id.clone(),
            logical_name: "rows".into(),
            type_hint: "json".into(),
            size: 2,
            sensitivity: crate::analysis_kernel::KernelSensitivity::Internal,
            persistence: crate::analysis_kernel::KernelObjectPersistence::Checkpointed,
            content_hash: Some("c".repeat(64)),
            artifact_locator: Some("artifact://kernel/object-1".into()),
            provenance: "core:analysis-kernel".into(),
            created_at_ms: 2,
            invalidated_at_ms: None,
        };
        let checkpoint = runtime
            .capture_with_analysis_kernel(AnalysisKernelCheckpointInput {
                task_id: "task-kernel",
                workspace_root: Path::new("C:/workspace/evohime"),
                status: CheckpointStatus::InProgress,
                reason: CheckpointCaptureReason::BeforeCompaction,
                ledger: None,
                session: &session,
                objects: &[object],
            })
            .await
            .expect("kernel refs persist through checkpoint capture");
        assert!(checkpoint
            .artifact_refs
            .iter()
            .any(|reference| reference.id == "object-1"));
        assert!(checkpoint.validate().is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn unknown_outcome_blocks_resume_and_corrupt_latest_falls_back() {
        let path = temporary_database_path("recovery");
        let _ = std::fs::remove_file(&path);
        let journal = crate::EventJournal::open(&path).expect("journal opens");
        let runtime = TaskCheckpointRuntime::new(journal.clone());
        let workspace = Path::new("C:/workspace/evohime");
        let first = runtime
            .capture(
                "task-2",
                workspace,
                CheckpointStatus::InProgress,
                CheckpointCaptureReason::RunStarted,
                None,
            )
            .await
            .expect("initial checkpoint persists");
        let second = runtime
            .capture(
                "task-2",
                workspace,
                CheckpointStatus::InProgress,
                CheckpointCaptureReason::BeforeCompaction,
                None,
            )
            .await
            .expect("latest checkpoint persists");
        journal
            .database()
            .lock()
            .await
            .connection()
            .execute(
                "UPDATE task_checkpoints SET canonical_json = ?1 WHERE id = ?2",
                rusqlite::params![b"{}", second.id],
            )
            .expect("corruption writes");
        let fallback = runtime
            .recover("task-2", workspace)
            .await
            .expect("corrupt latest falls back");
        assert_eq!(fallback.checkpoint.as_ref().unwrap().id, first.id);
        assert_eq!(fallback.disposition, RecoveryDisposition::Replayable);

        journal
            .database()
            .lock()
            .await
            .append_event("task-2", "run.recovery.blocked", br#"{"reason":"unknown"}"#)
            .expect("unknown outcome persists");
        let blocked = runtime
            .recover("task-2", workspace)
            .await
            .expect("blocked recovery reads");
        assert_eq!(blocked.disposition, RecoveryDisposition::Blocked);
        assert!(blocked
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("blind retry"));
        let _ = std::fs::remove_file(path);
    }
}
