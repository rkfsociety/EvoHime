//! Fail-closed confidence policy layered over the existing Model Gateway.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CascadeDecision {
    Unavailable,
    NeedsReview,
    Eligible,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CascadePolicy {
    pub schema_version: u32,
    pub policy_id: String,
    pub revision: u64,
    pub confidence_threshold: f32,
    pub policy_hash: String,
}

impl CascadePolicy {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("unsupported_schema_version");
        }
        if self.policy_id.is_empty() || self.revision == 0 || self.policy_hash.is_empty() {
            return Err("invalid_policy_identity");
        }
        if !self.confidence_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.confidence_threshold)
        {
            return Err("invalid_confidence_threshold");
        }
        Ok(())
    }

    pub const fn decide(
        &self,
        producer_available: bool,
        route_available: bool,
        confidence: Option<f32>,
    ) -> CascadeDecision {
        if !producer_available || !route_available {
            return CascadeDecision::Unavailable;
        }
        let Some(value) = confidence else {
            return CascadeDecision::NeedsReview;
        };
        if !value.is_finite() || value < 0.0 || value > 1.0 {
            return CascadeDecision::Denied;
        }
        if value >= self.confidence_threshold {
            CascadeDecision::Eligible
        } else {
            CascadeDecision::NeedsReview
        }
    }
}
