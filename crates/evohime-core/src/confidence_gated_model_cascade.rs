//! Fail-closed confidence policy layered over the existing Model Gateway.

use serde::{Deserialize, Serialize};

/// Current confidence cascade policy schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Decision produced from provider availability and a confidence score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CascadeDecision {
    /// A producer or route is unavailable, so the cascade cannot proceed.
    Unavailable,
    /// The score is absent or below threshold and requires human review.
    NeedsReview,
    /// The score meets the threshold and is within the valid range.
    Eligible,
    /// The supplied score is non-finite or outside the valid range.
    Denied,
}

/// Versioned policy that gates cascade eligibility by model confidence.
///
/// ```
/// use evohime_core::confidence_gated_model_cascade::{CascadeDecision, CascadePolicy, SCHEMA_VERSION};
/// let policy = CascadePolicy {
///     schema_version: SCHEMA_VERSION,
///     policy_id: "default".into(),
///     revision: 1,
///     confidence_threshold: 0.8,
///     policy_hash: "sha256:policy".into(),
/// };
/// assert!(policy.validate().is_ok());
/// assert_eq!(policy.decide(true, true, Some(0.9)), CascadeDecision::Eligible);
/// assert_eq!(policy.decide(true, true, None), CascadeDecision::NeedsReview);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CascadePolicy {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable identity of this policy.
    pub policy_id: String,
    /// Positive revision number used to order policy updates.
    pub revision: u64,
    /// Minimum confidence required for eligibility, from 0.0 through 1.0.
    pub confidence_threshold: f32,
    /// Digest identifying the canonical policy content.
    pub policy_hash: String,
}

impl CascadePolicy {
    /// Validates the schema, identity, and finite confidence threshold.
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

    /// Maps availability and confidence to a fail-closed cascade decision.
    ///
    /// Missing confidence requires review. Invalid numeric confidence is denied.
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
