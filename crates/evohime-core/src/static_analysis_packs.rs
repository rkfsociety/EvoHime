//! Core-owned metadata contract for evidence-preserving static-analysis packs.
//! Analyzer execution remains an explicitly registered adapter and is never
//! implied by a pack record.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ITEMS: usize = 256;
pub const MAX_TEXT: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RolloutMode {
    Audit,
    BaselineNoNew,
    Warn,
    Enforce,
    Disabled,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    BuiltIn,
    ProjectLocal,
    TrustedImported,
    UntrustedImported,
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
pub enum FindingState {
    New,
    Existing,
    Resolved,
    Suppressed,
    AcceptedBaseline,
    NeedsReview,
    Stale,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    HighConfidence,
    ProjectOpinionated,
    Experimental,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleDefinition {
    pub id: String,
    pub revision: u64,
    pub category: String,
    pub languages: Vec<String>,
    pub description: String,
    pub evidence_class: EvidenceClass,
    pub default_severity: String,
    pub default_mode: RolloutMode,
    pub detector_ref: String,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisPack {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub display_name: String,
    pub languages: Vec<String>,
    pub rules: Vec<RuleDefinition>,
    pub trust_state: TrustState,
    pub default_mode: RolloutMode,
    pub analyzer_ref: String,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisCoverage {
    pub workspace_fingerprint: String,
    pub pack_id: String,
    pub discovered_files: u32,
    pub eligible_files: u32,
    pub scanned_files: u32,
    pub unsupported_files: Vec<String>,
    pub parse_failures: Vec<String>,
    pub analyzer_failures: Vec<String>,
    pub excluded_generated: Vec<String>,
    pub excluded_vendor: Vec<String>,
    pub state: CoverageState,
    pub observed_at_ms: i64,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisFinding {
    pub id: String,
    pub pack_id: String,
    pub rule_id: String,
    pub rule_revision: u64,
    pub analyzer_ref: String,
    pub workspace_fingerprint: String,
    pub file_ref: String,
    pub fingerprint: String,
    pub category: String,
    pub severity: String,
    pub explanation: String,
    pub state: FindingState,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Baseline {
    pub id: String,
    pub pack_id: String,
    pub pack_revision: u64,
    pub analyzer_ref: String,
    pub rule_config_hash: String,
    pub workspace_fingerprint: String,
    pub accepted_findings: Vec<String>,
    pub accepted_at_ms: i64,
    pub accepted_by: String,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdoptionReport {
    pub pack_id: String,
    pub workspace_fingerprint: String,
    pub files_scanned: u32,
    pub findings_total: u32,
    pub findings_by_rule: Vec<(String, u32)>,
    pub coverage_state: CoverageState,
    pub recommended_rollout: RolloutMode,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisDelta {
    pub baseline_id: String,
    pub current_scan_id: String,
    pub introduced: Vec<String>,
    pub resolved: Vec<String>,
    pub persisting: Vec<String>,
    pub stale: Vec<String>,
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnalysisError {
    #[error("invalid static analysis contract: {0}")]
    Invalid(String),
    #[error("static analysis contract limit exceeded: {0}")]
    Limit(String),
    #[error("static analysis adapter unavailable")]
    AdapterUnavailable,
}
fn hash<T: Serialize>(value: &T) -> Result<String, AnalysisError> {
    serde_json::to_vec(value)
        .map(|v| hex::encode(Sha256::digest(v)))
        .map_err(|e| AnalysisError::Invalid(e.to_string()))
}
fn bounded_text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_TEXT
        && !value.bytes().any(|b| b.is_ascii_control())
}
pub fn validate_pack(pack: &AnalysisPack) -> Result<(), AnalysisError> {
    if pack.schema_version != SCHEMA_VERSION
        || !bounded_text(&pack.id)
        || pack.revision == 0
        || !bounded_text(&pack.display_name)
        || !bounded_text(&pack.analyzer_ref)
        || pack.rules.is_empty()
        || pack.rules.len() > MAX_ITEMS
        || pack.languages.len() > MAX_ITEMS
    {
        return Err(AnalysisError::Invalid("pack identity".into()));
    }
    if pack.trust_state == TrustState::UntrustedImported
        && pack.default_mode == RolloutMode::Enforce
    {
        return Err(AnalysisError::Invalid(
            "untrusted pack cannot enforce".into(),
        ));
    }
    if pack.rules.iter().any(|r| {
        !bounded_text(&r.id)
            || r.revision == 0
            || !bounded_text(&r.category)
            || !bounded_text(&r.detector_ref)
            || !bounded_text(&r.description)
    }) {
        return Err(AnalysisError::Invalid("rule identity".into()));
    }
    let mut copy = pack.clone();
    copy.content_hash.clear();
    if pack.content_hash != hash(&copy)? {
        return Err(AnalysisError::Invalid("pack content_hash".into()));
    }
    Ok(())
}
pub fn validate_coverage(coverage: &AnalysisCoverage) -> Result<(), AnalysisError> {
    if coverage.pack_id.trim().is_empty()
        || coverage.workspace_fingerprint.trim().is_empty()
        || coverage.scanned_files > coverage.eligible_files
        || coverage.eligible_files > coverage.discovered_files
        || coverage.observed_at_ms <= 0
        || coverage.unsupported_files.len() > MAX_ITEMS
        || coverage.parse_failures.len() > MAX_ITEMS
        || coverage.analyzer_failures.len() > MAX_ITEMS
    {
        return Err(AnalysisError::Invalid("coverage bounds".into()));
    }
    let mut copy = coverage.clone();
    copy.content_hash.clear();
    if coverage.content_hash != hash(&copy)? {
        return Err(AnalysisError::Invalid("coverage content_hash".into()));
    }
    Ok(())
}
pub fn validate_finding(finding: &AnalysisFinding) -> Result<(), AnalysisError> {
    if [
        &finding.id,
        &finding.pack_id,
        &finding.rule_id,
        &finding.analyzer_ref,
        &finding.workspace_fingerprint,
        &finding.file_ref,
        &finding.fingerprint,
        &finding.category,
        &finding.severity,
    ]
    .iter()
    .any(|v| !bounded_text(v))
        || !bounded_text(&finding.explanation)
    {
        return Err(AnalysisError::Invalid("finding metadata".into()));
    }
    let mut copy = finding.clone();
    copy.content_hash.clear();
    if finding.content_hash != hash(&copy)? {
        return Err(AnalysisError::Invalid("finding content_hash".into()));
    }
    Ok(())
}
pub fn safe_outcome(
    mode: RolloutMode,
    coverage: CoverageState,
    findings: &[AnalysisFinding],
) -> &'static str {
    if mode == RolloutMode::Disabled {
        return "disabled";
    }
    if !matches!(coverage, CoverageState::Complete) {
        return "incomplete";
    }
    if findings.iter().any(|f| {
        matches!(f.state, FindingState::New | FindingState::NeedsReview)
            && (f.severity == "error" || f.severity == "critical")
    }) {
        return "blocked";
    }
    if mode == RolloutMode::Audit || mode == RolloutMode::Warn || mode == RolloutMode::BaselineNoNew
    {
        "observed"
    } else {
        "passed"
    }
}
