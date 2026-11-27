//! Core-owned metadata contract for evidence-preserving static-analysis packs.
//! Analyzer execution remains an explicitly registered adapter and is never
//! implied by a pack record.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema version accepted by static-analysis pack records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum rules, language tags, or coverage entries in a pack.
pub const MAX_ITEMS: usize = 256;
/// Maximum text size accepted by this contract.
pub const MAX_TEXT: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enforcement behavior applied to findings from an analysis pack.
pub enum RolloutMode {
    /// Record findings without blocking work.
    Audit,
    /// Block only findings that are new relative to the accepted baseline.
    BaselineNoNew,
    /// Report findings as warnings without enforcing a block.
    Warn,
    /// Block work when enforced findings remain.
    Enforce,
    /// Do not run or apply this analysis pack.
    Disabled,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Provenance classification that constrains pack enforcement.
pub enum TrustState {
    /// Pack is shipped and maintained as a built-in contract.
    BuiltIn,
    /// Pack is managed within the current project.
    ProjectLocal,
    /// Imported pack has been explicitly trusted.
    TrustedImported,
    /// Imported pack has not been trusted for enforcement.
    UntrustedImported,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Completeness result for one analyzer scan.
pub enum CoverageState {
    /// Every eligible file was scanned successfully.
    Complete,
    /// Some eligible files were skipped or failed.
    Partial,
    /// The requested scope is unsupported by this analyzer.
    UnsupportedScope,
    /// The analyzer failed to produce a usable result.
    Failed,
    /// Coverage completeness is unavailable.
    Unknown,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Lifecycle classification of a finding relative to its baseline.
pub enum FindingState {
    /// Finding is newly introduced in the current scan.
    New,
    /// Finding was already present in the accepted baseline.
    Existing,
    /// Previously accepted finding is absent from the current scan.
    Resolved,
    /// Finding is suppressed by an explicit project decision.
    Suppressed,
    /// Finding is accepted as part of the current baseline.
    AcceptedBaseline,
    /// Finding requires review before rollout can proceed.
    NeedsReview,
    /// Finding refers to an outdated rule or scan context.
    Stale,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Confidence and maturity category assigned to a rule.
pub enum EvidenceClass {
    /// Rule is based on strong, repeatable evidence.
    HighConfidence,
    /// Rule encodes a project-specific engineering preference.
    ProjectOpinionated,
    /// Rule has limited validation and should be rolled out cautiously.
    Experimental,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Versioned detector and reporting contract for one analysis rule.
pub struct RuleDefinition {
    /// Stable identifier of this rule, pack, or finding.
    pub id: String,
    /// Positive revision of the referenced rule or pack.
    pub revision: u64,
    /// Classification used to group rules or findings.
    pub category: String,
    /// Programming languages supported by the rule or pack.
    pub languages: Vec<String>,
    /// Human-readable explanation of the rule.
    pub description: String,
    /// Evidence and confidence category for the rule.
    pub evidence_class: EvidenceClass,
    /// Severity assigned when the rule reports a finding.
    pub default_severity: String,
    /// Default rollout policy for this rule or pack.
    pub default_mode: RolloutMode,
    /// Reference to an explicitly registered analyzer detector.
    pub detector_ref: String,
    /// Integrity hash of canonical serialized content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Trusted, versioned collection of static-analysis rules.
pub struct AnalysisPack {
    /// Serialized schema version supported by this contract.
    pub schema_version: u32,
    /// Stable identifier of this rule, pack, or finding.
    pub id: String,
    /// Positive revision of the referenced rule or pack.
    pub revision: u64,
    /// Human-readable pack name.
    pub display_name: String,
    /// Programming languages supported by the rule or pack.
    pub languages: Vec<String>,
    /// Versioned rule definitions contained in this pack.
    pub rules: Vec<RuleDefinition>,
    /// Provenance trust assigned to the pack.
    pub trust_state: TrustState,
    /// Default rollout policy for this rule or pack.
    pub default_mode: RolloutMode,
    /// Registered analyzer responsible for evaluating the pack.
    pub analyzer_ref: String,
    /// Integrity hash of canonical serialized content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Counts and exclusions describing what a scan actually covered.
pub struct AnalysisCoverage {
    /// Stable privacy-safe identity of the scanned workspace.
    pub workspace_fingerprint: String,
    /// Identifier of the analysis pack used for this result.
    pub pack_id: String,
    /// Files discovered within the workspace scope.
    pub discovered_files: u32,
    /// Discovered files eligible for the analyzer.
    pub eligible_files: u32,
    /// Eligible files actually scanned.
    pub scanned_files: u32,
    /// Files skipped because the analyzer does not support them.
    pub unsupported_files: Vec<String>,
    /// Files the analyzer could not parse.
    pub parse_failures: Vec<String>,
    /// Files or checks that failed during analyzer execution.
    pub analyzer_failures: Vec<String>,
    /// Generated files excluded from the scan.
    pub excluded_generated: Vec<String>,
    /// Third-party vendor files excluded from the scan.
    pub excluded_vendor: Vec<String>,
    /// Aggregate coverage or finding lifecycle state.
    pub state: CoverageState,
    /// Observation time as Unix epoch milliseconds.
    pub observed_at_ms: i64,
    /// Integrity hash of canonical serialized content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Validated, integrity-protected finding from an analysis pack.
pub struct AnalysisFinding {
    /// Stable identifier of this rule, pack, or finding.
    pub id: String,
    /// Identifier of the analysis pack used for this result.
    pub pack_id: String,
    /// Identifier of the rule that produced the finding.
    pub rule_id: String,
    /// Revision of the rule used to produce the finding.
    pub rule_revision: u64,
    /// Registered analyzer responsible for evaluating the pack.
    pub analyzer_ref: String,
    /// Stable privacy-safe identity of the scanned workspace.
    pub workspace_fingerprint: String,
    /// Portable reference to the file containing the finding.
    pub file_ref: String,
    /// Stable content-derived identity used to compare findings across scans.
    pub fingerprint: String,
    /// Classification used to group rules or findings.
    pub category: String,
    /// Rule severity assigned to the finding.
    pub severity: String,
    /// Evidence-based explanation of the finding.
    pub explanation: String,
    /// Aggregate coverage or finding lifecycle state.
    pub state: FindingState,
    /// Integrity hash of canonical serialized content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Accepted finding set used as the comparison point for later scans.
pub struct Baseline {
    /// Stable identifier of this rule, pack, or finding.
    pub id: String,
    /// Identifier of the analysis pack used for this result.
    pub pack_id: String,
    /// Pack revision associated with this baseline.
    pub pack_revision: u64,
    /// Registered analyzer responsible for evaluating the pack.
    pub analyzer_ref: String,
    /// Integrity hash of the rule configuration used by the scan.
    pub rule_config_hash: String,
    /// Stable privacy-safe identity of the scanned workspace.
    pub workspace_fingerprint: String,
    /// Finding fingerprints accepted in this baseline.
    pub accepted_findings: Vec<String>,
    /// Time the baseline was accepted as Unix epoch milliseconds.
    pub accepted_at_ms: i64,
    /// Identity that approved the baseline.
    pub accepted_by: String,
    /// Integrity hash of canonical serialized content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Summary of observed findings and coverage for pack adoption.
pub struct AdoptionReport {
    /// Identifier of the analysis pack used for this result.
    pub pack_id: String,
    /// Stable privacy-safe identity of the scanned workspace.
    pub workspace_fingerprint: String,
    /// Number of files actually scanned during adoption.
    pub files_scanned: u32,
    /// Total findings produced during adoption.
    pub findings_total: u32,
    /// Finding counts grouped by rule identifier.
    pub findings_by_rule: Vec<(String, u32)>,
    /// Coverage completeness achieved by the scan.
    pub coverage_state: CoverageState,
    /// Rollout mode suggested from the adoption evidence.
    pub recommended_rollout: RolloutMode,
    /// Integrity hash of canonical serialized content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Finding changes between an accepted baseline and a current scan.
pub struct AnalysisDelta {
    /// Baseline identifier used for comparison.
    pub baseline_id: String,
    /// Identifier of the scan being compared.
    pub current_scan_id: String,
    /// Findings present now but absent from the baseline.
    pub introduced: Vec<String>,
    /// Baseline findings absent from the current scan.
    pub resolved: Vec<String>,
    /// Findings present in both baseline and current scan.
    pub persisting: Vec<String>,
    /// Findings excluded because their source rule or context is outdated.
    pub stale: Vec<String>,
    /// Integrity hash of canonical serialized content.
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
/// Invalid contract data, exceeded limit, or unavailable analyzer adapter.
pub enum AnalysisError {
    /// Pack, rule, coverage, or finding value is invalid.
    #[error("invalid static analysis contract: {0}")]
    Invalid(String),
    /// A configured text or collection bound was exceeded.
    #[error("static analysis contract limit exceeded: {0}")]
    Limit(String),
    /// The referenced analyzer adapter is not registered.
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
/// Validates pack identity, rules, trust constraints, bounds, and content hash.
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
/// Validates scan counts, coverage metadata, and content hash.
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
/// Validates finding metadata and its integrity hash.
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
/// Returns a conservative rollout outcome from mode, coverage, and findings.
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
