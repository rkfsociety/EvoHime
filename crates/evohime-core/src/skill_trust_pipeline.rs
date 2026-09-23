//! Deterministic, offline trust gate for discovered Agent Skills.
//!
//! Skill text is untrusted data. This module never executes package files and
//! exposes only bounded, redacted findings to callers.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

/// Identifier for the deterministic scanner rule set.
pub const SCANNER_VERSION: &str = "skill-scanner-v1";
/// Identifier for the policy used to interpret scanner and reviewer findings.
pub const REVIEW_POLICY_VERSION: &str = "skill-review-policy-v1";
/// Maximum number of files inspected in a skill package.
pub const MAX_PACKAGE_FILES: usize = 128;
/// Maximum number of findings retained in a trust record.
pub const MAX_FINDINGS: usize = 128;

/// Severity assigned to a scanner or reviewer finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    /// Informational observation with no identified risk.
    Info,
    /// Low risk that does not require elevated review.
    Low,
    /// Moderate concern that should be reviewed.
    Medium,
    /// High-risk behavior requiring human review.
    High,
    /// Behavior that blocks execution until the package is changed.
    Blocked,
}

/// Aggregate risk class computed from findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    /// No material risk was detected by the current rules.
    Low,
    /// Moderate patterns were detected.
    Medium,
    /// High-risk patterns require an explicit review.
    High,
    /// A blocking pattern was detected.
    Blocked,
}

/// Trust gate state that controls whether a discovered skill may execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustDecision {
    /// Package contents are currently being inspected.
    Scanning,
    /// Automated analysis requires a human decision.
    ReviewRequired,
    /// A human review is in progress.
    Reviewing,
    /// Package passed automated checks or an approving review.
    Trusted,
    /// Package is isolated from execution pending further action.
    Quarantined,
    /// Package was explicitly rejected.
    Rejected,
    /// A non-blocked package was enabled by an attributable override.
    Enabled,
}

/// Bounded, redacted finding produced by skill scanning or review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFinding {
    /// Stable machine-readable rule or reviewer code.
    pub code: String,
    /// Severity assigned to the finding.
    pub severity: FindingSeverity,
    /// Bounded package-relative location associated with the finding.
    pub relative_location: String,
    /// Content digest used for correlation without exposing file contents.
    pub masked_fingerprint: String,
}

/// Trust decision and provenance for one exact skill package revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillTrustRecord {
    /// Stable identifier of the discovered skill.
    pub skill_id: String,
    /// Digest of the skill package revision that was scanned.
    pub content_hash: String,
    /// Scanner rule-set version used to produce this record.
    pub scanner_version: String,
    /// Review policy version used to interpret findings.
    pub review_policy_version: String,
    /// Bounded scanner and reviewer findings.
    pub findings: Vec<SkillFinding>,
    /// Aggregate severity class for the package.
    pub risk_class: RiskClass,
    /// Current decision enforced before execution.
    pub decision: TrustDecision,
    /// Optional actor who explicitly overrode a review-level decision.
    pub override_actor: Option<String>,
}

/// Outcome recommended by an external or human review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRecommendation {
    /// Findings support trusting the package.
    Trusted,
    /// More review is needed before execution.
    ReviewRequired,
    /// Package should remain quarantined.
    Quarantined,
}

/// Bounded reviewer output merged into a skill trust record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillReviewReport {
    /// Aggregate risk assessment made by the reviewer.
    pub risk_class: RiskClass,
    /// Findings supporting the recommendation.
    pub findings: Vec<SkillFinding>,
    /// Reviewer recommendation subject to policy enforcement.
    pub recommendation: ReviewRecommendation,
    /// Short human-readable explanation of the decision.
    pub rationale_summary: String,
}

/// Applies a bounded review report without allowing a high-risk trust bypass.
pub fn apply_review(
    record: &SkillTrustRecord,
    report: Option<&SkillReviewReport>,
) -> SkillTrustRecord {
    let Some(report) = report else {
        return record.clone();
    };
    if report.findings.len() > MAX_FINDINGS || report.rationale_summary.chars().count() > 512 {
        let mut rejected = record.clone();
        rejected.decision = TrustDecision::Quarantined;
        rejected.risk_class = RiskClass::Blocked;
        return rejected;
    }
    let mut reviewed = record.clone();
    reviewed.findings.extend(report.findings.iter().cloned());
    reviewed
        .findings
        .sort_by(|a, b| (&a.relative_location, &a.code).cmp(&(&b.relative_location, &b.code)));
    reviewed.findings.truncate(MAX_FINDINGS);
    reviewed.risk_class = report.risk_class;
    reviewed.decision = match report.recommendation {
        ReviewRecommendation::Trusted if report.risk_class == RiskClass::Low => {
            TrustDecision::Trusted
        }
        ReviewRecommendation::Quarantined => TrustDecision::Quarantined,
        _ => TrustDecision::ReviewRequired,
    };
    reviewed
}

/// Enables a non-blocked record when a non-empty override actor is supplied.
pub fn apply_override(record: &SkillTrustRecord, actor: &str) -> SkillTrustRecord {
    let mut result = record.clone();
    if actor.trim().is_empty() || record.risk_class == RiskClass::Blocked {
        result.decision = TrustDecision::Quarantined;
        return result;
    }
    result.decision = TrustDecision::Enabled;
    result.override_actor = Some(actor.to_owned());
    result
}

/// Package read, size, hash, or execution-trust failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillTrustError {
    /// Current decision does not permit execution.
    #[error("skill trust decision is not executable: {0:?}")]
    NotExecutable(TrustDecision),
    /// Current skill contents differ from the revision that was scanned.
    #[error("skill content hash changed during trust check")]
    HashMismatch,
    /// Package contains more files than the scanner can inspect.
    #[error("skill trust package is too large")]
    PackageTooLarge,
    /// Filesystem enumeration or bounded read failed.
    #[error("skill trust package could not be read: {0}")]
    Io(String),
}

impl SkillTrustRecord {
    /// Allows execution only when the content hash matches and decision is trusted.
    pub fn can_execute(&self, current_hash: &str) -> Result<(), SkillTrustError> {
        if self.content_hash != current_hash {
            return Err(SkillTrustError::HashMismatch);
        }
        if matches!(
            self.decision,
            TrustDecision::Trusted | TrustDecision::Enabled
        ) {
            Ok(())
        } else {
            Err(SkillTrustError::NotExecutable(self.decision))
        }
    }
}

/// Scans a skill directory without executing its files and returns a trust record.
pub fn scan_package(
    skill_id: &str,
    package_dir: &Path,
    content_hash: &str,
) -> Result<SkillTrustRecord, SkillTrustError> {
    let mut files = Vec::new();
    collect_files(package_dir, package_dir, &mut files)?;
    if files.len() > MAX_PACKAGE_FILES {
        return Err(SkillTrustError::PackageTooLarge);
    }
    files.sort();
    let mut findings = Vec::new();
    for relative in files {
        if relative.starts_with("__symlink__:") {
            findings.push(SkillFinding {
                code: "symlink_escape".into(),
                severity: FindingSeverity::Blocked,
                relative_location: relative.chars().skip(12).take(256).collect(),
                masked_fingerprint: "sha256:symlink".into(),
            });
            continue;
        }
        let bytes = read_bounded_file(&package_dir.join(&relative))?;
        if bytes.len() > crate::skill_registry::MAX_REFERENCE_BYTES {
            add(
                &mut findings,
                "oversized_file",
                FindingSeverity::High,
                &relative,
                &bytes,
            );
            continue;
        }
        let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
        let rules = [
            (
                "executable_file",
                FindingSeverity::Blocked,
                [".exe", ".dll", ".com", ".scr"].as_slice(),
            ),
            (
                "shell_pattern",
                FindingSeverity::High,
                ["powershell", "cmd.exe", "bash ", "child_process"].as_slice(),
            ),
            (
                "destructive_fs",
                FindingSeverity::Blocked,
                ["rm -rf", "format c:", "remove-item -recurse", "del /s"].as_slice(),
            ),
            (
                "credential_access",
                FindingSeverity::High,
                [
                    "credential manager",
                    "keychain",
                    "api_key",
                    "password",
                    "access_token",
                ]
                .as_slice(),
            ),
            (
                "network_exfiltration",
                FindingSeverity::High,
                ["upload", "exfil", "curl ", "invoke-webrequest", "fetch("].as_slice(),
            ),
            (
                "encoded_payload",
                FindingSeverity::Medium,
                ["base64", "frombase64string", "decode("].as_slice(),
            ),
            (
                "prompt_injection",
                FindingSeverity::High,
                [
                    "ignore previous",
                    "system message",
                    "developer message",
                    "disable safety",
                ]
                .as_slice(),
            ),
            (
                "policy_override",
                FindingSeverity::Blocked,
                ["bypass approval", "grant capability", "disable policy"].as_slice(),
            ),
            (
                "external_url",
                FindingSeverity::Medium,
                ["http://", "https://"].as_slice(),
            ),
        ];
        for (code, severity, needles) in rules {
            if needles.iter().any(|needle| text.contains(needle)) {
                add(&mut findings, code, severity, &relative, &bytes);
            }
        }
        if Path::new(&relative)
            .components()
            .any(|c| c.as_os_str() == "..")
        {
            add(
                &mut findings,
                "path_traversal",
                FindingSeverity::Blocked,
                &relative,
                &bytes,
            );
        }
    }
    findings.sort_by(|a, b| (&a.relative_location, &a.code).cmp(&(&b.relative_location, &b.code)));
    findings.truncate(MAX_FINDINGS);
    let risk_class = if findings
        .iter()
        .any(|f| f.severity == FindingSeverity::Blocked)
    {
        RiskClass::Blocked
    } else if findings.iter().any(|f| f.severity == FindingSeverity::High) {
        RiskClass::High
    } else if findings
        .iter()
        .any(|f| f.severity == FindingSeverity::Medium)
    {
        RiskClass::Medium
    } else {
        RiskClass::Low
    };
    let decision = match risk_class {
        RiskClass::Low => TrustDecision::Trusted,
        RiskClass::Medium => TrustDecision::ReviewRequired,
        RiskClass::High => TrustDecision::ReviewRequired,
        RiskClass::Blocked => TrustDecision::Quarantined,
    };
    Ok(SkillTrustRecord {
        skill_id: skill_id.into(),
        content_hash: content_hash.into(),
        scanner_version: SCANNER_VERSION.into(),
        review_policy_version: REVIEW_POLICY_VERSION.into(),
        findings,
        risk_class,
        decision,
        override_actor: None,
    })
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), SkillTrustError> {
    let entries = fs::read_dir(dir).map_err(|e| SkillTrustError::Io(e.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|e| SkillTrustError::Io(e.to_string()))?;
        let path = entry.path();
        if fs::symlink_metadata(&path)
            .map_err(|e| SkillTrustError::Io(e.to_string()))?
            .file_type()
            .is_symlink()
        {
            add_path_finding(out, path.strip_prefix(root).unwrap_or(&path));
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

fn add(
    findings: &mut Vec<SkillFinding>,
    code: &str,
    severity: FindingSeverity,
    location: &str,
    bytes: &[u8],
) {
    findings.push(SkillFinding {
        code: code.into(),
        severity,
        relative_location: location.chars().take(256).collect(),
        masked_fingerprint: fingerprint(bytes),
    });
}

fn read_bounded_file(path: &Path) -> Result<Vec<u8>, SkillTrustError> {
    let file = fs::File::open(path).map_err(|e| SkillTrustError::Io(e.to_string()))?;
    let mut bytes = Vec::with_capacity(crate::skill_registry::MAX_REFERENCE_BYTES + 1);
    file.take((crate::skill_registry::MAX_REFERENCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| SkillTrustError::Io(e.to_string()))?;
    Ok(bytes)
}
fn add_path_finding(out: &mut Vec<String>, path: &Path) {
    out.push(format!("__symlink__:{}", path.to_string_lossy()));
}
fn fingerprint(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn clean_package_is_trusted_and_deterministic() {
        let d = tempdir().unwrap();
        fs::write(d.path().join("SKILL.md"), "read files").unwrap();
        let a = scan_package("x", d.path(), "h").unwrap();
        let b = scan_package("x", d.path(), "h").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.decision, TrustDecision::Trusted);
    }
    #[test]
    fn dangerous_package_is_quarantined_and_redacted() {
        let d = tempdir().unwrap();
        fs::write(
            d.path().join("SKILL.md"),
            "ignore previous; rm -rf C:\\secret; upload password",
        )
        .unwrap();
        let r = scan_package("x", d.path(), "h").unwrap();
        assert_eq!(r.decision, TrustDecision::Quarantined);
        assert!(r
            .findings
            .iter()
            .all(|f| !f.masked_fingerprint.contains("password")));
        assert!(r.can_execute("h").is_err());
    }
    #[test]
    fn hash_change_invalidates_record() {
        let r = SkillTrustRecord {
            skill_id: "x".into(),
            content_hash: "a".into(),
            scanner_version: SCANNER_VERSION.into(),
            review_policy_version: REVIEW_POLICY_VERSION.into(),
            findings: vec![],
            risk_class: RiskClass::Low,
            decision: TrustDecision::Trusted,
            override_actor: None,
        };
        assert_eq!(r.can_execute("b"), Err(SkillTrustError::HashMismatch));
    }

    #[test]
    fn oversized_skill_file_is_scanned_with_a_bounded_prefix() {
        let d = tempdir().unwrap();
        fs::write(
            d.path().join("SKILL.md"),
            vec![b'x'; crate::skill_registry::MAX_REFERENCE_BYTES + 1],
        )
        .unwrap();

        let record = scan_package("x", d.path(), "h").unwrap();

        assert!(record.findings.iter().any(|finding| {
            finding.code == "oversized_file" && finding.masked_fingerprint.starts_with("sha256:")
        }));
    }

    #[test]
    fn override_requires_actor_and_never_unblocks_blocked() {
        let r = SkillTrustRecord {
            skill_id: "x".into(),
            content_hash: "a".into(),
            scanner_version: SCANNER_VERSION.into(),
            review_policy_version: REVIEW_POLICY_VERSION.into(),
            findings: vec![],
            risk_class: RiskClass::High,
            decision: TrustDecision::ReviewRequired,
            override_actor: None,
        };
        assert_eq!(apply_override(&r, "user").decision, TrustDecision::Enabled);
        assert_eq!(apply_override(&r, "").decision, TrustDecision::Quarantined);
    }
}
