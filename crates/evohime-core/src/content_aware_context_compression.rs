//! Deterministic, metadata-first compaction for bounded context items.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Stable contract identifier used to version compression metadata.
pub const CONTRACT_ID: &str = "content-aware-context-compression-v1";
/// Maximum input size processed by one compaction operation.
pub const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
/// Maximum emitted compact block size.
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024;

/// Content category used to select safe compaction behavior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    /// Unstructured human-readable text.
    PlainText,
    /// Source code where syntax and symbol boundaries matter.
    SourceCode,
    /// Unified diff where hunk structure must be preserved.
    UnifiedDiff,
    /// Structured JSON document.
    Json,
    /// Newline-delimited JSON records.
    JsonLines,
    /// YAML or YAML-like structured text.
    Yaml,
    /// Comma- or tab-separated tabular data.
    CsvTsv,
    /// Build output containing diagnostic and progress lines.
    BuildLog,
    /// Test output containing pass/fail evidence.
    TestOutput,
    /// Diagnostic messages that should not lose failure evidence.
    Diagnostics,
    /// Search results with source references.
    SearchResults,
    /// Extracted human-readable HTML content.
    HtmlText,
    /// Accessibility tree or semantic UI representation.
    AccessibilityTree,
    /// Content category could not be determined.
    Unknown,
}

/// Information-loss class of a compacted representation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LossClass {
    /// Encoding changes without removing source information.
    LosslessReencoding,
    /// Bounded region is omitted while surrounding structure is retained.
    StructurePreservingElision,
    /// Projection retains evidence while excluding some source fields.
    EvidencePreservingProjection,
    /// Meaning is summarized and exact source detail is not retained.
    SemanticSummary,
}

/// Result of deciding whether a compaction is beneficial and safe.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenefitDecision {
    /// Input was reduced within the configured output bound.
    Compress,
    /// Input was already small enough to keep unchanged.
    NoBenefit,
    /// Content policy requires retaining the original representation.
    Protected,
    /// Compaction could not safely complete.
    Failed,
}

/// Half-open line range omitted from a compact block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmittedRegion {
    /// First omitted line index, inclusive.
    pub start: u32,
    /// First retained line index after the omitted region.
    pub end: u32,
    /// Stable reason code for the omission.
    pub reason: String,
}

/// Compacted context with source integrity, loss, and recovery metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactContextBlock {
    /// Reference to the original context source.
    pub source_ref: String,
    /// SHA-256 digest of the uncompressed source.
    pub source_hash: String,
    /// SHA-256 digest of the emitted compact body.
    pub compact_hash: String,
    /// Detected content category.
    pub kind: ContentKind,
    /// Information-loss class of this representation.
    pub loss: LossClass,
    /// Compact text provided to the downstream consumer.
    pub body: String,
    /// Source ranges excluded during compaction.
    pub omitted: Vec<OmittedRegion>,
    /// Whether any source content is absent from the compact body.
    pub incomplete: bool,
    /// Whether compaction reduced the original content.
    pub decision: BenefitDecision,
}

impl CompactContextBlock {
    /// Checks source identity, hashes, output bound, and omission count.
    pub fn validate(&self) -> Result<(), CompressionError> {
        if self.source_ref.trim().is_empty()
            || self.body.len() > MAX_OUTPUT_BYTES
            || self.source_hash.len() != 64
            || self.compact_hash.len() != 64
            || self.omitted.len() > 256
        {
            return Err(CompressionError::InvalidBlock);
        }
        if !self.source_hash.bytes().all(|b| b.is_ascii_hexdigit())
            || !self.compact_hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(CompressionError::InvalidBlock);
        }
        Ok(())
    }
}

/// Strategy for retrieving an omitted source region when needed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStrategy {
    /// Retrieve the exact previously omitted range.
    ExactRegion,
    /// Retrieve content surrounding a stable locator.
    AroundLocator,
    /// Retrieve a value by a structured-data path.
    StructuredPath,
    /// Retrieve a source line range.
    LineRange,
    /// Retrieve the next page of a paginated source.
    NextPage,
    /// Expand a previously grouped source section.
    ExpandGroup,
    /// Retrieve an original source segment within a byte bound.
    OriginalBounded,
}

/// Bounded request for recovering a slice from the original source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoverContextSlice {
    /// Reference to the original context source.
    pub source_ref: String,
    /// Expected digest of that source revision.
    pub source_hash: String,
    /// Retrieval method used to locate the omitted content.
    pub strategy: RecoveryStrategy,
    /// Inclusive start offset or index.
    pub start: u32,
    /// Exclusive end offset or index.
    pub end: u32,
    /// Maximum returned size in bytes.
    pub max_bytes: u32,
}

impl RecoverContextSlice {
    /// Validates the source reference, range, digest length, and size bound.
    pub fn validate(&self) -> Result<(), CompressionError> {
        if self.source_ref.trim().is_empty()
            || self.source_hash.len() != 64
            || self.start > self.end
            || self.end - self.start > self.max_bytes
            || self.max_bytes == 0
        {
            Err(CompressionError::InvalidRecovery)
        } else {
            Ok(())
        }
    }
}

/// Classifies common structured, diagnostic, and plain-text content formats.
pub fn classify(input: &str) -> ContentKind {
    let trimmed = input.trim_start();
    if trimmed.starts_with("diff --git ") {
        ContentKind::UnifiedDiff
    } else if trimmed.starts_with('{') || trimmed.starts_with('[') {
        ContentKind::Json
    } else if input
        .lines()
        .any(|line| line.contains("error:") || line.contains("ERROR"))
    {
        ContentKind::Diagnostics
    } else if input.lines().count() > 16
        && input
            .lines()
            .all(|line| line.contains(':') || line.trim().is_empty())
    {
        ContentKind::Yaml
    } else {
        ContentKind::PlainText
    }
}

/// Produces a bounded deterministic context block while preserving diagnostics.
pub fn compact(
    source_ref: impl Into<String>,
    input: &str,
    max_lines: usize,
) -> Result<CompactContextBlock, CompressionError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(CompressionError::InputTooLarge);
    }
    let source_ref = source_ref.into();
    if source_ref.trim().is_empty() || max_lines == 0 {
        return Err(CompressionError::InvalidInput);
    }
    let kind = classify(input);
    let lines: Vec<&str> = input.lines().collect();
    if matches!(
        kind,
        ContentKind::Diagnostics | ContentKind::BuildLog | ContentKind::TestOutput
    ) && lines.len() > max_lines
        && lines[max_lines / 2..lines.len() - (max_lines - max_lines / 2)]
            .iter()
            .any(|line| {
                let lower = line.to_ascii_lowercase();
                lower.contains("error") || lower.contains("fail") || lower.contains("panic")
            })
    {
        return Err(CompressionError::InvalidInput);
    }
    let keep = if lines.len() <= max_lines {
        lines.clone()
    } else {
        let head = max_lines / 2;
        let tail = max_lines - head;
        lines[..head]
            .iter()
            .chain(lines[lines.len() - tail..].iter())
            .copied()
            .collect()
    };
    let body = keep.join("\n");
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let source_hash = format!("{:x}", hasher.finalize());
    let mut compact_hasher = Sha256::new();
    compact_hasher.update(body.as_bytes());
    let compact_hash = format!("{:x}", compact_hasher.finalize());
    let mut omitted = Vec::new();
    if lines.len() > max_lines {
        omitted.push(OmittedRegion {
            start: (max_lines / 2) as u32,
            end: (lines.len() - (max_lines - max_lines / 2)) as u32,
            reason: "bounded_middle_elision".into(),
        });
    }
    let decision = if lines.len() <= max_lines {
        BenefitDecision::NoBenefit
    } else {
        BenefitDecision::Compress
    };
    let block = CompactContextBlock {
        source_ref,
        source_hash,
        compact_hash,
        kind,
        loss: if matches!(kind, ContentKind::Json | ContentKind::JsonLines) {
            LossClass::EvidencePreservingProjection
        } else {
            LossClass::StructurePreservingElision
        },
        body,
        omitted,
        incomplete: lines.len() > max_lines,
        decision,
    };
    block.validate()?;
    Ok(block)
}

/// Invalid input, size, compact block, or recovery request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionError {
    /// Input is empty, invalid for its content kind, or requests no lines.
    InvalidInput,
    /// Input exceeds the configured byte limit.
    InputTooLarge,
    /// Compact block metadata or hashes are invalid.
    InvalidBlock,
    /// Recovery range or source reference is invalid.
    InvalidRecovery,
}
impl std::fmt::Display for CompressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CompressionError {}
