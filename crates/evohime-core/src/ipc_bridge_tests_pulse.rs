use super::*;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::hive_state::{
    HiveStateSnapshot, HIVE_OBSERVATION_OBSERVING, HIVE_OBSERVATION_WAITING_FOR_TASKS,
};
use evohime_hive_contract::control::{HiveObservationControlRequest, HiveObservationSnapshot};

#[cfg(windows)]
#[tokio::test]
async fn observation_daemon_reports_status_and_wakes_on_settings_change_via_ipc() {
    use tokio::net::windows::named_pipe::ServerOptions;

    let directory = tempfile::tempdir().expect("temp dir");
    let journal =
        EventJournal::open(directory.path().join("observation-ipc.db")).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(16, None, journal.clone());
    let pipe_name = format!(
        r"\\.\pipe\evohime-hive-bridge-test-{}",
        uuid::Uuid::new_v4()
    );
    let server_pipe = pipe_name.clone();
    let notify_count = Arc::new(AtomicUsize::new(0));
    let notify_counter = Arc::clone(&notify_count);
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
    let mut next_server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&server_pipe)
        .expect("create test named pipe");
    let server_task = tokio::spawn(async move {
        let mut kept_servers = Vec::with_capacity(3);
        for idx in 0..3 {
            let mut server = next_server;
            server.connect().await.expect("accept client");
            if idx < 2 {
                next_server = ServerOptions::new()
                    .create(&server_pipe)
                    .expect("create next test named pipe");
            } else {
                next_server = ServerOptions::new()
                    .create(&server_pipe)
                    .expect("create trailing test named pipe");
            }
            let req_bytes = evohime_desktop_ipc::transport::read_frame(&mut server)
                .await
                .expect("read request");
            let req: HiveObservationControlRequest =
                serde_json::from_slice(&req_bytes).expect("decode request");
            let resp_bytes = match req {
                HiveObservationControlRequest::GetHiveStateSnapshot => {
                    let mut snap = HiveStateSnapshot::default();
                    snap.observation_status = HIVE_OBSERVATION_WAITING_FOR_TASKS.to_owned();
                    serde_json::to_vec(&snap).expect("encode hive state")
                }
                HiveObservationControlRequest::NotifySettingsChanged => {
                    notify_counter.fetch_add(1, Ordering::SeqCst);
                    serde_json::to_vec(&HiveObservationSnapshot {
                        schema_version: 1,
                        owner: "hive_observation".to_owned(),
                        status: HIVE_OBSERVATION_OBSERVING.to_owned(),
                        enabled: true,
                        next_wake_at_ms: Some(12345),
                    })
                    .expect("encode snapshot")
                }
                _ => serde_json::to_vec(&HiveObservationSnapshot {
                    schema_version: 1,
                    owner: "hive_observation".to_owned(),
                    status: HIVE_OBSERVATION_WAITING_FOR_TASKS.to_owned(),
                    enabled: true,
                    next_wake_at_ms: Some(12345),
                })
                .expect("encode snapshot"),
            };
            evohime_desktop_ipc::transport::write_frame(&mut server, &resp_bytes)
                .await
                .expect("write response");
            let _ = tokio::io::AsyncWriteExt::flush(&mut server).await;
            kept_servers.push(server);
        }
        let _ = done_rx.await;
        drop(kept_servers);
        drop(next_server);
    });

    tokio::time::sleep(Duration::from_millis(20)).await;
    let proxy =
        Arc::new(crate::external_pulse::ExternalHiveObservationProxy::with_endpoint(pipe_name));
    let bridge =
        IpcBridge::with_coordinator(journal, coordinator).with_external_observation_proxy(proxy);

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
    for _ in 0..50 {
        if notify_count.load(Ordering::SeqCst) >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        notify_count.load(Ordering::SeqCst) >= 1,
        "settings change must notify external hive daemon"
    );

    let snapshot = bridge.hive_state_snapshot().await;
    assert!(
        snapshot.observation_status == HIVE_OBSERVATION_WAITING_FOR_TASKS
            || snapshot.observation_status == HIVE_OBSERVATION_OBSERVING
    );
    let _ = done_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), server_task).await;
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
