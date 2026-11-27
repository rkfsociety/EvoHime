//! Shared grounded-research DTOs, redaction, and digest helpers used by
//! `research_pipeline`, `research_fetch`, `research_search`, and `research_gate`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Maximum source URL length in Unicode scalar values.
pub const MAX_URL_CHARS: usize = 2_048;
/// Maximum evidence title length in Unicode scalar values.
pub const MAX_TITLE_CHARS: usize = 256;
/// Maximum publisher name length in Unicode scalar values.
pub const MAX_PUBLISHER_CHARS: usize = 256;
/// Maximum content type length in Unicode scalar values.
pub const MAX_CONTENT_TYPE_CHARS: usize = 128;
/// Maximum redacted excerpt length in Unicode scalar values.
pub const MAX_EXCERPT_CHARS: usize = 4_096;
/// Maximum evidence lifetime, in milliseconds.
pub const MAX_TTL_MS: u64 = 31 * 24 * 60 * 60 * 1_000;
/// Maximum source identifier length in Unicode scalar values.
pub const MAX_SOURCE_ID_CHARS: usize = 128;
/// Maximum revision identifier length in Unicode scalar values.
pub const MAX_REVISION_ID_CHARS: usize = 128;
/// Maximum locator string length in Unicode scalar values.
pub const MAX_LOCATOR_CHARS: usize = 1_024;
/// Maximum JSON snapshot metadata length in Unicode scalar values.
pub const MAX_JSON_METADATA_CHARS: usize = 32 * 1024;

/// Validation failures for the offline research evidence contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// A required field is empty.
    EmptyField(&'static str),
    /// A field exceeds its maximum length.
    FieldTooLong {
        /// Name of the field that exceeded its limit.
        field: &'static str,
        /// Maximum permitted length.
        max: usize,
    },
    /// A required Unix timestamp is zero or invalid.
    InvalidTimestamp,
    /// Evidence lifetime is outside the supported range.
    InvalidTtl,
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::InvalidTimestamp => write!(f, "timestamp must be a positive Unix timestamp"),
            Self::InvalidTtl => write!(f, "ttl must be between 1 ms and {MAX_TTL_MS} ms"),
        }
    }
}

impl std::error::Error for ContractError {}

/// Bounded, non-secret metadata identifying the origin of evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceMetadata {
    /// URL identifying the evidence source.
    pub url: String,
    /// Human-readable source title.
    pub title: String,
    /// Publisher or organization associated with the source.
    pub publisher: String,
    /// Source content type, such as a MIME type.
    pub content_type: String,
    /// Retrieval time as a positive Unix timestamp in milliseconds.
    pub retrieved_at_ms: u64,
}

impl SourceMetadata {
    /// Creates and validates bounded, non-secret source metadata.
    pub fn new(
        url: impl Into<String>,
        title: impl Into<String>,
        publisher: impl Into<String>,
        content_type: impl Into<String>,
        retrieved_at_ms: u64,
    ) -> Result<Self, ContractError> {
        let value = Self {
            url: url.into(),
            title: title.into(),
            publisher: publisher.into(),
            content_type: content_type.into(),
            retrieved_at_ms,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ContractError> {
        validate_text("url", &self.url, MAX_URL_CHARS, true)?;
        validate_text("title", &self.title, MAX_TITLE_CHARS, true)?;
        validate_text("publisher", &self.publisher, MAX_PUBLISHER_CHARS, true)?;
        validate_text(
            "content_type",
            &self.content_type,
            MAX_CONTENT_TYPE_CHARS,
            true,
        )?;
        if self.retrieved_at_ms == 0 {
            return Err(ContractError::InvalidTimestamp);
        }
        Ok(())
    }
}

/// Redacted, bounded research evidence with freshness bounds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchEvidence {
    /// Validated origin metadata for the captured evidence.
    pub source: SourceMetadata,
    /// Bounded excerpt after common credential forms have been redacted.
    pub excerpt: String,
    /// SHA-256 digest of the exact redacted UTF-8 excerpt bytes.
    pub excerpt_sha256: String,
    /// Capture time as a positive Unix timestamp in milliseconds.
    pub captured_at_ms: u64,
    /// Evidence lifetime in milliseconds, bounded by [`MAX_TTL_MS`].
    pub ttl_ms: u64,
}

impl ResearchEvidence {
    /// Redacts, bounds, and captures an excerpt with a finite lifetime.
    pub fn capture(
        source: SourceMetadata,
        excerpt: impl AsRef<str>,
        captured_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<Self, ContractError> {
        if captured_at_ms == 0 {
            return Err(ContractError::InvalidTimestamp);
        }
        if !(1..=MAX_TTL_MS).contains(&ttl_ms) {
            return Err(ContractError::InvalidTtl);
        }
        let excerpt = redact_excerpt(excerpt.as_ref())?;
        let excerpt_sha256 = sha256_hex(excerpt.as_bytes());
        Ok(Self {
            source,
            excerpt,
            excerpt_sha256,
            captured_at_ms,
            ttl_ms,
        })
    }

    /// Returns the expiry time in Unix milliseconds, or `None` on integer overflow.
    pub fn expires_at_ms(&self) -> Option<u64> {
        self.captured_at_ms.checked_add(self.ttl_ms)
    }

    /// Returns whether `now_ms` falls within the evidence freshness interval.
    pub fn is_fresh_at(&self, now_ms: u64) -> bool {
        now_ms >= self.captured_at_ms && now_ms < self.expires_at_ms().unwrap_or(u64::MAX)
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), ContractError> {
    if required && value.trim().is_empty() {
        return Err(ContractError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(ContractError::FieldTooLong { field, max });
    }
    Ok(())
}

/// Redacts common bearer/API-token forms and bounds the result by Unicode
/// scalar values, preserving valid UTF-8 and making the transformation stable.
pub fn redact_excerpt(input: &str) -> Result<String, ContractError> {
    if input.chars().count() > MAX_EXCERPT_CHARS {
        return Err(ContractError::FieldTooLong {
            field: "excerpt",
            max: MAX_EXCERPT_CHARS,
        });
    }
    let mut output = Vec::new();
    for token in input.split_inclusive(char::is_whitespace) {
        let trimmed = token.trim_end_matches(char::is_whitespace);
        let suffix = &token[trimmed.len()..];
        let redacted = if is_secret_token(trimmed) {
            "[REDACTED]"
        } else {
            trimmed
        };
        output.push(redacted);
        output.push(suffix);
    }
    Ok(output.concat())
}

fn is_secret_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("aiza")
        || (token.contains('@') && token.split('@').next().is_some_and(|part| !part.is_empty()))
}

/// Computes lowercase hex-encoded SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

/// Origin category assigned to an acquired research source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSourceKind {
    /// File already present inside the selected workspace.
    WorkspaceFile,
    /// Explicitly selected range within a workspace file.
    WorkspaceSelection,
    /// User-uploaded file.
    UploadedFile,
    /// Manually supplied plain text.
    PlainText,
    /// Markdown document.
    Markdown,
    /// PDF document.
    Pdf,
    /// Acquired web page.
    WebPage,
    /// File acquired from GitHub.
    GitHubFile,
    /// Snapshot acquired from a GitHub repository.
    GitHubRepositorySnapshot,
    /// Artifact already stored by the project.
    ProjectArtifact,
    /// User-authored note.
    ManualNote,
    /// Document acquired through an external connector.
    ExternalConnectorDocument,
}

/// Acquisition/processing lifecycle status of a source record.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSourceStatus {
    /// Source record exists but acquisition has not started.
    Pending,
    /// Source bytes are being acquired.
    Acquiring,
    /// Acquired content is being parsed.
    Parsing,
    /// Parsed content is being divided into evidence units.
    Extracting,
    /// Extracted content is being added to the local index.
    Indexing,
    /// Source is ready for grounded research.
    Ready,
    /// Some content is ready but part of processing failed or was omitted.
    PartiallyReady,
    /// Acquisition or processing failed.
    Failed,
    /// Source revision is no longer current.
    Stale,
    /// Source cannot currently be accessed.
    Unavailable,
    /// Source is being removed from the collection.
    Removing,
}

/// Trust classification attached to research evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTrust {
    /// Evidence originated from the current workspace.
    Workspace,
    /// Evidence was directly supplied by the user.
    UserProvided,
    /// Evidence was acquired from an external source.
    AcquiredExternal,
    /// Evidence was derived from other recorded content.
    Derived,
    /// Evidence origin or integrity has not been verified.
    Unverified,
}

/// Locator scheme used to identify the source location of evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLocatorKind {
    /// File line range.
    FileRange,
    /// Page and section location.
    PageSection,
    /// Paragraph index.
    Paragraph,
    /// Range within parsed HTML content.
    HtmlRange,
    /// Block within a stored project artifact.
    ArtifactBlock,
}

/// Bounded source location for one evidence item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceLocator {
    /// Kind of source location represented by `value`.
    pub kind: EvidenceLocatorKind,
    /// Source-specific locator text.
    pub value: String,
    /// Optional inclusive start index, line, or page.
    pub start: Option<u32>,
    /// Optional inclusive end index, line, or page.
    pub end: Option<u32>,
}

impl EvidenceLocator {
    /// Checks locator length and requires range endpoints to be both absent or ordered.
    pub fn validate(&self) -> Result<(), GroundedResearchError> {
        validate_grounded_text("locator.value", &self.value, MAX_LOCATOR_CHARS)?;
        match (self.start, self.end) {
            (Some(start), Some(end)) if start <= end => Ok(()),
            (None, None) => Ok(()),
            _ => Err(GroundedResearchError::InvalidLocator),
        }
    }
}

/// Immutable identity and processing metadata for one acquired source revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSourceRevision {
    /// Stable identifier for this immutable source revision.
    pub revision_id: String,
    /// Identifier of the parent source record.
    pub source_id: String,
    /// Monotonically increasing source revision number.
    pub revision: u64,
    /// Digest of the acquired source bytes.
    pub content_hash: String,
    /// Snapshot locator describing the acquired origin.
    pub origin_snapshot: String,
    /// Parser implementation version used to process the source.
    pub parser_version: String,
    /// Indexing profile used for derived representations.
    pub index_profile: String,
    /// Current acquisition and processing status.
    pub status: ResearchSourceStatus,
    /// Trust classification assigned to this revision.
    pub trust: EvidenceTrust,
    /// Root locator used to resolve evidence locations.
    pub locator_root: String,
}

impl ResearchSourceRevision {
    /// Creates a ready source revision and hashes the supplied immutable bytes.
    #[allow(clippy::too_many_arguments)]
    pub fn from_snapshot(
        revision_id: impl Into<String>,
        source_id: impl Into<String>,
        revision: u64,
        bytes: &[u8],
        origin_snapshot: impl Into<String>,
        parser_version: impl Into<String>,
        index_profile: impl Into<String>,
        trust: EvidenceTrust,
        locator_root: impl Into<String>,
    ) -> Result<Self, GroundedResearchError> {
        let result = Self {
            revision_id: revision_id.into(),
            source_id: source_id.into(),
            revision,
            content_hash: sha256_hex(bytes),
            origin_snapshot: origin_snapshot.into(),
            parser_version: parser_version.into(),
            index_profile: index_profile.into(),
            status: ResearchSourceStatus::Ready,
            trust,
            locator_root: locator_root.into(),
        };
        result.validate()?;
        Ok(result)
    }

    /// Validates identifiers, bounds, and required metadata for the revision.
    pub fn validate(&self) -> Result<(), GroundedResearchError> {
        validate_grounded_text("revision_id", &self.revision_id, MAX_REVISION_ID_CHARS)?;
        validate_grounded_text("source_id", &self.source_id, MAX_SOURCE_ID_CHARS)?;
        validate_grounded_text("content_hash", &self.content_hash, 128)?;
        validate_grounded_text(
            "origin_snapshot",
            &self.origin_snapshot,
            MAX_JSON_METADATA_CHARS,
        )?;
        validate_grounded_text("parser_version", &self.parser_version, 64)?;
        validate_grounded_text("index_profile", &self.index_profile, 64)?;
        validate_grounded_text("locator_root", &self.locator_root, MAX_LOCATOR_CHARS)?;
        if self.revision == 0 || !is_hex_hash(&self.content_hash) {
            return Err(GroundedResearchError::InvalidIdentity);
        }
        serde_json::from_str::<serde_json::Value>(&self.origin_snapshot)
            .map_err(|_| GroundedResearchError::InvalidSnapshot)?;
        Ok(())
    }
}

/// Evidence item bound to a specific source revision and locator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceItem {
    /// Stable identifier of the evidence record.
    pub evidence_id: String,
    /// Source revision from which this evidence was extracted.
    pub revision_id: String,
    /// Location of the evidence within the source snapshot.
    pub locator: EvidenceLocator,
    /// Digest of the evidence content.
    pub content_hash: String,
    /// Trust classification inherited or assigned to the evidence.
    pub trust: EvidenceTrust,
}

impl EvidenceItem {
    /// Validates the evidence identifier, locator, digest, and trust metadata.
    pub fn validate(&self) -> Result<(), GroundedResearchError> {
        validate_grounded_text("evidence_id", &self.evidence_id, 128)?;
        validate_grounded_text("revision_id", &self.revision_id, MAX_REVISION_ID_CHARS)?;
        validate_grounded_text("content_hash", &self.content_hash, 128)?;
        self.locator.validate()?;
        if !is_hex_hash(&self.content_hash) {
            return Err(GroundedResearchError::InvalidIdentity);
        }
        Ok(())
    }

    /// Checks that this item matches the ready source revision it cites.
    pub fn validate_against_revision(
        &self,
        revision: &ResearchSourceRevision,
    ) -> Result<(), GroundedResearchError> {
        self.validate()?;
        revision.validate()?;
        if self.revision_id != revision.revision_id
            || revision.status != ResearchSourceStatus::Ready
            || self.content_hash != revision.content_hash
        {
            return Err(GroundedResearchError::StaleReference);
        }
        Ok(())
    }
}

/// Lifecycle state of a grounded research session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSessionState {
    /// Session is queued and has not begun execution.
    Queued,
    /// Session is executing research steps.
    Running,
    /// Cancellation has been requested and is being processed.
    Cancelling,
    /// Session completed with its intended result.
    Completed,
    /// Session completed with explicitly partial coverage.
    Partial,
    /// Session failed.
    Failed,
    /// Session was interrupted and may be recoverable.
    Interrupted,
}

/// Coverage classification recorded for a grounded research artifact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCoverage {
    /// Research reached its declared scope with no known source or budget limit.
    Complete,
    /// Research covers only part of the declared scope.
    Partial,
    /// Research stopped at its configured resource budget.
    BudgetLimited,
    /// Research stopped because source coverage was limited.
    SourceLimited,
    /// Research failed before producing a usable result.
    Failed,
}

/// Research mode driving acquisition depth and synthesis shape.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchMode {
    /// Short investigation with a small source and time budget.
    QuickResearch,
    /// Multi-source investigation with deeper evidence collection.
    DeepResearch,
    /// Side-by-side investigation of supplied subjects.
    Comparison,
    /// Review and synthesis of a body of literature.
    LiteratureReview,
    /// Focused investigation of a technical question or system.
    TechnicalInvestigation,
    /// Learning-oriented collection of bounded study notes.
    LearningNotes,
}

/// Source selection policy for a research session.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourcePolicy {
    /// Restrict research to sources explicitly selected by the caller.
    SelectedOnly,
    /// Include selected sources and permitted workspace content.
    SelectedPlusWorkspace,
    /// Include selected sources and permitted web acquisition.
    SelectedPlusWeb,
    /// Permit broader acquisition within the active policy and budget.
    OpenResearchWithinPolicy,
}

/// Resource limits captured for one grounded research session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchBudget {
    /// Maximum number of source records that may be considered.
    pub max_sources: u32,
    /// Maximum number of subtasks that may be created.
    pub max_subtasks: u32,
    /// Maximum tool calls permitted in the session.
    pub max_tool_calls: u32,
    /// Maximum session duration in milliseconds.
    pub max_duration_ms: u64,
    /// Maximum model token usage.
    pub max_tokens: u64,
}

impl ResearchBudget {
    /// Rejects zero or over-limit values for bounded research resources.
    pub fn validate(&self) -> Result<(), GroundedResearchError> {
        if self.max_sources == 0
            || self.max_sources > 256
            || self.max_subtasks == 0
            || self.max_subtasks > 64
            || self.max_tool_calls > 512
            || self.max_duration_ms == 0
            || self.max_duration_ms > 30 * 60 * 1_000
            || self.max_tokens > 2_000_000
        {
            return Err(GroundedResearchError::LimitExceeded("research budget"));
        }
        Ok(())
    }
}

/// Validation, lineage, freshness, lifecycle, or bound failure in grounded research.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundedResearchError {
    /// A required metadata field is empty.
    EmptyField(&'static str),
    /// A field exceeds its maximum length.
    FieldTooLong {
        /// Name of the field that exceeded its limit.
        field: &'static str,
        /// Maximum permitted length.
        max: usize,
    },
    /// An identifier or digest does not meet its immutable identity format.
    InvalidIdentity,
    /// A policy or origin snapshot is not valid JSON.
    InvalidSnapshot,
    /// Evidence locator fields or range endpoints are invalid.
    InvalidLocator,
    /// A collection or value exceeds a contract bound.
    LimitExceeded(&'static str),
    /// A referenced revision is not current or available.
    StaleReference,
}

impl fmt::Display for GroundedResearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::InvalidIdentity => write!(f, "invalid immutable research identity"),
            Self::InvalidSnapshot => write!(f, "invalid research policy snapshot"),
            Self::InvalidLocator => write!(f, "invalid evidence locator"),
            Self::LimitExceeded(field) => write!(f, "{field} exceeds its bound"),
            Self::StaleReference => write!(f, "research reference is stale or unavailable"),
        }
    }
}

impl std::error::Error for GroundedResearchError {}

fn validate_grounded_text(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), GroundedResearchError> {
    if value.trim().is_empty() {
        return Err(GroundedResearchError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(GroundedResearchError::FieldTooLong { field, max });
    }
    Ok(())
}

fn is_hex_hash(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_hexdigit())
}
