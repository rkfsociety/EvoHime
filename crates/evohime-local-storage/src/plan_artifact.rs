//! Durable, versioned Plan Artifact contract. This store is the only mutable
//! authority for plan acceptance and execution state.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema version encoded in every supported plan artifact.
pub const PLAN_ARTIFACT_SCHEMA_VERSION: u32 = 1;
/// Maximum serialized size of a plan artifact in bytes.
pub const MAX_ARTIFACT_BYTES: usize = 64 * 1024;
/// Maximum number of steps allowed in one plan.
pub const MAX_STEPS: usize = 128;
/// Maximum character count for individual title, objective, step, and provenance text.
pub const MAX_TEXT_CHARS: usize = 4096;
/// Maximum combined UTF-8 byte length for assumptions and risks.
pub const MAX_NOTES_CHARS: usize = 64 * 1024;
/// Maximum number of acceptance criteria in one plan.
pub const MAX_CRITERIA: usize = 64;
/// Maximum number of references in one plan.
pub const MAX_REFS: usize = 32;

/// Lifecycle states accepted by the plan artifact store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanArtifactStatus {
    /// Plan is editable and has not been accepted for execution.
    Draft,
    /// Plan was approved for execution.
    Accepted,
    /// Execution has started from an accepted plan.
    Executing,
    /// Execution is paused and may be resumed.
    Paused,
    /// Current plan cannot continue without a replacement or revision.
    ReplanRequired,
    /// All planned work is complete.
    Completed,
    /// Execution ended with a known failure.
    Failed,
    /// The final execution outcome cannot be determined safely.
    UnknownOutcome,
}

impl PlanArtifactStatus {
    /// Returns the stable snake-case value persisted in SQLite.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Accepted => "accepted",
            Self::Executing => "executing",
            Self::Paused => "paused",
            Self::ReplanRequired => "replan_required",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::UnknownOutcome => "unknown_outcome",
        }
    }
    /// Parses a persisted status value, returning `None` for unknown values.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "draft" => Self::Draft,
            "accepted" => Self::Accepted,
            "executing" => Self::Executing,
            "paused" => Self::Paused,
            "replan_required" => Self::ReplanRequired,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "unknown_outcome" => Self::UnknownOutcome,
            _ => return None,
        })
    }
}

/// One ordered unit of work in a plan artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    /// Stable step identifier within the plan.
    pub id: String,
    /// Human-readable work description.
    pub description: String,
    /// Optional capability identifier expected to perform the step.
    pub capability_ref: Option<String>,
    /// Risk classification recorded for this step.
    pub risk: String,
}

/// A condition and evidence type used to decide whether a plan is complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCriterion {
    /// Stable criterion identifier within the plan.
    pub id: String,
    /// Observable condition that determines whether the plan is accepted as complete.
    pub description: String,
    /// Kind of evidence required to evaluate the criterion.
    pub evidence_kind: String,
    /// Whether failure to satisfy this criterion blocks completion.
    pub required: bool,
}

/// Identifies the actor and request that produced a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanProvenance {
    /// Identity of the actor that authored or submitted the plan.
    pub actor: String,
    /// Identifier of the originating request.
    pub request_id: String,
    /// Correlation identifier used to connect related operations.
    pub correlation_id: String,
}

/// Versioned, validated plan content persisted as immutable revisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArtifactV1 {
    /// Serialization schema version; currently [`PLAN_ARTIFACT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable plan identifier.
    pub id: String,
    /// Monotonically increasing content revision.
    pub revision: u64,
    /// Optimistic concurrency version used by state transitions.
    pub version: u64,
    /// Current lifecycle status.
    pub status: PlanArtifactStatus,
    /// Short human-readable plan title.
    pub title: String,
    /// Intended outcome of the plan.
    pub objective: String,
    /// Ordered work items; must contain between one and [`MAX_STEPS`] entries.
    pub steps: Vec<PlanStep>,
    /// Assumptions used when preparing the plan.
    pub assumptions: Vec<String>,
    /// Known risks associated with the plan.
    pub risks: Vec<String>,
    /// Conditions and evidence used to assess completion.
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    /// References supporting or constraining the plan.
    pub references: Vec<String>,
    /// Origin information for the plan.
    pub provenance: PlanProvenance,
    /// SHA-256 digest of the canonical artifact with this field cleared.
    pub content_hash: String,
}

/// Immutable plan and policy references captured when execution begins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanExecutionSnapshot {
    /// Plan whose accepted revision is being executed.
    pub artifact_id: String,
    /// Immutable plan revision selected for execution.
    pub revision: u64,
    /// Content digest of the selected plan revision.
    pub content_hash: String,
    /// Digest of the policy state captured at execution start.
    pub policy_snapshot_hash: String,
    /// Optional task identifier associated with execution.
    pub task_id: Option<String>,
    /// Optional workflow run identifier associated with execution.
    pub workflow_run_id: Option<String>,
    /// Correlation identifier carried into execution.
    pub correlation_id: String,
}

/// Inputs required to bind an accepted plan to an execution context.
pub struct CreateExecutionSnapshot<'a> {
    /// Identifier of the accepted plan.
    pub id: &'a str,
    /// Expected current version for optimistic concurrency.
    pub expected_version: u64,
    /// Digest of the effective policy snapshot.
    pub policy_snapshot_hash: &'a str,
    /// Optional task identifier to bind to execution.
    pub task_id: Option<&'a str>,
    /// Optional workflow run identifier to bind to execution.
    pub workflow_run_id: Option<&'a str>,
    /// Correlation identifier to persist with the snapshot.
    pub correlation_id: &'a str,
    /// Idempotency key for the state transition.
    pub idempotency_key: &'a str,
    /// Snapshot creation time in Unix milliseconds.
    pub now_ms: i64,
}

/// Validation, concurrency, and lifecycle failures from the plan artifact contract.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlanArtifactError {
    /// Artifact content or a request field is invalid.
    #[error("invalid plan artifact: {0}")]
    Invalid(String),
    /// Artifact uses a schema version this implementation cannot read.
    #[error("unsupported plan artifact schema {0}")]
    UnsupportedVersion(u32),
    /// Artifact exceeded a documented size or collection bound.
    #[error("plan artifact exceeds a contract limit: {0}")]
    LimitExceeded(String),
    /// Optimistic concurrency check found a newer version.
    #[error("plan artifact is stale: expected {expected}, current {current}")]
    Stale {
        /// Version requested by the caller.
        expected: u64,
        /// Version currently stored.
        current: u64,
    },
    /// Requested lifecycle transition is not permitted.
    #[error("invalid plan artifact transition {from} -> {to}")]
    InvalidTransition {
        /// Current lifecycle state.
        from: String,
        /// Requested lifecycle state.
        to: String,
    },
    /// An idempotency key was reused for different artifact content.
    #[error("idempotency key conflict")]
    IdempotencyConflict,
}

impl PlanArtifactV1 {
    /// Checks schema, required text, collection bounds, and serialized size.
    ///
    /// # Examples
    ///
    /// ```
    /// use evohime_local_storage::plan_artifact::{
    ///     AcceptanceCriterion, PlanArtifactStatus, PlanArtifactV1, PlanProvenance, PlanStep,
    ///     PLAN_ARTIFACT_SCHEMA_VERSION,
    /// };
    ///
    /// let plan = PlanArtifactV1 {
    ///     schema_version: PLAN_ARTIFACT_SCHEMA_VERSION,
    ///     id: "plan-1".into(),
    ///     revision: 1,
    ///     version: 1,
    ///     status: PlanArtifactStatus::Draft,
    ///     title: "Document the API".into(),
    ///     objective: "Make the public contract understandable".into(),
    ///     steps: vec![PlanStep {
    ///         id: "step-1".into(),
    ///         description: "Document exported types".into(),
    ///         capability_ref: None,
    ///         risk: "low".into(),
    ///     }],
    ///     assumptions: vec![],
    ///     risks: vec![],
    ///     acceptance_criteria: vec![AcceptanceCriterion {
    ///         id: "criterion-1".into(),
    ///         description: "The rustdoc build succeeds".into(),
    ///         evidence_kind: "build".into(),
    ///         required: true,
    ///     }],
    ///     references: vec![],
    ///     provenance: PlanProvenance {
    ///         actor: "author".into(),
    ///         request_id: "request-1".into(),
    ///         correlation_id: "trace-1".into(),
    ///     },
    ///     content_hash: String::new(),
    /// };
    /// let sealed = plan.seal().unwrap();
    /// assert!(!sealed.content_hash.is_empty());
    /// ```
    pub fn validate(&self) -> Result<(), PlanArtifactError> {
        if self.schema_version != PLAN_ARTIFACT_SCHEMA_VERSION {
            return Err(PlanArtifactError::UnsupportedVersion(self.schema_version));
        }
        for (name, value) in [
            ("id", &self.id),
            ("title", &self.title),
            ("objective", &self.objective),
        ] {
            if value.trim().is_empty() {
                return Err(PlanArtifactError::Invalid(format!("{name} is empty")));
            }
        }
        if self.revision == 0 || self.version == 0 {
            return Err(PlanArtifactError::Invalid(
                "revision/version must be positive".into(),
            ));
        }
        if self.steps.is_empty() || self.steps.len() > MAX_STEPS {
            return Err(PlanArtifactError::LimitExceeded("steps".into()));
        }
        if self.acceptance_criteria.is_empty() || self.acceptance_criteria.len() > MAX_CRITERIA {
            return Err(PlanArtifactError::LimitExceeded(
                "acceptance_criteria".into(),
            ));
        }
        if self.references.len() > MAX_REFS {
            return Err(PlanArtifactError::LimitExceeded("references".into()));
        }
        let notes: usize = self
            .assumptions
            .iter()
            .chain(self.risks.iter())
            .map(|v| v.len())
            .sum();
        if notes > MAX_NOTES_CHARS {
            return Err(PlanArtifactError::LimitExceeded(
                "assumptions_and_risks".into(),
            ));
        }
        for value in [
            &self.title,
            &self.objective,
            &self.provenance.actor,
            &self.provenance.request_id,
            &self.provenance.correlation_id,
        ] {
            if value.chars().count() > MAX_TEXT_CHARS {
                return Err(PlanArtifactError::LimitExceeded("text".into()));
            }
        }
        for step in &self.steps {
            if step.id.trim().is_empty()
                || step.description.trim().is_empty()
                || step.description.chars().count() > MAX_TEXT_CHARS
            {
                return Err(PlanArtifactError::Invalid("invalid step".into()));
            }
        }
        let bytes =
            serde_json::to_vec(self).map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        if bytes.len() > MAX_ARTIFACT_BYTES {
            return Err(PlanArtifactError::LimitExceeded("canonical_bytes".into()));
        }
        Ok(())
    }
    /// Computes the canonical SHA-256 digest with `content_hash` excluded.
    pub fn canonical_hash(&self) -> Result<String, PlanArtifactError> {
        let mut copy = self.clone();
        copy.content_hash.clear();
        let bytes =
            serde_json::to_vec(&copy).map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
    /// Validates the artifact and returns it with its canonical content digest populated.
    pub fn seal(mut self) -> Result<Self, PlanArtifactError> {
        self.validate()?;
        self.content_hash = self.canonical_hash()?;
        Ok(self)
    }
}

/// SQLite access layer for immutable plan revisions and execution snapshots.
pub struct PlanArtifactStore<'a> {
    connection: &'a Connection,
}
impl<'a> PlanArtifactStore<'a> {
    /// Creates a store borrowing the SQLite connection used for all operations.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }
    /// Creates the revision and execution snapshot tables and indexes in `tx`.
    pub fn install_schema(tx: &Transaction<'_>) -> Result<(), rusqlite::Error> {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS plan_artifact_revisions (artifact_id TEXT NOT NULL, revision INTEGER NOT NULL, version INTEGER NOT NULL, status TEXT NOT NULL, content_hash TEXT NOT NULL, artifact_json BLOB NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(artifact_id, revision), UNIQUE(artifact_id, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_plan_artifact_current ON plan_artifact_revisions(artifact_id, revision DESC); CREATE TABLE IF NOT EXISTS plan_execution_snapshots (artifact_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, policy_snapshot_hash TEXT NOT NULL, task_id TEXT, workflow_run_id TEXT, correlation_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(artifact_id, revision));")
    }
    /// Validates and inserts the initial artifact revision.
    ///
    /// Repeating the same idempotency key with identical content returns the existing artifact;
    /// using that key for different content returns [`PlanArtifactError::IdempotencyConflict`].
    pub fn create(
        &self,
        artifact: &PlanArtifactV1,
        idempotency_key: &str,
        now_ms: i64,
    ) -> Result<PlanArtifactV1, PlanArtifactError> {
        let artifact = artifact.clone().seal()?;
        if idempotency_key.trim().is_empty() {
            return Err(PlanArtifactError::Invalid(
                "idempotency_key is empty".into(),
            ));
        }
        let json =
            serde_json::to_vec(&artifact).map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        if let Some(existing) = self
            .connection
            .query_row(
                "SELECT artifact_json FROM plan_artifact_revisions WHERE artifact_id=?1 AND idempotency_key=?2",
                params![artifact.id, idempotency_key],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|e| PlanArtifactError::Invalid(e.to_string()))?
        {
            let old: PlanArtifactV1 = serde_json::from_slice(&existing)
                .map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
            if old.content_hash == artifact.content_hash {
                return Ok(old);
            }
            return Err(PlanArtifactError::IdempotencyConflict);
        }
        self.connection
            .execute(
                "INSERT INTO plan_artifact_revisions VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    artifact.id,
                    artifact.revision,
                    artifact.version,
                    artifact.status.as_str(),
                    artifact.content_hash,
                    json,
                    idempotency_key,
                    now_ms
                ],
            )
            .map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        Ok(artifact)
    }
    /// Loads the latest revision for an artifact identifier, if present.
    pub fn get(&self, id: &str) -> Result<Option<PlanArtifactV1>, PlanArtifactError> {
        self.connection.query_row("SELECT artifact_json FROM plan_artifact_revisions WHERE artifact_id=?1 ORDER BY revision DESC LIMIT 1",[id],|r|r.get::<_,Vec<u8>>(0)).optional().map_err(|e|PlanArtifactError::Invalid(e.to_string()))?.map(|v|serde_json::from_slice(&v).map_err(|e|PlanArtifactError::Invalid(e.to_string()))).transpose()
    }
    /// Appends a new revision after checking the expected version and allowed status transition.
    ///
    /// Returns [`PlanArtifactError::Stale`] when another writer has already advanced the plan.
    pub fn transition(
        &self,
        id: &str,
        expected_version: u64,
        status: PlanArtifactStatus,
        idempotency_key: &str,
        now_ms: i64,
    ) -> Result<PlanArtifactV1, PlanArtifactError> {
        let current = self
            .get(id)?
            .ok_or_else(|| PlanArtifactError::Invalid("artifact not found".into()))?;
        if current.version != expected_version {
            return Err(PlanArtifactError::Stale {
                expected: expected_version,
                current: current.version,
            });
        }
        if !allowed(current.status, status) {
            return Err(PlanArtifactError::InvalidTransition {
                from: current.status.as_str().into(),
                to: status.as_str().into(),
            });
        }
        let mut next = current;
        next.version += 1;
        next.status = status;
        next.revision += 1;
        next = next.seal()?;
        let json =
            serde_json::to_vec(&next).map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        self.connection
            .execute(
                "INSERT INTO plan_artifact_revisions VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    next.id,
                    next.revision,
                    next.version,
                    next.status.as_str(),
                    next.content_hash,
                    json,
                    idempotency_key,
                    now_ms
                ],
            )
            .map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        Ok(next)
    }

    /// Records an execution snapshot and advances an accepted plan to `Executing`.
    ///
    /// The expected version must match, the current status must be `Accepted`, and snapshot
    /// provenance fields must be non-empty.
    pub fn create_execution_snapshot(
        &self,
        request: CreateExecutionSnapshot<'_>,
    ) -> Result<PlanExecutionSnapshot, PlanArtifactError> {
        let CreateExecutionSnapshot {
            id,
            expected_version,
            policy_snapshot_hash,
            task_id,
            workflow_run_id,
            correlation_id,
            idempotency_key,
            now_ms,
        } = request;
        let current = self
            .get(id)?
            .ok_or_else(|| PlanArtifactError::Invalid("artifact not found".into()))?;
        if current.version != expected_version {
            return Err(PlanArtifactError::Stale {
                expected: expected_version,
                current: current.version,
            });
        }
        if current.status != PlanArtifactStatus::Accepted {
            return Err(PlanArtifactError::InvalidTransition {
                from: current.status.as_str().into(),
                to: PlanArtifactStatus::Executing.as_str().into(),
            });
        }
        if policy_snapshot_hash.trim().is_empty() || correlation_id.trim().is_empty() {
            return Err(PlanArtifactError::Invalid(
                "snapshot provenance is empty".into(),
            ));
        }
        let next = self.transition(
            id,
            expected_version,
            PlanArtifactStatus::Executing,
            idempotency_key,
            now_ms,
        )?;
        let snapshot = PlanExecutionSnapshot {
            artifact_id: next.id.clone(),
            revision: next.revision,
            content_hash: next.content_hash,
            policy_snapshot_hash: policy_snapshot_hash.to_owned(),
            task_id: task_id.map(str::to_owned),
            workflow_run_id: workflow_run_id.map(str::to_owned),
            correlation_id: correlation_id.to_owned(),
        };
        self.connection
            .execute(
                "INSERT INTO plan_execution_snapshots VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    snapshot.artifact_id,
                    snapshot.revision,
                    snapshot.content_hash,
                    snapshot.policy_snapshot_hash,
                    snapshot.task_id,
                    snapshot.workflow_run_id,
                    snapshot.correlation_id,
                    now_ms
                ],
            )
            .map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        Ok(snapshot)
    }
}
fn allowed(from: PlanArtifactStatus, to: PlanArtifactStatus) -> bool {
    matches!(
        (from, to),
        (PlanArtifactStatus::Draft, PlanArtifactStatus::Accepted)
            | (PlanArtifactStatus::Accepted, PlanArtifactStatus::Executing)
            | (
                PlanArtifactStatus::Executing,
                PlanArtifactStatus::Paused
                    | PlanArtifactStatus::ReplanRequired
                    | PlanArtifactStatus::Completed
                    | PlanArtifactStatus::Failed
                    | PlanArtifactStatus::UnknownOutcome
            )
            | (
                PlanArtifactStatus::Paused,
                PlanArtifactStatus::Executing | PlanArtifactStatus::ReplanRequired
            )
            | (
                PlanArtifactStatus::ReplanRequired,
                PlanArtifactStatus::Draft
            )
    )
}

#[cfg(test)]
#[path = "plan_artifact_tests.rs"]
mod tests;
