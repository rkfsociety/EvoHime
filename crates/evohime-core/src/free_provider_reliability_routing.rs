//! Bounded provider access/reliability metadata; gateway remains transport owner.
use serde::{Deserialize, Serialize};

pub const CONTRACT_ID: &str = "free-provider-reliability-routing-v1";
pub const MAX_PROVIDER_PROFILE_ID_BYTES: usize = 128;
pub const MAX_PROVIDER_PROFILE_TRANSPORT_BYTES: usize = 64;
pub const MAX_PROVIDER_PROFILE_ENDPOINT_BYTES: usize = 512;
pub const MAX_PROVIDER_PROFILE_REGION_BYTES: usize = 64;
pub const MAX_PROVIDER_PROFILE_CREDENTIAL_BINDING_BYTES: usize = 128;
pub const MAX_PROVIDER_MODEL_ID_BYTES: usize = 256;
pub const MAX_RELIABILITY_LATENCY_MS: f64 = 86_400_000.0;

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
        if !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_profile_token(&self.transport, MAX_PROVIDER_PROFILE_TRANSPORT_BYTES)
            || !valid_profile_endpoint(&self.endpoint)
            || !valid_profile_token(&self.region, MAX_PROVIDER_PROFILE_REGION_BYTES)
            || !valid_credential_binding(&self.credential_binding)
            || self.content_hash.len() != 64
            || !self
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("invalid provider profile");
        }
        Ok(())
    }
}

fn valid_profile_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value == value.trim()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn valid_profile_endpoint(value: &str) -> bool {
    value.len() <= MAX_PROVIDER_PROFILE_ENDPOINT_BYTES
        && value == value.trim()
        && (value.starts_with("https://") || value.starts_with("http://"))
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        && !value.contains(['?', '#', '@'])
        && value
            .split_once("://")
            .is_some_and(|(_, authority)| !authority.is_empty() && !authority.starts_with('/'))
}

fn valid_credential_binding(value: &str) -> bool {
    valid_profile_token(value, MAX_PROVIDER_PROFILE_CREDENTIAL_BINDING_BYTES)
        && !value.to_ascii_lowercase().contains("secret")
        && !value.to_ascii_lowercase().contains("bearer")
        && !value.to_ascii_lowercase().starts_with("sk-")
        && !value.to_ascii_lowercase().starts_with("gsk_")
        && !value.to_ascii_lowercase().starts_with("aiza")
}
impl ReliabilitySnapshot {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_profile_token(&self.provider_id, MAX_PROVIDER_PROFILE_ID_BYTES)
            || !valid_model_id(&self.model_id)
            || self.sample_count > 256
            || !self.success_rate.is_finite()
            || !(0.0..=1.0).contains(&self.success_rate)
            || !valid_latency(self.p50_ms)
            || !valid_latency(self.p95_ms)
            || !valid_latency(self.jitter_ms)
            || self
                .p50_ms
                .zip(self.p95_ms)
                .is_some_and(|(p50, p95)| p50 > p95)
            || self.class != classify(self)
        {
            return Err("invalid reliability snapshot");
        }
        Ok(())
    }
}

fn valid_model_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PROVIDER_MODEL_ID_BYTES
        && value == value.trim()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}

fn valid_latency(value: Option<f64>) -> bool {
    value.map_or(true, |value| {
        value.is_finite() && (0.0..=MAX_RELIABILITY_LATENCY_MS).contains(&value)
    })
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

    fn profile() -> ProviderProfile {
        ProviderProfile {
            provider_id: "openrouter".into(),
            transport: "openai_compatible".into(),
            endpoint: "https://openrouter.ai/api/v1".into(),
            region: "global".into(),
            credential_binding: "cred:openrouter".into(),
            content_hash: "a".repeat(64),
        }
    }

    #[test]
    fn provider_profile_accepts_bounded_secret_free_metadata() {
        assert!(profile().validate().is_ok());
    }

    #[test]
    fn provider_profile_rejects_unbounded_or_secret_bearing_metadata() {
        let mut oversized = profile();
        oversized.provider_id = "p".repeat(MAX_PROVIDER_PROFILE_ID_BYTES + 1);
        assert_eq!(oversized.validate(), Err("invalid provider profile"));

        let mut endpoint_with_secret = profile();
        endpoint_with_secret.endpoint =
            "https://provider.example/v1?api_key=secret-provider-key".into();
        assert_eq!(
            endpoint_with_secret.validate(),
            Err("invalid provider profile")
        );

        let mut secret_binding = profile();
        secret_binding.credential_binding = "sk-live-provider-key".into();
        assert_eq!(secret_binding.validate(), Err("invalid provider profile"));
    }

    #[test]
    fn provider_profile_rejects_non_hex_content_hash() {
        let mut invalid = profile();
        invalid.content_hash = "z".repeat(64);
        assert_eq!(invalid.validate(), Err("invalid provider profile"));
    }

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
        assert!(s.validate().is_ok());
    }

    #[test]
    fn reliability_snapshot_rejects_unbounded_or_inconsistent_metadata() {
        let mut invalid = ReliabilitySnapshot {
            provider_id: "provider".into(),
            model_id: "provider/model:free".into(),
            sample_count: 3,
            success_rate: 1.0,
            p50_ms: Some(100.0),
            p95_ms: Some(50.0),
            jitter_ms: Some(2.0),
            class: ReliabilityClass::Excellent,
        };
        assert_eq!(invalid.validate(), Err("invalid reliability snapshot"));

        invalid.p95_ms = Some(f64::NAN);
        assert_eq!(invalid.validate(), Err("invalid reliability snapshot"));

        invalid.p95_ms = Some(200.0);
        invalid.model_id = "model with spaces".into();
        assert_eq!(invalid.validate(), Err("invalid reliability snapshot"));

        invalid.model_id = "provider/model:free".into();
        invalid.class = ReliabilityClass::Healthy;
        assert_eq!(invalid.validate(), Err("invalid reliability snapshot"));
    }
}
