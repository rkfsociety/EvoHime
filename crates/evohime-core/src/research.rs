//! Самостоятельный data-contract для offline research evidence.
//!
//! Модуль намеренно не подключён к `lib.rs`: его можно интегрировать в Core
//! отдельным изменением, когда будут готовы IPC и storage-контуры research.
//! Все размеры ограничены до сериализации, а JSON получается из структуры с
//! фиксированным порядком полей.

use serde::{Deserialize, Serialize};
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

/// Immutable evidence captured offline. `excerpt` is already redacted and
/// `excerpt_sha256` hashes exactly those redacted UTF-8 bytes.
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
    ///
    /// # Example
    ///
    /// ```
    /// use evohime_core::research::{ResearchEvidence, SourceMetadata};
    ///
    /// let source = SourceMetadata::new(
    ///     "https://example.invalid/report",
    ///     "Report",
    ///     "Example publisher",
    ///     "text/plain",
    ///     1_000,
    /// )?;
    /// let evidence = ResearchEvidence::capture(source, "sk-secret-token", 2_000, 60_000)?;
    /// assert_eq!(evidence.excerpt, "[REDACTED]");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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

    /// Stable compact JSON suitable for hashing, storage, or IPC fixtures.
    pub fn to_deterministic_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
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
    let redacted = output.concat();
    Ok(redacted)
}

fn is_secret_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("AIza")
        || (token.contains('@') && token.split('@').next().is_some_and(|part| !part.is_empty()))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha256(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Version of the grounded-research metadata contract.  It is independent
/// from the legacy offline evidence record above.
pub const GROUNDED_RESEARCH_SCHEMA_VERSION: u32 = 1;
/// Maximum identifier length for a grounded research source.
pub const MAX_SOURCE_ID_CHARS: usize = 128;
/// Maximum identifier length for a grounded research revision.
pub const MAX_REVISION_ID_CHARS: usize = 128;
/// Maximum evidence locator length.
pub const MAX_LOCATOR_CHARS: usize = 1_024;
/// Maximum serialized metadata size in characters.
pub const MAX_JSON_METADATA_CHARS: usize = 32 * 1024;
/// Maximum number of sources in one grounded research collection.
pub const MAX_COLLECTION_SOURCES: usize = 256;
/// Maximum number of extracted structural units.
pub const MAX_STRUCTURAL_UNITS: usize = 4_096;
/// Maximum length of one extracted structural unit.
pub const MAX_STRUCTURAL_UNIT_CHARS: usize = 16 * 1024;

/// Structural class assigned to an extracted text unit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StructuralUnitKind {
    /// Markdown heading.
    Heading,
    /// Paragraph of ordinary text.
    Paragraph,
    /// Fenced code block line.
    Code,
    /// List item.
    List,
    /// Table row.
    Table,
}

/// Bounded text unit extracted from an inert Markdown or plain-text source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuralUnit {
    /// Zero-based position in the extraction result.
    pub ordinal: u32,
    /// Structural category of the unit.
    pub kind: StructuralUnitKind,
    /// Extracted text content.
    pub text: String,
    /// One-based first source line represented by the unit.
    pub start_line: u32,
    /// One-based last source line represented by the unit.
    pub end_line: u32,
}

/// Deterministic, non-executable Markdown/text extraction.  The parser keeps
/// only bounded structural units; it never interprets embedded scripts,
/// macros or links as instructions.
pub fn extract_structural_units(
    input: &str,
    max_units: usize,
) -> Result<Vec<StructuralUnit>, GroundedResearchError> {
    if max_units == 0 || max_units > MAX_STRUCTURAL_UNITS {
        return Err(GroundedResearchError::LimitExceeded("structural units"));
    }
    let mut result = Vec::new();
    let mut paragraph = Vec::new();
    let mut paragraph_start = 0u32;
    let flush = |result: &mut Vec<StructuralUnit>, paragraph: &mut Vec<&str>, start: u32| {
        if paragraph.is_empty() || result.len() >= max_units {
            return;
        }
        let text = paragraph.join(" ");
        if text.chars().count() <= MAX_STRUCTURAL_UNIT_CHARS {
            result.push(StructuralUnit {
                ordinal: result.len() as u32,
                kind: StructuralUnitKind::Paragraph,
                text,
                start_line: start,
                end_line: start + paragraph.len().saturating_sub(1) as u32,
            });
        }
        paragraph.clear();
    };
    for (line_index, line) in input.lines().enumerate() {
        let line_number = line_index as u32 + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush(&mut result, &mut paragraph, paragraph_start);
            continue;
        }
        let (kind, text) = if let Some(value) = trimmed.strip_prefix("#") {
            (Some(StructuralUnitKind::Heading), value.trim())
        } else if trimmed.starts_with("```") {
            (Some(StructuralUnitKind::Code), trimmed)
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            (Some(StructuralUnitKind::List), trimmed)
        } else if trimmed.starts_with('|') {
            (Some(StructuralUnitKind::Table), trimmed)
        } else {
            (None, trimmed)
        };
        if let Some(kind) = kind {
            flush(&mut result, &mut paragraph, paragraph_start);
            if result.len() < max_units && text.chars().count() <= MAX_STRUCTURAL_UNIT_CHARS {
                result.push(StructuralUnit {
                    ordinal: result.len() as u32,
                    kind,
                    text: text.to_owned(),
                    start_line: line_number,
                    end_line: line_number,
                });
            }
        } else {
            if paragraph.is_empty() {
                paragraph_start = line_number;
            }
            paragraph.push(trimmed);
        }
        if result.len() >= max_units {
            break;
        }
    }
    flush(&mut result, &mut paragraph, paragraph_start);
    Ok(result)
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

/// Acquisition and processing lifecycle of a research source.
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
    /// Creates the immutable identity for an acquired snapshot.  Volatile
    /// fetch timestamps are deliberately excluded from `content_hash`.
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

    /// Returns whether the supplied bytes still match this revision's digest.
    pub fn classify_snapshot(&self, bytes: &[u8]) -> ResearchSourceStatus {
        if sha256_hex(bytes) == self.content_hash {
            self.status
        } else {
            ResearchSourceStatus::Stale
        }
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

/// Relationship declared between a claim and its evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CitationKind {
    /// Evidence directly supports the claim.
    DirectSupport,
    /// Evidence supports only part of the claim.
    PartialSupport,
    /// Evidence provides relevant context without direct support.
    Context,
    /// Evidence contradicts the claim.
    Contradiction,
    /// Claim is derived from multiple evidence items.
    DerivedFromMultiple,
}

/// Citation binding one research claim to one evidence item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchCitation {
    /// Stable citation identifier.
    pub citation_id: String,
    /// Claim identifier supported or qualified by this citation.
    pub claim_id: String,
    /// Identifier of the cited evidence item.
    pub evidence_id: String,
    /// Declared relationship between claim and evidence.
    pub kind: CitationKind,
    /// Validation state; `verified_locator` establishes lineage and addressability only.
    pub validation: String,
}

/// Validates lineage before a citation can be exposed as verified.  A
/// verified locator proves addressability only; it never asserts factual
/// truth of the claim.
///
/// # Example
///
/// ```
/// use evohime_core::research::{
///     validate_research_citation, CitationKind, EvidenceItem, EvidenceLocator,
///     EvidenceLocatorKind, EvidenceTrust, ResearchCitation, ResearchClaim,
///     ResearchSourceRevision, ResearchSourceStatus,
/// };
///
/// let revision = ResearchSourceRevision::from_snapshot(
///     "rev-1", "source-1", 1, b"evidence", "{}", "parser-v1", "index-v1",
///     EvidenceTrust::Workspace, "src/report.md",
/// )?;
/// let evidence = EvidenceItem {
///     evidence_id: "evidence-1".into(),
///     revision_id: revision.revision_id.clone(),
///     locator: EvidenceLocator { kind: EvidenceLocatorKind::FileRange, value: "src/report.md".into(), start: Some(1), end: Some(2) },
///     content_hash: revision.content_hash.clone(),
///     trust: EvidenceTrust::Workspace,
/// };
/// let claim = ResearchClaim { claim_id: "claim-1".into(), text_hash: "a".repeat(64), inference: false };
/// let citation = ResearchCitation {
///     citation_id: "citation-1".into(), claim_id: claim.claim_id.clone(),
///     evidence_id: evidence.evidence_id.clone(), kind: CitationKind::DirectSupport,
///     validation: "verified_locator".into(),
/// };
/// validate_research_citation(&citation, &claim, &evidence, &revision)?;
/// assert_eq!(revision.classify_snapshot(b"evidence"), ResearchSourceStatus::Ready);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn validate_research_citation(
    citation: &ResearchCitation,
    claim: &ResearchClaim,
    evidence: &EvidenceItem,
    revision: &ResearchSourceRevision,
) -> Result<(), GroundedResearchError> {
    validate_grounded_text("citation_id", &citation.citation_id, 128)?;
    validate_grounded_text("claim_id", &citation.claim_id, 128)?;
    validate_grounded_text("evidence_id", &citation.evidence_id, 128)?;
    validate_grounded_text("validation", &citation.validation, 64)?;
    validate_grounded_text("claim.text_hash", &claim.text_hash, 128)?;
    if citation.claim_id != claim.claim_id
        || citation.evidence_id != evidence.evidence_id
        || citation.validation != "verified_locator"
    {
        return Err(GroundedResearchError::InvalidCitation);
    }
    evidence.validate_against_revision(revision)
}

/// Bounded claim metadata; the claim's text is represented by a digest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchClaim {
    /// Stable identifier for the claim.
    pub claim_id: String,
    /// Digest of claim text stored outside this metadata record.
    pub text_hash: String,
    /// Whether the claim is an inference rather than a direct source statement.
    pub inference: bool,
}

/// Immutable research output linking claims to cited source evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchArtifact {
    /// Stable artifact identifier.
    pub artifact_id: String,
    /// Monotonically increasing artifact revision.
    pub revision: u64,
    /// Research session that produced this artifact.
    pub session_id: String,
    /// Digest of the canonical artifact content.
    pub content_hash: String,
    /// Coverage level achieved by the research session.
    pub coverage: ResearchCoverage,
    /// Claims contained in the artifact.
    pub claims: Vec<ResearchClaim>,
    /// Evidence citations associated with those claims.
    pub citations: Vec<ResearchCitation>,
    /// Whether this artifact revision is immutable.
    pub immutable: bool,
}

/// Research workflow profile that controls depth and output style.
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

/// Policy snapshots and source revisions bound to one research session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSession {
    /// Stable session identifier.
    pub session_id: String,
    /// Workspace that owns the session.
    pub workspace_id: String,
    /// Source collection used by the session.
    pub collection_id: String,
    /// Research workflow profile.
    pub mode: ResearchMode,
    /// Permitted source-acquisition boundary.
    pub source_policy: SourcePolicy,
    /// Immutable source revision identifiers pinned at session start.
    pub pinned_revision_ids: Vec<String>,
    /// Serialized snapshot of the effective tool policy.
    pub tool_policy_snapshot: String,
    /// Serialized snapshot of the effective model policy.
    pub model_policy_snapshot: String,
    /// Resource bounds captured for this session.
    pub budget: ResearchBudget,
    /// Current lifecycle state.
    pub state: ResearchSessionState,
}

/// Applies one of the allowed grounded research session state transitions.
pub fn transition_research_session(
    from: ResearchSessionState,
    to: ResearchSessionState,
) -> Result<ResearchSessionState, GroundedResearchError> {
    let valid = matches!(
        (from, to),
        (ResearchSessionState::Queued, ResearchSessionState::Running)
            | (
                ResearchSessionState::Running,
                ResearchSessionState::Cancelling
            )
            | (
                ResearchSessionState::Running,
                ResearchSessionState::Completed
            )
            | (ResearchSessionState::Running, ResearchSessionState::Partial)
            | (ResearchSessionState::Running, ResearchSessionState::Failed)
            | (
                ResearchSessionState::Running,
                ResearchSessionState::Interrupted
            )
            | (
                ResearchSessionState::Cancelling,
                ResearchSessionState::Partial
            )
            | (
                ResearchSessionState::Cancelling,
                ResearchSessionState::Failed
            )
            | (
                ResearchSessionState::Interrupted,
                ResearchSessionState::Running
            )
    );
    if valid {
        Ok(to)
    } else {
        Err(GroundedResearchError::InvalidTransition)
    }
}

impl ResearchSession {
    /// Validates session identifiers, policy snapshots, revision references, and budget.
    pub fn validate(&self) -> Result<(), GroundedResearchError> {
        validate_grounded_text("session_id", &self.session_id, 128)?;
        validate_grounded_text("workspace_id", &self.workspace_id, 256)?;
        validate_grounded_text("collection_id", &self.collection_id, 128)?;
        validate_grounded_text(
            "tool_policy_snapshot",
            &self.tool_policy_snapshot,
            MAX_JSON_METADATA_CHARS,
        )?;
        validate_grounded_text(
            "model_policy_snapshot",
            &self.model_policy_snapshot,
            MAX_JSON_METADATA_CHARS,
        )?;
        serde_json::from_str::<serde_json::Value>(&self.tool_policy_snapshot)
            .map_err(|_| GroundedResearchError::InvalidSnapshot)?;
        serde_json::from_str::<serde_json::Value>(&self.model_policy_snapshot)
            .map_err(|_| GroundedResearchError::InvalidSnapshot)?;
        if self.pinned_revision_ids.len() > MAX_COLLECTION_SOURCES {
            return Err(GroundedResearchError::LimitExceeded("pinned revisions"));
        }
        self.budget.validate()
    }
}

/// Bounded work unit created within a grounded research session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSubtask {
    /// Stable subtask identifier.
    pub subtask_id: String,
    /// Parent research session identifier.
    pub session_id: String,
    /// Digest of the subtask objective.
    pub objective_hash: String,
    /// Current lifecycle state of the subtask.
    pub state: ResearchSessionState,
    /// Evidence identifiers collected by this subtask.
    pub evidence_ids: Vec<String>,
}

/// Recorded conflict between a bounded set of research evidence items.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceConflict {
    /// Stable conflict identifier.
    pub conflict_id: String,
    /// Evidence records participating in the conflict.
    pub evidence_ids: Vec<String>,
    /// Digest of the conflict description.
    pub description_hash: String,
}

/// Evidence additions and stale references between two artifact revisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchDelta {
    /// Stable identifier for this delta record.
    pub delta_id: String,
    /// Identifier of the older artifact revision.
    pub previous_artifact_id: String,
    /// Identifier of the newer artifact revision.
    pub current_artifact_id: String,
    /// Evidence identifiers present only in the current artifact.
    pub added_evidence_ids: Vec<String>,
    /// Evidence identifiers present only in the previous artifact.
    pub stale_evidence_ids: Vec<String>,
}

/// Verifies that artifact claims and citations match the supplied lineage records.
pub fn validate_artifact_lineage(
    artifact: &ResearchArtifact,
    claims: &[ResearchClaim],
    citations: &[ResearchCitation],
) -> Result<(), GroundedResearchError> {
    artifact.validate()?;
    if artifact.claims != claims || artifact.citations != citations {
        return Err(GroundedResearchError::InvalidCitation);
    }
    let claim_ids = claims
        .iter()
        .map(|claim| claim.claim_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for citation in citations {
        if !claim_ids.contains(citation.claim_id.as_str()) {
            return Err(GroundedResearchError::InvalidCitation);
        }
    }
    Ok(())
}

/// Computes evidence additions and removals between two validated artifacts.
pub fn derive_research_delta(
    previous: &ResearchArtifact,
    current: &ResearchArtifact,
    delta_id: impl Into<String>,
) -> Result<ResearchDelta, GroundedResearchError> {
    previous.validate()?;
    current.validate()?;
    let previous_ids = previous
        .citations
        .iter()
        .map(|citation| citation.evidence_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let current_ids = current
        .citations
        .iter()
        .map(|citation| citation.evidence_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    Ok(ResearchDelta {
        delta_id: delta_id.into(),
        previous_artifact_id: previous.artifact_id.clone(),
        current_artifact_id: current.artifact_id.clone(),
        added_evidence_ids: current_ids.difference(&previous_ids).cloned().collect(),
        stale_evidence_ids: previous_ids.difference(&current_ids).cloned().collect(),
    })
}

impl ResearchArtifact {
    /// Validates artifact identity, immutable digest shape, and bounded lineage.
    pub fn validate(&self) -> Result<(), GroundedResearchError> {
        validate_grounded_text("artifact_id", &self.artifact_id, 128)?;
        validate_grounded_text("session_id", &self.session_id, 128)?;
        validate_grounded_text("content_hash", &self.content_hash, 128)?;
        if self.revision == 0 || !self.immutable || !is_hex_hash(&self.content_hash) {
            return Err(GroundedResearchError::InvalidIdentity);
        }
        if self.claims.len() > 256 || self.citations.len() > 256 {
            return Err(GroundedResearchError::LimitExceeded("artifact lineage"));
        }
        for claim in &self.claims {
            validate_grounded_text("claim_id", &claim.claim_id, 128)?;
            validate_grounded_text("claim.text_hash", &claim.text_hash, 128)?;
        }
        for citation in &self.citations {
            validate_grounded_text("citation_id", &citation.citation_id, 128)?;
            validate_grounded_text("citation.claim_id", &citation.claim_id, 128)?;
            validate_grounded_text("citation.evidence_id", &citation.evidence_id, 128)?;
            validate_grounded_text("citation.validation", &citation.validation, 64)?;
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
    /// A citation failed its identity or lineage checks.
    InvalidCitation,
    /// A session lifecycle transition is not permitted.
    InvalidTransition,
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
            Self::InvalidCitation => write!(f, "invalid research citation lineage"),
            Self::InvalidTransition => write!(f, "invalid research session transition"),
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
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

// Small dependency-free SHA-256 implementation for the contract's content hash.
fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in chunk.as_chunks::<4>().0.iter().take(16).enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            (hh, g, f, e, d, c, b, a) = (
                g,
                f,
                e,
                d.wrapping_add(temp1),
                c,
                b,
                a,
                temp1.wrapping_add(temp2),
            );
        }
        for (value, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *value = (*value).wrapping_add(add);
        }
    }
    let mut out = [0u8; 32];
    for (i, value) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceMetadata {
        SourceMetadata::new(
            "https://example.test/a",
            "Example",
            "Example Org",
            "text/html",
            1_700_000_000_000,
        )
        .unwrap()
    }

    #[test]
    fn captures_redacted_excerpt_and_sha256() {
        let evidence = ResearchEvidence::capture(
            source(),
            "Useful text sk-secret alice@example.test",
            2_000,
            1_000,
        )
        .unwrap();
        assert_eq!(evidence.excerpt, "Useful text [REDACTED] [REDACTED]");
        assert_eq!(
            evidence.excerpt_sha256,
            "35a7d5361181b0e82bb8016999917970bcf66e41a34b36f76451f41f559d1b36"
        );
        assert!(evidence.is_fresh_at(2_999));
        assert!(!evidence.is_fresh_at(3_000));
    }

    #[test]
    fn deterministic_json_is_stable_and_round_trips() {
        let evidence = ResearchEvidence::capture(source(), "same", 2_000, 1_000).unwrap();
        let json = evidence.to_deterministic_json().unwrap();
        assert_eq!(json, evidence.to_deterministic_json().unwrap());
        assert!(json.starts_with("{\"source\":{\"url\":\"https://example.test/a\""));
        assert_eq!(
            serde_json::from_str::<ResearchEvidence>(&json).unwrap(),
            evidence
        );
    }

    #[test]
    fn bounds_metadata_excerpt_and_ttl() {
        assert!(matches!(
            SourceMetadata::new("", "title", "publisher", "text/plain", 1),
            Err(ContractError::EmptyField("url"))
        ));
        assert!(matches!(
            ResearchEvidence::capture(source(), "x", 1, 0),
            Err(ContractError::InvalidTtl)
        ));
        assert!(matches!(
            ResearchEvidence::capture(source(), "x".repeat(MAX_EXCERPT_CHARS + 1), 1, 1),
            Err(ContractError::FieldTooLong {
                field: "excerpt",
                ..
            })
        ));
    }

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn grounded_snapshot_identity_and_locator_are_revision_bound() {
        let revision = ResearchSourceRevision::from_snapshot(
            "revision-1",
            "source-1",
            1,
            b"snapshot",
            "{\"url\":\"https://example.test\"}",
            "parser/v1",
            "index/v1",
            EvidenceTrust::AcquiredExternal,
            "https://example.test",
        )
        .unwrap();
        let item = EvidenceItem {
            evidence_id: "evidence-1".into(),
            revision_id: revision.revision_id.clone(),
            locator: EvidenceLocator {
                kind: EvidenceLocatorKind::HtmlRange,
                value: revision.locator_root.clone(),
                start: Some(0),
                end: Some(8),
            },
            content_hash: revision.content_hash.clone(),
            trust: EvidenceTrust::AcquiredExternal,
        };
        assert!(item.validate_against_revision(&revision).is_ok());
        let mut stale = revision.clone();
        stale.status = ResearchSourceStatus::Stale;
        assert!(matches!(
            item.validate_against_revision(&stale),
            Err(GroundedResearchError::StaleReference)
        ));
    }

    #[test]
    fn grounded_session_transitions_are_fail_closed() {
        assert!(transition_research_session(
            ResearchSessionState::Queued,
            ResearchSessionState::Running
        )
        .is_ok());
        assert!(matches!(
            transition_research_session(
                ResearchSessionState::Queued,
                ResearchSessionState::Completed
            ),
            Err(GroundedResearchError::InvalidTransition)
        ));
    }

    #[test]
    fn structural_extraction_is_bounded_and_non_executable() {
        let units =
            extract_structural_units("# Heading\n\ntext\n\n- item\n\n<script>run()</script>", 8)
                .unwrap();
        assert_eq!(units[0].kind, StructuralUnitKind::Heading);
        assert!(units.iter().any(|unit| unit.text.contains("script")));
        assert!(extract_structural_units("text", 0).is_err());
    }
}
