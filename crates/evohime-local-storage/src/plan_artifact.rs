//! Durable, versioned Plan Artifact contract. This store is the only mutable
//! authority for plan acceptance and execution state.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PLAN_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const MAX_ARTIFACT_BYTES: usize = 64 * 1024;
pub const MAX_STEPS: usize = 128;
pub const MAX_TEXT_CHARS: usize = 4096;
pub const MAX_NOTES_CHARS: usize = 64 * 1024;
pub const MAX_CRITERIA: usize = 64;
pub const MAX_REFS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanArtifactStatus {
    Draft,
    Accepted,
    Executing,
    Paused,
    ReplanRequired,
    Completed,
    Failed,
    UnknownOutcome,
}

impl PlanArtifactStatus {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    pub capability_ref: Option<String>,
    pub risk: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCriterion {
    pub id: String,
    pub description: String,
    pub evidence_kind: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanProvenance {
    pub actor: String,
    pub request_id: String,
    pub correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArtifactV1 {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub version: u64,
    pub status: PlanArtifactStatus,
    pub title: String,
    pub objective: String,
    pub steps: Vec<PlanStep>,
    pub assumptions: Vec<String>,
    pub risks: Vec<String>,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub references: Vec<String>,
    pub provenance: PlanProvenance,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanExecutionSnapshot {
    pub artifact_id: String,
    pub revision: u64,
    pub content_hash: String,
    pub policy_snapshot_hash: String,
    pub task_id: Option<String>,
    pub workflow_run_id: Option<String>,
    pub correlation_id: String,
}

pub struct CreateExecutionSnapshot<'a> {
    pub id: &'a str,
    pub expected_version: u64,
    pub policy_snapshot_hash: &'a str,
    pub task_id: Option<&'a str>,
    pub workflow_run_id: Option<&'a str>,
    pub correlation_id: &'a str,
    pub idempotency_key: &'a str,
    pub now_ms: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlanArtifactError {
    #[error("invalid plan artifact: {0}")]
    Invalid(String),
    #[error("unsupported plan artifact schema {0}")]
    UnsupportedVersion(u32),
    #[error("plan artifact exceeds a contract limit: {0}")]
    LimitExceeded(String),
    #[error("plan artifact is stale: expected {expected}, current {current}")]
    Stale { expected: u64, current: u64 },
    #[error("invalid plan artifact transition {from} -> {to}")]
    InvalidTransition { from: String, to: String },
    #[error("idempotency key conflict")]
    IdempotencyConflict,
}

impl PlanArtifactV1 {
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
    pub fn canonical_hash(&self) -> Result<String, PlanArtifactError> {
        let mut copy = self.clone();
        copy.content_hash.clear();
        let bytes =
            serde_json::to_vec(&copy).map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
    pub fn seal(mut self) -> Result<Self, PlanArtifactError> {
        self.validate()?;
        self.content_hash = self.canonical_hash()?;
        Ok(self)
    }
}

pub struct PlanArtifactStore<'a> {
    connection: &'a Connection,
}
impl<'a> PlanArtifactStore<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }
    pub fn install_schema(tx: &Transaction<'_>) -> Result<(), rusqlite::Error> {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS plan_artifact_revisions (artifact_id TEXT NOT NULL, revision INTEGER NOT NULL, version INTEGER NOT NULL, status TEXT NOT NULL, content_hash TEXT NOT NULL, artifact_json BLOB NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(artifact_id, revision), UNIQUE(artifact_id, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_plan_artifact_current ON plan_artifact_revisions(artifact_id, revision DESC); CREATE TABLE IF NOT EXISTS plan_execution_snapshots (artifact_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, policy_snapshot_hash TEXT NOT NULL, task_id TEXT, workflow_run_id TEXT, correlation_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(artifact_id, revision));")
    }
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
    pub fn get(&self, id: &str) -> Result<Option<PlanArtifactV1>, PlanArtifactError> {
        self.connection.query_row("SELECT artifact_json FROM plan_artifact_revisions WHERE artifact_id=?1 ORDER BY revision DESC LIMIT 1",[id],|r|r.get::<_,Vec<u8>>(0)).optional().map_err(|e|PlanArtifactError::Invalid(e.to_string()))?.map(|v|serde_json::from_slice(&v).map_err(|e|PlanArtifactError::Invalid(e.to_string()))).transpose()
    }
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
        let mut next = current.clone();
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
            content_hash: next.content_hash.clone(),
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
mod tests {
    use super::*;
    fn artifact() -> PlanArtifactV1 {
        PlanArtifactV1 {
            schema_version: 1,
            id: "a".into(),
            revision: 1,
            version: 1,
            status: PlanArtifactStatus::Draft,
            title: "T".into(),
            objective: "O".into(),
            steps: vec![PlanStep {
                id: "s".into(),
                description: "do".into(),
                capability_ref: None,
                risk: "low".into(),
            }],
            assumptions: vec![],
            risks: vec![],
            acceptance_criteria: vec![AcceptanceCriterion {
                id: "c".into(),
                description: "pass".into(),
                evidence_kind: "TestsPass".into(),
                required: true,
            }],
            references: vec![],
            provenance: PlanProvenance {
                actor: "core".into(),
                request_id: "r".into(),
                correlation_id: "c".into(),
            },
            content_hash: String::new(),
        }
    }
    #[test]
    fn seal_hash_and_transitions() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE plan_artifact_revisions (artifact_id TEXT,revision INTEGER,version INTEGER,status TEXT,content_hash TEXT,artifact_json BLOB,idempotency_key TEXT,created_at_ms INTEGER,PRIMARY KEY(artifact_id,revision),UNIQUE(artifact_id,idempotency_key))").unwrap();
        let s = PlanArtifactStore::new(&db);
        let a = s.create(&artifact(), "i", 0).unwrap();
        assert!(!a.content_hash.is_empty());
        let b = s
            .transition("a", 1, PlanArtifactStatus::Accepted, "j", 1)
            .unwrap();
        assert_eq!(b.version, 2);
        assert!(matches!(
            s.transition("a", 1, PlanArtifactStatus::Executing, "k", 2),
            Err(PlanArtifactError::Stale { .. })
        ));
    }
}
