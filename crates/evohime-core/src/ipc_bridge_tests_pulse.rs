use super::*;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::eva_pulse::{
    FailedWorkerContext, HiveObservationDaemon, HiveObservationOptions, ObservationCycleReport,
    ObservationProbe, ObservationTaskSink,
};
use crate::hive_state::{HIVE_OBSERVATION_OBSERVING, HIVE_OBSERVATION_WAITING_FOR_TASKS};

struct CalmProbe {
    cycles: AtomicUsize,
}

impl ObservationProbe for CalmProbe {
    fn observe(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<ObservationCycleReport, String>> + Send + '_>> {
        self.cycles.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(ObservationCycleReport {
                local_observation_interval_secs: 1,
                llm_dispatch_interval_secs: 300,
                daily_cost_nano_usd: Some(0),
                daily_budget_nano_usd: 500_000_000,
                provider_window_exhausted: false,
                provider_window_wake_at_ms: None,
                rate_limited_until_ms: None,
                failed_workers: Vec::new(),
                failed_worker_contexts: Vec::new(),
                route_is_primary: true,
                eva_workspace_path: None,
                proactive_evolution_enabled: false,
                roadmap_items: Vec::new(),
                selected_provider: Some("literouter".to_owned()),
                fallback_provider: None,
                routmy_tokens_remaining: None,
                quota_reset_utc_midnight: false,
                allow_paid_fallback: false,
                literouter_all_models_exhausted: false,
                evolution_in_flight: false,
            })
        })
    }
}

struct NoopSink;

impl ObservationTaskSink for NoopSink {
    fn dispatch_recovery(
        &self,
        _failed_workers: &[FailedWorkerContext],
        _eva_workspace_path: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
        Box::pin(async move { Err("not_used".into()) })
    }

    fn dispatch_evolution(
        &self,
        _items: &[crate::eva_pulse::RoadmapItem],
        _eva_workspace_path: Option<&str>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>> {
        Box::pin(async move { Err("not_used".into()) })
    }
}

#[tokio::test]
async fn observation_daemon_reports_status_and_wakes_on_settings_change_via_ipc() {
    let directory = tempfile::tempdir().expect("temp dir");
    let journal =
        EventJournal::open(directory.path().join("observation-ipc.db")).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(16, None, journal.clone());
    let probe = Arc::new(CalmProbe {
        cycles: AtomicUsize::new(0),
    });
    let daemon = HiveObservationDaemon::spawn(
        Arc::clone(&probe) as Arc<dyn ObservationProbe>,
        Arc::new(NoopSink) as Arc<dyn ObservationTaskSink>,
        HiveObservationOptions {
            interval_override: Some(Duration::from_secs(60)),
            llm_dispatch_interval_override: Some(Duration::from_secs(300)),
        },
        None,
    );
    daemon.run();
    let bridge = IpcBridge::with_coordinator(journal, coordinator)
        .with_observation_daemon(Arc::new(daemon.clone()));

    tokio::time::sleep(Duration::from_millis(30)).await;
    let initial_cycles = probe.cycles.load(Ordering::SeqCst);
    assert!(initial_cycles >= 1);

    let (event_type, payload) = super::ambient::ambient_call(
        &bridge,
        generated::command_envelope::Command::HiveObservationStatus(
            generated::HiveObservationStatusCommand {
                schema_version: 1,
                request_id: "observation-status".into(),
            },
        ),
    )
    .await;
    assert_eq!(event_type, "hive.observation_status");
    assert_eq!(payload["owner"], "hive_observation");
    assert_eq!(payload["status"], HIVE_OBSERVATION_WAITING_FOR_TASKS);
    assert_eq!(payload["enabled"], true);

    bridge
        .set_hive_setting("proactive_evolution_enabled", "true")
        .await
        .expect("setting saves");
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(
        probe.cycles.load(Ordering::SeqCst) > initial_cycles,
        "internal notify must wake Level 1 observation on settings change"
    );

    let snapshot = bridge.hive_state_snapshot().await;
    assert!(
        snapshot.observation_status == HIVE_OBSERVATION_WAITING_FOR_TASKS
            || snapshot.observation_status == HIVE_OBSERVATION_OBSERVING
    );
    daemon.shutdown();
}

#[tokio::test]
async fn saves_validated_eva_workspace_path_and_rejects_non_git_directory() {
    let directory = tempfile::tempdir().expect("temp dir");
    let journal =
        EventJournal::open(directory.path().join("workspace-setting.db")).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);

    let checkout = tempfile::tempdir().expect("checkout");
    let status = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(checkout.path())
        .status()
        .expect("git init");
    assert!(status.success());
    let path = checkout.path().to_string_lossy().into_owned();
    let saved = bridge
        .save_eva_workspace_path(&path)
        .await
        .expect("valid isolated git checkout saves");
    let saved: serde_json::Value = serde_json::from_slice(&saved).expect("saved payload");
    assert_eq!(saved["eva_workspace_path"], path);
    assert_eq!(saved["self_development_enabled"], true);
    assert_eq!(saved["watched_repositories"], serde_json::json!([]));
    let persisted = journal.hive_settings().await.expect("settings load");
    assert_eq!(persisted.eva_workspace_path(), Some(path.as_str()));
    assert!(persisted.watched_repositories.is_empty());

    let plain = tempfile::tempdir().expect("plain directory");
    let rejected = bridge
        .save_eva_workspace_path(&plain.path().to_string_lossy())
        .await
        .expect("invalid workspace is reported to UI");
    let rejected: serde_json::Value = serde_json::from_slice(&rejected).expect("rejection payload");
    assert_eq!(rejected["accepted"], false);
    assert_eq!(rejected["self_development_enabled"], false);
    assert!(rejected["error_code"]
        .as_str()
        .is_some_and(|error| !error.is_empty()));
    let reloaded = journal.hive_settings().await.expect("settings reload");
    assert!(reloaded.eva_workspace_path().is_none());
    assert!(reloaded.watched_repositories.is_empty());
}

#[tokio::test]
async fn set_hive_setting_daily_budget_usd() {
    let directory = tempfile::tempdir().expect("temp dir");
    let journal =
        EventJournal::open(directory.path().join("hive-budget.db")).expect("journal opens");
    let bridge = IpcBridge::new(journal.clone());

    let saved = bridge
        .set_hive_setting("daily_budget_usd", "2.5")
        .await
        .expect("valid daily budget saves");
    let saved: serde_json::Value = serde_json::from_slice(&saved).expect("saved json");
    assert_eq!(saved["accepted"], true);
    assert_eq!(saved["daily_budget_usd"], 2.5);
    assert_eq!(
        journal
            .hive_settings()
            .await
            .expect("settings load")
            .daily_budget_usd,
        2.5
    );

    let rejected = bridge
        .set_hive_setting("daily_budget_usd", "10001")
        .await
        .expect("out-of-range budget returns rejection payload");
    let rejected: serde_json::Value = serde_json::from_slice(&rejected).expect("rejected json");
    assert_eq!(rejected["accepted"], false);
    assert_eq!(
        journal
            .hive_settings()
            .await
            .expect("settings remain unchanged")
            .daily_budget_usd,
        2.5
    );
}
