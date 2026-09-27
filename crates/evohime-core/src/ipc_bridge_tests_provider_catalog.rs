use super::*;

use crate::free_provider_reliability_routing::{
    CatalogFailureCode, ProviderCatalogSnapshot, ProviderProfile,
};
use evohime_model_gateway::{ModelGatewayConfig, ModelRouteConfig};
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn event_trigger_registry_is_scoped_versioned_and_source_gated() {
    let journal_path = std::env::temp_dir().join(format!(
        "evohime-ipc-event-triggers-{}-{}.db",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let bridge = IpcBridge::new(journal.clone());
    let workspace_path = std::env::temp_dir().to_string_lossy().into_owned();
    let template = crate::workflow_templates::template("repository-research")
        .expect("repository research template");
    let mut definition = crate::event_trigger_runtime::TriggerDefinition {
        contract_version: crate::event_trigger_runtime::CONTRACT_VERSION.into(),
        trigger_id: "trigger-a".into(),
        owner_scope: "settings".into(),
        source_kind: crate::event_trigger_runtime::SourceKind::SystemEvent,
        event_kind: "task_completed".into(),
        workspace_path,
        workflow: crate::event_trigger_runtime::WorkflowBinding {
            workflow_id: "repository-research".into(),
            workflow_version: u64::from(template.version),
            execution_hash: crate::event_trigger_runtime::canonical_hash(template.graph())
                .expect("workflow hash"),
        },
        mapping: [("question".into(), "workspace".into())]
            .into_iter()
            .collect(),
        state: crate::event_trigger_runtime::TriggerState::Draft,
        content_hash: String::new(),
        created_at_ms: 1,
    };
    definition.content_hash =
        crate::event_trigger_runtime::canonical_hash(&definition).expect("definition hash");
    let response = bridge
        .dispatch_event_trigger_runtime(generated::EventTriggerRuntimeCommand {
            schema_version: 1,
            request_id: "save-1".into(),
            owner_scope: "settings".into(),
            operation: "save".into(),
            payload: serde_json::to_vec(&definition).expect("serialize definition"),
            expected_version: 0,
            idempotency_key: "save-1".into(),
        })
        .await;
    assert_eq!(response["status"], "ok");
    assert_eq!(response["version"], 1);

    let listed = bridge
        .dispatch_event_trigger_runtime(generated::EventTriggerRuntimeCommand {
            schema_version: 1,
            request_id: "list-1".into(),
            owner_scope: "settings".into(),
            operation: "list".into(),
            payload: Vec::new(),
            expected_version: 0,
            idempotency_key: "list-1".into(),
        })
        .await;
    assert_eq!(listed["triggers"][0]["version"], 1);
    assert_eq!(listed["sources"]["system_event"], "available");
    assert_eq!(listed["sources"]["local_workspace_event"], "available");

    let hidden = bridge
        .dispatch_event_trigger_runtime(generated::EventTriggerRuntimeCommand {
            schema_version: 1,
            request_id: "list-other".into(),
            owner_scope: "other".into(),
            operation: "list".into(),
            payload: Vec::new(),
            expected_version: 0,
            idempotency_key: "list-other".into(),
        })
        .await;
    assert!(hidden["triggers"].as_array().is_some_and(Vec::is_empty));

    let resumed = bridge
        .dispatch_event_trigger_runtime(generated::EventTriggerRuntimeCommand {
            schema_version: 1,
            request_id: "resume-1".into(),
            owner_scope: "settings".into(),
            operation: "resume".into(),
            payload: br#"{"trigger_id":"trigger-a"}"#.to_vec(),
            expected_version: 1,
            idempotency_key: "resume-1".into(),
        })
        .await;
    assert_eq!(resumed["status"], "ok");
    assert_eq!(resumed["trigger"]["state"], "active");

    let paused = bridge
        .dispatch_event_trigger_runtime(generated::EventTriggerRuntimeCommand {
            schema_version: 1,
            request_id: "pause-1".into(),
            owner_scope: "settings".into(),
            operation: "pause".into(),
            payload: br#"{"trigger_id":"trigger-a"}"#.to_vec(),
            expected_version: 2,
            idempotency_key: "pause-1".into(),
        })
        .await;
    assert_eq!(paused["status"], "ok");
    assert_eq!(paused["version"], 3);
    assert_eq!(paused["trigger"]["state"], "paused");

    let stale = bridge
        .dispatch_event_trigger_runtime(generated::EventTriggerRuntimeCommand {
            schema_version: 1,
            request_id: "stale-delete".into(),
            owner_scope: "settings".into(),
            operation: "delete".into(),
            payload: br#"{"trigger_id":"trigger-a"}"#.to_vec(),
            expected_version: 1,
            idempotency_key: "stale-delete".into(),
        })
        .await;
    assert_eq!(stale["error_code"], "stale_version");

    let deleted = bridge
        .dispatch_event_trigger_runtime(generated::EventTriggerRuntimeCommand {
            schema_version: 1,
            request_id: "delete-1".into(),
            owner_scope: "settings".into(),
            operation: "delete".into(),
            payload: br#"{"trigger_id":"trigger-a"}"#.to_vec(),
            expected_version: 3,
            idempotency_key: "delete-1".into(),
        })
        .await;
    assert_eq!(deleted["status"], "ok");
    assert_eq!(deleted["version"], 4);

    drop(bridge);
    drop(journal);
    let _ = std::fs::remove_file(journal_path);
}

#[tokio::test]
async fn active_system_trigger_starts_one_pinned_workflow_per_journal_event() {
    let journal_path = std::env::temp_dir().join(format!(
        "evohime-event-trigger-dispatch-{}-{}.db",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let bridge = IpcBridge::new(journal.clone());
    let template =
        crate::workflow_templates::template("repository-research").expect("workflow template");
    let mut definition = crate::event_trigger_runtime::TriggerDefinition {
        contract_version: crate::event_trigger_runtime::CONTRACT_VERSION.into(),
        trigger_id: "trigger-dispatch".into(),
        owner_scope: "settings".into(),
        source_kind: crate::event_trigger_runtime::SourceKind::SystemEvent,
        event_kind: "task_completed".into(),
        workspace_path: "C:/workspace".into(),
        workflow: crate::event_trigger_runtime::WorkflowBinding {
            workflow_id: template.template_id.clone(),
            workflow_version: u64::from(template.version),
            execution_hash: crate::event_trigger_runtime::canonical_hash(template.graph())
                .expect("workflow hash"),
        },
        mapping: [("question".into(), "workspace".into())]
            .into_iter()
            .collect(),
        state: crate::event_trigger_runtime::TriggerState::Active,
        content_hash: String::new(),
        created_at_ms: 1,
    };
    definition.content_hash =
        crate::event_trigger_runtime::canonical_hash(&definition).expect("definition hash");
    {
        let database = journal.database().lock().await;
        evohime_local_storage::put_event_trigger_definition(
            database.connection(),
            &definition.trigger_id,
            &definition.owner_scope,
            &definition,
            &definition.content_hash,
            1,
            1,
        )
        .expect("trigger persists");
    }
    let event = evohime_local_storage::EventRecord {
        sequence_id: 41,
        task_id: "ordinary-task".into(),
        event_type: "task.completed".into(),
        payload: br#"{"TaskCompleted":{"task_id":"ordinary-task","final_message":"must not be forwarded"}}"#.to_vec(),
        created_at: "2026-09-27T00:00:00Z".into(),
    };
    let mut runtime = crate::event_trigger_runtime::Runtime::default();
    bridge
        .dispatch_journalled_event_trigger(&event, &mut runtime)
        .await
        .expect("trigger dispatches");
    let run_id =
        super::ipc_bridge_event_trigger::trigger_run_id(&definition.trigger_id, event.sequence_id);
    let started = journal
        .workflow_run(&run_id)
        .await
        .expect("workflow lookup")
        .is_some();
    if !started {
        let database = journal.database().lock().await;
        let stored: Vec<u8> = database
            .connection()
            .query_row(
                "SELECT envelope_json FROM event_trigger_events WHERE event_id=?1",
                [&format!("event-{}", &run_id["event-trigger-".len()..])],
                |row| row.get(0),
            )
            .expect("outcome record exists");
        panic!(
            "workflow was not started; recorded result: {}",
            String::from_utf8_lossy(&stored)
        );
    }
    bridge
        .dispatch_journalled_event_trigger(&event, &mut runtime)
        .await
        .expect("duplicate journal event is ignored");
    let database = journal.database().lock().await;
    let events = evohime_local_storage::list_event_trigger_events(
        database.connection(),
        &definition.trigger_id,
        &definition.owner_scope,
        crate::task_memory::now_millis() as i64,
        10,
    )
    .expect("redacted history loads");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].outcome, "dispatched");
    drop(database);
    drop(bridge);
    drop(journal);
    let _ = std::fs::remove_file(journal_path);
}

#[tokio::test]
async fn active_workspace_trigger_starts_pinned_workflow_for_matching_root() {
    let journal_path = std::env::temp_dir().join(format!(
        "evohime-workspace-trigger-dispatch-{}-{}.db",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let journal = EventJournal::open(&journal_path).expect("journal opens");
    let bridge = IpcBridge::new(journal.clone());
    let workspace = tempfile::tempdir().expect("workspace creates");
    let root = workspace.path().canonicalize().expect("workspace path");
    let file_path = root.join("changed.txt");
    std::fs::write(&file_path, "changed").expect("file exists");
    let template =
        crate::workflow_templates::template("repository-research").expect("workflow template");
    let mut definition = crate::event_trigger_runtime::TriggerDefinition {
        contract_version: crate::event_trigger_runtime::CONTRACT_VERSION.into(),
        trigger_id: "trigger-workspace".into(),
        owner_scope: "settings".into(),
        source_kind: crate::event_trigger_runtime::SourceKind::LocalWorkspaceEvent,
        event_kind: "file_changed".into(),
        workspace_path: root.to_string_lossy().into_owned(),
        workflow: crate::event_trigger_runtime::WorkflowBinding {
            workflow_id: template.template_id.clone(),
            workflow_version: u64::from(template.version),
            execution_hash: crate::event_trigger_runtime::canonical_hash(template.graph())
                .expect("workflow hash"),
        },
        mapping: [("question".into(), "path".into())].into_iter().collect(),
        state: crate::event_trigger_runtime::TriggerState::Active,
        content_hash: String::new(),
        created_at_ms: 1,
    };
    definition.content_hash =
        crate::event_trigger_runtime::canonical_hash(&definition).expect("definition hash");
    {
        let database = journal.database().lock().await;
        evohime_local_storage::put_event_trigger_definition(
            database.connection(),
            &definition.trigger_id,
            &definition.owner_scope,
            &definition,
            &definition.content_hash,
            1,
            1,
        )
        .expect("trigger persists");
    }
    let mut runtime = crate::event_trigger_runtime::Runtime::default();
    bridge
        .dispatch_workspace_event_trigger(&root, &file_path, "modified", &mut runtime)
        .await
        .expect("matching filesystem event dispatches");
    let runs = journal
        .list_workflow_runs(20)
        .await
        .expect("workflow runs load");
    assert_eq!(
        runs.iter()
            .filter(|run| run.task_id.starts_with("event-trigger-"))
            .count(),
        1
    );
    drop(bridge);
    drop(journal);
    let _ = std::fs::remove_file(journal_path);
}

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
            routes: HashMap::from([(route_name, route.clone())]),
        }),
    );

    let unobserved_projection = bridge
        .provider_catalog_projection(&route, Some("model-a"))
        .await;
    assert_eq!(unobserved_projection["catalog"]["state"], "unobserved");
    assert!(unobserved_projection["catalog"]["configured_model_eligible"].is_null());

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

    let projection = bridge
        .provider_catalog_projection(&route, Some("model-a"))
        .await;
    assert_eq!(projection["catalog"]["state"], "stale");
    assert_eq!(projection["catalog"]["failure_code"], "network");
    assert_eq!(projection["catalog"]["configured_model_eligible"], false);
    assert_eq!(projection["provider"]["credential_status"], "configured");
    assert_eq!(projection["provider"]["id"], "custom_openai_compatible");
    assert_eq!(
        projection["provider"]["profile_id"],
        serde_json::Value::Null
    );
    assert_eq!(projection["models"][0]["id"], "model-a");
    let projection_text = serde_json::to_string(&projection).expect("projection serializes");
    assert!(!projection_text.contains("provider.example"));
    assert!(!projection_text.contains("test-key"));

    let account_id = "0123456789abcdef0123456789abcdef";
    let cloudflare_route = ModelRouteConfig::openai_compatible(
        "cloudflare-token",
        evohime_model_gateway::ProviderProfileId::cloudflare_base_url(account_id)
            .expect("Cloudflare account URL"),
        "@cf/meta/llama-3.1-8b-instruct",
    )
    .with_provider_profile(
        evohime_model_gateway::ProviderProfileId::CloudflareWorkersAi,
        Some(account_id.into()),
    );
    let cloudflare_projection = bridge
        .provider_catalog_projection(&cloudflare_route, Some("@cf/meta/llama-3.1-8b-instruct"))
        .await;
    assert_eq!(
        cloudflare_projection["provider"]["id"],
        "cloudflare_workers_ai"
    );
    assert_eq!(
        cloudflare_projection["provider"]["profile_id"],
        "cloudflare_workers_ai"
    );
    let cloudflare_projection_text =
        serde_json::to_string(&cloudflare_projection).expect("projection serializes");
    assert!(!cloudflare_projection_text.contains(account_id));
    assert!(!cloudflare_projection_text.contains("cloudflare-token"));

    let now_ms = crate::task_memory::now_millis();
    let expired = ProviderCatalogSnapshot::fresh_from_catalog(
        &profile,
        &[evohime_model_gateway::ModelCatalogEntry {
            id: "model-a".into(),
            context_tokens: Some(8_192),
            max_output_tokens: Some(1_024),
        }],
        3,
        "b".repeat(64),
        now_ms.saturating_sub(2_000),
        now_ms.saturating_sub(1_000),
    )
    .expect("expired snapshot");
    bridge
        .provider_catalog_snapshots
        .write()
        .expect("catalog cache write lock")
        .insert(
            crate::free_provider_reliability_routing::provider_catalog_scope_key(&profile),
            expired,
        );
    let expired_projection = bridge
        .provider_catalog_projection(&route, Some("model-a"))
        .await;
    assert_eq!(expired_projection["catalog"]["state"], "expired");
    assert_eq!(
        expired_projection["catalog"]["configured_model_eligible"],
        false
    );

    let _ = std::fs::remove_file(journal_path);
}
