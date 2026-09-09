//! Durable metadata authority for diff-bound Code Review Lane records.
//! Large explanations and patches stay in Artifact Handoff; this store keeps
//! only bounded review identity, finding metadata and reconciliation state.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_PATHS: usize = 512;
pub const MAX_FINDINGS: usize = 512;
pub const MAX_EVIDENCE_REFS: usize = 32;
pub const MAX_TEXT_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    LocalWorkspaceChangeSet,
    AgentGitChangeSet,
    TaskWorktreeDiff,
    CommitRange,
    RemotePullRequest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    Open,
    Acknowledged,
    ResolvedByCode,
    Dismissed,
    AcceptedRisk,
    Superseded,
    Stale,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Complete,
    Partial,
    UnsupportedScope,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Clean,
    ChangesRequested,
    ReviewIncomplete,
    NeedsHumanReview,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewTarget {
    pub schema_version: u32,
    pub id: String,
    pub kind: TargetKind,
    pub base_revision: String,
    pub head_revision: String,
    pub diff_hash: String,
    pub workspace_fingerprint: Option<String>,
    pub changed_paths: Vec<String>,
    pub created_at_ms: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewFinding {
    pub id: String,
    pub fingerprint: String,
    pub target_id: String,
    pub category: String,
    pub severity: String,
    pub confidence: String,
    pub file_ref: String,
    pub title: String,
    pub state: FindingState,
    pub evidence_refs: Vec<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewCoverage {
    pub changed_files: u32,
    pub eligible_files: u32,
    pub reviewed_files: u32,
    pub skipped_generated: Vec<String>,
    pub unsupported_files: Vec<String>,
    pub context_failures: Vec<String>,
    pub state: CoverageState,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewRecord {
    pub schema_version: u32,
    pub review_id: String,
    pub revision: u64,
    pub target: CodeReviewTarget,
    pub findings: Vec<CodeReviewFinding>,
    pub coverage: CodeReviewCoverage,
    pub verdict: ReviewVerdict,
    pub interrupted: bool,
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodeReviewError {
    #[error("invalid code review record: {0}")]
    Invalid(String),
    #[error("code review record exceeds a contract limit: {0}")]
    LimitExceeded(String),
    #[error("stale code review revision: expected {expected}, current {current}")]
    Stale { expected: u64, current: u64 },
    #[error("idempotency key conflict")]
    IdempotencyConflict,
}

fn hash<T: Serialize>(value: &T) -> Result<String, CodeReviewError> {
    serde_json::to_vec(value)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|e| CodeReviewError::Invalid(e.to_string()))
}

impl CodeReviewTarget {
    pub fn seal(mut self) -> Result<Self, CodeReviewError> {
        self.content_hash.clear();
        self.content_hash = hash(&self)?;
        self.validate()?;
        Ok(self)
    }
    pub fn validate(&self) -> Result<(), CodeReviewError> {
        if self.schema_version != SCHEMA_VERSION
            || self.id.trim().is_empty()
            || self.base_revision.trim().is_empty()
            || self.head_revision.trim().is_empty()
            || self.diff_hash.is_empty()
            || self.diff_hash.len() > 128
            || self.created_at_ms <= 0
        {
            return Err(CodeReviewError::Invalid("target identity".into()));
        }
        if self.changed_paths.is_empty()
            || self.changed_paths.len() > MAX_PATHS
            || self
                .changed_paths
                .iter()
                .any(|path| path.is_empty() || path.len() > 512)
        {
            return Err(CodeReviewError::LimitExceeded("changed_paths".into()));
        }
        let mut unsigned = self.clone();
        unsigned.content_hash.clear();
        if self.content_hash != hash(&unsigned)? {
            return Err(CodeReviewError::Invalid("target content_hash".into()));
        }
        Ok(())
    }
}

impl CodeReviewFinding {
    pub fn seal(mut self) -> Result<Self, CodeReviewError> {
        self.content_hash.clear();
        self.content_hash = hash(&self)?;
        self.validate()?;
        Ok(self)
    }
    fn validate(&self) -> Result<(), CodeReviewError> {
        if self.id.trim().is_empty()
            || self.fingerprint.trim().is_empty()
            || self.target_id.trim().is_empty()
            || self.category.trim().is_empty()
            || self.severity.trim().is_empty()
            || self.confidence.trim().is_empty()
            || self.file_ref.len() > MAX_TEXT_BYTES
            || self.title.is_empty()
            || self.title.len() > MAX_TEXT_BYTES
            || self
                .evidence_refs
                .iter()
                .any(|reference| reference.is_empty() || reference.len() > MAX_TEXT_BYTES)
        {
            return Err(CodeReviewError::Invalid("finding metadata".into()));
        }
        let mut unsigned = self.clone();
        unsigned.content_hash.clear();
        if self.content_hash != hash(&unsigned)? {
            return Err(CodeReviewError::Invalid("finding content_hash".into()));
        }
        Ok(())
    }
}

impl CodeReviewCoverage {
    pub fn seal(mut self) -> Result<Self, CodeReviewError> {
        self.content_hash.clear();
        self.content_hash = hash(&self)?;
        self.validate()?;
        Ok(self)
    }
    fn validate(&self) -> Result<(), CodeReviewError> {
        if self.reviewed_files > self.eligible_files
            || self.eligible_files > self.changed_files
            || self.skipped_generated.len() > MAX_PATHS
            || self.unsupported_files.len() > MAX_PATHS
            || self.context_failures.len() > MAX_PATHS
        {
            return Err(CodeReviewError::Invalid("coverage bounds".into()));
        }
        if self
            .skipped_generated
            .iter()
            .chain(self.unsupported_files.iter())
            .chain(self.context_failures.iter())
            .any(|path| path.is_empty() || path.len() > 512)
        {
            return Err(CodeReviewError::LimitExceeded("coverage paths".into()));
        }
        let mut unsigned = self.clone();
        unsigned.content_hash.clear();
        if self.content_hash != hash(&unsigned)? {
            return Err(CodeReviewError::Invalid("coverage content_hash".into()));
        }
        Ok(())
    }
}

impl CodeReviewRecord {
    pub fn seal(mut self) -> Result<Self, CodeReviewError> {
        self.target = self.target.seal()?;
        self.findings = self
            .findings
            .into_iter()
            .map(CodeReviewFinding::seal)
            .collect::<Result<_, _>>()?;
        self.coverage = self.coverage.seal()?;
        self.content_hash.clear();
        self.content_hash = hash(&self)?;
        self.validate()?;
        Ok(self)
    }
    pub fn validate(&self) -> Result<(), CodeReviewError> {
        if self.schema_version != SCHEMA_VERSION
            || self.review_id.trim().is_empty()
            || self.revision == 0
        {
            return Err(CodeReviewError::Invalid("review identity".into()));
        }
        self.target.validate()?;
        if self.findings.len() > MAX_FINDINGS
            || self.findings.iter().any(|finding| {
                finding.target_id != self.target.id
                    || finding.evidence_refs.len() > MAX_EVIDENCE_REFS
            })
        {
            return Err(CodeReviewError::LimitExceeded("findings".into()));
        }
        self.findings
            .iter()
            .try_for_each(CodeReviewFinding::validate)?;
        self.coverage.validate()?;
        let mut unsigned = self.clone();
        unsigned.content_hash.clear();
        if self.content_hash != hash(&unsigned)? {
            return Err(CodeReviewError::Invalid("review content_hash".into()));
        }
        Ok(())
    }
}

pub struct CodeReviewStore<'a> {
    connection: &'a Connection,
}
impl<'a> CodeReviewStore<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }
    pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS code_review_revisions (review_id TEXT NOT NULL, revision INTEGER NOT NULL, target_hash TEXT NOT NULL, status TEXT NOT NULL, review_json BLOB NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(review_id, revision), UNIQUE(review_id, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_code_review_current ON code_review_revisions(review_id, revision DESC);")
    }
    pub fn save(
        &self,
        record: &CodeReviewRecord,
        expected_revision: Option<u64>,
        idempotency_key: &str,
        now_ms: i64,
    ) -> Result<CodeReviewRecord, CodeReviewError> {
        let record = record.clone().seal()?;
        if idempotency_key.trim().is_empty() {
            return Err(CodeReviewError::Invalid("idempotency_key".into()));
        }
        if let Some(existing) = self.connection.query_row("SELECT review_json FROM code_review_revisions WHERE review_id=?1 AND idempotency_key=?2", params![record.review_id, idempotency_key], |row| row.get::<_, Vec<u8>>(0)).optional().map_err(|e| CodeReviewError::Invalid(e.to_string()))? { let old: CodeReviewRecord = serde_json::from_slice(&existing).map_err(|e| CodeReviewError::Invalid(e.to_string()))?; if old.content_hash == record.content_hash { return Ok(old); } return Err(CodeReviewError::IdempotencyConflict); }
        let current: Option<u64> = self
            .connection
            .query_row(
                "SELECT MAX(revision) FROM code_review_revisions WHERE review_id=?1",
                params![record.review_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| CodeReviewError::Invalid(e.to_string()))?
            .flatten();
        let expected_next = current.unwrap_or(0).saturating_add(1);
        if record.revision != expected_next {
            return Err(CodeReviewError::Stale {
                expected: expected_next,
                current: current.unwrap_or(0),
            });
        }
        if let Some(expected) = expected_revision {
            let actual = current.unwrap_or(0);
            if expected != actual {
                return Err(CodeReviewError::Stale {
                    expected,
                    current: actual,
                });
            }
        }
        let json =
            serde_json::to_vec(&record).map_err(|e| CodeReviewError::Invalid(e.to_string()))?;
        self.connection.execute("INSERT INTO code_review_revisions(review_id,revision,target_hash,status,review_json,idempotency_key,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![record.review_id, record.revision, record.target.content_hash, format!("{:?}", record.verdict), json, idempotency_key, now_ms]).map_err(|e| CodeReviewError::Invalid(e.to_string()))?;
        Ok(record)
    }
    pub fn load_current(
        &self,
        review_id: &str,
    ) -> Result<Option<CodeReviewRecord>, CodeReviewError> {
        self.connection.query_row("SELECT review_json FROM code_review_revisions WHERE review_id=?1 ORDER BY revision DESC LIMIT 1", params![review_id], |row| row.get::<_, Vec<u8>>(0)).optional().map_err(|e| CodeReviewError::Invalid(e.to_string())).and_then(|value| value.map(|bytes| serde_json::from_slice(&bytes).map_err(|e| CodeReviewError::Invalid(e.to_string()))).transpose())
    }
}

pub fn reconcile(
    previous: &mut CodeReviewRecord,
    current_target: &CodeReviewTarget,
    current_fingerprints: &[String],
) -> Result<(), CodeReviewError> {
    current_target.validate()?;
    if previous.target.id == current_target.id {
        return Err(CodeReviewError::Invalid(
            "re-review target must be a new revision".into(),
        ));
    }
    for finding in &mut previous.findings {
        if finding.state == FindingState::Open
            && !current_fingerprints
                .iter()
                .any(|fingerprint| fingerprint == &finding.fingerprint)
        {
            finding.state = FindingState::Stale;
        }
    }
    previous.interrupted = false;
    *previous = previous.clone().seal()?;
    Ok(())
}
