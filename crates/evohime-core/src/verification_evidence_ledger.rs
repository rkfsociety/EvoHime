//! Core-owned verification evidence and readiness contract.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Stable identifier for this verification contract.
pub const CONTRACT_ID: &str = "verification-evidence-ledger-v1";
/// Maximum number of verification lanes evaluated for one target.
pub const MAX_LANES: usize = 128;
/// Maximum number of evidence records retained in one readiness snapshot.
pub const MAX_EVIDENCE: usize = 256;

/// Fingerprint binding verification evidence to a workspace revision and scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceVerificationFingerprint {
    /// Stable workspace root identifier.
    pub root_id: String,
    /// Portion of the workspace covered by this fingerprint.
    pub scope: String,
    /// SHA-256 digest of the normalized content bytes.
    pub content_hash: String,
    /// Normalization format used to produce the digest.
    pub normalization: String,
    /// Whether the fingerprint covers the complete declared scope.
    pub complete: bool,
}

impl WorkspaceVerificationFingerprint {
    /// Creates a `content-v1` fingerprint from raw content bytes.
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
            content_hash: format!("{:x}", hasher.finalize()),
            normalization: "content-v1".into(),
            complete,
        }
    }
    /// Validates required identity fields, normalization, and digest format.
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

/// Outcome reported by a verification lane.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// Verification has been accepted but has not started.
    Queued,
    /// Verification is currently running.
    Running,
    /// Verification completed successfully.
    Passed,
    /// Verification completed and reported a failure.
    Failed,
    /// Verification was cancelled.
    Cancelled,
    /// Verification exceeded its allowed duration.
    TimedOut,
    /// Required verifier or execution resource was unavailable.
    Unavailable,
    /// Verifier violated its communication contract.
    ProtocolError,
    /// Previously collected evidence no longer describes the target revision.
    Invalidated,
    /// Status value is not recognized by this implementation.
    Unknown,
}

impl VerificationStatus {
    /// Returns whether this status represents a successful verification.
    pub fn is_success(self) -> bool {
        self == Self::Passed
    }
}

/// Verifier result with before/after fingerprints and provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationEvidence {
    /// Stable identifier for this evidence record.
    pub evidence_id: String,
    /// Verification lane that produced the evidence.
    pub lane_id: String,
    /// Reported verification outcome.
    pub status: VerificationStatus,
    /// Target fingerprint captured before verification started.
    pub before: WorkspaceVerificationFingerprint,
    /// Optional target fingerprint captured after verification finished.
    pub after: Option<WorkspaceVerificationFingerprint>,
    /// Stable identifier of the verifier implementation.
    pub verifier_id: String,
    /// Version of the verifier that produced the evidence.
    pub verifier_version: String,
    /// Optional reference to the verifier's detailed output artifact.
    pub artifact_ref: Option<String>,
    /// Human-readable explanation or failure reason.
    pub reason: String,
}

impl VerificationEvidence {
    /// Checks evidence bounds and requires matching after-state data for success.
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

/// Aggregate readiness decision derived from required verification evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessVerdict {
    /// All required lanes passed against the current target fingerprint.
    Ready,
    /// A known blocker prevents proceeding.
    Blocked,
    /// One or more required lanes lack current successful evidence.
    NeedsVerification,
    /// Verification results require an explicit human decision.
    NeedsHumanReview,
    /// The available target fingerprint or evidence is incomplete.
    Incomplete,
    /// Readiness could not be determined from supplied information.
    Unknown,
}

/// Marks whether a verification lane is required for readiness.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationRequirement {
    /// Stable verification lane identifier.
    pub lane_id: String,
    /// Whether this lane must pass before readiness is granted.
    pub required: bool,
}

/// Evidence and requirements used to compute readiness for one target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationReadinessSnapshot {
    /// Identifier of the artifact or task being evaluated.
    pub target_id: String,
    /// Fingerprint against which successful evidence must match.
    pub fingerprint: WorkspaceVerificationFingerprint,
    /// Required and optional verification lanes.
    pub requirements: Vec<VerificationRequirement>,
    /// Collected verifier results considered during evaluation.
    pub evidence: Vec<VerificationEvidence>,
    /// Aggregate result of evaluating requirements against the evidence.
    pub verdict: ReadinessVerdict,
}

/// Computes readiness and accepts passes only when before and after fingerprints match.
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

/// Invalid fingerprint/evidence data or a ledger bound violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerError {
    /// Fingerprint identity, normalization, or digest is invalid.
    InvalidFingerprint,
    /// Required evidence identity or fields are invalid.
    InvalidEvidence,
    /// Successful evidence omitted its after-verification fingerprint.
    MissingAfterFingerprint,
    /// Number of lanes or evidence records exceeds its limit.
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
