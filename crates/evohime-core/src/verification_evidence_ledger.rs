//! Core-owned verification evidence and readiness contract.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CONTRACT_ID: &str = "verification-evidence-ledger-v1";
pub const MAX_LANES: usize = 128;
pub const MAX_EVIDENCE: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceVerificationFingerprint {
    pub root_id: String,
    pub scope: String,
    pub content_hash: String,
    pub normalization: String,
    pub complete: bool,
}

impl WorkspaceVerificationFingerprint {
    pub fn from_content(
        root_id: impl Into<String>,
        scope: impl Into<String>,
        content: &[u8],
        complete: bool,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content);
        Self {
            root_id: root_id.into(),
            scope: scope.into(),
            content_hash: format!("{hasher:x}"),
            normalization: "content-v1".into(),
            complete,
        }
    }
    pub fn validate(&self) -> Result<(), LedgerError> {
        if self.root_id.trim().is_empty()
            || self.scope.trim().is_empty()
            || self.normalization != "content-v1"
            || self.content_hash.len() != 64
            || !self.content_hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(LedgerError::InvalidFingerprint);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Cancelled,
    TimedOut,
    Unavailable,
    ProtocolError,
    Invalidated,
    Unknown,
}

impl VerificationStatus {
    pub fn is_success(self) -> bool {
        self == Self::Passed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationEvidence {
    pub evidence_id: String,
    pub lane_id: String,
    pub status: VerificationStatus,
    pub before: WorkspaceVerificationFingerprint,
    pub after: Option<WorkspaceVerificationFingerprint>,
    pub verifier_id: String,
    pub verifier_version: String,
    pub artifact_ref: Option<String>,
    pub reason: String,
}

impl VerificationEvidence {
    pub fn validate(&self) -> Result<(), LedgerError> {
        for value in [
            &self.evidence_id,
            &self.lane_id,
            &self.verifier_id,
            &self.verifier_version,
            &self.reason,
        ] {
            if value.trim().is_empty() || value.len() > 256 {
                return Err(LedgerError::InvalidEvidence);
            }
        }
        self.before.validate()?;
        if let Some(after) = &self.after {
            after.validate()?;
        }
        if self.status.is_success() && self.after.is_none() {
            return Err(LedgerError::MissingAfterFingerprint);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessVerdict {
    Ready,
    Blocked,
    NeedsVerification,
    NeedsHumanReview,
    Incomplete,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationRequirement {
    pub lane_id: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationReadinessSnapshot {
    pub target_id: String,
    pub fingerprint: WorkspaceVerificationFingerprint,
    pub requirements: Vec<VerificationRequirement>,
    pub evidence: Vec<VerificationEvidence>,
    pub verdict: ReadinessVerdict,
}

pub fn evaluate_readiness(
    target_id: impl Into<String>,
    fingerprint: WorkspaceVerificationFingerprint,
    requirements: Vec<VerificationRequirement>,
    evidence: Vec<VerificationEvidence>,
) -> Result<VerificationReadinessSnapshot, LedgerError> {
    fingerprint.validate()?;
    if requirements.len() > MAX_LANES || evidence.len() > MAX_EVIDENCE {
        return Err(LedgerError::LimitExceeded);
    }
    for item in &evidence {
        item.validate()?;
    }
    let verdict = if requirements.iter().any(|req| {
        req.required
            && !evidence.iter().any(|e| {
                e.lane_id == req.lane_id
                    && e.status == VerificationStatus::Passed
                    && e.before.content_hash == fingerprint.content_hash
                    && e.after
                        .as_ref()
                        .is_some_and(|after| after.content_hash == fingerprint.content_hash)
            })
    }) {
        ReadinessVerdict::NeedsVerification
    } else {
        ReadinessVerdict::Ready
    };
    Ok(VerificationReadinessSnapshot {
        target_id: target_id.into(),
        fingerprint,
        requirements,
        evidence,
        verdict,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerError {
    InvalidFingerprint,
    InvalidEvidence,
    MissingAfterFingerprint,
    LimitExceeded,
}
impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LedgerError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_evidence_never_ready() {
        let fp = WorkspaceVerificationFingerprint::from_content("root", "scope", b"x", true);
        let snapshot = evaluate_readiness(
            "task",
            fp.clone(),
            vec![VerificationRequirement {
                lane_id: "unit".into(),
                required: true,
            }],
            vec![VerificationEvidence {
                evidence_id: "e".into(),
                lane_id: "unit".into(),
                status: VerificationStatus::Failed,
                before: fp,
                after: None,
                verifier_id: "v".into(),
                verifier_version: "1".into(),
                artifact_ref: None,
                reason: "failed".into(),
            }],
        )
        .unwrap();
        assert_eq!(snapshot.verdict, ReadinessVerdict::NeedsVerification);
    }
}
