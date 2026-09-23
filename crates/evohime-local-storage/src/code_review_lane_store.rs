//! Durable metadata authority for diff-bound Code Review Lane records.
//! Large explanations and patches stay in Artifact Handoff; this store keeps
//! only bounded review identity, finding metadata and reconciliation state.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Serialized contract version for persisted code review records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum changed paths in a review target or coverage record.
pub const MAX_PATHS: usize = 512;
/// Maximum findings attached to a review.
pub const MAX_FINDINGS: usize = 512;
/// Maximum evidence references attached to one finding.
pub const MAX_EVIDENCE_REFS: usize = 32;
/// Maximum UTF-8 byte length for bounded review text.
pub const MAX_TEXT_BYTES: usize = 8 * 1024;

/// Source revision range reviewed by the lane.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    /// A locally persisted agent-authored change set.
    LocalWorkspaceChangeSet,
    /// A change set produced through the agent Git workflow.
    AgentGitChangeSet,
    /// A diff from a task worktree.
    TaskWorktreeDiff,
    /// A range between two commits.
    CommitRange,
    /// A remote pull request diff.
    RemotePullRequest,
}

/// Lifecycle state attached to a review finding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    /// Finding has not yet been addressed.
    Open,
    /// A reviewer has acknowledged the finding.
    Acknowledged,
    /// A code change resolved the finding.
    ResolvedByCode,
    /// Finding was dismissed as not actionable.
    Dismissed,
    /// Risk was accepted without changing the code.
    AcceptedRisk,
    /// A newer review superseded this finding.
    Superseded,
    /// Finding no longer applies to the current target.
    Stale,
}

/// Coverage result for the files in a review target.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// All eligible files were reviewed.
    Complete,
    /// Some eligible files could not be reviewed.
    Partial,
    /// The target scope is unsupported.
    UnsupportedScope,
    /// Review execution failed.
    Failed,
    /// Coverage cannot be determined.
    Unknown,
}

/// Overall disposition of a code review.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    /// No actionable changes were found within the recorded coverage.
    Clean,
    /// The review found changes that should be addressed.
    ChangesRequested,
    /// Coverage was insufficient to make a complete assessment.
    ReviewIncomplete,
    /// A human decision is required.
    NeedsHumanReview,
    /// Review could not proceed due to a blocking condition.
    Blocked,
    /// Verdict is not known.
    Unknown,
}

/// Immutable identity and source revisions for a code review target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewTarget {
    /// Schema version for this target record.
    pub schema_version: u32,
    /// Stable target identifier.
    pub id: String,
    /// Kind of source being reviewed.
    pub kind: TargetKind,
    /// Base commit or source revision.
    pub base_revision: String,
    /// Head commit or source revision.
    pub head_revision: String,
    /// Digest of the diff bytes.
    pub diff_hash: String,
    /// Optional fingerprint of the workspace used for review.
    pub workspace_fingerprint: Option<String>,
    /// Changed repository-relative paths in the target.
    pub changed_paths: Vec<String>,
    /// Target creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Digest of the target metadata excluding this field.
    pub content_hash: String,
}

/// Bounded finding metadata and evidence references for a reviewed target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewFinding {
    /// Stable finding identifier.
    pub id: String,
    /// Content-derived identity used to reconcile findings across revisions.
    pub fingerprint: String,
    /// Review target identifier.
    pub target_id: String,
    /// Finding category, such as correctness or security.
    pub category: String,
    /// Severity label assigned by the reviewer.
    pub severity: String,
    /// Confidence label assigned by the reviewer.
    pub confidence: String,
    /// Repository-relative file reference.
    pub file_ref: String,
    /// Short finding title.
    pub title: String,
    /// Current disposition of the finding.
    pub state: FindingState,
    /// Artifact or evidence identifiers supporting the finding.
    pub evidence_refs: Vec<String>,
    /// Digest of the finding metadata excluding this field.
    pub content_hash: String,
}

/// File counts and explicit reasons for review coverage gaps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewCoverage {
    /// Number of changed files in the target.
    pub changed_files: u32,
    /// Number of files eligible for automated review.
    pub eligible_files: u32,
    /// Number of eligible files actually reviewed.
    pub reviewed_files: u32,
    /// Generated files intentionally excluded from review.
    pub skipped_generated: Vec<String>,
    /// Files whose format or language was unsupported.
    pub unsupported_files: Vec<String>,
    /// Files omitted because required context could not be obtained.
    pub context_failures: Vec<String>,
    /// Aggregate state of review coverage.
    pub state: CoverageState,
    /// Digest of coverage metadata excluding this field.
    pub content_hash: String,
}

/// Complete versioned review result tied to one immutable target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewRecord {
    /// Schema version for the review record.
    pub schema_version: u32,
    /// Stable identifier for the logical review.
    pub review_id: String,
    /// Monotonically increasing review revision.
    pub revision: u64,
    /// Source target analyzed in this revision.
    pub target: CodeReviewTarget,
    /// Findings produced for the target.
    pub findings: Vec<CodeReviewFinding>,
    /// Coverage evidence for the review.
    pub coverage: CodeReviewCoverage,
    /// Overall review disposition.
    pub verdict: ReviewVerdict,
    /// Whether the previous review execution was interrupted.
    pub interrupted: bool,
    /// Digest of the review record excluding this field.
    pub content_hash: String,
}

/// Validation, size-bound, concurrency, and idempotency failures for review persistence.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CodeReviewError {
    /// Record content or identity is invalid.
    #[error("invalid code review record: {0}")]
    Invalid(String),
    /// Record exceeded a collection or text bound.
    #[error("code review record exceeds a contract limit: {0}")]
    LimitExceeded(String),
    /// The requested revision does not match the current revision.
    #[error("stale code review revision: expected {expected}, current {current}")]
    Stale {
        /// Revision expected by the caller.
        expected: u64,
        /// Current stored revision.
        current: u64,
    },
    /// An idempotency key was reused with different review content.
    #[error("idempotency key conflict")]
    IdempotencyConflict,
}

fn hash<T: Serialize>(value: &T) -> Result<String, CodeReviewError> {
    serde_json::to_vec(value)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|e| CodeReviewError::Invalid(e.to_string()))
}

impl CodeReviewTarget {
    /// Computes the target content hash, then validates the sealed target.
    pub fn seal(mut self) -> Result<Self, CodeReviewError> {
        self.content_hash.clear();
        self.content_hash = hash(&self)?;
        self.validate()?;
        Ok(self)
    }
    /// Validates target identity, changed paths, and its canonical content hash.
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
    /// Computes the finding content hash, then validates its metadata.
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
    /// Computes the coverage content hash, then validates counts, paths, and digest.
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
    /// Seals nested records and computes the final review digest.
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
    /// Validates identity, target linkage, collection bounds, and nested digests.
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

/// SQLite repository that stores immutable revisions of code review records.
pub struct CodeReviewStore<'a> {
    connection: &'a Connection,
}
impl<'a> CodeReviewStore<'a> {
    /// Creates a store borrowing the caller-owned SQLite connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }
    /// Creates the code review revision table and current-revision index in `tx`.
    pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS code_review_revisions (review_id TEXT NOT NULL, revision INTEGER NOT NULL, target_hash TEXT NOT NULL, status TEXT NOT NULL, review_json BLOB NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(review_id, revision), UNIQUE(review_id, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_code_review_current ON code_review_revisions(review_id, revision DESC);")
    }
    /// Seals and inserts the next review revision using optimistic concurrency and idempotency.
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
    /// Loads the latest persisted review revision for an identifier.
    pub fn load_current(
        &self,
        review_id: &str,
    ) -> Result<Option<CodeReviewRecord>, CodeReviewError> {
        self.connection.query_row("SELECT review_json FROM code_review_revisions WHERE review_id=?1 ORDER BY revision DESC LIMIT 1", params![review_id], |row| row.get::<_, Vec<u8>>(0)).optional().map_err(|e| CodeReviewError::Invalid(e.to_string())).and_then(|value| value.map(|bytes| serde_json::from_slice(&bytes).map_err(|e| CodeReviewError::Invalid(e.to_string()))).transpose())
    }
}

/// Marks findings absent from a new target's fingerprints as stale and reseals the prior review.
///
/// The target must be a new revision with a different target identifier.
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
