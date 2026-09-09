//! Core-owned project quality requirements; execution/evidence stays in the ledger.
use serde::{Deserialize, Serialize};

pub const CONTRACT_ID: &str = "project-quality-contract-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QualityValue {
    Boolean(bool),
    Integer(i64),
    Decimal(f64),
    DurationMs(u64),
    Bytes(u64),
    Percentage(f64),
    Count(u64),
    SeverityRank(u8),
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    FixedThreshold,
    Range,
    BooleanPass,
    PresenceRequired,
    NonRegressionRatchet,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityConstraint {
    pub id: String,
    pub lane_id: String,
    pub metric: String,
    pub comparator: Comparator,
    pub expected: QualityValue,
    pub required: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectQualityContract {
    pub contract_id: String,
    pub revision: u64,
    pub content_hash: String,
    pub status: String,
    pub constraints: Vec<QualityConstraint>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityVerdict {
    Ready,
    ReadyWithExceptions,
    NeedsVerification,
    QualityRegression,
    NeedsPolicyReview,
    Blocked,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityReadinessSnapshot {
    pub target_id: String,
    pub contract_revision: u64,
    pub verdict: QualityVerdict,
    pub failed_constraints: Vec<String>,
    pub missing_constraints: Vec<String>,
}

impl ProjectQualityContract {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.contract_id.trim().is_empty()
            || self.revision == 0
            || self.content_hash.len() != 64
            || self.constraints.len() > 256
        {
            return Err("invalid quality contract");
        }
        for c in &self.constraints {
            if c.id.trim().is_empty() || c.lane_id.trim().is_empty() || c.metric.trim().is_empty() {
                return Err("invalid quality constraint");
            }
        }
        Ok(())
    }
}

pub fn evaluate(
    contract: &ProjectQualityContract,
    target_id: impl Into<String>,
    evidence: &[crate::verification_evidence_ledger::VerificationEvidence],
) -> Result<QualityReadinessSnapshot, &'static str> {
    contract.validate()?;
    let mut missing = Vec::new();
    let mut failed = Vec::new();
    for constraint in &contract.constraints {
        let match_evidence = evidence.iter().find(|item| {
            item.lane_id == constraint.lane_id
                && item.status == crate::verification_evidence_ledger::VerificationStatus::Passed
        });
        if match_evidence.is_none() && constraint.required {
            missing.push(constraint.id.clone());
        }
    }
    let verdict = if !failed.is_empty() {
        QualityVerdict::QualityRegression
    } else if !missing.is_empty() {
        QualityVerdict::NeedsVerification
    } else {
        QualityVerdict::Ready
    };
    Ok(QualityReadinessSnapshot {
        target_id: target_id.into(),
        contract_revision: contract.revision,
        verdict,
        failed_constraints: std::mem::take(&mut failed),
        missing_constraints: missing,
    })
}
