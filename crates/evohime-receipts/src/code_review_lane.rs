//! Conservative code-review lane verdict evaluation.
//!
//! Interrupted reviews and partial, failed, unknown, or unsupported coverage
//! yield `"review_incomplete"`. Open critical or high findings yield
//! `"changes_requested"`; otherwise the recorded verdict is preserved.

use serde::{Deserialize, Serialize};

/// Outcome of evaluating a code review lane record.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    /// No blocking findings and complete coverage.
    Clean,
    /// One or more open high/critical findings require changes.
    ChangesRequested,
    /// Review was interrupted or coverage was incomplete.
    ReviewIncomplete,
}

/// Coverage status reported for a code review lane.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// All eligible files were reviewed.
    Complete,
    /// Only a subset of eligible files was reviewed.
    Partial,
    /// Coverage calculation or review failed.
    Failed,
    /// Coverage state could not be determined.
    Unknown,
    /// Target scope is not supported by the reviewer.
    UnsupportedScope,
}

/// Lifecycle state of an individual review finding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingState {
    /// Finding is still open.
    Open,
    /// Finding was resolved.
    Resolved,
    /// Finding became stale after target changes.
    Stale,
}

/// Minimal finding projection needed for conservative verdict derivation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewFindingView {
    /// Lifecycle state of the finding.
    pub state: FindingState,
    /// Severity label (`"critical"`, `"high"`, etc.).
    pub severity: String,
}

/// Minimal coverage projection needed for conservative verdict derivation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewCoverageView {
    /// Coverage state of the review.
    pub state: CoverageState,
}

/// Minimal code-review record projection needed for conservative verdict derivation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeReviewRecordView {
    /// Findings recorded in the review.
    pub findings: Vec<CodeReviewFindingView>,
    /// Coverage summary for the review.
    pub coverage: CodeReviewCoverageView,
    /// Recorded verdict before conservative clamping.
    pub verdict: ReviewVerdict,
    /// Whether the review run was interrupted.
    pub interrupted: bool,
}

/// Derives a conservative verdict that cannot mark incomplete coverage as clean.
pub fn conservative_verdict(record: &CodeReviewRecordView) -> ReviewVerdict {
    if record.interrupted
        || matches!(
            record.coverage.state,
            CoverageState::Partial
                | CoverageState::Failed
                | CoverageState::Unknown
                | CoverageState::UnsupportedScope
        )
    {
        return ReviewVerdict::ReviewIncomplete;
    }
    if record.findings.iter().any(|finding| {
        matches!(finding.state, FindingState::Open)
            && matches!(finding.severity.as_str(), "critical" | "high")
    }) {
        return ReviewVerdict::ChangesRequested;
    }
    record.verdict
}

/// Evaluates [`conservative_verdict`] from a serialized `CodeReviewRecord` JSON value.
pub fn conservative_verdict_json(value: &serde_json::Value) -> Result<ReviewVerdict, &'static str> {
    let record: CodeReviewRecordView =
        serde_json::from_value(value.clone()).map_err(|_| "invalid_code_review_record")?;
    Ok(conservative_verdict(&record))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_coverage_cannot_be_clean() {
        let record = CodeReviewRecordView {
            findings: vec![],
            coverage: CodeReviewCoverageView {
                state: CoverageState::Partial,
            },
            verdict: ReviewVerdict::Clean,
            interrupted: false,
        };
        assert_eq!(
            conservative_verdict(&record),
            ReviewVerdict::ReviewIncomplete
        );
    }

    #[test]
    fn open_high_finding_requests_changes() {
        let record = CodeReviewRecordView {
            findings: vec![CodeReviewFindingView {
                state: FindingState::Open,
                severity: "high".to_owned(),
            }],
            coverage: CodeReviewCoverageView {
                state: CoverageState::Complete,
            },
            verdict: ReviewVerdict::Clean,
            interrupted: false,
        };
        assert_eq!(
            conservative_verdict(&record),
            ReviewVerdict::ChangesRequested
        );
    }
}
