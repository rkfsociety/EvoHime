//! Durable TaskCheckpoint contract and immutable storage (plan 23.1).
//!
//! A checkpoint is a bounded continuity record. Core-derived facts are kept
//! separate from model proposals, and the canonical hash covers every field
//! except the hash itself. The store is append-only: a checkpoint id can be
//! replayed with the same payload, but it cannot be overwritten.

use std::{collections::HashSet, path::Path};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::StorageError;

/// Current serialized schema version for task checkpoints.
pub const TASK_CHECKPOINT_VERSION: u32 = 1;
/// Maximum canonical checkpoint payload size in bytes.
pub const TASK_CHECKPOINT_MAX_BYTES: usize = 256 * 1024;
/// Maximum number of entries in each checkpoint collection.
pub const TASK_CHECKPOINT_MAX_ITEMS: usize = 128;
/// Maximum number of references retained per checkpoint collection.
pub const TASK_CHECKPOINT_MAX_REFS: usize = 64;
/// Maximum text length for individual checkpoint items.
pub const TASK_CHECKPOINT_MAX_TEXT_CHARS: usize = 4_096;
/// Maximum length for the optional narrative summary.
pub const TASK_CHECKPOINT_MAX_SUMMARY_CHARS: usize = 8_192;
/// Maximum character length for checkpoint and related identifiers.
pub const TASK_CHECKPOINT_MAX_ID_CHARS: usize = 128;
/// Maximum character length for workspace-relative paths.
pub const TASK_CHECKPOINT_MAX_PATH_CHARS: usize = 512;
/// Default maximum number of checkpoints returned by one read operation.
pub const TASK_CHECKPOINT_READ_LIMIT: usize = 128;

/// Validation, lineage, and persistence errors for task checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TaskCheckpointError {
    /// The serialized checkpoint version is not supported.
    #[error("unsupported task checkpoint version {0}")]
    UnsupportedVersion(u32),
    /// The canonical checkpoint bytes could not be decoded.
    #[error("invalid task checkpoint encoding: {reason}")]
    InvalidEncoding {
        /// Why the canonical bytes could not be decoded.
        reason: String,
    },
    /// A checkpoint field violates its value or size constraints.
    #[error("invalid task checkpoint field {field}: {reason}")]
    InvalidField {
        /// Name of the invalid checkpoint field.
        field: &'static str,
        /// Why the field failed validation.
        reason: String,
    },
    /// Persisted indexed metadata does not agree with the canonical payload.
    #[error("invalid stored task checkpoint metadata in {field}: {reason}")]
    InvalidStoredMetadata {
        /// Name of the indexed metadata field that disagrees with the payload.
        field: &'static str,
        /// Why the stored metadata failed validation.
        reason: String,
    },
    /// A file reference escapes the checkpoint's workspace root.
    #[error("task checkpoint path is outside the workspace: {0}")]
    InvalidPath(String),
    /// Text in a field failed sensitive-data screening.
    #[error("task checkpoint contains sensitive text in {field}")]
    SensitiveText {
        /// Checkpoint text field rejected by sensitive-data screening.
        field: &'static str,
    },
    /// Model-proposed evidence was used for a field reserved for Core-derived authority.
    #[error("model-proposed data cannot provide Core authority for {field}")]
    AuthorityViolation {
        /// Field that requires Core-derived rather than model-proposed provenance.
        field: &'static str,
    },
    /// Canonical payload exceeds the configured byte bound.
    #[error("task checkpoint is too large: {0} bytes")]
    TooLarge(usize),
    /// Stored hash does not match the canonical payload digest.
    #[error("task checkpoint content hash mismatch: expected {expected}, got {actual}")]
    ContentHashMismatch {
        /// Hash recorded in the checkpoint metadata.
        expected: String,
        /// Hash computed from canonical checkpoint content.
        actual: String,
    },
    /// The referenced parent checkpoint does not exist.
    #[error("task checkpoint parent {id} was not found")]
    ParentNotFound {
        /// Missing parent checkpoint identifier.
        id: String,
    },
    /// Parent checkpoint belongs to a different workspace.
    #[error("task checkpoint parent belongs to another workspace")]
    ParentWorkspaceMismatch,
    /// Child checkpoint source sequence is not newer than its parent.
    #[error("task checkpoint event sequence must be newer than its parent")]
    ParentSequenceNotNewer,
    /// Status transition is not permitted by the checkpoint state machine.
    #[error("task checkpoint transition from {from:?} to {to:?} is not allowed")]
    InvalidStateTransition {
        /// Parent checkpoint status.
        from: CheckpointStatus,
        /// Child checkpoint status requested by the new record.
        to: CheckpointStatus,
    },
    /// Existing checkpoint identifier was reused with different canonical content.
    #[error("immutable task checkpoint id {id} cannot be overwritten")]
    ImmutableConflict {
        /// Checkpoint identifier already stored with different content.
        id: String,
    },
}

impl TaskCheckpointError {
    /// Returns the stable machine-readable error code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::InvalidEncoding { .. } => "invalid_encoding",
            Self::InvalidField { .. } => "invalid_field",
            Self::InvalidStoredMetadata { .. } => "invalid_stored_metadata",
            Self::InvalidPath(_) => "invalid_path",
            Self::SensitiveText { .. } => "sensitive_text",
            Self::AuthorityViolation { .. } => "authority_violation",
            Self::TooLarge(_) => "too_large",
            Self::ContentHashMismatch { .. } => "content_hash_mismatch",
            Self::ParentNotFound { .. } => "parent_not_found",
            Self::ParentWorkspaceMismatch => "parent_workspace_mismatch",
            Self::ParentSequenceNotNewer => "parent_sequence_not_newer",
            Self::InvalidStateTransition { .. } => "invalid_state_transition",
            Self::ImmutableConflict { .. } => "immutable_conflict",
        }
    }
}

/// Lifecycle states supported by the task checkpoint contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    /// Task is actively running.
    InProgress,
    /// Task is paused and may later resume.
    Paused,
    /// Task is waiting for an approval decision.
    WaitingApproval,
    /// Task can resume from the recorded checkpoint.
    Resumable,
    /// Task cannot proceed until an external blocker is resolved.
    Blocked,
    /// Task completed successfully.
    Completed,
    /// Task ended with a failure.
    Failed,
    /// Checkpoint is outdated relative to current task state.
    Stale,
    /// Checkpoint conflicts with another branch of task history.
    Conflicted,
}

impl CheckpointStatus {
    /// Allowed projection transitions. Every transition is append-only: a
    /// child checkpoint may keep the state or move it along this table.
    ///
    /// ```text
    /// in_progress   -> in_progress, paused, waiting_approval, resumable,
    ///                  blocked, completed, failed, stale, conflicted
    /// paused        -> paused, resumable, in_progress, failed, stale, conflicted
    /// waiting_approval -> waiting_approval, resumable, in_progress, blocked,
    ///                     failed, stale, conflicted
    /// resumable     -> resumable, in_progress, paused, waiting_approval,
    ///                  completed, failed, stale, conflicted
    /// blocked       -> blocked, resumable, in_progress, failed, stale, conflicted
    /// completed     -> completed, stale, conflicted
    /// failed        -> failed, resumable, in_progress, stale, conflicted
    /// stale         -> stale, resumable, conflicted
    /// conflicted    -> conflicted, resumable, in_progress
    /// ```
    pub const fn allows_transition_to(self, next: Self) -> bool {
        use CheckpointStatus::*;
        match self {
            InProgress => matches!(
                next,
                InProgress
                    | Paused
                    | WaitingApproval
                    | Resumable
                    | Blocked
                    | Completed
                    | Failed
                    | Stale
                    | Conflicted
            ),
            Paused => matches!(
                next,
                Paused | Resumable | InProgress | Failed | Stale | Conflicted
            ),
            WaitingApproval => matches!(
                next,
                WaitingApproval | Resumable | InProgress | Blocked | Failed | Stale | Conflicted
            ),
            Resumable => matches!(
                next,
                Resumable
                    | InProgress
                    | Paused
                    | WaitingApproval
                    | Completed
                    | Failed
                    | Stale
                    | Conflicted
            ),
            Blocked => matches!(
                next,
                Blocked | Resumable | InProgress | Failed | Stale | Conflicted
            ),
            Completed => matches!(next, Completed | Stale | Conflicted),
            Failed => matches!(next, Failed | Resumable | InProgress | Stale | Conflicted),
            Stale => matches!(next, Stale | Resumable | Conflicted),
            Conflicted => matches!(next, Conflicted | Resumable | InProgress),
        }
    }
}

/// Provenance category attached to checkpoint evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Provenance {
    /// Value was derived or confirmed by trusted Core execution.
    CoreDerived {
        /// Source label identifying the Core evidence origin.
        source: String,
    },
    /// Value was proposed by a model and does not grant Core authority.
    ModelProposed {
        /// Source label identifying the model proposal origin.
        source: String,
    },
}

impl Provenance {
    /// Marks evidence as derived by Core and records its source label.
    pub fn core(source: impl Into<String>) -> Self {
        Self::CoreDerived {
            source: source.into(),
        }
    }

    /// Marks evidence as a model proposal from the supplied source label.
    pub fn model(source: impl Into<String>) -> Self {
        Self::ModelProposed {
            source: source.into(),
        }
    }

    fn is_core_derived(&self) -> bool {
        matches!(self, Self::CoreDerived { .. })
    }
}

/// Bounded checkpoint text together with its authority provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointItem {
    /// Bounded, sensitive-data-screened text value.
    pub text: String,
    /// Origin label controlling whether the value may support authority.
    pub provenance: Provenance,
}

impl CheckpointItem {
    /// Creates an item marked as Core-derived evidence.
    pub fn core(text: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            provenance: Provenance::core(source),
        }
    }

    /// Creates an item marked as a non-authoritative model proposal.
    pub fn model(text: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            provenance: Provenance::model(source),
        }
    }
}

/// A checkpoint decision with explicit Core or model provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointDecision {
    /// Bounded decision text retained for continuity.
    pub text: String,
    /// Origin label distinguishing Core decisions from model suggestions.
    pub provenance: Provenance,
}

impl CheckpointDecision {
    /// Creates a decision recorded as Core-derived.
    pub fn core(text: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            provenance: Provenance::core(source),
        }
    }
}

/// Reference to a workspace-relative file read by Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileReadRef {
    /// Workspace-relative path that was read.
    pub path: String,
    /// Optional evidence identifier for the read result.
    pub evidence_ref: Option<String>,
    /// Provenance for the recorded file-read fact.
    pub provenance: Provenance,
}

impl FileReadRef {
    /// Creates a Core-derived file-read reference with an evidence identifier.
    pub fn core(path: impl Into<String>, evidence_ref: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            evidence_ref: Some(evidence_ref.into()),
            provenance: Provenance::core("core:file-read"),
        }
    }
}

/// Kind of filesystem change recorded in a checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    /// A workspace-relative file appeared in the recorded change set.
    Created,
    /// An existing workspace-relative file was updated.
    Modified,
    /// A previously existing workspace-relative file was removed.
    Deleted,
    /// A file was moved or renamed within the workspace.
    Renamed,
}

/// File change evidence with before/after hashes and Core provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    /// Workspace-relative path affected by the change.
    pub path: String,
    /// SHA-256 digest before the change, when the file existed.
    pub before_hash: Option<String>,
    /// SHA-256 digest after the change, when the file remains.
    pub after_hash: Option<String>,
    /// Kind of filesystem change recorded.
    pub change_kind: FileChangeKind,
    /// Optional reference to Core evidence for the change.
    pub evidence_ref: Option<String>,
    /// Core provenance required for authoritative file-change records.
    pub provenance: Provenance,
}

/// Possible execution outcomes for a recorded test or check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    /// The named check completed successfully.
    Passed,
    /// The named check ran and reported failure.
    Failed,
    /// The check was intentionally not run.
    Skipped,
    /// The result could not be determined.
    Unknown,
}

/// Test result evidence attributed to Core execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestEvidence {
    /// Human-readable name of the test or check.
    pub name: String,
    /// Recorded outcome of the check.
    pub status: TestStatus,
    /// Optional reference to the Core run evidence.
    pub evidence_ref: Option<String>,
    /// Core provenance required for test evidence.
    pub provenance: Provenance,
}

impl TestEvidence {
    /// Creates evidence attributed to a Core test run.
    pub fn core(
        name: impl Into<String>,
        status: TestStatus,
        evidence_ref: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status,
            evidence_ref: Some(evidence_ref.into()),
            provenance: Provenance::core("core:test-run"),
        }
    }
}

/// Result state of a recorded quality or policy gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    /// The gate accepted the recorded state.
    Passed,
    /// The gate rejected the recorded state.
    Failed,
    /// The gate cannot proceed until a condition is resolved.
    Blocked,
    /// The gate was intentionally not evaluated.
    Skipped,
}

/// Gate result evidence attributed to Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateEvidence {
    /// Stable identifier of the gate.
    pub id: String,
    /// Recorded gate outcome.
    pub status: GateStatus,
    /// Optional reference to the evidence used by the gate.
    pub evidence_ref: Option<String>,
    /// Core provenance required for gate outcomes.
    pub provenance: Provenance,
}

impl GateEvidence {
    /// Creates a gate result attributed to Core.
    pub fn core(
        id: impl Into<String>,
        status: GateStatus,
        evidence_ref: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            status,
            evidence_ref: Some(evidence_ref.into()),
            provenance: Provenance::core("core:gate"),
        }
    }
}

/// Lifecycle states for an approval request captured in a checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    /// The requested approval has not received a decision.
    Pending,
    /// The approval was granted.
    Approved,
    /// The approval was rejected.
    Denied,
    /// The approval expired before a decision was recorded.
    Expired,
}

/// Pending or resolved approval record with Core provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingApproval {
    /// Stable approval request identifier.
    pub id: String,
    /// Current state of the approval request.
    pub state: ApprovalState,
    /// Core provenance for the approval record.
    pub provenance: Provenance,
}

/// Disclosure classification for a referenced checkpoint object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointSensitivity {
    /// The referenced object is safe to disclose publicly.
    Public,
    /// The object is for internal use.
    Internal,
    /// The object contains sensitive information and needs restricted handling.
    Sensitive,
    /// The object is a secret and cannot be included in checkpoint references.
    Secret,
}

/// Typed reference to a workflow, child task, or artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointRef {
    /// Identifier of the referenced object.
    pub id: String,
    /// Object category, such as workflow, child task, or artifact.
    pub kind: String,
    /// Optional SHA-256 digest of the referenced content.
    pub content_hash: Option<String>,
    /// Disclosure classification; secret references are rejected.
    pub sensitivity: CheckpointSensitivity,
    /// Core provenance for this reference.
    pub provenance: Provenance,
}

/// Versioned, canonical continuity snapshot for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCheckpointV1 {
    /// Stable checkpoint identifier.
    pub id: String,
    /// Schema version; currently [`TASK_CHECKPOINT_VERSION`].
    pub version: u32,
    /// Workspace owning this checkpoint.
    pub workspace_id: String,
    /// Optional conversation associated with the checkpoint.
    pub chat_id: Option<String>,
    /// Optional persisted goal associated with the checkpoint.
    pub goal_id: Option<String>,
    /// Optional parent checkpoint identifier for a continuation.
    pub parent_checkpoint_id: Option<String>,
    /// Short description of the task being checkpointed.
    pub objective: String,
    /// Lifecycle state of the checkpoint.
    pub status: CheckpointStatus,
    /// Completed facts; entries must be Core-derived.
    pub completed_items: Vec<CheckpointItem>,
    /// Remaining work; entries may be model-proposed.
    pub remaining_items: Vec<CheckpointItem>,
    /// Recorded decisions with explicit provenance.
    pub decisions: Vec<CheckpointDecision>,
    /// Unresolved blockers; entries must be Core-derived.
    pub blockers: Vec<CheckpointItem>,
    /// Workspace-relative files read by Core.
    pub files_read: Vec<FileReadRef>,
    /// Workspace-relative file changes recorded by Core.
    pub files_changed: Vec<FileChange>,
    /// Tests with a successful outcome.
    pub tests_passed: Vec<TestEvidence>,
    /// Tests with a failed outcome.
    pub tests_failed: Vec<TestEvidence>,
    /// Core-evaluated policy or workflow gates.
    pub gates: Vec<GateEvidence>,
    /// Approval requests and their recorded states.
    pub pending_approvals: Vec<PendingApproval>,
    /// References to workflows related to this checkpoint.
    pub workflow_refs: Vec<CheckpointRef>,
    /// References to child checkpoints.
    pub child_refs: Vec<CheckpointRef>,
    /// References to artifacts associated with the task.
    pub artifact_refs: Vec<CheckpointRef>,
    /// Questions still requiring resolution.
    pub open_questions: Vec<CheckpointItem>,
    /// Suggested next action, when one is available.
    pub next_action: Option<CheckpointItem>,
    /// Optional concise narrative summary with explicit provenance.
    pub narrative_summary: Option<CheckpointItem>,
    /// Sequence number of the source event that produced this checkpoint.
    pub source_event_seq: i64,
    /// Creation timestamp in Unix milliseconds.
    pub created_at: i64,
    /// SHA-256 digest of normalized canonical JSON with this field cleared.
    pub content_hash: String,
}

/// Storage-only decode shape. `TaskCheckpointV1` deliberately has no public
/// `Deserialize` implementation: imported or model-provided JSON must not be
/// able to construct a Core-authoritative checkpoint. Only canonical bytes
/// already present in this storage module cross this boundary.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskCheckpointWire {
    id: String,
    version: u32,
    workspace_id: String,
    chat_id: Option<String>,
    goal_id: Option<String>,
    parent_checkpoint_id: Option<String>,
    objective: String,
    status: CheckpointStatus,
    completed_items: Vec<CheckpointItem>,
    remaining_items: Vec<CheckpointItem>,
    decisions: Vec<CheckpointDecision>,
    blockers: Vec<CheckpointItem>,
    files_read: Vec<FileReadRef>,
    files_changed: Vec<FileChange>,
    tests_passed: Vec<TestEvidence>,
    tests_failed: Vec<TestEvidence>,
    gates: Vec<GateEvidence>,
    pending_approvals: Vec<PendingApproval>,
    workflow_refs: Vec<CheckpointRef>,
    child_refs: Vec<CheckpointRef>,
    artifact_refs: Vec<CheckpointRef>,
    open_questions: Vec<CheckpointItem>,
    next_action: Option<CheckpointItem>,
    narrative_summary: Option<CheckpointItem>,
    source_event_seq: i64,
    created_at: i64,
    content_hash: String,
}

impl From<TaskCheckpointWire> for TaskCheckpointV1 {
    fn from(wire: TaskCheckpointWire) -> Self {
        Self {
            id: wire.id,
            version: wire.version,
            workspace_id: wire.workspace_id,
            chat_id: wire.chat_id,
            goal_id: wire.goal_id,
            parent_checkpoint_id: wire.parent_checkpoint_id,
            objective: wire.objective,
            status: wire.status,
            completed_items: wire.completed_items,
            remaining_items: wire.remaining_items,
            decisions: wire.decisions,
            blockers: wire.blockers,
            files_read: wire.files_read,
            files_changed: wire.files_changed,
            tests_passed: wire.tests_passed,
            tests_failed: wire.tests_failed,
            gates: wire.gates,
            pending_approvals: wire.pending_approvals,
            workflow_refs: wire.workflow_refs,
            child_refs: wire.child_refs,
            artifact_refs: wire.artifact_refs,
            open_questions: wire.open_questions,
            next_action: wire.next_action,
            narrative_summary: wire.narrative_summary,
            source_event_seq: wire.source_event_seq,
            created_at: wire.created_at,
            content_hash: wire.content_hash,
        }
    }
}

impl TaskCheckpointV1 {
    /// Normalizes and seals a checkpoint. The hash is a SHA-256 digest of the
    /// canonical JSON with `content_hash` cleared.
    pub fn seal(mut self) -> Result<Self, TaskCheckpointError> {
        self.normalize();
        self.validate_body()?;
        self.content_hash = self.compute_content_hash()?;
        self.validate()?;
        Ok(self)
    }

    /// Checks normalization, field constraints, provenance, and content hash.
    pub fn validate(&self) -> Result<(), TaskCheckpointError> {
        let normalized = self.normalized();
        normalized.validate_body()?;
        let actual = normalized.compute_content_hash_unchecked()?;
        if self.content_hash != actual {
            return Err(TaskCheckpointError::ContentHashMismatch {
                expected: self.content_hash.clone(),
                actual,
            });
        }
        if self != &normalized {
            return Err(invalid_field(
                "normalization",
                "checkpoint must be sealed from normalized fields",
            ));
        }
        normalized.serialize_checked()?;
        Ok(())
    }

    /// Returns canonical JSON including the sealed content hash.
    pub fn canonical_json(&self) -> Result<Vec<u8>, TaskCheckpointError> {
        self.validate()?;
        self.serialize_checked()
    }

    /// Computes the SHA-256 digest over normalized checkpoint data.
    pub fn compute_content_hash(&self) -> Result<String, TaskCheckpointError> {
        self.normalized().compute_content_hash_unchecked()
    }

    fn compute_content_hash_unchecked(&self) -> Result<String, TaskCheckpointError> {
        let mut unsigned = self.clone();
        unsigned.content_hash.clear();
        let json = serde_json::to_vec(&unsigned)
            .map_err(|error| invalid_field("serialization", error.to_string()))?;
        Ok(hex::encode(Sha256::digest(json)))
    }

    fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.normalize();
        normalized
    }

    fn serialize_checked(&self) -> Result<Vec<u8>, TaskCheckpointError> {
        let json = serde_json::to_vec(self)
            .map_err(|error| invalid_field("serialization", error.to_string()))?;
        if json.len() > TASK_CHECKPOINT_MAX_BYTES {
            return Err(TaskCheckpointError::TooLarge(json.len()));
        }
        Ok(json)
    }

    fn normalize(&mut self) {
        self.id = self.id.trim().to_owned();
        self.workspace_id = self.workspace_id.trim().to_owned();
        self.chat_id = self.chat_id.take().map(|value| value.trim().to_owned());
        self.goal_id = self.goal_id.take().map(|value| value.trim().to_owned());
        self.parent_checkpoint_id = self
            .parent_checkpoint_id
            .take()
            .map(|value| value.trim().to_owned());
        self.objective = self.objective.trim().to_owned();
        self.content_hash = self.content_hash.trim().to_ascii_lowercase();
        for item in self
            .completed_items
            .iter_mut()
            .chain(self.remaining_items.iter_mut())
            .chain(self.blockers.iter_mut())
            .chain(self.open_questions.iter_mut())
        {
            normalize_item(item);
        }
        for decision in &mut self.decisions {
            decision.text = decision.text.trim().to_owned();
            normalize_provenance(&mut decision.provenance);
        }
        for file in &mut self.files_read {
            file.path = normalize_path(&file.path);
            file.evidence_ref = normalize_optional(&file.evidence_ref);
            normalize_provenance(&mut file.provenance);
        }
        for file in &mut self.files_changed {
            file.path = normalize_path(&file.path);
            file.before_hash = normalize_optional(&file.before_hash);
            file.after_hash = normalize_optional(&file.after_hash);
            file.evidence_ref = normalize_optional(&file.evidence_ref);
            normalize_provenance(&mut file.provenance);
        }
        for test in self
            .tests_passed
            .iter_mut()
            .chain(self.tests_failed.iter_mut())
        {
            test.name = test.name.trim().to_owned();
            test.evidence_ref = normalize_optional(&test.evidence_ref);
            normalize_provenance(&mut test.provenance);
        }
        for gate in &mut self.gates {
            gate.id = gate.id.trim().to_owned();
            gate.evidence_ref = normalize_optional(&gate.evidence_ref);
            normalize_provenance(&mut gate.provenance);
        }
        for approval in &mut self.pending_approvals {
            approval.id = approval.id.trim().to_owned();
            normalize_provenance(&mut approval.provenance);
        }
        for reference in self
            .workflow_refs
            .iter_mut()
            .chain(self.child_refs.iter_mut())
            .chain(self.artifact_refs.iter_mut())
        {
            reference.id = reference.id.trim().to_owned();
            reference.kind = reference.kind.trim().to_owned();
            reference.content_hash = normalize_optional(&reference.content_hash);
            normalize_provenance(&mut reference.provenance);
        }
        if let Some(item) = &mut self.next_action {
            normalize_item(item);
        }
        if let Some(item) = &mut self.narrative_summary {
            normalize_item(item);
        }
    }

    fn validate_body(&self) -> Result<(), TaskCheckpointError> {
        if self.version != TASK_CHECKPOINT_VERSION {
            return Err(TaskCheckpointError::UnsupportedVersion(self.version));
        }
        validate_id("id", &self.id)?;
        validate_id("workspace_id", &self.workspace_id)?;
        validate_optional_id("chat_id", &self.chat_id)?;
        validate_optional_id("goal_id", &self.goal_id)?;
        validate_optional_id("parent_checkpoint_id", &self.parent_checkpoint_id)?;
        validate_text("objective", &self.objective, TASK_CHECKPOINT_MAX_TEXT_CHARS)?;
        ensure_safe_text("objective", &self.objective)?;
        if self.source_event_seq < 0 {
            return Err(invalid_field("source_event_seq", "must be non-negative"));
        }
        if self.created_at < 0 {
            return Err(invalid_field("created_at", "must be non-negative"));
        }
        validate_items("completed_items", &self.completed_items, true)?;
        validate_items("remaining_items", &self.remaining_items, false)?;
        for decision in &self.decisions {
            validate_text(
                "decisions.text",
                &decision.text,
                TASK_CHECKPOINT_MAX_TEXT_CHARS,
            )?;
            ensure_safe_text("decisions.text", &decision.text)?;
            validate_provenance(&decision.provenance, "decisions")?;
        }
        if self.decisions.len() > TASK_CHECKPOINT_MAX_ITEMS {
            return Err(invalid_field("decisions", "too many items"));
        }
        validate_items("blockers", &self.blockers, true)?;
        if self.files_read.len() > TASK_CHECKPOINT_MAX_ITEMS {
            return Err(invalid_field("files_read", "too many items"));
        }
        for file in &self.files_read {
            validate_path(&file.path)?;
            validate_optional_ref("files_read.evidence_ref", &file.evidence_ref)?;
            validate_core_provenance(&file.provenance, "files_read")?;
        }
        if self.files_changed.len() > TASK_CHECKPOINT_MAX_ITEMS {
            return Err(invalid_field("files_changed", "too many items"));
        }
        for file in &self.files_changed {
            validate_path(&file.path)?;
            validate_hash("files_changed.before_hash", &file.before_hash)?;
            validate_hash("files_changed.after_hash", &file.after_hash)?;
            validate_optional_ref("files_changed.evidence_ref", &file.evidence_ref)?;
            validate_core_provenance(&file.provenance, "files_changed")?;
        }
        validate_tests("tests_passed", &self.tests_passed)?;
        validate_tests("tests_failed", &self.tests_failed)?;
        if self.gates.len() > TASK_CHECKPOINT_MAX_ITEMS {
            return Err(invalid_field("gates", "too many items"));
        }
        for gate in &self.gates {
            validate_id("gates.id", &gate.id)?;
            validate_optional_ref("gates.evidence_ref", &gate.evidence_ref)?;
            validate_core_provenance(&gate.provenance, "gates")?;
        }
        if self.pending_approvals.len() > TASK_CHECKPOINT_MAX_ITEMS {
            return Err(invalid_field("pending_approvals", "too many items"));
        }
        for approval in &self.pending_approvals {
            validate_id("pending_approvals.id", &approval.id)?;
            validate_core_provenance(&approval.provenance, "pending_approvals")?;
        }
        validate_refs("workflow_refs", &self.workflow_refs)?;
        validate_refs("child_refs", &self.child_refs)?;
        validate_refs("artifact_refs", &self.artifact_refs)?;
        validate_items("open_questions", &self.open_questions, false)?;
        if let Some(item) = &self.next_action {
            validate_item(item, "next_action", false)?;
        }
        if let Some(item) = &self.narrative_summary {
            validate_text(
                "narrative_summary.text",
                &item.text,
                TASK_CHECKPOINT_MAX_SUMMARY_CHARS,
            )?;
            ensure_safe_text("narrative_summary", &item.text)?;
            validate_provenance(&item.provenance, "narrative_summary")?;
        }
        if !self.content_hash.is_empty() {
            validate_hash_string("content_hash", &self.content_hash)?;
        }
        Ok(())
    }
}

fn normalize_item(item: &mut CheckpointItem) {
    item.text = item.text.trim().to_owned();
    normalize_provenance(&mut item.provenance);
}

fn normalize_provenance(provenance: &mut Provenance) {
    match provenance {
        Provenance::CoreDerived { source } | Provenance::ModelProposed { source } => {
            *source = source.trim().to_owned();
        }
    }
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value.as_ref().map(|value| value.trim().to_owned())
}

fn normalize_path(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn validate_items(
    field: &'static str,
    items: &[CheckpointItem],
    core_only: bool,
) -> Result<(), TaskCheckpointError> {
    if items.len() > TASK_CHECKPOINT_MAX_ITEMS {
        return Err(invalid_field(field, "too many items"));
    }
    for item in items {
        validate_item(item, field, core_only)?;
    }
    Ok(())
}

fn validate_item(
    item: &CheckpointItem,
    field: &'static str,
    core_only: bool,
) -> Result<(), TaskCheckpointError> {
    validate_text(field, &item.text, TASK_CHECKPOINT_MAX_TEXT_CHARS)?;
    ensure_safe_text(field, &item.text)?;
    if core_only {
        validate_core_provenance(&item.provenance, field)
    } else {
        validate_provenance(&item.provenance, field)
    }
}

fn validate_tests(field: &'static str, tests: &[TestEvidence]) -> Result<(), TaskCheckpointError> {
    if tests.len() > TASK_CHECKPOINT_MAX_ITEMS {
        return Err(invalid_field(field, "too many items"));
    }
    for test in tests {
        validate_text(field, &test.name, TASK_CHECKPOINT_MAX_TEXT_CHARS)?;
        ensure_safe_text(field, &test.name)?;
        validate_optional_ref(field, &test.evidence_ref)?;
        validate_core_provenance(&test.provenance, field)?;
    }
    Ok(())
}

fn validate_refs(field: &'static str, refs: &[CheckpointRef]) -> Result<(), TaskCheckpointError> {
    if refs.len() > TASK_CHECKPOINT_MAX_REFS {
        return Err(invalid_field(field, "too many references"));
    }
    for reference in refs {
        validate_id(field, &reference.id)?;
        validate_id(field, &reference.kind)?;
        validate_hash(field, &reference.content_hash)?;
        if matches!(reference.sensitivity, CheckpointSensitivity::Secret) {
            return Err(invalid_field(field, "secret references are not allowed"));
        }
        validate_core_provenance(&reference.provenance, field)?;
    }
    Ok(())
}

fn validate_provenance(
    provenance: &Provenance,
    field: &'static str,
) -> Result<(), TaskCheckpointError> {
    let source = match provenance {
        Provenance::CoreDerived { source } | Provenance::ModelProposed { source } => source,
    };
    validate_text("provenance.source", source, TASK_CHECKPOINT_MAX_ID_CHARS)?;
    ensure_safe_text(field, source)
}

fn validate_core_provenance(
    provenance: &Provenance,
    field: &'static str,
) -> Result<(), TaskCheckpointError> {
    validate_provenance(provenance, field)?;
    if !provenance.is_core_derived() {
        return Err(TaskCheckpointError::AuthorityViolation { field });
    }
    Ok(())
}

fn validate_id(field: &'static str, value: &str) -> Result<(), TaskCheckpointError> {
    validate_text(field, value, TASK_CHECKPOINT_MAX_ID_CHARS)?;
    if value.chars().any(|character| character.is_control()) {
        return Err(invalid_field(field, "control characters are not allowed"));
    }
    ensure_safe_text(field, value)
}

fn validate_optional_id(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), TaskCheckpointError> {
    if let Some(value) = value {
        validate_id(field, value)?;
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_chars: usize,
) -> Result<(), TaskCheckpointError> {
    if value.is_empty() {
        return Err(invalid_field(field, "must not be empty"));
    }
    if value.chars().count() > max_chars {
        return Err(invalid_field(
            field,
            format!("exceeds {max_chars} characters"),
        ));
    }
    if value.contains('\0') {
        return Err(invalid_field(field, "NUL is not allowed"));
    }
    Ok(())
}

fn ensure_safe_text(field: &'static str, value: &str) -> Result<(), TaskCheckpointError> {
    let lower = value.to_ascii_lowercase();
    let words = lower
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let secret_marker = lower.contains("authorization: bearer")
        || lower.contains("authorization=bearer")
        || lower.contains("bearer ")
        || lower.contains("begin private key")
        || words.iter().any(|word| {
            matches!(
                *word,
                "api_key"
                    | "apikey"
                    | "access_token"
                    | "refreshtoken"
                    | "refresh_token"
                    | "client_secret"
                    | "password"
                    | "passwd"
                    | "secret"
                    | "token"
                    | "credential"
                    | "credentials"
            )
        })
        || words
            .windows(2)
            .any(|pair| matches!(pair, ["api", "key"] | ["private", "key"]));
    if secret_marker {
        return Err(TaskCheckpointError::SensitiveText { field });
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), TaskCheckpointError> {
    if path.chars().count() > TASK_CHECKPOINT_MAX_PATH_CHARS {
        return Err(TaskCheckpointError::InvalidPath(path.to_owned()));
    }
    let normalized = normalize_path(path);
    let windows_drive = normalized.as_bytes().get(1) == Some(&b':');
    if normalized.is_empty()
        || normalized.starts_with('/')
        || windows_drive
        || Path::new(&normalized).is_absolute()
        || normalized.split('/').any(|segment| segment == "..")
    {
        return Err(TaskCheckpointError::InvalidPath(path.to_owned()));
    }
    Ok(())
}

fn validate_optional_ref(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), TaskCheckpointError> {
    if let Some(value) = value {
        validate_id(field, value)?;
    }
    Ok(())
}

fn validate_hash(field: &'static str, value: &Option<String>) -> Result<(), TaskCheckpointError> {
    if let Some(value) = value {
        validate_hash_string(field, value)?;
    }
    Ok(())
}

fn validate_hash_string(field: &'static str, value: &str) -> Result<(), TaskCheckpointError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_field(field, "must be a SHA-256 hex digest"));
    }
    Ok(())
}

fn invalid_field(field: &'static str, reason: impl Into<String>) -> TaskCheckpointError {
    TaskCheckpointError::InvalidField {
        field,
        reason: reason.into(),
    }
}

/// Creates the checkpoint table and lookup indexes if they do not exist.
///
/// This function does not migrate or rewrite existing checkpoint data.
/// Creates the append-only task checkpoint table and lookup indexes.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS task_checkpoints (
            id TEXT PRIMARY KEY NOT NULL,
            version INTEGER NOT NULL,
            workspace_id TEXT NOT NULL,
            chat_id TEXT,
            goal_id TEXT,
            parent_checkpoint_id TEXT REFERENCES task_checkpoints(id),
            status TEXT NOT NULL,
            source_event_seq INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            canonical_json BLOB NOT NULL,
            CHECK(version = 1),
            CHECK(source_event_seq >= 0),
            CHECK(created_at >= 0)
        );
        CREATE INDEX IF NOT EXISTS idx_task_checkpoints_workspace_seq
            ON task_checkpoints(workspace_id, source_event_seq DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_task_checkpoints_parent
            ON task_checkpoints(parent_checkpoint_id);",
    )
}

/// Outcome of inserting an immutable checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    /// A new immutable checkpoint was persisted.
    Inserted,
    /// The same identifier and canonical content were already persisted.
    AlreadyPresent,
}

/// Append-only access to task checkpoints in a SQLite connection.
pub struct TaskCheckpointStore<'a> {
    connection: &'a Connection,
}

struct StoredCheckpointRow {
    id: String,
    version: i64,
    workspace_id: String,
    chat_id: Option<String>,
    goal_id: Option<String>,
    parent_checkpoint_id: Option<String>,
    status: String,
    source_event_seq: i64,
    created_at: i64,
    content_hash: String,
    canonical_json: Vec<u8>,
}

impl<'a> TaskCheckpointStore<'a> {
    /// Creates a store backed by an existing connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Validates and appends a sealed checkpoint, rejecting identifier conflicts.
    ///
    /// Re-inserting identical canonical content returns [`InsertOutcome::AlreadyPresent`];
    /// identifiers cannot be used to replace existing records.
    pub fn insert(&self, checkpoint: &TaskCheckpointV1) -> Result<InsertOutcome, StorageError> {
        checkpoint.validate()?;
        let canonical_json = checkpoint.canonical_json()?;
        let existing: Option<(String, Vec<u8>)> = self
            .connection
            .query_row(
                "SELECT content_hash, canonical_json FROM task_checkpoints WHERE id = ?1",
                [&checkpoint.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((existing_hash, existing_json)) = existing {
            if existing_hash == checkpoint.content_hash && existing_json == canonical_json {
                return Ok(InsertOutcome::AlreadyPresent);
            }
            return Err(TaskCheckpointError::ImmutableConflict {
                id: checkpoint.id.clone(),
            }
            .into());
        }

        if let Some(parent_id) = &checkpoint.parent_checkpoint_id {
            let Some(parent) = self.get(parent_id)? else {
                return Err(TaskCheckpointError::ParentNotFound {
                    id: parent_id.clone(),
                }
                .into());
            };
            if parent.workspace_id != checkpoint.workspace_id {
                return Err(TaskCheckpointError::ParentWorkspaceMismatch.into());
            }
            if checkpoint.source_event_seq <= parent.source_event_seq {
                return Err(TaskCheckpointError::ParentSequenceNotNewer.into());
            }
            if !parent.status.allows_transition_to(checkpoint.status) {
                return Err(TaskCheckpointError::InvalidStateTransition {
                    from: parent.status,
                    to: checkpoint.status,
                }
                .into());
            }
        }

        self.connection.execute(
            "INSERT INTO task_checkpoints
                (id, version, workspace_id, chat_id, goal_id, parent_checkpoint_id,
                 status, source_event_seq, created_at, content_hash, canonical_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                checkpoint.id,
                checkpoint.version,
                checkpoint.workspace_id,
                checkpoint.chat_id,
                checkpoint.goal_id,
                checkpoint.parent_checkpoint_id,
                status_as_str(checkpoint.status),
                checkpoint.source_event_seq,
                checkpoint.created_at,
                checkpoint.content_hash,
                canonical_json,
            ],
        )?;
        Ok(InsertOutcome::Inserted)
    }

    /// Loads and validates the checkpoint with the given identifier.
    pub fn get(&self, id: &str) -> Result<Option<TaskCheckpointV1>, StorageError> {
        let row: Option<StoredCheckpointRow> = self
            .connection
            .query_row(
                "SELECT id, version, workspace_id, chat_id, goal_id,
                        parent_checkpoint_id, status, source_event_seq, created_at,
                        content_hash, canonical_json
                 FROM task_checkpoints WHERE id = ?1",
                [id],
                read_checkpoint_row,
            )
            .optional()?;
        row.map(decode_stored_checkpoint).transpose()
    }

    /// Lists checkpoints for a workspace, newest source event first.
    ///
    /// The requested limit is capped at [`TASK_CHECKPOINT_READ_LIMIT`].
    pub fn list(
        &self,
        workspace_id: &str,
        limit: usize,
    ) -> Result<Vec<TaskCheckpointV1>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, version, workspace_id, chat_id, goal_id,
                    parent_checkpoint_id, status, source_event_seq, created_at,
                    content_hash, canonical_json
             FROM task_checkpoints
             WHERE workspace_id = ?1
             ORDER BY source_event_seq DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(
            rusqlite::params![
                workspace_id,
                i64::try_from(limit.min(TASK_CHECKPOINT_READ_LIMIT)).unwrap_or(i64::MAX),
            ],
            read_checkpoint_row,
        )?;
        rows.map(|row| {
            row.map_err(StorageError::from)
                .and_then(decode_stored_checkpoint)
        })
        .collect()
    }

    /// Returns the newest valid checkpoint. Corrupt latest rows are ignored so
    /// the caller can replay from the previous valid parent chain.
    pub fn latest_valid(
        &self,
        workspace_id: &str,
    ) -> Result<Option<TaskCheckpointV1>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, version, workspace_id, chat_id, goal_id,
                    parent_checkpoint_id, status, source_event_seq, created_at,
                    content_hash, canonical_json
             FROM task_checkpoints
             WHERE workspace_id = ?1
             ORDER BY source_event_seq DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(
            rusqlite::params![workspace_id, TASK_CHECKPOINT_READ_LIMIT as i64],
            read_checkpoint_row,
        )?;
        let stored_rows = rows.collect::<rusqlite::Result<Vec<StoredCheckpointRow>>>()?;
        for row in stored_rows {
            if let Ok(checkpoint) = decode_stored_checkpoint(row) {
                if !self.has_valid_parent_chain(&checkpoint)? {
                    continue;
                }
                return Ok(Some(checkpoint));
            }
        }
        Ok(None)
    }

    /// Returns the newest valid checkpoint for one workspace/chat scope.
    /// Chat scope is the runtime task identity; keeping it in the query avoids
    /// accidentally continuing another task that shares the workspace.
    pub fn latest_valid_for_chat(
        &self,
        workspace_id: &str,
        chat_id: &str,
    ) -> Result<Option<TaskCheckpointV1>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, version, workspace_id, chat_id, goal_id,
                    parent_checkpoint_id, status, source_event_seq, created_at,
                    content_hash, canonical_json
             FROM task_checkpoints
             WHERE workspace_id = ?1 AND chat_id = ?2
             ORDER BY source_event_seq DESC, id DESC LIMIT ?3",
        )?;
        let rows = statement.query_map(
            rusqlite::params![workspace_id, chat_id, TASK_CHECKPOINT_READ_LIMIT as i64],
            read_checkpoint_row,
        )?;
        let stored_rows = rows.collect::<rusqlite::Result<Vec<StoredCheckpointRow>>>()?;
        for row in stored_rows {
            if let Ok(checkpoint) = decode_stored_checkpoint(row) {
                if !self.has_valid_parent_chain(&checkpoint)? {
                    continue;
                }
                return Ok(Some(checkpoint));
            }
        }
        Ok(None)
    }

    fn has_valid_parent_chain(&self, checkpoint: &TaskCheckpointV1) -> Result<bool, StorageError> {
        let mut current = checkpoint.clone();
        let mut seen = HashSet::new();
        for _ in 0..=TASK_CHECKPOINT_READ_LIMIT {
            let Some(parent_id) = current.parent_checkpoint_id.as_deref() else {
                return Ok(true);
            };
            if !seen.insert(parent_id.to_owned()) {
                return Ok(false);
            }
            let parent = match self.get(parent_id) {
                Ok(Some(parent)) => parent,
                Ok(None) | Err(StorageError::TaskCheckpoint(_)) => return Ok(false),
                Err(error) => return Err(error),
            };
            if parent.workspace_id != current.workspace_id
                || parent.source_event_seq >= current.source_event_seq
                || !parent.status.allows_transition_to(current.status)
            {
                return Ok(false);
            }
            current = parent;
        }
        Ok(false)
    }
}

fn status_as_str(status: CheckpointStatus) -> &'static str {
    match status {
        CheckpointStatus::InProgress => "in_progress",
        CheckpointStatus::Paused => "paused",
        CheckpointStatus::WaitingApproval => "waiting_approval",
        CheckpointStatus::Resumable => "resumable",
        CheckpointStatus::Blocked => "blocked",
        CheckpointStatus::Completed => "completed",
        CheckpointStatus::Failed => "failed",
        CheckpointStatus::Stale => "stale",
        CheckpointStatus::Conflicted => "conflicted",
    }
}

fn read_checkpoint_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCheckpointRow> {
    Ok(StoredCheckpointRow {
        id: row.get(0)?,
        version: row.get(1)?,
        workspace_id: row.get(2)?,
        chat_id: row.get(3)?,
        goal_id: row.get(4)?,
        parent_checkpoint_id: row.get(5)?,
        status: row.get(6)?,
        source_event_seq: row.get(7)?,
        created_at: row.get(8)?,
        content_hash: row.get(9)?,
        canonical_json: row.get(10)?,
    })
}

fn decode_stored_checkpoint(row: StoredCheckpointRow) -> Result<TaskCheckpointV1, StorageError> {
    let checkpoint = decode_checkpoint(&row.canonical_json)?;
    let expected_version =
        u32::try_from(row.version).map_err(|_| TaskCheckpointError::InvalidStoredMetadata {
            field: "version",
            reason: "SQL version is outside the contract range".into(),
        })?;
    let metadata_matches = checkpoint.id == row.id
        && checkpoint.version == expected_version
        && checkpoint.workspace_id == row.workspace_id
        && checkpoint.chat_id == row.chat_id
        && checkpoint.goal_id == row.goal_id
        && checkpoint.parent_checkpoint_id == row.parent_checkpoint_id
        && status_as_str(checkpoint.status) == row.status
        && checkpoint.source_event_seq == row.source_event_seq
        && checkpoint.created_at == row.created_at
        && checkpoint.content_hash == row.content_hash;
    if !metadata_matches {
        return Err(TaskCheckpointError::InvalidStoredMetadata {
            field: "metadata",
            reason: "SQL metadata does not match canonical checkpoint JSON".into(),
        }
        .into());
    }
    Ok(checkpoint)
}

fn decode_checkpoint(json: &[u8]) -> Result<TaskCheckpointV1, StorageError> {
    if json.len() > TASK_CHECKPOINT_MAX_BYTES {
        return Err(TaskCheckpointError::TooLarge(json.len()).into());
    }
    let wire: TaskCheckpointWire =
        serde_json::from_slice(json).map_err(|error| TaskCheckpointError::InvalidEncoding {
            reason: error.to_string(),
        })?;
    let checkpoint: TaskCheckpointV1 = wire.into();
    checkpoint.validate()?;
    Ok(checkpoint)
}

#[cfg(test)]
#[path = "task_checkpoint_tests.rs"]
mod tests;
