use super::*;

#[test]
fn route_preflight_rejects_known_stale_catalog_before_provider_dispatch() {
    let route =
        ModelRouteConfig::openai_compatible("test-key", "https://provider.example/v1", "model-a");
    let profile = ProviderProfile::from_route_config(&route).expect("profile");
    let fresh = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &[ModelCatalogEntry {
            id: "model-a".into(),
            context_tokens: None,
            max_output_tokens: None,
        }],
        1,
        "a".repeat(64),
        1_000,
        2_000,
    )
    .expect("fresh catalog");
    let stale = ProviderCatalogSnapshot::stale_after_failure(
        &profile,
        &fresh,
        2,
        CatalogFailureCode::Timeout,
    )
    .expect("stale catalog");
    let cache = new_provider_catalog_cache();
    cache
        .write()
        .expect("cache write")
        .insert(provider_catalog_scope_key(&profile), stale);
    let config = evohime_model_gateway::ModelGatewayConfig {
        default_route: "default".into(),
        routes: std::collections::HashMap::from([("default".into(), route)]),
    };
    let preflight = ProviderCatalogRoutePreflight::new(config, cache.clone());
    let error = preflight
        .check("default", Some("model-a"), 1_500)
        .expect_err("stale catalog must fail closed");
    assert!(matches!(error, ProviderError::Config(code) if code == "provider_catalog_stale"));

    let unavailable = ProviderCatalogSnapshot::failure(
        &profile,
        3,
        "d".repeat(64),
        ProviderCatalogState::Unavailable,
        CatalogFailureCode::ModelNotFound,
        3_000,
        4_000,
    )
    .expect("model-not-found snapshot");
    cache
        .write()
        .expect("cache write")
        .insert(provider_catalog_scope_key(&profile), unavailable);
    let error = preflight
        .check("default", Some("model-a"), 3_500)
        .expect_err("missing model must fail closed");
    assert!(matches!(error, ProviderError::Config(code) if code == "provider_model_not_found"));

    let expired = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &[ModelCatalogEntry {
            id: "model-a".into(),
            context_tokens: None,
            max_output_tokens: None,
        }],
        4,
        "e".repeat(64),
        4_000,
        5_000,
    )
    .expect("expired snapshot");
    cache
        .write()
        .expect("cache write")
        .insert(provider_catalog_scope_key(&profile), expired);
    let error = preflight
        .check("default", Some("model-a"), 5_000)
        .expect_err("expired catalog must fail closed");
    assert!(matches!(error, ProviderError::Config(code) if code == "provider_catalog_expired"));
}

#[test]
fn discovery_unsupported_keeps_explicit_manual_model_eligible() {
    let route = ModelRouteConfig::openai_compatible(
        "test-key",
        "https://provider.example/v1",
        "manual-model-id",
    );
    let profile = ProviderProfile::from_route_config(&route).expect("profile");
    let unsupported = ProviderCatalogSnapshot::failure(
        &profile,
        1,
        "a".repeat(64),
        ProviderCatalogState::DiscoveryUnsupported,
        CatalogFailureCode::DiscoveryUnsupported,
        1_000,
        2_000,
    )
    .expect("unsupported discovery snapshot");
    let cache = new_provider_catalog_cache();
    cache
        .write()
        .expect("cache write")
        .insert(provider_catalog_scope_key(&profile), unsupported);
    let config = evohime_model_gateway::ModelGatewayConfig {
        default_route: "default".into(),
        routes: std::collections::HashMap::from([("default".into(), route)]),
    };
    let preflight = ProviderCatalogRoutePreflight::new(config, cache);

    assert!(preflight
        .check("default", Some("manual-model-id"), 1_500)
        .is_ok());
}

#[test]
fn capability_preflight_blocks_only_confirmed_unsupported_requirements() {
    let route =
        ModelRouteConfig::openai_compatible("test-key", "https://provider.example/v1", "model-a");
    let profile = ProviderProfile::from_route_config(&route).expect("profile");
    let mut fresh = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &[ModelCatalogEntry {
            id: "model-a".into(),
            context_tokens: None,
            max_output_tokens: None,
        }],
        1,
        "a".repeat(64),
        1_000,
        2_000,
    )
    .expect("fresh catalog");
    fresh.models[0].capabilities.push(CapabilityFlag {
        capability: ModelCapability::ToolCalls,
        state: CapabilityState::Unsupported,
        provenance: CapabilityProvenance::ProviderDeclared,
    });
    fresh.validate().expect("bounded capability evidence");

    let cache = new_provider_catalog_cache();
    cache
        .write()
        .expect("cache write")
        .insert(provider_catalog_scope_key(&profile), fresh);
    let config = evohime_model_gateway::ModelGatewayConfig {
        default_route: "default".into(),
        routes: std::collections::HashMap::from([("default".into(), route)]),
    };
    let preflight = ProviderCatalogRoutePreflight::new(config, cache);

    assert!(preflight
        .check_for_request("default", Some("model-a"), false, 1_500)
        .is_ok());
    let error = preflight
        .check_for_request("default", Some("model-a"), true, 1_500)
        .expect_err("confirmed unsupported tool calls must be blocked before dispatch");
    assert!(
        matches!(error, ProviderError::Config(code) if code == "provider_model_capability_unsupported")
    );
}

#[test]
fn unknown_capability_does_not_block_manual_model_execution() {
    let route =
        ModelRouteConfig::openai_compatible("test-key", "https://provider.example/v1", "model-a");
    let profile = ProviderProfile::from_route_config(&route).expect("profile");
    let mut fresh = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &[ModelCatalogEntry {
            id: "model-a".into(),
            context_tokens: None,
            max_output_tokens: None,
        }],
        1,
        "a".repeat(64),
        1_000,
        2_000,
    )
    .expect("fresh catalog");
    fresh.models[0].capabilities.push(CapabilityFlag {
        capability: ModelCapability::ToolCalls,
        state: CapabilityState::Unknown,
        provenance: CapabilityProvenance::Unknown,
    });

    let cache = new_provider_catalog_cache();
    cache
        .write()
        .expect("cache write")
        .insert(provider_catalog_scope_key(&profile), fresh);
    let config = evohime_model_gateway::ModelGatewayConfig {
        default_route: "default".into(),
        routes: std::collections::HashMap::from([("default".into(), route)]),
    };
    let preflight = ProviderCatalogRoutePreflight::new(config, cache);

    assert!(preflight
        .check_for_request("default", Some("model-a"), true, 1_500)
        .is_ok());
}

fn profile() -> ProviderProfile {
    ProviderProfile {
        schema_version: PROVIDER_PROFILE_SCHEMA_VERSION,
        provider_id: "openrouter".into(),
        provider_family: ProviderFamily::OpenRouter,
        transport: "openai_compatible".into(),
        transport_kind: TransportKind::OpenAiCompatible,
        endpoint: "https://openrouter.ai/api/v1".into(),
        region: "global".into(),
        credential_binding: "credential:openrouter".into(),
        content_hash: "a".repeat(64),
        revision: 1,
    }
}

fn evidence() -> FreeAccessEvidence {
    FreeAccessEvidence {
        schema_version: FREE_ACCESS_EVIDENCE_SCHEMA_VERSION,
        provider_id: "openrouter".into(),
        model_id: "provider/model:free".into(),
        credential_binding: "cred:openrouter".into(),
        region: "global".into(),
        advertised_state: FreeAccessState::FreeTierLimited,
        observed_state: ObservedFreeAccessState::VerifiedFreeLimited,
        activation: ActivationState::Completed,
        allowance: AllowanceKind::Recurring,
        limits: vec![FreeAccessLimit {
            scope: EvidenceLimitScope::Model,
            source: EvidenceLimitSource::Observed,
            unit: CreditUnit::Requests,
            allowance: AllowanceKind::Recurring,
            limit: Some(20),
            remaining: Some(19),
            observed_at_ms: 1_000,
            resets_at_ms: Some(2_000),
        }],
        successful_sample_count: 3,
        confidence_bps: 9_000,
        observed_at_ms: 1_000,
        expires_at_ms: 2_000,
        invalidation: None,
        failure_reason: None,
        content_hash: "b".repeat(64),
        revision: 1,
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

    let mut mismatched_transport = profile();
    mismatched_transport.transport_kind = TransportKind::Ollama;
    assert_eq!(
        mismatched_transport.validate(),
        Err("invalid provider profile")
    );
}

#[test]
fn provider_profile_rejects_non_hex_content_hash() {
    let mut invalid = profile();
    invalid.content_hash = "z".repeat(64);
    assert_eq!(invalid.validate(), Err("invalid provider profile"));
}

#[test]
fn legacy_profile_defaults_keep_transport_compatible() {
    let legacy = serde_json::json!({
        "provider_id": "openrouter",
        "transport": "openai_compatible",
        "endpoint": "https://openrouter.ai/api/v1",
        "region": "global",
        "credential_binding": "cred:openrouter",
        "content_hash": "a".repeat(64)
    });
    let parsed: ProviderProfile = serde_json::from_value(legacy).expect("legacy profile");
    assert!(parsed.validate().is_ok());
    assert_eq!(
        parsed.resolved_transport_kind(),
        TransportKind::OpenAiCompatible
    );
}

#[test]
fn route_config_adapter_keeps_credentials_out_of_profile_metadata() {
    let route = ModelRouteConfig::openai_compatible(
        "sk-live-provider-key",
        "https://api.example/v1",
        "model",
    );
    let profile = ProviderProfile::from_route_config(&route).expect("profile");
    assert!(profile.validate().is_ok());
    assert_eq!(profile.provider_id, "custom_openai_compatible");
    assert_eq!(profile.provider_family, ProviderFamily::Unknown);
    assert_eq!(profile.transport_kind, TransportKind::OpenAiCompatible);
    assert!(!serde_json::to_string(&profile)
        .expect("profile json")
        .contains("sk-live-provider-key"));
}

#[test]
fn legacy_openai_endpoint_maps_to_openai_without_sniffing_custom_hosts() {
    let route = ModelRouteConfig::openai_compatible(
        "test-key",
        "https://api.openai.com/v1/",
        "gpt-4.1-mini",
    );
    let profile = ProviderProfile::from_route_config(&route).expect("profile");
    assert_eq!(profile.provider_id, "openai");
    assert_eq!(profile.provider_family, ProviderFamily::OpenAi);
}

#[test]
fn cloudflare_catalog_scope_isolated_by_account_id() {
    let first_account = "0123456789abcdef0123456789abcdef";
    let second_account = "fedcba9876543210fedcba9876543210";
    let first = ModelRouteConfig::openai_compatible(
        "test-token",
        ProviderProfileId::cloudflare_base_url(first_account).expect("first account URL"),
        "@cf/model",
    )
    .with_provider_profile(
        ProviderProfileId::CloudflareWorkersAi,
        Some(first_account.into()),
    );
    let second = ModelRouteConfig::openai_compatible(
        "test-token",
        ProviderProfileId::cloudflare_base_url(second_account).expect("second account URL"),
        "@cf/model",
    )
    .with_provider_profile(
        ProviderProfileId::CloudflareWorkersAi,
        Some(second_account.into()),
    );
    let first_profile = ProviderProfile::from_route_config(&first).expect("first profile");
    let second_profile = ProviderProfile::from_route_config(&second).expect("second profile");

    assert_eq!(first_profile.provider_id, second_profile.provider_id);
    assert_ne!(
        first_profile.credential_binding,
        second_profile.credential_binding
    );
    assert_ne!(
        provider_catalog_scope_key(&first_profile),
        provider_catalog_scope_key(&second_profile)
    );
}

#[test]
fn builtin_profiles_are_bounded_and_versioned() {
    let profiles = builtin_provider_profiles();
    assert_eq!(profiles.len(), 8);
    assert!(profiles.iter().all(|profile| profile.validate().is_ok()));
    assert!(profiles
        .iter()
        .all(|profile| profile.credential_binding.starts_with("credential:")));
    let mut ids: Vec<_> = profiles
        .iter()
        .map(|profile| profile.provider_id.as_str())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), profiles.len());
}

#[test]
fn trusted_builtin_endpoints_keep_provider_identity_separate_from_transport() {
    for builtin in builtin_provider_profiles() {
        let route = ModelRouteConfig::openai_compatible(
            "provider-key",
            builtin.endpoint.clone(),
            "provider-model",
        );
        let profile = ProviderProfile::from_route_config(&route).expect("profile");
        assert_eq!(profile.provider_id, builtin.provider_id);
        assert_eq!(profile.provider_family, builtin.provider_family);
        assert_eq!(profile.transport_kind, TransportKind::OpenAiCompatible);
        assert_eq!(profile.credential_binding, builtin.credential_binding);
    }
}

#[test]
fn model_descriptor_adapts_gateway_catalog_with_fail_closed_metadata() {
    let entry = ModelCatalogEntry {
        id: "provider/model".into(),
        context_tokens: Some(16_384),
        max_output_tokens: Some(2_048),
    };
    let descriptor =
        ProviderModelDescriptor::from_catalog_entry(&profile(), &entry, 7, "c".repeat(64))
            .expect("descriptor");
    assert!(descriptor.validate().is_ok());
    assert_eq!(descriptor.model_id, entry.id);
    assert_eq!(descriptor.limits.context_tokens, Some(16_384));
    assert!(descriptor.capabilities.is_empty());
    assert_eq!(descriptor.privacy, PrivacyClass::Unknown);
    assert_eq!(descriptor.usage.source, UsageSource::Unknown);

    let mut duplicate = descriptor.clone();
    duplicate.capabilities = vec![
        CapabilityFlag {
            capability: ModelCapability::Chat,
            state: CapabilityState::Supported,
            provenance: CapabilityProvenance::ProviderDeclared,
        },
        CapabilityFlag {
            capability: ModelCapability::Chat,
            state: CapabilityState::Unknown,
            provenance: CapabilityProvenance::Unknown,
        },
    ];
    assert_eq!(
        duplicate.validate(),
        Err("invalid provider model descriptor")
    );
}

#[test]
fn provider_profile_snapshot_round_trips_through_metadata_store() {
    let entry = ModelCatalogEntry {
        id: "provider/model".into(),
        context_tokens: Some(8_192),
        max_output_tokens: Some(1_024),
    };
    let profile = profile();
    let descriptor =
        ProviderModelDescriptor::from_catalog_entry(&profile, &entry, 1, "c".repeat(64))
            .expect("descriptor");
    let record = profile
        .to_storage_record(&[descriptor], 1, "d".repeat(64), 1_000)
        .expect("storage record");

    let database = rusqlite::Connection::open_in_memory().expect("sqlite");
    evohime_local_storage::provider_profile_catalog_store::install_schema(&database)
        .expect("schema");
    assert!(
        evohime_local_storage::provider_profile_catalog_store::put(&database, &record)
            .expect("write")
    );
    let stored = evohime_local_storage::provider_profile_catalog_store::get(
        &database,
        "openrouter",
        "credential:openrouter",
        "global",
    )
    .expect("read")
    .expect("snapshot");
    assert!(!String::from_utf8(stored.profile_json)
        .expect("profile json")
        .contains("secret"));
    assert!(!String::from_utf8(stored.catalog_json)
        .expect("catalog json")
        .contains("prompt"));
    assert_eq!(stored.state, "fresh");
    assert_eq!(stored.failure_code, None);
}

#[test]
fn catalog_snapshot_recovers_lifecycle_and_failure_from_store() {
    let profile = profile();
    let snapshot = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &[ModelCatalogEntry {
            id: "provider/model".into(),
            context_tokens: Some(8_192),
            max_output_tokens: Some(1_024),
        }],
        1,
        "c".repeat(64),
        1_000,
        2_000,
    )
    .expect("fresh snapshot");
    let database = rusqlite::Connection::open_in_memory().expect("sqlite");
    evohime_local_storage::provider_profile_catalog_store::install_schema(&database)
        .expect("schema");
    let record = snapshot
        .to_storage_record(&profile)
        .expect("storage record");
    assert!(
        evohime_local_storage::provider_profile_catalog_store::put(&database, &record)
            .expect("fresh write")
    );
    let stored = evohime_local_storage::provider_profile_catalog_store::get(
        &database,
        "openrouter",
        "credential:openrouter",
        "global",
    )
    .expect("fresh read")
    .expect("fresh snapshot");
    assert_eq!(
        ProviderCatalogSnapshot::from_storage_record(&stored).expect("fresh recovery"),
        snapshot
    );

    let failure = ProviderCatalogSnapshot::failure(
        &profile,
        2,
        "d".repeat(64),
        ProviderCatalogState::Unavailable,
        CatalogFailureCode::Network,
        2_000,
        3_000,
    )
    .expect("failure snapshot");
    let failure_record = failure.to_storage_record(&profile).expect("failure record");
    assert!(
        evohime_local_storage::provider_profile_catalog_store::put(&database, &failure_record)
            .expect("failure write")
    );
    let stored_failure = evohime_local_storage::provider_profile_catalog_store::get(
        &database,
        "openrouter",
        "credential:openrouter",
        "global",
    )
    .expect("failure read")
    .expect("failure snapshot");
    let recovered_failure =
        ProviderCatalogSnapshot::from_storage_record(&stored_failure).expect("recovery");
    assert_eq!(recovered_failure.state, ProviderCatalogState::Unavailable);
    assert_eq!(recovered_failure.failure, Some(CatalogFailureCode::Network));
    assert!(!recovered_failure.route_eligible_at("provider/model", 2_500));

    let missing_model = ProviderCatalogSnapshot::failure(
        &profile,
        3,
        "e".repeat(64),
        ProviderCatalogState::Unavailable,
        CatalogFailureCode::ModelNotFound,
        3_000,
        4_000,
    )
    .expect("model-not-found snapshot");
    let missing_model_record = missing_model
        .to_storage_record(&profile)
        .expect("model-not-found record");
    assert_eq!(
        missing_model_record.failure_code.as_deref(),
        Some("model_not_found")
    );
    assert!(evohime_local_storage::provider_profile_catalog_store::put(
        &database,
        &missing_model_record
    )
    .expect("model-not-found write"));
    let stored_missing_model = evohime_local_storage::provider_profile_catalog_store::get(
        &database,
        "openrouter",
        "credential:openrouter",
        "global",
    )
    .expect("model-not-found read")
    .expect("model-not-found snapshot");
    assert_eq!(
        ProviderCatalogSnapshot::from_storage_record(&stored_missing_model)
            .expect("model-not-found recovery")
            .failure,
        Some(CatalogFailureCode::ModelNotFound)
    );
}

#[test]
fn catalog_snapshot_deduplicates_deterministically_and_fails_closed_on_expiry() {
    let profile = profile();
    let entries = vec![
        ModelCatalogEntry {
            id: "provider/z".into(),
            context_tokens: Some(8_192),
            max_output_tokens: Some(1_024),
        },
        ModelCatalogEntry {
            id: "provider/a".into(),
            context_tokens: Some(1_024),
            max_output_tokens: Some(256),
        },
        ModelCatalogEntry {
            id: "provider/a".into(),
            context_tokens: Some(4_096),
            max_output_tokens: Some(512),
        },
    ];
    let snapshot = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &entries,
        3,
        "c".repeat(64),
        1_000,
        2_000,
    )
    .expect("snapshot");
    assert_eq!(
        snapshot
            .models
            .iter()
            .map(|model| model.model_id.as_str())
            .collect::<Vec<_>>(),
        vec!["provider/a", "provider/z"]
    );
    assert_eq!(snapshot.models[0].limits.context_tokens, Some(4_096));
    assert!(snapshot.route_eligible_at("provider/a", 1_500));
    assert!(!snapshot.route_eligible_at("provider/a", 2_000));

    let mut stale = snapshot;
    stale.state = ProviderCatalogState::Stale;
    assert!(stale.validate().is_ok());
    assert!(!stale.route_eligible_at("provider/a", 1_500));
}

#[test]
fn stale_catalog_preserves_models_but_never_becomes_route_eligible() {
    let profile = profile();
    let fresh = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &[ModelCatalogEntry {
            id: "provider/model".into(),
            context_tokens: Some(8_192),
            max_output_tokens: Some(1_024),
        }],
        1,
        "c".repeat(64),
        1_000,
        2_000,
    )
    .expect("fresh snapshot");
    let stale = ProviderCatalogSnapshot::stale_after_failure(
        &profile,
        &fresh,
        2,
        CatalogFailureCode::Timeout,
    )
    .expect("stale snapshot");
    assert_eq!(stale.state, ProviderCatalogState::Stale);
    assert_eq!(stale.failure, Some(CatalogFailureCode::Timeout));
    assert_eq!(stale.gateway_entries().expect("entries").len(), 1);
    assert!(!stale.route_eligible_at("provider/model", 1_500));
    let database = rusqlite::Connection::open_in_memory().expect("sqlite");
    evohime_local_storage::provider_profile_catalog_store::install_schema(&database)
        .expect("schema");
    let fresh_record = fresh.to_storage_record(&profile).expect("fresh record");
    assert!(
        evohime_local_storage::provider_profile_catalog_store::put(&database, &fresh_record)
            .expect("fresh write")
    );
    let record = stale.to_storage_record(&profile).expect("storage record");
    assert_eq!(record.state, "stale");
    assert_eq!(record.failure_code.as_deref(), Some("timeout"));
    assert!(
        evohime_local_storage::provider_profile_catalog_store::put(&database, &record)
            .expect("stale write")
    );
}

#[test]
fn catalog_failures_are_typed_and_never_replay_provider_text() {
    let error = ProviderError::Api(
        "provider response body https://provider.test contains malformed JSON".into(),
    );
    assert_eq!(
        classify_catalog_error(&error),
        CatalogFailureCode::MalformedResponse
    );
    assert_eq!(
        classify_catalog_error(&ProviderError::Api(
            "provider model catalog request failed with HTTP 404".into()
        )),
        CatalogFailureCode::DiscoveryUnsupported
    );
    assert_eq!(
        classify_catalog_error(&ProviderError::Api("provider model not found".into())),
        CatalogFailureCode::ModelNotFound
    );
    let encoded = serde_json::to_string(&classify_catalog_error(&error)).expect("code json");
    assert!(!encoded.contains("provider.test"));
    assert!(!encoded.contains("malformed JSON"));

    let failure = ProviderCatalogSnapshot::failure(
        &profile(),
        2,
        "d".repeat(64),
        ProviderCatalogState::CredentialRejected,
        CatalogFailureCode::CredentialRejected,
        1_000,
        2_000,
    )
    .expect("failure snapshot");
    assert!(failure.validate().is_ok());
    assert!(!failure.route_eligible_at("provider/model", 1_500));

    let mut inconsistent = failure;
    inconsistent.state = ProviderCatalogState::Fresh;
    assert_eq!(
        inconsistent.validate(),
        Err("inconsistent provider catalog snapshot")
    );
}

#[test]
fn free_access_evidence_is_scoped_fresh_and_strict_only_when_verified() {
    let value = evidence();
    assert!(value.validate().is_ok());
    assert_eq!(value.freshness_at(1_500), EvidenceFreshness::Fresh);
    assert!(value.is_strictly_free_at(1_500));
    assert_eq!(value.freshness_at(2_000), EvidenceFreshness::Expired);
    assert!(!value.is_strictly_free_at(2_000));

    let mut trial = value.clone();
    trial.observed_state = ObservedFreeAccessState::TrialOnly;
    trial.allowance = AllowanceKind::TrialCredit;
    trial.activation = ActivationState::NotRequired;
    assert!(trial.validate().is_ok());
    assert!(!trial.is_strictly_free_at(1_500));
}

#[test]
fn free_access_evidence_rejects_conflicting_semantics_and_raw_storage() {
    let mut invalid = evidence();
    invalid.observed_state = ObservedFreeAccessState::CreditOnly;
    assert_eq!(invalid.validate(), Err("inconsistent free access evidence"));

    let database = rusqlite::Connection::open_in_memory().expect("sqlite");
    evohime_local_storage::free_access_evidence_store::install_schema(&database).expect("schema");
    let record = evidence().to_storage_record().expect("storage record");
    assert!(
        evohime_local_storage::free_access_evidence_store::put(&database, &record)
            .expect("evidence write")
    );
    let stored = evohime_local_storage::free_access_evidence_store::get(
        &database,
        "openrouter",
        "provider/model:free",
        "cred:openrouter",
        "global",
    )
    .expect("evidence read")
    .expect("stored evidence");
    let json = String::from_utf8(stored.evidence_json).expect("json");
    assert!(!json.contains("prompt"));
    assert!(!json.contains("secret"));
}

#[test]
fn free_access_evidence_round_trips_only_when_storage_scope_matches() {
    let value = evidence();
    let record = value.to_storage_record().expect("storage record");
    let recovered = FreeAccessEvidence::from_storage_record(&record).expect("recovery");
    assert_eq!(recovered, value);

    let mut mismatched = record.clone();
    mismatched.region = "eu".into();
    assert_eq!(
        FreeAccessEvidence::from_storage_record(&mismatched),
        Err("free access evidence scope mismatch")
    );

    let mut inconsistent = record;
    inconsistent.revision = 2;
    assert_eq!(
        FreeAccessEvidence::from_storage_record(&inconsistent),
        Err("free access evidence scope mismatch")
    );
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
