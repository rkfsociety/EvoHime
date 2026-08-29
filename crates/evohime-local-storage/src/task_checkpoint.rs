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

pub const TASK_CHECKPOINT_VERSION: u32 = 1;
pub const TASK_CHECKPOINT_MAX_BYTES: usize = 256 * 1024;
pub const TASK_CHECKPOINT_MAX_ITEMS: usize = 128;
pub const TASK_CHECKPOINT_MAX_REFS: usize = 64;
pub const TASK_CHECKPOINT_MAX_TEXT_CHARS: usize = 4_096;
pub const TASK_CHECKPOINT_MAX_SUMMARY_CHARS: usize = 8_192;
pub const TASK_CHECKPOINT_MAX_ID_CHARS: usize = 128;
pub const TASK_CHECKPOINT_MAX_PATH_CHARS: usize = 512;
pub const TASK_CHECKPOINT_READ_LIMIT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TaskCheckpointError {
    #[error("unsupported task checkpoint version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid task checkpoint encoding: {reason}")]
    InvalidEncoding { reason: String },
    #[error("invalid task checkpoint field {field}: {reason}")]
    InvalidField { field: &'static str, reason: String },
    #[error("invalid stored task checkpoint metadata in {field}: {reason}")]
    InvalidStoredMetadata { field: &'static str, reason: String },
    #[error("task checkpoint path is outside the workspace: {0}")]
    InvalidPath(String),
    #[error("task checkpoint contains sensitive text in {field}")]
    SensitiveText { field: &'static str },
    #[error("model-proposed data cannot provide Core authority for {field}")]
    AuthorityViolation { field: &'static str },
    #[error("task checkpoint is too large: {0} bytes")]
    TooLarge(usize),
    #[error("task checkpoint content hash mismatch: expected {expected}, got {actual}")]
    ContentHashMismatch { expected: String, actual: String },
    #[error("task checkpoint parent {id} was not found")]
    ParentNotFound { id: String },
    #[error("task checkpoint parent belongs to another workspace")]
    ParentWorkspaceMismatch,
    #[error("task checkpoint event sequence must be newer than its parent")]
    ParentSequenceNotNewer,
    #[error("task checkpoint transition from {from:?} to {to:?} is not allowed")]
    InvalidStateTransition {
        from: CheckpointStatus,
        to: CheckpointStatus,
    },
    #[error("immutable task checkpoint id {id} cannot be overwritten")]
    ImmutableConflict { id: String },
}

impl TaskCheckpointError {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    InProgress,
    Paused,
    WaitingApproval,
    Resumable,
    Blocked,
    Completed,
    Failed,
    Stale,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Provenance {
    CoreDerived { source: String },
    ModelProposed { source: String },
}

impl Provenance {
    pub fn core(source: impl Into<String>) -> Self {
        Self::CoreDerived {
            source: source.into(),
        }
    }

    pub fn model(source: impl Into<String>) -> Self {
        Self::ModelProposed {
            source: source.into(),
        }
    }

    fn is_core_derived(&self) -> bool {
        matches!(self, Self::CoreDerived { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointItem {
    pub text: String,
    pub provenance: Provenance,
}

impl CheckpointItem {
    pub fn core(text: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            provenance: Provenance::core(source),
        }
    }

    pub fn model(text: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            provenance: Provenance::model(source),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointDecision {
    pub text: String,
    pub provenance: Provenance,
}

impl CheckpointDecision {
    pub fn core(text: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            provenance: Provenance::core(source),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileReadRef {
    pub path: String,
    pub evidence_ref: Option<String>,
    pub provenance: Provenance,
}

impl FileReadRef {
    pub fn core(path: impl Into<String>, evidence_ref: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            evidence_ref: Some(evidence_ref.into()),
            provenance: Provenance::core("core:file-read"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub change_kind: FileChangeKind,
    pub evidence_ref: Option<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestEvidence {
    pub name: String,
    pub status: TestStatus,
    pub evidence_ref: Option<String>,
    pub provenance: Provenance,
}

impl TestEvidence {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Passed,
    Failed,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateEvidence {
    pub id: String,
    pub status: GateStatus,
    pub evidence_ref: Option<String>,
    pub provenance: Provenance,
}

impl GateEvidence {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    Pending,
    Approved,
    Denied,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingApproval {
    pub id: String,
    pub state: ApprovalState,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointSensitivity {
    Public,
    Internal,
    Sensitive,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointRef {
    pub id: String,
    pub kind: String,
    pub content_hash: Option<String>,
    pub sensitivity: CheckpointSensitivity,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCheckpointV1 {
    pub id: String,
    pub version: u32,
    pub workspace_id: String,
    pub chat_id: Option<String>,
    pub goal_id: Option<String>,
    pub parent_checkpoint_id: Option<String>,
    pub objective: String,
    pub status: CheckpointStatus,
    pub completed_items: Vec<CheckpointItem>,
    pub remaining_items: Vec<CheckpointItem>,
    pub decisions: Vec<CheckpointDecision>,
    pub blockers: Vec<CheckpointItem>,
    pub files_read: Vec<FileReadRef>,
    pub files_changed: Vec<FileChange>,
    pub tests_passed: Vec<TestEvidence>,
    pub tests_failed: Vec<TestEvidence>,
    pub gates: Vec<GateEvidence>,
    pub pending_approvals: Vec<PendingApproval>,
    pub workflow_refs: Vec<CheckpointRef>,
    pub child_refs: Vec<CheckpointRef>,
    pub artifact_refs: Vec<CheckpointRef>,
    pub open_questions: Vec<CheckpointItem>,
    pub next_action: Option<CheckpointItem>,
    pub narrative_summary: Option<CheckpointItem>,
    pub source_event_seq: i64,
    pub created_at: i64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    AlreadyPresent,
}

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
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

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
mod tests {
    use super::*;

    fn checkpoint() -> TaskCheckpointV1 {
        TaskCheckpointV1 {
            id: "checkpoint-1".into(),
            version: TASK_CHECKPOINT_VERSION,
            workspace_id: "workspace-1".into(),
            chat_id: Some("chat-1".into()),
            goal_id: Some("goal-1".into()),
            parent_checkpoint_id: None,
            objective: "Implement the checkpoint contract".into(),
            status: CheckpointStatus::InProgress,
            completed_items: vec![CheckpointItem::core("repository inspected", "event:1")],
            remaining_items: vec![CheckpointItem::model("add runtime integration", "model:1")],
            decisions: vec![CheckpointDecision::core(
                "Core owns checkpoint state",
                "policy:1",
            )],
            blockers: Vec::new(),
            files_read: vec![FileReadRef::core("docs/architecture.md", "file:1")],
            files_changed: Vec::new(),
            tests_passed: vec![TestEvidence::core(
                "contract test",
                TestStatus::Passed,
                "test:1",
            )],
            tests_failed: Vec::new(),
            gates: vec![GateEvidence::core("storage", GateStatus::Passed, "gate:1")],
            pending_approvals: Vec::new(),
            workflow_refs: Vec::new(),
            child_refs: Vec::new(),
            artifact_refs: Vec::new(),
            open_questions: vec![CheckpointItem::model(
                "Should UI expose history?",
                "model:2",
            )],
            next_action: Some(CheckpointItem::model("Implement runtime hook", "model:3")),
            narrative_summary: Some(CheckpointItem::model("Bounded summary", "model:4")),
            source_event_seq: 1,
            created_at: 1_700_000_000_000,
            content_hash: String::new(),
        }
    }

    #[test]
    fn sealing_is_deterministic_and_validation_rejects_tampering() {
        let sealed = checkpoint().seal().expect("checkpoint seals");
        let second = checkpoint().seal().expect("same checkpoint seals");
        assert_eq!(sealed.content_hash, second.content_hash);
        assert_eq!(
            sealed.canonical_json().unwrap(),
            second.canonical_json().unwrap()
        );
        let expected_json = r#"{"id":"checkpoint-1","version":1,"workspace_id":"workspace-1","chat_id":"chat-1","goal_id":"goal-1","parent_checkpoint_id":null,"objective":"Implement the checkpoint contract","status":"in_progress","completed_items":[{"text":"repository inspected","provenance":{"core_derived":{"source":"event:1"}}}],"remaining_items":[{"text":"add runtime integration","provenance":{"model_proposed":{"source":"model:1"}}}],"decisions":[{"text":"Core owns checkpoint state","provenance":{"core_derived":{"source":"policy:1"}}}],"blockers":[],"files_read":[{"path":"docs/architecture.md","evidence_ref":"file:1","provenance":{"core_derived":{"source":"core:file-read"}}}],"files_changed":[],"tests_passed":[{"name":"contract test","status":"passed","evidence_ref":"test:1","provenance":{"core_derived":{"source":"core:test-run"}}}],"tests_failed":[],"gates":[{"id":"storage","status":"passed","evidence_ref":"gate:1","provenance":{"core_derived":{"source":"core:gate"}}}],"pending_approvals":[],"workflow_refs":[],"child_refs":[],"artifact_refs":[],"open_questions":[{"text":"Should UI expose history?","provenance":{"model_proposed":{"source":"model:2"}}}],"next_action":{"text":"Implement runtime hook","provenance":{"model_proposed":{"source":"model:3"}}},"narrative_summary":{"text":"Bounded summary","provenance":{"model_proposed":{"source":"model:4"}}},"source_event_seq":1,"created_at":1700000000000,"content_hash":"e33e378d675e088554628371bee6e6f6a03ce56a2d1658d3be1d38fc5dcf3e3d"}"#;
        assert_eq!(
            String::from_utf8(sealed.canonical_json().unwrap()).unwrap(),
            expected_json
        );
        assert_eq!(
            sealed.content_hash,
            "e33e378d675e088554628371bee6e6f6a03ce56a2d1658d3be1d38fc5dcf3e3d"
        );
        sealed.validate().expect("sealed checkpoint validates");

        let mut tampered = sealed.clone();
        tampered.objective.push_str(" changed");
        assert!(matches!(
            tampered.validate(),
            Err(TaskCheckpointError::ContentHashMismatch { .. })
        ));
    }

    #[test]
    fn model_proposed_evidence_cannot_confirm_effects_or_tests() {
        let mut invalid = checkpoint();
        invalid.tests_passed[0].provenance = Provenance::model("model:forged");
        let error = invalid.seal().expect_err("model evidence must be rejected");
        assert!(matches!(
            error,
            TaskCheckpointError::AuthorityViolation { .. }
        ));
    }

    #[test]
    fn secrets_and_workspace_escape_are_rejected_before_persistence() {
        let mut secret = checkpoint();
        secret.objective = "api_key=super-secret-value".into();
        assert!(matches!(
            secret.seal(),
            Err(TaskCheckpointError::SensitiveText { .. })
        ));

        let mut traversal = checkpoint();
        traversal.files_read[0].path = "../outside.txt".into();
        assert!(matches!(
            traversal.seal(),
            Err(TaskCheckpointError::InvalidPath(_))
        ));
    }

    #[test]
    fn store_is_immutable_idempotent_and_skips_corrupt_latest() {
        let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema installs");
        let store = TaskCheckpointStore::new(&connection);
        let mut first = checkpoint();
        first.status = CheckpointStatus::Paused;
        let first = first.seal().expect("checkpoint seals");
        assert_eq!(store.insert(&first).unwrap(), InsertOutcome::Inserted);
        assert_eq!(store.insert(&first).unwrap(), InsertOutcome::AlreadyPresent);
        assert_eq!(store.get(&first.id).unwrap(), Some(first.clone()));
        assert_eq!(store.get("missing").unwrap(), None);

        let mut conflicting = first.clone();
        conflicting.objective = "different".into();
        conflicting.content_hash = String::new();
        conflicting = conflicting.seal().expect("conflicting checkpoint seals");
        assert!(matches!(
            store.insert(&conflicting),
            Err(StorageError::TaskCheckpoint(
                TaskCheckpointError::ImmutableConflict { .. }
            ))
        ));

        let mut child = checkpoint();
        child.id = "checkpoint-2".into();
        child.parent_checkpoint_id = Some(first.id.clone());
        child.source_event_seq = 2;
        child = child.seal().expect("child seals");
        assert_eq!(store.insert(&child).unwrap(), InsertOutcome::Inserted);
        let mut grandchild = checkpoint();
        grandchild.id = "checkpoint-3".into();
        grandchild.parent_checkpoint_id = Some(child.id.clone());
        grandchild.source_event_seq = 3;
        grandchild = grandchild.seal().expect("grandchild seals");
        assert_eq!(store.insert(&grandchild).unwrap(), InsertOutcome::Inserted);
        assert_eq!(store.list("workspace-1", 10).unwrap().len(), 3);
        assert_eq!(
            store.latest_valid("workspace-1").unwrap().unwrap().id,
            grandchild.id
        );

        connection
            .execute(
                "UPDATE task_checkpoints SET canonical_json = ?1 WHERE id = ?2",
                rusqlite::params![b"{\"not\":\"a checkpoint\"}", child.id],
            )
            .expect("test corruption writes");
        let latest = store
            .latest_valid("workspace-1")
            .unwrap()
            .expect("fallback");
        assert_eq!(latest.id, first.id);

        connection
            .execute(
                "UPDATE task_checkpoints SET content_hash = ?1 WHERE id = ?2",
                rusqlite::params!["0".repeat(64), first.id],
            )
            .expect("test metadata corruption writes");
        assert!(matches!(
            store.get(&first.id),
            Err(StorageError::TaskCheckpoint(
                TaskCheckpointError::InvalidStoredMetadata {
                    field: "metadata",
                    ..
                }
            ))
        ));
    }

    #[test]
    fn parent_must_be_same_workspace_and_older_event_sequence() {
        let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema installs");
        let store = TaskCheckpointStore::new(&connection);
        let first = checkpoint().seal().expect("checkpoint seals");
        store.insert(&first).expect("parent inserts");

        let mut child = checkpoint();
        child.id = "checkpoint-2".into();
        child.parent_checkpoint_id = Some(first.id.clone());
        child.source_event_seq = first.source_event_seq;
        let child = child.seal().expect("child seals");
        assert!(matches!(
            store.insert(&child),
            Err(StorageError::TaskCheckpoint(
                TaskCheckpointError::ParentSequenceNotNewer
            ))
        ));
    }

    #[test]
    fn version_unknown_fields_secret_refs_and_bounds_fail_closed() {
        let mut unknown_version = checkpoint();
        unknown_version.version = 2;
        assert!(matches!(
            unknown_version.seal(),
            Err(TaskCheckpointError::UnsupportedVersion(2))
        ));

        let mut oversized = checkpoint();
        oversized.narrative_summary = Some(CheckpointItem::model(
            "x".repeat(TASK_CHECKPOINT_MAX_SUMMARY_CHARS + 1),
            "model:oversized",
        ));
        assert!(matches!(
            oversized.seal(),
            Err(TaskCheckpointError::InvalidField {
                field: "narrative_summary.text",
                ..
            })
        ));

        let mut secret_ref = checkpoint();
        secret_ref.artifact_refs.push(CheckpointRef {
            id: "artifact-1".into(),
            kind: "artifact".into(),
            content_hash: None,
            sensitivity: CheckpointSensitivity::Secret,
            provenance: Provenance::core("core:artifact"),
        });
        assert!(matches!(
            secret_ref.seal(),
            Err(TaskCheckpointError::InvalidField {
                field: "artifact_refs",
                ..
            })
        ));

        let mut many = checkpoint();
        many.completed_items =
            vec![CheckpointItem::core("done", "event:1"); TASK_CHECKPOINT_MAX_ITEMS + 1];
        assert!(matches!(
            many.seal(),
            Err(TaskCheckpointError::InvalidField {
                field: "completed_items",
                ..
            })
        ));

        let json = serde_json::to_value(checkpoint()).unwrap();
        let mut object = json.as_object().unwrap().clone();
        object.insert("unexpected_authority".into(), serde_json::json!(true));
        let error = serde_json::from_value::<TaskCheckpointWire>(serde_json::Value::Object(object))
            .expect_err("unknown fields must not be accepted");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn invalid_status_transition_is_rejected() {
        let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
        install_schema(&connection).expect("schema installs");
        let store = TaskCheckpointStore::new(&connection);
        let mut first = checkpoint();
        first.status = CheckpointStatus::Paused;
        let first = first.seal().expect("checkpoint seals");
        store.insert(&first).expect("parent inserts");

        let mut child = checkpoint();
        child.id = "checkpoint-2".into();
        child.parent_checkpoint_id = Some(first.id.clone());
        child.source_event_seq = 2;
        child.status = CheckpointStatus::Completed;
        child = child.seal().expect("child seals");
        assert!(matches!(
            store.insert(&child),
            Err(StorageError::TaskCheckpoint(
                TaskCheckpointError::InvalidStateTransition {
                    from: CheckpointStatus::Paused,
                    to: CheckpointStatus::Completed,
                }
            ))
        ));
    }

    #[test]
    fn secret_shaped_identifiers_and_refs_are_rejected() {
        let mut invalid = checkpoint();
        invalid.id = "token=not-for-storage".into();
        assert!(matches!(
            invalid.seal(),
            Err(TaskCheckpointError::SensitiveText { field: "id" })
        ));

        let mut invalid = checkpoint();
        invalid.files_read[0].evidence_ref = Some("secret-ref".into());
        assert!(matches!(
            invalid.seal(),
            Err(TaskCheckpointError::SensitiveText {
                field: "files_read.evidence_ref"
            })
        ));
    }

    #[test]
    fn hash_uses_normalized_fields() {
        let sealed = checkpoint().seal().expect("checkpoint seals");
        let mut padded = checkpoint();
        padded.objective = "  Implement the checkpoint contract  ".into();
        padded.files_read[0].path = "docs\\architecture.md".into();
        let padded = padded.seal().expect("padded checkpoint seals");
        assert_eq!(sealed.content_hash, padded.content_hash);
        assert_eq!(
            sealed.canonical_json().unwrap(),
            padded.canonical_json().unwrap()
        );
    }

    #[test]
    fn existing_schema_31_gets_checkpoint_table_with_backup() {
        let path = std::env::temp_dir().join(format!(
            "evohime-task-checkpoint-migration-{}.db",
            std::process::id()
        ));
        let backup = path.with_extension("db.bak");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);
        {
            let connection = rusqlite::Connection::open(&path).expect("sqlite");
            connection
                .execute_batch("CREATE TABLE legacy_marker(value TEXT); PRAGMA user_version = 31;")
                .expect("legacy schema seeds");
        }

        let database = crate::LocalDatabase::open(&path).expect("schema migrates");
        assert_eq!(database.schema_version().unwrap(), crate::SCHEMA_VERSION);
        let table_exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'task_checkpoints'",
                [],
                |row| row.get(0),
            )
            .expect("table lookup");
        assert_eq!(table_exists, 1);
        assert!(backup.exists());
        drop(database);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);
    }
}
