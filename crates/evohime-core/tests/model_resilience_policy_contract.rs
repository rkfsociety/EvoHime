use evohime_core::model_resilience_policy::{
    builtin_policy, normalize_provider_error, AttemptOutcome, DataResidency,
    ModelResiliencePolicyRules, PolicyError,
};
use evohime_model_gateway::providers::ProviderError;

#[test]
fn contract_is_versioned_bounded_and_ephemeral() {
    let policy = builtin_policy();
    assert_eq!(policy.schema_version, 1);
    assert!(policy.validate().is_ok());
    assert_eq!(policy.canonical_hash().unwrap().len(), 64);
    assert!(policy.compatible_fallbacks().unwrap().is_empty());
    let rules = ModelResiliencePolicyRules {
        backoff_max_ms: 30_001,
        ..Default::default()
    };
    let invalid =
        evohime_core::model_resilience_policy::ModelResiliencePolicyDefinition { rules, ..policy };
    assert!(matches!(invalid.validate(), Err(PolicyError::Invalid(_))));
    assert_eq!(evohime_local_storage::SCHEMA_VERSION, 52);
    assert_eq!(DataResidency::Any, DataResidency::Any);
}

#[test]
fn provider_details_are_normalized_and_unknown_is_fail_closed() {
    assert_eq!(
        normalize_provider_error(&ProviderError::Http("timeout".into())),
        evohime_model_gateway::FailureCategory::Timeout
    );
    assert_eq!(
        normalize_provider_error(&ProviderError::Api("429 rate limit".into())),
        evohime_model_gateway::FailureCategory::RateLimited
    );
    let outcome = builtin_policy()
        .next_attempt(
            0,
            evohime_model_gateway::FailureCategory::Timeout,
            false,
            true,
        )
        .unwrap();
    assert_eq!(outcome.outcome, AttemptOutcome::UnknownOutcome);
}
