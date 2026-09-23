//! Core-owned project quality requirements; execution/evidence stays in the ledger.
use serde::{Deserialize, Serialize};

/// Stable identifier written into project quality contracts.
pub const CONTRACT_ID: &str = "project-quality-contract-v1";

/// Typed expected value for a project quality metric.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QualityValue {
    /// Expected boolean state.
    Boolean(bool),
    /// Expected signed integer.
    Integer(i64),
    /// Expected decimal value.
    Decimal(f64),
    /// Expected duration in milliseconds.
    DurationMs(u64),
    /// Expected byte count.
    Bytes(u64),
    /// Expected percentage value.
    Percentage(f64),
    /// Expected unsigned count.
    Count(u64),
    /// Expected severity rank, where the meaning of each rank is metric-specific.
    SeverityRank(u8),
}
/// Comparison policy declared for a quality constraint.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    /// Compare an observed value with one fixed threshold.
    FixedThreshold,
    /// Require an observed value to fall within a configured range.
    Range,
    /// Require a passing boolean observation.
    BooleanPass,
    /// Require the presence of evidence for the metric.
    PresenceRequired,
    /// Compare against a prior baseline using a non-regression rule.
    NonRegressionRatchet,
}
/// One required or optional metric constraint associated with a quality lane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityConstraint {
    /// Stable constraint identifier.
    pub id: String,
    /// Verification lane expected to provide evidence.
    pub lane_id: String,
    /// Metric name governed by this constraint.
    pub metric: String,
    /// Declared comparison policy.
    pub comparator: Comparator,
    /// Typed expected value or threshold.
    pub expected: QualityValue,
    /// Whether missing evidence prevents readiness.
    pub required: bool,
}
/// Versioned, content-addressed set of project quality requirements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectQualityContract {
    /// Stable contract identifier.
    pub contract_id: String,
    /// Monotonically increasing contract revision.
    pub revision: u64,
    /// Digest of the canonical contract content.
    pub content_hash: String,
    /// Lifecycle or review status of the contract.
    pub status: String,
    /// Quality constraints evaluated for readiness.
    pub constraints: Vec<QualityConstraint>,
}
/// Readiness result derived from a contract and verification evidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityVerdict {
    /// Required evidence is present and the current evaluator reports readiness.
    Ready,
    /// Readiness is acceptable with declared exceptions.
    ReadyWithExceptions,
    /// One or more required constraints lack passing evidence.
    NeedsVerification,
    /// Evidence shows a quality regression.
    QualityRegression,
    /// A policy decision is required before continuing.
    NeedsPolicyReview,
    /// Requirements block the current target.
    Blocked,
    /// Available data is insufficient to determine a verdict.
    Unknown,
}
/// Snapshot containing the readiness decision for one target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QualityReadinessSnapshot {
    /// Target evaluated against the quality contract.
    pub target_id: String,
    /// Contract revision used for evaluation.
    pub contract_revision: u64,
    /// Overall readiness verdict.
    pub verdict: QualityVerdict,
    /// Constraint identifiers whose evidence indicates failure.
    pub failed_constraints: Vec<String>,
    /// Required constraint identifiers without passing lane evidence.
    pub missing_constraints: Vec<String>,
}

impl ProjectQualityContract {
    /// Checks required identifiers, revision, digest length, and constraint bounds.
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

/// Evaluates whether each required constraint has passing evidence for its lane.
///
/// This current implementation checks evidence presence and lane status; it
/// does not compare evidence metric values against `expected` thresholds.
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
