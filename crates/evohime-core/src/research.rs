//! Самостоятельный data-contract для offline research evidence.
//!
//! Модуль намеренно не подключён к `lib.rs`: его можно интегрировать в Core
//! отдельным изменением, когда будут готовы IPC и storage-контуры research.
//! Все размеры ограничены до сериализации, а JSON получается из структуры с
//! фиксированным порядком полей.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_URL_CHARS: usize = 2_048;
pub const MAX_TITLE_CHARS: usize = 256;
pub const MAX_PUBLISHER_CHARS: usize = 256;
pub const MAX_CONTENT_TYPE_CHARS: usize = 128;
pub const MAX_EXCERPT_CHARS: usize = 4_096;
pub const MAX_TTL_MS: u64 = 31 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    EmptyField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    InvalidTimestamp,
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
    pub url: String,
    pub title: String,
    pub publisher: String,
    pub content_type: String,
    pub retrieved_at_ms: u64,
}

impl SourceMetadata {
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
    pub source: SourceMetadata,
    pub excerpt: String,
    pub excerpt_sha256: String,
    pub captured_at_ms: u64,
    pub ttl_ms: u64,
}

impl ResearchEvidence {
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

    pub fn expires_at_ms(&self) -> Option<u64> {
        self.captured_at_ms.checked_add(self.ttl_ms)
    }

    pub fn is_fresh_at(&self, now_ms: u64) -> bool {
        now_ms >= self.captured_at_ms && now_ms < self.expires_at_ms().unwrap_or(u64::MAX)
    }

    /// Stable compact JSON suitable for hashing, storage, or IPC fixtures.
    pub fn to_deterministic_json(&self) -> String {
        serde_json::to_string(self).expect("ResearchEvidence is serializable")
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
pub const MAX_SOURCE_ID_CHARS: usize = 128;
pub const MAX_REVISION_ID_CHARS: usize = 128;
pub const MAX_LOCATOR_CHARS: usize = 1_024;
pub const MAX_JSON_METADATA_CHARS: usize = 32 * 1024;
pub const MAX_COLLECTION_SOURCES: usize = 256;
pub const MAX_STRUCTURAL_UNITS: usize = 4_096;
pub const MAX_STRUCTURAL_UNIT_CHARS: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StructuralUnitKind {
    Heading,
    Paragraph,
    Code,
    List,
    Table,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuralUnit {
    pub ordinal: u32,
    pub kind: StructuralUnitKind,
    pub text: String,
    pub start_line: u32,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSourceKind {
    WorkspaceFile,
    WorkspaceSelection,
    UploadedFile,
    PlainText,
    Markdown,
    Pdf,
    WebPage,
    GitHubFile,
    GitHubRepositorySnapshot,
    ProjectArtifact,
    ManualNote,
    ExternalConnectorDocument,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSourceStatus {
    Pending,
    Acquiring,
    Parsing,
    Extracting,
    Indexing,
    Ready,
    PartiallyReady,
    Failed,
    Stale,
    Unavailable,
    Removing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTrust {
    Workspace,
    UserProvided,
    AcquiredExternal,
    Derived,
    Unverified,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLocatorKind {
    FileRange,
    PageSection,
    Paragraph,
    HtmlRange,
    ArtifactBlock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceLocator {
    pub kind: EvidenceLocatorKind,
    pub value: String,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

impl EvidenceLocator {
    pub fn validate(&self) -> Result<(), GroundedResearchError> {
        validate_grounded_text("locator.value", &self.value, MAX_LOCATOR_CHARS)?;
        match (self.start, self.end) {
            (Some(start), Some(end)) if start <= end => Ok(()),
            (None, None) => Ok(()),
            _ => Err(GroundedResearchError::InvalidLocator),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSourceRevision {
    pub revision_id: String,
    pub source_id: String,
    pub revision: u64,
    pub content_hash: String,
    pub origin_snapshot: String,
    pub parser_version: String,
    pub index_profile: String,
    pub status: ResearchSourceStatus,
    pub trust: EvidenceTrust,
    pub locator_root: String,
}

impl ResearchSourceRevision {
    /// Creates the immutable identity for an acquired snapshot.  Volatile
    /// fetch timestamps are deliberately excluded from `content_hash`.
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

    pub fn classify_snapshot(&self, bytes: &[u8]) -> ResearchSourceStatus {
        if sha256_hex(bytes) == self.content_hash {
            self.status
        } else {
            ResearchSourceStatus::Stale
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceItem {
    pub evidence_id: String,
    pub revision_id: String,
    pub locator: EvidenceLocator,
    pub content_hash: String,
    pub trust: EvidenceTrust,
}

impl EvidenceItem {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCoverage {
    Complete,
    Partial,
    BudgetLimited,
    SourceLimited,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CitationKind {
    DirectSupport,
    PartialSupport,
    Context,
    Contradiction,
    DerivedFromMultiple,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchCitation {
    pub citation_id: String,
    pub claim_id: String,
    pub evidence_id: String,
    pub kind: CitationKind,
    pub validation: String,
}

/// Validates lineage before a citation can be exposed as verified.  A
/// verified locator proves addressability only; it never asserts factual
/// truth of the claim.
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchClaim {
    pub claim_id: String,
    pub text_hash: String,
    pub inference: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchArtifact {
    pub artifact_id: String,
    pub revision: u64,
    pub session_id: String,
    pub content_hash: String,
    pub coverage: ResearchCoverage,
    pub claims: Vec<ResearchClaim>,
    pub citations: Vec<ResearchCitation>,
    pub immutable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchMode {
    QuickResearch,
    DeepResearch,
    Comparison,
    LiteratureReview,
    TechnicalInvestigation,
    LearningNotes,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourcePolicy {
    SelectedOnly,
    SelectedPlusWorkspace,
    SelectedPlusWeb,
    OpenResearchWithinPolicy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSessionState {
    Queued,
    Running,
    Cancelling,
    Completed,
    Partial,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchBudget {
    pub max_sources: u32,
    pub max_subtasks: u32,
    pub max_tool_calls: u32,
    pub max_duration_ms: u64,
    pub max_tokens: u64,
}

impl ResearchBudget {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSession {
    pub session_id: String,
    pub workspace_id: String,
    pub collection_id: String,
    pub mode: ResearchMode,
    pub source_policy: SourcePolicy,
    pub pinned_revision_ids: Vec<String>,
    pub tool_policy_snapshot: String,
    pub model_policy_snapshot: String,
    pub budget: ResearchBudget,
    pub state: ResearchSessionState,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSubtask {
    pub subtask_id: String,
    pub session_id: String,
    pub objective_hash: String,
    pub state: ResearchSessionState,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceConflict {
    pub conflict_id: String,
    pub evidence_ids: Vec<String>,
    pub description_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchDelta {
    pub delta_id: String,
    pub previous_artifact_id: String,
    pub current_artifact_id: String,
    pub added_evidence_ids: Vec<String>,
    pub stale_evidence_ids: Vec<String>,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundedResearchError {
    EmptyField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    InvalidIdentity,
    InvalidSnapshot,
    InvalidLocator,
    LimitExceeded(&'static str),
    StaleReference,
    InvalidCitation,
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
        let json = evidence.to_deterministic_json();
        assert_eq!(json, evidence.to_deterministic_json());
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
