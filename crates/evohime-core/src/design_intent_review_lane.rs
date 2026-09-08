//! Bounded Core-owned design-intent review metadata.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesignIntent {
    pub intent_id: String,
    pub scope: String,
    pub statement: String,
    pub content_hash: String,
    pub revision: u64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approved,
    NeedsReview,
    Unknown,
    Rejected,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewRecord {
    pub review_id: String,
    pub intent: DesignIntent,
    pub verdict: ReviewVerdict,
    pub evidence_refs: Vec<String>,
    pub reviewer_id: String,
}
impl DesignIntent {
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
