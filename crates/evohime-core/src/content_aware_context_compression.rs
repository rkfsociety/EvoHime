//! Deterministic, metadata-first compaction for bounded context items.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CONTRACT_ID: &str = "content-aware-context-compression-v1";
pub const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    PlainText,
    SourceCode,
    UnifiedDiff,
    Json,
    JsonLines,
    Yaml,
    CsvTsv,
    BuildLog,
    TestOutput,
    Diagnostics,
    SearchResults,
    HtmlText,
    AccessibilityTree,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LossClass {
    LosslessReencoding,
    StructurePreservingElision,
    EvidencePreservingProjection,
    SemanticSummary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenefitDecision {
    Compress,
    NoBenefit,
    Protected,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmittedRegion {
    pub start: u32,
    pub end: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactContextBlock {
    pub source_ref: String,
    pub source_hash: String,
    pub compact_hash: String,
    pub kind: ContentKind,
    pub loss: LossClass,
    pub body: String,
    pub omitted: Vec<OmittedRegion>,
    pub incomplete: bool,
    pub decision: BenefitDecision,
}

impl CompactContextBlock {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStrategy {
    ExactRegion,
    AroundLocator,
    StructuredPath,
    LineRange,
    NextPage,
    ExpandGroup,
    OriginalBounded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoverContextSlice {
    pub source_ref: String,
    pub source_hash: String,
    pub strategy: RecoveryStrategy,
    pub start: u32,
    pub end: u32,
    pub max_bytes: u32,
}

impl RecoverContextSlice {
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
    let source_hash = format!("{hasher:x}");
    let mut compact_hasher = Sha256::new();
    compact_hasher.update(body.as_bytes());
    let compact_hash = format!("{compact_hasher:x}");
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionError {
    InvalidInput,
    InputTooLarge,
    InvalidBlock,
    InvalidRecovery,
}
impl std::fmt::Display for CompressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CompressionError {}
