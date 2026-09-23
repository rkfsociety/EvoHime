//! Core-owned model-call resilience contract.
//!
//! Routing chooses a primary profile; this module decides whether a failed
//! call may be retried or moved to an already-authorized compatible profile.
//! It contains metadata only. Provider payloads and credentials stay inside
//! `evohime-model-gateway` adapters, while durable dispatch evidence remains
//! owned by `evohime-model-provenance`.

use evohime_model_gateway::provider_contract::{FailureCategory, PrivacyClass};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use thiserror::Error;

/// Current schema version for model-call resilience policies.
pub const CONTRACT_VERSION: u32 = 1;
/// Stable policy identifier included in canonical hashes.
pub const CONTRACT_ID: &str = "model-resilience-policy-v1";
/// Maximum number of attempts allowed by a resilience policy.
pub const MAX_ATTEMPTS: u32 = 8;
/// Maximum number of configured fallback profiles.
pub const MAX_FALLBACKS: usize = 8;
/// Maximum exponential backoff delay in milliseconds.
pub const MAX_BACKOFF_MS: u64 = 30_000;
/// Maximum encoded profile identifier length.
pub const MAX_PROFILE_ID_BYTES: usize = 128;
/// Maximum encoded provider identifier length.
pub const MAX_PROVIDER_ID_BYTES: usize = 128;
/// Maximum encoded model identifier length.
pub const MAX_MODEL_ID_BYTES: usize = 256;
/// Maximum encoded capability identifier length.
pub const MAX_CAPABILITY_BYTES: usize = 64;

/// Data-location requirement applied to primary and fallback profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataResidency {
    /// Request data must remain on the local machine.
    Local,
    /// Request data must be processed within the European Union.
    EuropeanUnion,
    /// No additional residency restriction is imposed.
    Any,
}

/// Versioned provider profile metadata eligible for model-call routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelProfileRef {
    /// Stable profile identifier.
    pub id: String,
    /// Stable provider identifier.
    pub provider: String,
    /// Model identifier understood by the provider adapter.
    pub model: String,
    /// Capabilities supported by the profile.
    pub capabilities: BTreeSet<String>,
    /// Privacy boundary promised by the profile.
    pub privacy_boundary: PrivacyClass,
    /// Processing residency of the profile.
    pub residency: DataResidency,
    /// Digest binding the profile's metadata and revision.
    pub profile_hash: String,
}

/// Retry, fallback, privacy, residency, and capability constraints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelResiliencePolicyRules {
    /// Total attempt limit including the initial call.
    pub max_attempts: u32,
    /// Maximum number of fallback profiles considered.
    pub max_fallbacks: u32,
    /// Initial backoff delay in milliseconds.
    pub backoff_base_ms: u64,
    /// Upper bound for exponential backoff in milliseconds.
    pub backoff_max_ms: u64,
    /// Whether compatible fallback profiles may be selected.
    pub allow_fallback: bool,
    /// Minimum privacy boundary required of a profile.
    pub required_privacy: PrivacyClass,
    /// Required processing residency.
    pub required_residency: DataResidency,
    /// Capabilities that every selected profile must provide.
    pub required_capabilities: BTreeSet<String>,
}

/// Versioned policy with primary and pre-authorized compatible fallback profiles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelResiliencePolicyDefinition {
    /// Serialized contract version.
    pub schema_version: u32,
    /// Stable policy identifier.
    pub policy_id: String,
    /// Monotonic policy revision.
    pub version: u64,
    /// Retry and compatibility constraints.
    pub rules: ModelResiliencePolicyRules,
    /// First profile selected for a model call.
    pub primary: ModelProfileRef,
    /// Ordered compatible profiles eligible after primary failure.
    pub fallbacks: Vec<ModelProfileRef>,
}

/// Planned outcome for one provider attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    /// Provider returned a successful response.
    Success,
    /// Another attempt on the primary profile is permitted.
    Retried,
    /// A compatible fallback profile is selected.
    Fallback,
    /// Caller cancelled the operation.
    Cancelled,
    /// Failure category does not permit retry or fallback.
    Denied,
    /// Request may have reached the provider, so outcome cannot be retried safely.
    UnknownOutcome,
    /// Configured attempt budget is exhausted.
    Exhausted,
}

/// Bounded metadata describing a single planned attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptMetadata {
    /// One-based attempt number.
    pub attempt: u32,
    /// Profile selected for this attempt.
    pub profile_id: String,
    /// Normalized failure category, if the attempt follows a failure.
    pub failure: Option<FailureCategory>,
    /// Planned attempt outcome.
    pub outcome: AttemptOutcome,
    /// Delay before the next permitted attempt, in milliseconds.
    pub backoff_ms: u64,
}

/// Resilience policy compatibility, version, or validation failure.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    /// Policy uses an unsupported schema version.
    #[error("unsupported resilience policy schema {0}")]
    UnsupportedVersion(u32),
    /// Policy fields or profile metadata violate the contract.
    #[error("invalid resilience policy: {0}")]
    Invalid(String),
    /// No profile satisfies the required privacy, residency, or capability rules.
    #[error("profile is not compatible: {0}")]
    Incompatible(String),
}

/// Converts provider implementation errors to the stable Core taxonomy.
/// Callers must not persist or expose the provider error text as policy state.
pub fn normalize_provider_error(
    error: &evohime_model_gateway::providers::ProviderError,
) -> FailureCategory {
    use evohime_model_gateway::providers::ProviderError;
    match error {
        ProviderError::Config(_) => FailureCategory::InvalidRequest,
        ProviderError::Http(message) => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("timeout") {
                FailureCategory::Timeout
            } else if lower.contains("connection") {
                FailureCategory::ConnectionRefused
            } else if lower.contains("429") || lower.contains("rate limit") {
                FailureCategory::RateLimited
            } else {
                FailureCategory::ServerError
            }
        }
        ProviderError::Api(message) => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("429") || lower.contains("rate limit") {
                FailureCategory::RateLimited
            } else {
                FailureCategory::ServerError
            }
        }
        ProviderError::Stream(_) => FailureCategory::MalformedResponse,
    }
}

impl Default for ModelResiliencePolicyRules {
    /// Returns bounded retry defaults with internal privacy and chat capability.
    fn default() -> Self {
        Self {
            max_attempts: 3,
            max_fallbacks: 2,
            backoff_base_ms: 250,
            backoff_max_ms: 5_000,
            allow_fallback: true,
            required_privacy: PrivacyClass::Internal,
            required_residency: DataResidency::Any,
            required_capabilities: ["chat".to_string()].into_iter().collect(),
        }
    }
}

impl ModelResiliencePolicyDefinition {
    /// Checks schema, identity, bounds, duplicate profiles, and compatibility.
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.schema_version != CONTRACT_VERSION {
            return Err(PolicyError::UnsupportedVersion(self.schema_version));
        }
        if !valid_metadata_token(&self.policy_id, MAX_PROFILE_ID_BYTES) {
            return Err(PolicyError::Invalid("policy_id".into()));
        }
        let r = &self.rules;
        if !(1..=MAX_ATTEMPTS).contains(&r.max_attempts)
            || r.max_fallbacks > MAX_FALLBACKS as u32
            || r.backoff_base_ms > MAX_BACKOFF_MS
            || r.backoff_max_ms > MAX_BACKOFF_MS
            || r.backoff_base_ms > r.backoff_max_ms
            || r.required_capabilities.len() > 32
        {
            return Err(PolicyError::Invalid("bounds".into()));
        }
        validate_profile(&self.primary, r)?;
        let mut ids = BTreeSet::new();
        ids.insert(self.primary.id.as_str());
        for profile in self.fallbacks.iter().take(MAX_FALLBACKS) {
            validate_profile(profile, r)?;
            if !ids.insert(profile.id.as_str()) {
                return Err(PolicyError::Invalid("duplicate profile".into()));
            }
        }
        if self.fallbacks.len() > MAX_FALLBACKS {
            return Err(PolicyError::Invalid("fallbacks".into()));
        }
        Ok(())
    }

    /// Validates and hashes the policy under its stable contract identifier.
    pub fn canonical_hash(&self) -> Result<String, PolicyError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| PolicyError::Invalid(e.to_string()))?;
        let mut hasher = Sha256::new();
        hasher.update(CONTRACT_ID.as_bytes());
        hasher.update([0]);
        hasher.update(bytes);
        Ok(hex::encode(hasher.finalize()))
    }

    /// Returns the configured prefix of fallbacks that satisfies policy rules.
    pub fn compatible_fallbacks(&self) -> Result<Vec<&ModelProfileRef>, PolicyError> {
        self.validate()?;
        if !self.rules.allow_fallback || self.rules.max_fallbacks == 0 {
            return Ok(Vec::new());
        }
        Ok(self
            .fallbacks
            .iter()
            .take(self.rules.max_fallbacks as usize)
            .filter(|profile| is_compatible(profile, &self.rules))
            .collect())
    }

    /// Selects a profile and bounded retry outcome for the reported failure.
    pub fn next_attempt(
        &self,
        attempt: u32,
        failure: FailureCategory,
        cancelled: bool,
        dispatched: bool,
    ) -> Result<AttemptMetadata, PolicyError> {
        self.validate()?;
        let profile = if attempt == 0 {
            &self.primary
        } else {
            self.compatible_fallbacks()?
                .get((attempt - 1) as usize)
                .copied()
                .ok_or_else(|| PolicyError::Incompatible("no compatible fallback".into()))?
        };
        let outcome = if cancelled {
            AttemptOutcome::Cancelled
        } else if dispatched {
            AttemptOutcome::UnknownOutcome
        } else if !failure.opens_circuit() && !failure.triggers_cooldown() {
            AttemptOutcome::Denied
        } else if attempt + 1 >= self.rules.max_attempts {
            AttemptOutcome::Exhausted
        } else if attempt == 0 {
            AttemptOutcome::Retried
        } else {
            AttemptOutcome::Fallback
        };
        let factor = 1u64 << attempt.min(16);
        Ok(AttemptMetadata {
            attempt: attempt + 1,
            profile_id: profile.id.clone(),
            failure: Some(failure),
            outcome,
            backoff_ms: self
                .rules
                .backoff_base_ms
                .saturating_mul(factor)
                .min(self.rules.backoff_max_ms),
        })
    }
}

/// The shipped baseline. Real profile resolution is still performed by the
/// existing Core-owned routing catalog; this value is used for bounded status
/// projection and deterministic contract tests.
/// Returns the shipped bounded policy baseline for internal model calls.
pub fn builtin_policy() -> ModelResiliencePolicyDefinition {
    let profile = ModelProfileRef {
        id: "default".into(),
        provider: "configured".into(),
        model: "configured".into(),
        capabilities: ["chat".into()].into_iter().collect(),
        privacy_boundary: PrivacyClass::Internal,
        residency: DataResidency::Any,
        profile_hash: "0".repeat(64),
    };
    ModelResiliencePolicyDefinition {
        schema_version: CONTRACT_VERSION,
        policy_id: CONTRACT_ID.into(),
        version: 1,
        rules: ModelResiliencePolicyRules::default(),
        primary: profile,
        fallbacks: Vec::new(),
    }
}

fn validate_profile(
    profile: &ModelProfileRef,
    rules: &ModelResiliencePolicyRules,
) -> Result<(), PolicyError> {
    if profile.id.trim().is_empty()
        || !valid_metadata_token(&profile.id, MAX_PROFILE_ID_BYTES)
        || !valid_metadata_token(&profile.provider, MAX_PROVIDER_ID_BYTES)
        || !valid_model_id(&profile.model)
        || profile.profile_hash.len() != 64
        || !profile
            .profile_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || profile
            .capabilities
            .iter()
            .any(|capability| !valid_metadata_token(capability, MAX_CAPABILITY_BYTES))
    {
        return Err(PolicyError::Invalid("profile metadata".into()));
    }
    if rules
        .required_capabilities
        .iter()
        .any(|capability| !valid_metadata_token(capability, MAX_CAPABILITY_BYTES))
    {
        return Err(PolicyError::Invalid("capability metadata".into()));
    }
    if !is_compatible(profile, rules) {
        return Err(PolicyError::Incompatible(profile.id.clone()));
    }
    Ok(())
}

fn valid_metadata_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value == value.trim()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn valid_model_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MODEL_ID_BYTES
        && value == value.trim()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}

fn is_compatible(profile: &ModelProfileRef, rules: &ModelResiliencePolicyRules) -> bool {
    profile.privacy_boundary >= rules.required_privacy
        && (rules.required_residency == DataResidency::Any
            || profile.residency == rules.required_residency)
        && rules
            .required_capabilities
            .iter()
            .all(|capability| profile.capabilities.contains(capability))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, residency: DataResidency) -> ModelProfileRef {
        ModelProfileRef {
            id: id.into(),
            provider: "provider-a".into(),
            model: "model-a".into(),
            capabilities: ["chat".into()].into_iter().collect(),
            privacy_boundary: PrivacyClass::Internal,
            residency,
            profile_hash: "a".repeat(64),
        }
    }

    fn policy() -> ModelResiliencePolicyDefinition {
        ModelResiliencePolicyDefinition {
            schema_version: 1,
            policy_id: CONTRACT_ID.into(),
            version: 1,
            rules: ModelResiliencePolicyRules::default(),
            primary: profile("primary", DataResidency::Local),
            fallbacks: vec![profile("fallback", DataResidency::Local)],
        }
    }

    #[test]
    fn validates_hash_and_profile_contract() {
        let p = policy();
        assert!(p.validate().is_ok());
        assert_eq!(p.canonical_hash().unwrap().len(), 64);
    }
    #[test]
    fn rejects_duplicate_or_incompatible_profiles() {
        let mut p = policy();
        p.fallbacks[0].id = "primary".into();
        assert!(matches!(p.validate(), Err(PolicyError::Invalid(_))));
        let mut p = policy();
        p.rules.required_residency = DataResidency::EuropeanUnion;
        assert!(p.validate().is_err());
    }
    #[test]
    fn emits_bounded_retry_then_fallback_metadata() {
        let p = policy();
        let first = p
            .next_attempt(0, FailureCategory::Timeout, false, false)
            .unwrap();
        assert_eq!(first.outcome, AttemptOutcome::Retried);
        let second = p
            .next_attempt(1, FailureCategory::ServerError, false, false)
            .unwrap();
        assert_eq!(second.outcome, AttemptOutcome::Fallback);
        assert!(second.backoff_ms <= 5_000);
    }
    #[test]
    fn cancellation_and_dispatch_are_fail_closed() {
        let p = policy();
        assert_eq!(
            p.next_attempt(0, FailureCategory::Timeout, true, false)
                .unwrap()
                .outcome,
            AttemptOutcome::Cancelled
        );
        assert_eq!(
            p.next_attempt(0, FailureCategory::Timeout, false, true)
                .unwrap()
                .outcome,
            AttemptOutcome::UnknownOutcome
        );
    }
    #[test]
    fn unknown_policy_version_is_rejected() {
        let mut p = policy();
        p.schema_version = 2;
        assert_eq!(p.validate(), Err(PolicyError::UnsupportedVersion(2)));
    }

    #[test]
    fn rejects_unbounded_or_secret_like_profile_metadata() {
        let mut p = policy();
        p.primary.model = "https://provider.example/?prompt=secret".into();
        assert!(matches!(p.validate(), Err(PolicyError::Invalid(_))));

        let mut p = policy();
        p.primary.capabilities.insert("prompt secret".into());
        assert!(matches!(p.validate(), Err(PolicyError::Invalid(_))));

        let mut p = policy();
        p.primary.profile_hash = "z".repeat(64);
        assert!(matches!(p.validate(), Err(PolicyError::Invalid(_))));
    }
}
