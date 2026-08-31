//! Deterministic, offline trust gate for discovered Agent Skills.
//!
//! Skill text is untrusted data. This module never executes package files and
//! exposes only bounded, redacted findings to callers.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub const SCANNER_VERSION: &str = "skill-scanner-v1";
pub const REVIEW_POLICY_VERSION: &str = "skill-review-policy-v1";
pub const MAX_PACKAGE_FILES: usize = 128;
pub const MAX_FINDINGS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    Low,
    Medium,
    High,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustDecision {
    Scanning,
    ReviewRequired,
    Reviewing,
    Trusted,
    Quarantined,
    Rejected,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFinding {
    pub code: String,
    pub severity: FindingSeverity,
    pub relative_location: String,
    pub masked_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillTrustRecord {
    pub skill_id: String,
    pub content_hash: String,
    pub scanner_version: String,
    pub review_policy_version: String,
    pub findings: Vec<SkillFinding>,
    pub risk_class: RiskClass,
    pub decision: TrustDecision,
    pub override_actor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewRecommendation {
    Trusted,
    ReviewRequired,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillReviewReport {
    pub risk_class: RiskClass,
    pub findings: Vec<SkillFinding>,
    pub recommendation: ReviewRecommendation,
    pub rationale_summary: String,
}

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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillTrustError {
    #[error("skill trust decision is not executable: {0:?}")]
    NotExecutable(TrustDecision),
    #[error("skill content hash changed during trust check")]
    HashMismatch,
    #[error("skill trust package is too large")]
    PackageTooLarge,
    #[error("skill trust package could not be read: {0}")]
    Io(String),
}

impl SkillTrustRecord {
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
        let bytes = fs::read(package_dir.join(&relative))
            .map_err(|e| SkillTrustError::Io(e.to_string()))?;
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
