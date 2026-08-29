//! Durable Core-owned Goal contract and storage (plan 25.1).
//!
//! A Goal is a durable objective and a bounded progress projection. It is not
//! a scheduler and it never contains capabilities, credentials, prompts or
//! hidden model reasoning. Revisions and events are append-only; the current
//! projection is updated transactionally with its idempotency record.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::StorageError;

pub const GOAL_SCHEMA_VERSION: u32 = 1;
pub const GOAL_MAX_BYTES: usize = 256 * 1024;
pub const GOAL_MAX_ID_CHARS: usize = 128;
pub const GOAL_MAX_OBJECTIVE_CHARS: usize = 4_096;
pub const GOAL_MAX_CRITERIA: usize = 64;
pub const GOAL_MAX_LIST_ITEMS: usize = 128;
pub const GOAL_MAX_TEXT_CHARS: usize = 4_096;
pub const GOAL_MAX_READ_LIMIT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GoalError {
    #[error("unsupported goal version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid goal field {field}: {reason}")]
    InvalidField { field: String, reason: String },
    #[error("invalid stored goal: {0}")]
    InvalidStored(String),
    #[error("goal content hash mismatch: expected {expected}, got {actual}")]
    ContentHashMismatch { expected: String, actual: String },
    #[error("goal contains sensitive text in {field}")]
    SensitiveText { field: String },
    #[error("model-proposed data cannot provide Core authority for {field}")]
    AuthorityViolation { field: String },
    #[error("goal is too large: {0} bytes")]
    TooLarge(usize),
    #[error("goal {0} was not found")]
    NotFound(String),
    #[error("goal {0} already exists")]
    AlreadyExists(String),
    #[error("goal reference {kind}:{reference_id} was not found")]
    ReferenceNotFound { kind: String, reference_id: String },
    #[error("goal criterion {0} was not found")]
    CriterionNotFound(String),
    #[error("goal criterion ids must be unique")]
    DuplicateCriterion,
    #[error("goal transition from {from:?} to {to:?} is not allowed")]
    InvalidStateTransition { from: GoalStatus, to: GoalStatus },
    #[error("goal cannot be completed before every criterion has Core evidence")]
    CompletionEvidenceMissing,
    #[error("goal command idempotency key is required")]
    MissingIdempotencyKey,
}

impl GoalError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::InvalidField { .. } => "invalid_field",
            Self::InvalidStored(_) => "invalid_stored",
            Self::ContentHashMismatch { .. } => "content_hash_mismatch",
            Self::SensitiveText { .. } => "sensitive_text",
            Self::AuthorityViolation { .. } => "authority_violation",
            Self::TooLarge(_) => "too_large",
            Self::NotFound(_) => "not_found",
            Self::AlreadyExists(_) => "already_exists",
            Self::ReferenceNotFound { .. } => "reference_not_found",
            Self::CriterionNotFound(_) => "criterion_not_found",
            Self::DuplicateCriterion => "duplicate_criterion",
            Self::InvalidStateTransition { .. } => "invalid_state_transition",
            Self::CompletionEvidenceMissing => "completion_evidence_missing",
            Self::MissingIdempotencyKey => "missing_idempotency_key",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    BudgetLimited,
    Completed,
    Failed,
    Cancelled,
}

impl GoalStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::BudgetLimited => "budget_limited",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn allows_transition_to(self, next: Self) -> bool {
        use GoalStatus::*;
        match self {
            Active => matches!(
                next,
                Active | Paused | Blocked | BudgetLimited | Completed | Failed | Cancelled
            ),
            Paused => matches!(next, Paused | Active | Blocked | BudgetLimited | Cancelled),
            Blocked => matches!(next, Blocked | Active | Paused | BudgetLimited | Cancelled),
            BudgetLimited => matches!(next, BudgetLimited | Active | Paused | Cancelled),
            Completed => matches!(next, Completed),
            Failed => matches!(next, Failed),
            Cancelled => matches!(next, Cancelled),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalCriterionKind {
    Manual,
    Gate,
    WorkflowEvidence,
    Artifact,
}

impl GoalCriterionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Gate => "gate",
            Self::WorkflowEvidence => "workflow_evidence",
            Self::Artifact => "artifact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalCriterionStatus {
    Pending,
    Verified,
    Failed,
    Blocked,
}

impl GoalCriterionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Verified => "verified",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        }
    }
}

/// Provenance is deliberately a closed enum. Only the Core verification path
/// can write `Core`; a model summary can never turn into authoritative proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalProvenance {
    User,
    Core,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalCriterionV1 {
    pub id: String,
    pub kind: GoalCriterionKind,
    pub statement: String,
    pub status: GoalCriterionStatus,
    pub evidence_ref: Option<String>,
    pub verifier_id: Option<String>,
    pub verifier_version: Option<String>,
    pub verified_at_ms: Option<i64>,
    pub provenance: GoalProvenance,
}

impl GoalCriterionV1 {
    pub fn new(
        id: impl Into<String>,
        kind: GoalCriterionKind,
        statement: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            statement: statement.into(),
            status: GoalCriterionStatus::Pending,
            evidence_ref: None,
            verifier_id: None,
            verifier_version: None,
            verified_at_ms: None,
            provenance: GoalProvenance::User,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalV1 {
    pub id: String,
    pub version: u64,
    pub workspace_id: String,
    pub chat_id: Option<String>,
    pub objective: String,
    pub success_criteria: Vec<GoalCriterionV1>,
    pub status: GoalStatus,
    pub progress_summary: String,
    pub completed_criteria: Vec<String>,
    pub remaining_criteria: Vec<String>,
    pub blockers: Vec<String>,
    pub next_action: Option<String>,
    pub workflow_run_ids: Vec<String>,
    pub child_run_ids: Vec<String>,
    pub checkpoint_id: Option<String>,
    pub token_budget: Option<u64>,
    pub cost_budget_micros: Option<u64>,
    pub continuation_budget: Option<u64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub created_by: String,
    pub updated_by: String,
    pub content_hash: String,
}

impl GoalV1 {
    pub fn seal(mut self) -> Result<Self, GoalError> {
        self.normalize();
        self.validate_body()?;
        self.content_hash = self.compute_content_hash_unchecked()?;
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), GoalError> {
        let normalized = self.normalized();
        normalized.validate_body()?;
        let actual = normalized.compute_content_hash_unchecked()?;
        if self.content_hash != actual {
            return Err(GoalError::ContentHashMismatch {
                expected: self.content_hash.clone(),
                actual,
            });
        }
        if self != &normalized {
            return Err(invalid_field(
                "normalization",
                "goal must be sealed from normalized fields",
            ));
        }
        self.serialize_checked()?;
        Ok(())
    }

    pub fn canonical_json(&self) -> Result<Vec<u8>, GoalError> {
        self.validate()?;
        self.serialize_checked()
    }

    pub fn compute_content_hash(&self) -> Result<String, GoalError> {
        self.normalized().compute_content_hash_unchecked()
    }

    fn compute_content_hash_unchecked(&self) -> Result<String, GoalError> {
        let mut unsigned = self.clone();
        unsigned.content_hash.clear();
        let bytes = serde_json::to_vec(&unsigned)
            .map_err(|error| invalid_field("serialization", error.to_string()))?;
        let digest = Sha256::digest(bytes);
        Ok(hex::encode(digest.as_slice()))
    }

    fn serialize_checked(&self) -> Result<Vec<u8>, GoalError> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| invalid_field("serialization", error.to_string()))?;
        if bytes.len() > GOAL_MAX_BYTES {
            return Err(GoalError::TooLarge(bytes.len()));
        }
        Ok(bytes)
    }

    fn normalized(&self) -> Self {
        let mut normalized = self.clone();
        normalized.normalize();
        normalized
    }

    fn normalize(&mut self) {
        self.id = self.id.trim().to_owned();
        self.workspace_id = self.workspace_id.trim().to_owned();
        self.chat_id = normalize_optional(&self.chat_id);
        self.objective = self.objective.trim().to_owned();
        self.progress_summary = self.progress_summary.trim().to_owned();
        self.next_action = normalize_optional(&self.next_action);
        self.checkpoint_id = normalize_optional(&self.checkpoint_id);
        self.created_by = self.created_by.trim().to_owned();
        self.updated_by = self.updated_by.trim().to_owned();
        self.content_hash = self.content_hash.trim().to_ascii_lowercase();
        for criterion in &mut self.success_criteria {
            criterion.id = criterion.id.trim().to_owned();
            criterion.statement = criterion.statement.trim().to_owned();
            criterion.evidence_ref = normalize_optional(&criterion.evidence_ref);
            criterion.verifier_id = normalize_optional(&criterion.verifier_id);
            criterion.verifier_version = normalize_optional(&criterion.verifier_version);
        }
        self.completed_criteria = self
            .success_criteria
            .iter()
            .filter(|criterion| criterion.status == GoalCriterionStatus::Verified)
            .map(|criterion| criterion.id.clone())
            .collect();
        self.remaining_criteria = self
            .success_criteria
            .iter()
            .filter(|criterion| criterion.status != GoalCriterionStatus::Verified)
            .map(|criterion| criterion.id.clone())
            .collect();
        for value in self
            .blockers
            .iter_mut()
            .chain(self.workflow_run_ids.iter_mut())
            .chain(self.child_run_ids.iter_mut())
        {
            *value = value.trim().to_owned();
        }
    }

    fn validate_body(&self) -> Result<(), GoalError> {
        if self.version == 0 {
            return Err(GoalError::UnsupportedVersion(0));
        }
        validate_id("id", &self.id)?;
        validate_id("workspace_id", &self.workspace_id)?;
        validate_optional_id("chat_id", &self.chat_id)?;
        validate_text("objective", &self.objective, GOAL_MAX_OBJECTIVE_CHARS)?;
        ensure_safe_text("objective", &self.objective)?;
        validate_text(
            "progress_summary",
            &self.progress_summary,
            GOAL_MAX_TEXT_CHARS,
        )?;
        ensure_safe_text("progress_summary", &self.progress_summary)?;
        validate_optional_text("next_action", &self.next_action, GOAL_MAX_TEXT_CHARS)?;
        validate_optional_id("checkpoint_id", &self.checkpoint_id)?;
        validate_id("created_by", &self.created_by)?;
        validate_id("updated_by", &self.updated_by)?;
        if self.created_at_ms < 0
            || self.updated_at_ms < 0
            || self.updated_at_ms < self.created_at_ms
        {
            return Err(invalid_field(
                "timestamps",
                "must be non-negative and monotonic",
            ));
        }
        if self.success_criteria.is_empty() || self.success_criteria.len() > GOAL_MAX_CRITERIA {
            return Err(invalid_field(
                "success_criteria",
                "must contain 1..64 criteria",
            ));
        }
        let mut criterion_ids = HashSet::new();
        for criterion in &self.success_criteria {
            validate_id("success_criteria.id", &criterion.id)?;
            if !criterion_ids.insert(criterion.id.clone()) {
                return Err(GoalError::DuplicateCriterion);
            }
            validate_text(
                "success_criteria.statement",
                &criterion.statement,
                GOAL_MAX_TEXT_CHARS,
            )?;
            ensure_safe_text("success_criteria.statement", &criterion.statement)?;
            validate_optional_id("success_criteria.evidence_ref", &criterion.evidence_ref)?;
            validate_optional_id("success_criteria.verifier_id", &criterion.verifier_id)?;
            validate_optional_id(
                "success_criteria.verifier_version",
                &criterion.verifier_version,
            )?;
            if criterion.verified_at_ms.is_some_and(|value| value < 0) {
                return Err(invalid_field(
                    "success_criteria.verified_at_ms",
                    "must be non-negative",
                ));
            }
            if criterion.status == GoalCriterionStatus::Verified
                && (criterion.evidence_ref.is_none()
                    || criterion.verifier_id.is_none()
                    || criterion.verifier_version.is_none()
                    || criterion.verified_at_ms.is_none())
            {
                return Err(GoalError::AuthorityViolation {
                    field: format!("criterion {}", criterion.id),
                });
            }
            if criterion.status == GoalCriterionStatus::Verified
                && criterion.provenance != GoalProvenance::Core
            {
                return Err(GoalError::AuthorityViolation {
                    field: format!("criterion {}", criterion.id),
                });
            }
        }
        validate_list("blockers", &self.blockers, false)?;
        validate_list("workflow_run_ids", &self.workflow_run_ids, true)?;
        validate_list("child_run_ids", &self.child_run_ids, true)?;
        if self.status == GoalStatus::Completed
            && self
                .success_criteria
                .iter()
                .any(|criterion| criterion.status != GoalCriterionStatus::Verified)
        {
            return Err(GoalError::CompletionEvidenceMissing);
        }
        if !self.content_hash.is_empty() {
            validate_hash(&self.content_hash)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalMutationResult {
    pub action: String,
    pub goal: GoalV1,
    pub applied: bool,
    pub deduplicated: bool,
    pub event_sequence: i64,
}

/// Identity and provenance attached to one Core command. Keeping it together
/// prevents mutation APIs from accidentally dropping idempotency or actor
/// validation as fields are added to a command.
#[derive(Debug, Clone, Copy)]
pub struct GoalCommand<'a> {
    pub actor: &'a str,
    pub idempotency_key: &'a str,
    pub command_hash: &'a str,
}

impl<'a> GoalCommand<'a> {
    pub const fn new(actor: &'a str, idempotency_key: &'a str, command_hash: &'a str) -> Self {
        Self {
            actor,
            idempotency_key,
            command_hash,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GoalCriterionEvidence<'a> {
    pub criterion_id: &'a str,
    pub evidence_ref: &'a str,
    pub verifier_id: &'a str,
    pub verifier_version: &'a str,
}

impl<'a> GoalCriterionEvidence<'a> {
    pub const fn new(
        criterion_id: &'a str,
        evidence_ref: &'a str,
        verifier_id: &'a str,
        verifier_version: &'a str,
    ) -> Self {
        Self {
            criterion_id,
            evidence_ref,
            verifier_id,
            verifier_version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalRecoveryProjection {
    pub goal_id: String,
    pub status: GoalStatus,
    pub warning: String,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS goals (
            id TEXT PRIMARY KEY NOT NULL,
            version INTEGER NOT NULL,
            workspace_id TEXT NOT NULL,
            chat_id TEXT,
            objective TEXT NOT NULL,
            status TEXT NOT NULL,
            progress_summary TEXT NOT NULL,
            completed_criteria_json TEXT NOT NULL,
            remaining_criteria_json TEXT NOT NULL,
            blockers_json TEXT NOT NULL,
            next_action TEXT,
            workflow_run_ids_json TEXT NOT NULL,
            child_run_ids_json TEXT NOT NULL,
            checkpoint_id TEXT,
            token_budget INTEGER,
            cost_budget_micros INTEGER,
            continuation_budget INTEGER,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            created_by TEXT NOT NULL,
            updated_by TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            canonical_json BLOB NOT NULL,
            CHECK(version >= 1),
            CHECK(created_at_ms >= 0),
            CHECK(updated_at_ms >= created_at_ms)
        );
        CREATE INDEX IF NOT EXISTS idx_goals_workspace_status
            ON goals(workspace_id, status, updated_at_ms DESC, id DESC);
        CREATE TABLE IF NOT EXISTS goal_revisions (
            goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
            version INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            canonical_json BLOB NOT NULL,
            actor TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(goal_id, version)
        );
        CREATE TABLE IF NOT EXISTS goal_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
            goal_version INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            actor TEXT NOT NULL,
            idempotency_key TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            UNIQUE(goal_id, idempotency_key)
        );
        CREATE INDEX IF NOT EXISTS idx_goal_events_goal
            ON goal_events(goal_id, id DESC);
        CREATE TABLE IF NOT EXISTS goal_commands (
            idempotency_key TEXT PRIMARY KEY NOT NULL,
            command_hash TEXT NOT NULL,
            result_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS events (
            sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            payload BLOB NOT NULL,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );
        CREATE INDEX IF NOT EXISTS idx_events_task_sequence ON events(task_id, sequence_id);",
    )
}

pub struct GoalStore<'a> {
    connection: &'a Connection,
}

impl<'a> GoalStore<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn get(&self, id: &str) -> Result<Option<GoalV1>, StorageError> {
        let row: Option<StoredGoalRow> = self
            .connection
            .query_row(
                "SELECT id, version, workspace_id, chat_id, objective, status,
                        progress_summary, completed_criteria_json, remaining_criteria_json,
                        blockers_json, next_action, workflow_run_ids_json, child_run_ids_json,
                        checkpoint_id, token_budget, cost_budget_micros, continuation_budget,
                        created_at_ms, updated_at_ms, created_by, updated_by, content_hash,
                        canonical_json
                 FROM goals WHERE id = ?1",
                [id],
                read_goal_row,
            )
            .optional()?;
        row.map(decode_stored).transpose()
    }

    pub fn list(&self, workspace_id: &str, limit: usize) -> Result<Vec<GoalV1>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, version, workspace_id, chat_id, objective, status,
                    progress_summary, completed_criteria_json, remaining_criteria_json,
                    blockers_json, next_action, workflow_run_ids_json, child_run_ids_json,
                    checkpoint_id, token_budget, cost_budget_micros, continuation_budget,
                    created_at_ms, updated_at_ms, created_by, updated_by, content_hash,
                    canonical_json
             FROM goals WHERE workspace_id = ?1
             ORDER BY updated_at_ms DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![workspace_id, limit.min(GOAL_MAX_READ_LIMIT) as i64],
            read_goal_row,
        )?;
        rows.map(|row| row.map_err(StorageError::from).and_then(decode_stored))
            .collect()
    }

    pub fn create(
        &self,
        goal: &GoalV1,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, StorageError> {
        let goal = goal.clone().seal()?;
        validate_actor(command.actor)?;
        validate_command(command.idempotency_key, command.command_hash)?;
        if let Some(result) = self.replay(command.idempotency_key, command.command_hash)? {
            return Ok(result);
        }
        let existing: Option<i64> = self
            .connection
            .query_row("SELECT 1 FROM goals WHERE id = ?1", [&goal.id], |row| {
                row.get(0)
            })
            .optional()?;
        if existing.is_some() {
            return Err(GoalError::AlreadyExists(goal.id).into());
        }
        let canonical = goal.canonical_json()?;
        let transaction = self.connection.unchecked_transaction()?;
        insert_projection(&transaction, &goal, &canonical)?;
        insert_revision(&transaction, &goal, command.actor, &canonical)?;
        let event_sequence = append_change(
            &transaction,
            &goal,
            "goal.created",
            command.actor,
            command.idempotency_key,
        )?;
        let result = GoalMutationResult {
            action: "create".into(),
            goal,
            applied: true,
            deduplicated: false,
            event_sequence,
        };
        store_command(
            &transaction,
            command.idempotency_key,
            command.command_hash,
            &result,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn transition(
        &self,
        id: &str,
        expected_version: u64,
        next_status: GoalStatus,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, StorageError> {
        validate_actor(command.actor)?;
        validate_command(command.idempotency_key, command.command_hash)?;
        if let Some(result) = self.replay(command.idempotency_key, command.command_hash)? {
            return Ok(result);
        }
        let current = self.require(id)?;
        check_version(&current, expected_version)?;
        if !current.status.allows_transition_to(next_status) {
            return Err(GoalError::InvalidStateTransition {
                from: current.status,
                to: next_status,
            }
            .into());
        }
        let mut next = current;
        next.version += 1;
        next.status = next_status;
        next.updated_by = command.actor.trim().to_owned();
        next.updated_at_ms = now_ms();
        self.save(next, transition_action(next_status), command)
    }

    pub fn update(
        &self,
        id: &str,
        expected_version: u64,
        objective: Option<String>,
        criteria: Option<Vec<GoalCriterionV1>>,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, StorageError> {
        validate_actor(command.actor)?;
        validate_command(command.idempotency_key, command.command_hash)?;
        if let Some(result) = self.replay(command.idempotency_key, command.command_hash)? {
            return Ok(result);
        }
        let current = self.require(id)?;
        check_version(&current, expected_version)?;
        let mut next = current;
        if let Some(objective) = objective {
            next.objective = objective;
        }
        if let Some(criteria) = criteria {
            next.success_criteria = criteria;
        }
        next.version += 1;
        next.updated_by = command.actor.trim().to_owned();
        next.updated_at_ms = now_ms();
        self.save(next, "goal.updated", command)
    }

    pub fn verify_criterion(
        &self,
        id: &str,
        expected_version: u64,
        evidence: GoalCriterionEvidence<'_>,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, StorageError> {
        validate_actor(command.actor)?;
        validate_command(command.idempotency_key, command.command_hash)?;
        if let Some(result) = self.replay(command.idempotency_key, command.command_hash)? {
            return Ok(result);
        }
        validate_id("evidence_ref", evidence.evidence_ref)?;
        validate_id("verifier_id", evidence.verifier_id)?;
        validate_id("verifier_version", evidence.verifier_version)?;
        let current = self.require(id)?;
        check_version(&current, expected_version)?;
        let mut next = current;
        let Some(criterion) = next
            .success_criteria
            .iter_mut()
            .find(|criterion| criterion.id == evidence.criterion_id)
        else {
            return Err(GoalError::CriterionNotFound(evidence.criterion_id.into()).into());
        };
        criterion.status = GoalCriterionStatus::Verified;
        criterion.evidence_ref = Some(evidence.evidence_ref.trim().into());
        criterion.verifier_id = Some(evidence.verifier_id.trim().into());
        criterion.verifier_version = Some(evidence.verifier_version.trim().into());
        criterion.verified_at_ms = Some(now_ms());
        criterion.provenance = GoalProvenance::Core;
        next.version += 1;
        next.updated_by = command.actor.trim().to_owned();
        next.updated_at_ms = now_ms();
        if next.status == GoalStatus::Active
            && next
                .success_criteria
                .iter()
                .all(|criterion| criterion.status == GoalCriterionStatus::Verified)
        {
            next.status = GoalStatus::Completed;
        }
        self.save(next, "goal.criterion_verified", command)
    }

    pub fn link_reference(
        &self,
        id: &str,
        expected_version: u64,
        kind: &str,
        reference_id: &str,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, StorageError> {
        validate_actor(command.actor)?;
        validate_command(command.idempotency_key, command.command_hash)?;
        if let Some(result) = self.replay(command.idempotency_key, command.command_hash)? {
            return Ok(result);
        }
        validate_id("reference_kind", kind)?;
        validate_id("reference_id", reference_id)?;
        if !matches!(kind, "workflow" | "child" | "checkpoint") {
            return Err(
                invalid_field("reference_kind", "must be workflow, child or checkpoint").into(),
            );
        }
        let current = self.require(id)?;
        check_version(&current, expected_version)?;
        let mut next = current;
        match kind {
            "workflow"
                if !next
                    .workflow_run_ids
                    .iter()
                    .any(|value| value == reference_id) =>
            {
                next.workflow_run_ids.push(reference_id.into())
            }
            "child" if !next.child_run_ids.iter().any(|value| value == reference_id) => {
                next.child_run_ids.push(reference_id.into())
            }
            "checkpoint" => next.checkpoint_id = Some(reference_id.into()),
            _ => {}
        }
        next.version += 1;
        next.updated_by = command.actor.trim().to_owned();
        next.updated_at_ms = now_ms();
        self.save(next, "goal.reference_linked", command)
    }

    /// Startup recovery is read-only. It returns a bounded warning projection
    /// and deliberately never retries a workflow, child or external effect.
    pub fn recovery(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<GoalRecoveryProjection>, StorageError> {
        self.list(workspace_id, GOAL_MAX_READ_LIMIT)?
            .into_iter()
            .map(|goal| {
                let warning = if goal.status == GoalStatus::BudgetLimited {
                    "Продолжение остановлено лимитом бюджета; нужно явное решение пользователя."
                        .into()
                } else if goal.status == GoalStatus::Blocked {
                    "Цель заблокирована; Core не повторяет неизвестные эффекты автоматически."
                        .into()
                } else {
                    String::new()
                };
                Ok(GoalRecoveryProjection {
                    goal_id: goal.id,
                    status: goal.status,
                    warning,
                })
            })
            .collect()
    }

    fn require(&self, id: &str) -> Result<GoalV1, StorageError> {
        self.get(id)?
            .ok_or_else(|| GoalError::NotFound(id.into()).into())
    }

    fn save(
        &self,
        goal: GoalV1,
        action: &str,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, StorageError> {
        let goal = goal.seal()?;
        let canonical = goal.canonical_json()?;
        let transaction = self.connection.unchecked_transaction()?;
        let current_version: i64 = transaction.query_row(
            "SELECT version FROM goals WHERE id = ?1",
            [&goal.id],
            |row| row.get(0),
        )?;
        if current_version != (goal.version - 1) as i64 {
            return Err(StorageError::VersionConflict {
                entity: "goal",
                id: goal.id.clone(),
                expected: (goal.version - 1) as i64,
                current: current_version,
            });
        }
        update_projection(&transaction, &goal, &canonical)?;
        insert_revision(&transaction, &goal, command.actor, &canonical)?;
        let event_sequence = append_change(
            &transaction,
            &goal,
            action,
            command.actor,
            command.idempotency_key,
        )?;
        let result = GoalMutationResult {
            action: action.into(),
            goal,
            applied: true,
            deduplicated: false,
            event_sequence,
        };
        store_command(
            &transaction,
            command.idempotency_key,
            command.command_hash,
            &result,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    fn replay(
        &self,
        idempotency_key: &str,
        command_hash: &str,
    ) -> Result<Option<GoalMutationResult>, StorageError> {
        let stored: Option<(String, Vec<u8>)> = self
            .connection
            .query_row(
                "SELECT command_hash, result_json FROM goal_commands WHERE idempotency_key = ?1",
                [idempotency_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((stored_hash, result_json)) = stored else {
            return Ok(None);
        };
        if stored_hash != command_hash {
            return Err(StorageError::DeduplicationConflict {
                client_id: "goal".into(),
                request_id: idempotency_key.into(),
            });
        }
        let mut result: GoalMutationResult = serde_json::from_slice(&result_json)
            .map_err(|error| GoalError::InvalidStored(error.to_string()))?;
        result.deduplicated = true;
        Ok(Some(result))
    }
}

fn insert_projection(
    transaction: &Transaction<'_>,
    goal: &GoalV1,
    canonical: &[u8],
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO goals
         (id, version, workspace_id, chat_id, objective, status, progress_summary,
          completed_criteria_json, remaining_criteria_json, blockers_json, next_action,
          workflow_run_ids_json, child_run_ids_json, checkpoint_id, token_budget,
          cost_budget_micros, continuation_budget, created_at_ms, updated_at_ms,
          created_by, updated_by, content_hash, canonical_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                 ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
        params![
            goal.id,
            goal.version as i64,
            goal.workspace_id,
            goal.chat_id,
            goal.objective,
            goal.status.as_str(),
            goal.progress_summary,
            serde_json::to_string(&goal.completed_criteria)?,
            serde_json::to_string(&goal.remaining_criteria)?,
            serde_json::to_string(&goal.blockers)?,
            goal.next_action,
            serde_json::to_string(&goal.workflow_run_ids)?,
            serde_json::to_string(&goal.child_run_ids)?,
            goal.checkpoint_id,
            goal.token_budget.map(|value| value as i64),
            goal.cost_budget_micros.map(|value| value as i64),
            goal.continuation_budget.map(|value| value as i64),
            goal.created_at_ms,
            goal.updated_at_ms,
            goal.created_by,
            goal.updated_by,
            goal.content_hash,
            canonical,
        ],
    )?;
    Ok(())
}

fn update_projection(
    transaction: &Transaction<'_>,
    goal: &GoalV1,
    canonical: &[u8],
) -> Result<(), StorageError> {
    transaction.execute(
        "UPDATE goals SET version=?2, workspace_id=?3, chat_id=?4, objective=?5,
         status=?6, progress_summary=?7, completed_criteria_json=?8,
         remaining_criteria_json=?9, blockers_json=?10, next_action=?11,
         workflow_run_ids_json=?12, child_run_ids_json=?13, checkpoint_id=?14,
         token_budget=?15, cost_budget_micros=?16, continuation_budget=?17,
         updated_at_ms=?18, updated_by=?19, content_hash=?20, canonical_json=?21
         WHERE id=?1",
        params![
            goal.id,
            goal.version as i64,
            goal.workspace_id,
            goal.chat_id,
            goal.objective,
            goal.status.as_str(),
            goal.progress_summary,
            serde_json::to_string(&goal.completed_criteria)?,
            serde_json::to_string(&goal.remaining_criteria)?,
            serde_json::to_string(&goal.blockers)?,
            goal.next_action,
            serde_json::to_string(&goal.workflow_run_ids)?,
            serde_json::to_string(&goal.child_run_ids)?,
            goal.checkpoint_id,
            goal.token_budget.map(|value| value as i64),
            goal.cost_budget_micros.map(|value| value as i64),
            goal.continuation_budget.map(|value| value as i64),
            goal.updated_at_ms,
            goal.updated_by,
            goal.content_hash,
            canonical,
        ],
    )?;
    Ok(())
}

fn insert_revision(
    transaction: &Transaction<'_>,
    goal: &GoalV1,
    actor: &str,
    canonical: &[u8],
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO goal_revisions(goal_id, version, content_hash, canonical_json, actor, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![goal.id, goal.version as i64, goal.content_hash, canonical, actor, goal.updated_at_ms],
    )?;
    Ok(())
}

fn append_change(
    transaction: &Transaction<'_>,
    goal: &GoalV1,
    event_type: &str,
    actor: &str,
    idempotency_key: &str,
) -> Result<i64, StorageError> {
    let payload = serde_json::json!({
        "goal_id": goal.id,
        "goal_version": goal.version,
        "status": goal.status.as_str(),
        "event_type": event_type,
        "content_hash": goal.content_hash,
    });
    let payload_json = serde_json::to_vec(&payload)?;
    transaction.execute(
        "INSERT INTO goal_events(goal_id, goal_version, event_type, actor, idempotency_key, payload_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![goal.id, goal.version as i64, event_type, actor, idempotency_key,
            String::from_utf8_lossy(&payload_json), goal.updated_at_ms],
    )?;
    transaction.execute(
        "INSERT INTO events(task_id, event_type, payload) VALUES (?1, ?2, ?3)",
        params![goal.id, event_type, payload_json],
    )?;
    Ok(transaction.last_insert_rowid())
}

fn store_command(
    transaction: &Transaction<'_>,
    idempotency_key: &str,
    command_hash: &str,
    result: &GoalMutationResult,
) -> Result<(), StorageError> {
    transaction.execute(
        "INSERT INTO goal_commands(idempotency_key, command_hash, result_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            idempotency_key,
            command_hash,
            serde_json::to_vec(result)?,
            now_ms()
        ],
    )?;
    Ok(())
}

struct StoredGoalRow {
    id: String,
    version: i64,
    workspace_id: String,
    chat_id: Option<String>,
    objective: String,
    status: String,
    progress_summary: String,
    completed_criteria_json: String,
    remaining_criteria_json: String,
    blockers_json: String,
    next_action: Option<String>,
    workflow_run_ids_json: String,
    child_run_ids_json: String,
    checkpoint_id: Option<String>,
    token_budget: Option<i64>,
    cost_budget_micros: Option<i64>,
    continuation_budget: Option<i64>,
    created_at_ms: i64,
    updated_at_ms: i64,
    created_by: String,
    updated_by: String,
    content_hash: String,
    canonical_json: Vec<u8>,
}

fn read_goal_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredGoalRow> {
    Ok(StoredGoalRow {
        id: row.get(0)?,
        version: row.get(1)?,
        workspace_id: row.get(2)?,
        chat_id: row.get(3)?,
        objective: row.get(4)?,
        status: row.get(5)?,
        progress_summary: row.get(6)?,
        completed_criteria_json: row.get(7)?,
        remaining_criteria_json: row.get(8)?,
        blockers_json: row.get(9)?,
        next_action: row.get(10)?,
        workflow_run_ids_json: row.get(11)?,
        child_run_ids_json: row.get(12)?,
        checkpoint_id: row.get(13)?,
        token_budget: row.get(14)?,
        cost_budget_micros: row.get(15)?,
        continuation_budget: row.get(16)?,
        created_at_ms: row.get(17)?,
        updated_at_ms: row.get(18)?,
        created_by: row.get(19)?,
        updated_by: row.get(20)?,
        content_hash: row.get(21)?,
        canonical_json: row.get(22)?,
    })
}

fn decode_stored(row: StoredGoalRow) -> Result<GoalV1, StorageError> {
    if row.canonical_json.len() > GOAL_MAX_BYTES {
        return Err(GoalError::TooLarge(row.canonical_json.len()).into());
    }
    let goal: GoalV1 = serde_json::from_slice(&row.canonical_json)
        .map_err(|error| GoalError::InvalidStored(error.to_string()))?;
    goal.validate()?;
    let expected_version = u64::try_from(row.version).map_err(|_| {
        GoalError::InvalidStored("SQL version is outside the contract range".into())
    })?;
    let metadata_matches = goal.id == row.id
        && goal.version == expected_version
        && goal.workspace_id == row.workspace_id
        && goal.chat_id == row.chat_id
        && goal.objective == row.objective
        && goal.status.as_str() == row.status
        && goal.progress_summary == row.progress_summary
        && serde_json::to_string(&goal.completed_criteria)? == row.completed_criteria_json
        && serde_json::to_string(&goal.remaining_criteria)? == row.remaining_criteria_json
        && serde_json::to_string(&goal.blockers)? == row.blockers_json
        && goal.next_action == row.next_action
        && serde_json::to_string(&goal.workflow_run_ids)? == row.workflow_run_ids_json
        && serde_json::to_string(&goal.child_run_ids)? == row.child_run_ids_json
        && goal.checkpoint_id == row.checkpoint_id
        && goal
            .token_budget
            .map(i64::try_from)
            .transpose()
            .unwrap_or(None)
            == row.token_budget
        && goal
            .cost_budget_micros
            .map(i64::try_from)
            .transpose()
            .unwrap_or(None)
            == row.cost_budget_micros
        && goal
            .continuation_budget
            .map(i64::try_from)
            .transpose()
            .unwrap_or(None)
            == row.continuation_budget
        && goal.created_at_ms == row.created_at_ms
        && goal.updated_at_ms == row.updated_at_ms
        && goal.created_by == row.created_by
        && goal.updated_by == row.updated_by
        && goal.content_hash == row.content_hash;
    if !metadata_matches {
        return Err(GoalError::InvalidStored(
            "SQL metadata does not match canonical goal JSON".into(),
        )
        .into());
    }
    Ok(goal)
}

fn check_version(goal: &GoalV1, expected: u64) -> Result<(), StorageError> {
    if goal.version != expected {
        return Err(StorageError::VersionConflict {
            entity: "goal",
            id: goal.id.clone(),
            expected: expected as i64,
            current: goal.version as i64,
        });
    }
    Ok(())
}

fn validate_command(idempotency_key: &str, command_hash: &str) -> Result<(), StorageError> {
    if idempotency_key.trim().is_empty() {
        return Err(GoalError::MissingIdempotencyKey.into());
    }
    validate_id("idempotency_key", idempotency_key)?;
    validate_hash(command_hash)?;
    Ok(())
}

fn validate_actor(actor: &str) -> Result<(), StorageError> {
    validate_id("actor", actor).map_err(StorageError::from)
}

fn transition_action(status: GoalStatus) -> &'static str {
    match status {
        GoalStatus::Paused => "goal.paused",
        GoalStatus::Active => "goal.resumed",
        GoalStatus::Blocked => "goal.blocked",
        GoalStatus::BudgetLimited => "goal.budget_limited",
        GoalStatus::Completed => "goal.completed",
        GoalStatus::Failed => "goal.failed",
        GoalStatus::Cancelled => "goal.cancelled",
    }
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_optional_text(
    field: &str,
    value: &Option<String>,
    max: usize,
) -> Result<(), GoalError> {
    if let Some(value) = value {
        validate_text(field, value, max)?;
        ensure_safe_text(field, value)?;
    }
    Ok(())
}

fn validate_list(field: &str, values: &[String], ids: bool) -> Result<(), GoalError> {
    if values.len() > GOAL_MAX_LIST_ITEMS {
        return Err(invalid_field(field, "too many entries"));
    }
    for value in values {
        if ids {
            validate_id(field, value)?;
        } else {
            validate_text(field, value, GOAL_MAX_TEXT_CHARS)?;
            ensure_safe_text(field, value)?;
        }
    }
    Ok(())
}

fn validate_optional_id(field: &str, value: &Option<String>) -> Result<(), GoalError> {
    if let Some(value) = value {
        validate_id(field, value)?;
    }
    Ok(())
}

fn validate_id(field: &str, value: &str) -> Result<(), GoalError> {
    if value.is_empty() || value.chars().count() > GOAL_MAX_ID_CHARS {
        return Err(invalid_field(
            field,
            "must be a bounded non-empty identifier",
        ));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
    }) {
        return Err(invalid_field(field, "contains unsupported characters"));
    }
    Ok(())
}

fn validate_text(field: &str, value: &str, max: usize) -> Result<(), GoalError> {
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(invalid_field(
            field,
            "is empty, oversized or contains control characters",
        ));
    }
    Ok(())
}

fn ensure_safe_text(field: &str, value: &str) -> Result<(), GoalError> {
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
        return Err(GoalError::SensitiveText {
            field: field.into(),
        });
    }
    Ok(())
}

fn validate_hash(value: &str) -> Result<(), GoalError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_field(
            "content_hash",
            "must be a SHA-256 hex digest",
        ));
    }
    Ok(())
}

fn invalid_field(field: &str, reason: impl Into<String>) -> GoalError {
    GoalError::InvalidField {
        field: field.into(),
        reason: reason.into(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal() -> GoalV1 {
        GoalV1 {
            id: "goal-1".into(),
            version: 1,
            workspace_id: "workspace-1".into(),
            chat_id: Some("chat-1".into()),
            objective: "Собрать доказуемый результат".into(),
            success_criteria: vec![GoalCriterionV1::new(
                "criterion-1",
                GoalCriterionKind::Manual,
                "Проверить итог",
            )],
            status: GoalStatus::Active,
            progress_summary: "Ожидает проверки".into(),
            completed_criteria: Vec::new(),
            remaining_criteria: vec!["criterion-1".into()],
            blockers: Vec::new(),
            next_action: Some("Проверить результат".into()),
            workflow_run_ids: Vec::new(),
            child_run_ids: Vec::new(),
            checkpoint_id: None,
            token_budget: Some(100),
            cost_budget_micros: None,
            continuation_budget: Some(2),
            created_at_ms: 1,
            updated_at_ms: 1,
            created_by: "shell".into(),
            updated_by: "shell".into(),
            content_hash: String::new(),
        }
    }

    #[test]
    fn canonical_hash_is_stable_and_model_text_is_not_authority() {
        let sealed = goal().seal().expect("goal seals");
        assert_eq!(sealed.compute_content_hash().unwrap(), sealed.content_hash);
        let mut altered = sealed.clone();
        altered.progress_summary.push_str(" позже");
        assert!(matches!(
            altered.validate(),
            Err(GoalError::ContentHashMismatch { .. })
        ));
        let mut completed = sealed;
        completed.status = GoalStatus::Completed;
        assert!(matches!(
            completed.seal(),
            Err(GoalError::CompletionEvidenceMissing)
        ));
        let mut sensitive = goal();
        sensitive.objective = "доставить token: секрет".into();
        assert!(matches!(
            sensitive.seal(),
            Err(GoalError::SensitiveText { field }) if field == "objective"
        ));
        let mut model_verified = goal();
        model_verified.success_criteria[0].status = GoalCriterionStatus::Verified;
        model_verified.success_criteria[0].evidence_ref = Some("test-run-1".into());
        model_verified.success_criteria[0].verifier_id = Some("model".into());
        model_verified.success_criteria[0].verifier_version = Some("1".into());
        model_verified.success_criteria[0].verified_at_ms = Some(2);
        assert!(matches!(
            model_verified.seal(),
            Err(GoalError::AuthorityViolation { .. })
        ));
    }

    #[test]
    fn transitions_and_evidence_are_core_authoritative() {
        assert!(GoalStatus::Active.allows_transition_to(GoalStatus::Paused));
        assert!(!GoalStatus::Completed.allows_transition_to(GoalStatus::Active));
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let store = GoalStore::new(&connection);
        let created = store
            .create(
                &goal().seal().unwrap(),
                GoalCommand::new("shell", "request-1", &"1".repeat(64)),
            )
            .unwrap();
        let verified = store
            .verify_criterion(
                "goal-1",
                created.goal.version,
                GoalCriterionEvidence::new("criterion-1", "test-run-1", "core:tests", "v1"),
                GoalCommand::new("core", "request-2", &"2".repeat(64)),
            )
            .unwrap();
        assert_eq!(verified.goal.status, GoalStatus::Completed);
        assert_eq!(store.get("goal-1").unwrap().unwrap().version, 2);
    }

    #[test]
    fn storage_is_transactional_idempotent_and_rejects_stale_writes() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let store = GoalStore::new(&connection);
        let first = store
            .create(
                &goal().seal().unwrap(),
                GoalCommand::new("shell", "request-1", &"1".repeat(64)),
            )
            .unwrap();
        let replay = store
            .create(
                &goal().seal().unwrap(),
                GoalCommand::new("shell", "request-1", &"1".repeat(64)),
            )
            .unwrap();
        assert!(replay.deduplicated);
        assert!(matches!(
            store.transition(
                "goal-1",
                first.goal.version - 1,
                GoalStatus::Paused,
                GoalCommand::new("shell", "request-2", &"2".repeat(64))
            ),
            Err(StorageError::VersionConflict { .. })
        ));
        assert!(matches!(
            store.create(
                &goal().seal().unwrap(),
                GoalCommand::new("shell", "request-3", &"3".repeat(64))
            ),
            Err(StorageError::Goal(GoalError::AlreadyExists(_)))
        ));
    }

    #[test]
    fn stored_projection_rejects_sql_metadata_drift() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let store = GoalStore::new(&connection);
        store
            .create(
                &goal().seal().unwrap(),
                GoalCommand::new("shell", "request-1", &"1".repeat(64)),
            )
            .unwrap();
        connection
            .execute(
                "UPDATE goals SET objective = 'tampered' WHERE id = 'goal-1'",
                [],
            )
            .unwrap();
        assert!(matches!(
            store.get("goal-1"),
            Err(StorageError::Goal(GoalError::InvalidStored(_)))
        ));
    }

    #[test]
    fn links_multiple_runtime_refs_and_persists_budget_recovery_state() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let store = GoalStore::new(&connection);
        let created = store
            .create(
                &goal().seal().unwrap(),
                GoalCommand::new("shell", "request-1", &"1".repeat(64)),
            )
            .unwrap();
        let workflow = store
            .link_reference(
                "goal-1",
                created.goal.version,
                "workflow",
                "workflow-1",
                GoalCommand::new("core", "request-2", &"2".repeat(64)),
            )
            .unwrap();
        let child = store
            .link_reference(
                "goal-1",
                workflow.goal.version,
                "child",
                "child-1",
                GoalCommand::new("core", "request-3", &"3".repeat(64)),
            )
            .unwrap();
        let checkpoint = store
            .link_reference(
                "goal-1",
                child.goal.version,
                "checkpoint",
                "checkpoint-1",
                GoalCommand::new("core", "request-4", &"4".repeat(64)),
            )
            .unwrap();
        assert_eq!(checkpoint.goal.workflow_run_ids, vec!["workflow-1"]);
        assert_eq!(checkpoint.goal.child_run_ids, vec!["child-1"]);
        assert_eq!(
            checkpoint.goal.checkpoint_id.as_deref(),
            Some("checkpoint-1")
        );
        let budget_limited = store
            .transition(
                "goal-1",
                checkpoint.goal.version,
                GoalStatus::BudgetLimited,
                GoalCommand::new("core", "request-5", &"5".repeat(64)),
            )
            .unwrap();
        let recovery = store.recovery("workspace-1").unwrap();
        assert_eq!(budget_limited.goal.status, GoalStatus::BudgetLimited);
        assert!(recovery[0].warning.contains("лимитом бюджета"));
        let revisions: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM goal_revisions WHERE goal_id = 'goal-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(revisions, 5);
    }

    #[test]
    fn objective_update_creates_an_immutable_revision() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let store = GoalStore::new(&connection);
        let created = store
            .create(
                &goal().seal().unwrap(),
                GoalCommand::new("shell", "request-1", &"1".repeat(64)),
            )
            .unwrap();
        let updated = store
            .update(
                "goal-1",
                created.goal.version,
                Some("Обновлённая цель".into()),
                None,
                GoalCommand::new("shell", "request-2", &"2".repeat(64)),
            )
            .unwrap();
        assert_eq!(updated.goal.version, 2);
        assert_eq!(updated.goal.objective, "Обновлённая цель");
        let old_objective: String = connection
            .query_row(
                "SELECT json_extract(canonical_json, '$.objective')
                 FROM goal_revisions WHERE goal_id = 'goal-1' AND version = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_objective, "Собрать доказуемый результат");
    }

    #[test]
    fn durable_goal_survives_reopen_without_replaying_an_effect() {
        let path =
            std::env::temp_dir().join(format!("evohime-goal-reopen-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let connection = Connection::open(&path).unwrap();
            install_schema(&connection).unwrap();
            GoalStore::new(&connection)
                .create(
                    &goal().seal().unwrap(),
                    GoalCommand::new("shell", "request-1", &"1".repeat(64)),
                )
                .unwrap();
        }
        {
            let connection = Connection::open(&path).unwrap();
            install_schema(&connection).unwrap();
            let recovered = GoalStore::new(&connection).recovery("workspace-1").unwrap();
            assert_eq!(recovered.len(), 1);
            assert_eq!(recovered[0].status, GoalStatus::Active);
            assert!(recovered[0].warning.is_empty());
            assert_eq!(
                GoalStore::new(&connection)
                    .get("goal-1")
                    .unwrap()
                    .unwrap()
                    .version,
                1
            );
        }
        let _ = std::fs::remove_file(path);
    }
}
