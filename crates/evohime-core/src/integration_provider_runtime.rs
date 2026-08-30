//! Bounded runtime adapter for the SDK. Production network adapters are not
//! registered here; unavailable is explicit and fail-closed.

use crate::integration_provider_sdk::{
    fixture_echo_manifest, validate_manifest, CredentialStatus, SdkError,
};

pub trait CredentialResolver {
    fn status(&self, credential_id: &str) -> CredentialStatus;
    fn resolve_for_adapter(&self, credential_id: &str) -> Result<ScopedCredential, SdkError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedCredential {
    pub credential_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ProviderOutcome {
    Success { result: serde_json::Value },
    Unavailable { reason: String },
    Unknown { effect_id: String },
}

#[derive(Debug, Default)]
pub struct FixtureCredentialResolver;

impl CredentialResolver for FixtureCredentialResolver {
    fn status(&self, _credential_id: &str) -> CredentialStatus {
        CredentialStatus::Connected
    }
    fn resolve_for_adapter(&self, credential_id: &str) -> Result<ScopedCredential, SdkError> {
        if credential_id.is_empty() {
            return Err(SdkError::UnresolvedBinding);
        }
        Ok(ScopedCredential {
            credential_id: credential_id.into(),
        })
    }
}

pub fn validate_fixture_catalog() -> Result<(), SdkError> {
    validate_manifest(&fixture_echo_manifest())
}

pub fn invoke_fixture(
    provider_id: &str,
    action_id: &str,
    input: serde_json::Value,
) -> ProviderOutcome {
    if provider_id != "fixture.echo" || action_id != "echo" {
        return ProviderOutcome::Unavailable {
            reason: "provider_adapter_unavailable".into(),
        };
    }
    ProviderOutcome::Success {
        result: serde_json::json!({"value": input.get("value").cloned().unwrap_or(serde_json::Value::Null)}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_path_is_bounded_and_deterministic() {
        assert!(validate_fixture_catalog().is_ok());
        assert_eq!(
            invoke_fixture("fixture.echo", "echo", serde_json::json!({"value":"x"})),
            ProviderOutcome::Success {
                result: serde_json::json!({"value":"x"})
            }
        );
    }
    #[test]
    fn external_provider_is_explicitly_unavailable() {
        assert!(matches!(
            invoke_fixture("github", "issues.create", serde_json::json!({})),
            ProviderOutcome::Unavailable { .. }
        ));
    }
}
