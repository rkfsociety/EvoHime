//! Core runtime facade for diff-bound review metadata.
//!
//! AgentGitChangeSets, Artifact Handoff and Evidence Ledger remain the
//! authorities for their own domains; this facade only validates the review
//! target and derives conservative readiness from coverage and findings.

pub use evohime_local_storage::code_review_lane_store::{
    reconcile, CodeReviewCoverage, CodeReviewError, CodeReviewFinding, CodeReviewRecord,
    CodeReviewTarget, CoverageState, FindingState, ReviewVerdict, TargetKind,
};

pub fn conservative_verdict(record: &CodeReviewRecord) -> Result<ReviewVerdict, CodeReviewError> {
    record.validate()?;
    if record.interrupted || matches!(record.coverage.state, CoverageState::Partial | CoverageState::Failed | CoverageState::Unknown | CoverageState::UnsupportedScope) {
        return Ok(ReviewVerdict::ReviewIncomplete);
    }
    if record.findings.iter().any(|finding| matches!(finding.state, FindingState::Open) && matches!(finding.severity.as_str(), "critical" | "high")) {
        return Ok(ReviewVerdict::ChangesRequested);
    }
    Ok(record.verdict)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_coverage_cannot_be_clean() {
        let target = CodeReviewTarget { schema_version: 1, id: "target-a".into(), kind: TargetKind::CommitRange, base_revision: "a".into(), head_revision: "b".into(), diff_hash: "hash".into(), workspace_fingerprint: None, changed_paths: vec!["src/lib.rs".into()], created_at_ms: 1, content_hash: String::new() };
        let record = CodeReviewRecord { schema_version: 1, review_id: "review-a".into(), revision: 1, target, findings: vec![], coverage: CodeReviewCoverage { changed_files: 1, eligible_files: 1, reviewed_files: 0, skipped_generated: vec![], unsupported_files: vec![], context_failures: vec![], state: CoverageState::Partial, content_hash: String::new() }, verdict: ReviewVerdict::Clean, interrupted: false, content_hash: String::new() }.seal().unwrap();
        assert_eq!(conservative_verdict(&record), Ok(ReviewVerdict::ReviewIncomplete));
    }
}
