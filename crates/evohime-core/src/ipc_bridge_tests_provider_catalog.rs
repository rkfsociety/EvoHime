use super::*;

use crate::free_provider_reliability_routing::{
    CatalogFailureCode, ProviderCatalogSnapshot, ProviderProfile,
};
use evohime_model_gateway::{ModelGatewayConfig, ModelRouteConfig};
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn hydrates_configured_catalog_and_uses_it_after_refresh_failure() {
    let journal_path = std::env::temp_dir().join(format!(
        "evohime-ipc-provider-catalog-recovery-{}-{}.db",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let route =
        ModelRouteConfig::openai_compatible("test-key", "https://provider.example/v1", "model-a");
    let profile = ProviderProfile::from_route_config(&route).expect("profile");
    let snapshot = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &[evohime_model_gateway::ModelCatalogEntry {
            id: "model-a".into(),
            context_tokens: Some(8_192),
            max_output_tokens: Some(1_024),
        }],
        1,
        "a".repeat(64),
        1_000,
        2_000,
    )
    .expect("snapshot");
    let record = snapshot.to_storage_record(&profile).expect("record");
    {
        let database = journal.database().lock().await;
        assert!(evohime_local_storage::provider_profile_catalog_store::put(
            database.connection(),
            &record,
        )
        .expect("snapshot stores"));
    }

    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let route_name = "default".to_string();
    let bridge = IpcBridge::with_coordinator_and_approvals(
        journal.clone(),
        coordinator,
        ApprovalCoordinator::default(),
        Arc::new(ToolRegistry::bootstrap()),
        None,
        Some(ModelGatewayConfig {
            default_route: route_name.clone(),
            routes: HashMap::from([(route_name, route)]),
        }),
    );

    assert_eq!(bridge.hydrate_provider_catalog_snapshots().await, 1);
    let recovered = bridge
        .remember_provider_catalog_snapshot(
            &ModelRouteConfig::openai_compatible(
                "test-key",
                "https://provider.example/v1",
                "model-a",
            ),
            &[],
            Some(CatalogFailureCode::Network),
        )
        .await
        .expect("stale catalog returns recovered entries");
    assert_eq!(recovered[0].id, "model-a");

    let stored = {
        let database = journal.database().lock().await;
        evohime_local_storage::provider_profile_catalog_store::get(
            database.connection(),
            &profile.provider_id,
            &profile.credential_binding,
            &profile.region,
        )
        .expect("snapshot reads")
        .expect("snapshot exists")
    };
    let persisted = ProviderCatalogSnapshot::from_storage_record(&stored).expect("stale row");
    assert_eq!(persisted.revision, 2);
    assert_eq!(
        persisted.state,
        crate::free_provider_reliability_routing::ProviderCatalogState::Stale
    );
    assert_eq!(persisted.failure, Some(CatalogFailureCode::Network));

    let _ = std::fs::remove_file(journal_path);
}
