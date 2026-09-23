//! Bounded Core-owned design-intent review metadata.
//!
//! ```
//! use evohime_core::design_intent_review_lane::{DesignIntent, ReviewRecord, ReviewVerdict};
//! let intent = DesignIntent {
//!     intent_id: "intent-1".into(),
//!     scope: "screen-opaque-id".into(),
//!     statement: "Keep the primary action visible.".into(),
//!     content_hash: "a".repeat(64),
//!     revision: 1,
//! };
//! let review = ReviewRecord {
//!     review_id: "review-1".into(),
//!     intent,
//!     verdict: ReviewVerdict::NeedsReview,
//!     evidence_refs: vec!["artifact:design-1".into()],
//!     reviewer_id: "reviewer-1".into(),
//! };
//! assert!(review.validate().is_ok());
//! ```

use serde::{Deserialize, Serialize};
/// Core-authored design intent that a review record evaluates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesignIntent {
    /// Stable identifier for this intent.
    pub intent_id: String,
    /// UI, feature, or workspace area to which the intent applies.
    pub scope: String,
    /// Human-readable statement of the intended design outcome.
    pub statement: String,
    /// 64-character digest text for the canonical intent content.
    pub content_hash: String,
    /// Positive revision of the intent statement.
    pub revision: u64,
}
/// Outcome recorded by a design-intent review.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    /// The review found the intent satisfied by its evidence.
    Approved,
    /// The available evidence is insufficient for approval.
    NeedsReview,
    /// The review could not establish whether the intent is satisfied.
    Unknown,
    /// The evidence contradicts the stated intent.
    Rejected,
}
/// Review decision and the evidence references supporting it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewRecord {
    /// Stable identifier for this review record.
    pub review_id: String,
    /// Design intent being reviewed.
    pub intent: DesignIntent,
    /// Recorded review outcome.
    pub verdict: ReviewVerdict,
    /// References to artifacts or other evidence considered by the reviewer.
    pub evidence_refs: Vec<String>,
    /// Identifier of the reviewer responsible for the decision.
    pub reviewer_id: String,
}
impl DesignIntent {
    /// Checks that identifiers and statement are non-empty and revision/hash bounds hold.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.intent_id.is_empty()
            || self.scope.is_empty()
            || self.statement.is_empty()
            || self.revision == 0
            || self.content_hash.len() != 64
        {
            Err("invalid design intent")
        } else {
            Ok(())
        }
    }
}
impl ReviewRecord {
    /// Validates the nested intent and the review identifiers and evidence count.
    pub fn validate(&self) -> Result<(), &'static str> {
        self.intent.validate()?;
        if self.review_id.is_empty() || self.reviewer_id.is_empty() || self.evidence_refs.len() > 64
        {
            Err("invalid review record")
        } else {
            Ok(())
        }
    }
}
