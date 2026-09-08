//! Bounded provider access/reliability metadata; gateway remains transport owner.
use serde::{Deserialize, Serialize};

pub const CONTRACT_ID: &str = "free-provider-reliability-routing-v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderProfile {
    pub provider_id: String,
    pub transport: String,
    pub endpoint: String,
    pub region: String,
    pub credential_binding: String,
    pub content_hash: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FreeAccessState {
    Free,
    FreeTierLimited,
    TrialCredits,
    Paid,
    Unknown,
    Experimental,
    UnknownNeedsRefresh,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityClass {
    Excellent,
    Healthy,
    Degraded,
    Unstable,
    CoolingDown,
    QuotaLimited,
    Unavailable,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReliabilitySnapshot {
    pub provider_id: String,
    pub model_id: String,
    pub sample_count: u32,
    pub success_rate: f64,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub class: ReliabilityClass,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteSelectionExplanation {
    pub provider_id: String,
    pub model_id: String,
    pub reason: String,
    pub free_state: FreeAccessState,
    pub reliability: ReliabilityClass,
}

impl ProviderProfile {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.provider_id.trim().is_empty()
            || self.transport.trim().is_empty()
            || self.endpoint.trim().is_empty()
            || self.region.trim().is_empty()
            || self.credential_binding.trim().is_empty()
            || self.content_hash.len() != 64
        {
            Err("invalid provider profile")
        } else {
            Ok(())
        }
    }
}
impl ReliabilitySnapshot {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.provider_id.trim().is_empty()
            || self.model_id.trim().is_empty()
            || self.sample_count > 256
            || !self.success_rate.is_finite()
            || !(0.0..=1.0).contains(&self.success_rate)
        {
            Err("invalid reliability snapshot")
        } else {
            Ok(())
        }
    }
}

pub fn classify(snapshot: &ReliabilitySnapshot) -> ReliabilityClass {
    if snapshot.sample_count < 3 {
        ReliabilityClass::Unknown
    } else if snapshot.success_rate >= 0.99 {
        ReliabilityClass::Excellent
    } else if snapshot.success_rate >= 0.95 {
        ReliabilityClass::Healthy
    } else if snapshot.success_rate >= 0.8 {
        ReliabilityClass::Degraded
    } else {
        ReliabilityClass::Unstable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_samples_stay_unknown() {
        let s = ReliabilitySnapshot {
            provider_id: "p".into(),
            model_id: "m".into(),
            sample_count: 1,
            success_rate: 1.0,
            p50_ms: None,
            p95_ms: None,
            jitter_ms: None,
            class: ReliabilityClass::Unknown,
        };
        assert_eq!(classify(&s), ReliabilityClass::Unknown);
    }
}
