use super::{
    apply_component_set_staged, component_manifest, restore_file, verify_installation,
    wait_for_health_with_limit, ComponentSetApply, UpdateTransaction,
};
use sha2::Digest;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("evohime-updater-{name}-{nonce}"))
}

fn write_components(dir: &Path, prefix: &str) {
    fs::create_dir_all(dir).unwrap();
    for component in UpdateTransaction::COMPONENTS {
        fs::write(dir.join(component), format!("{prefix}:{component}")).unwrap();
    }
}

#[test]
fn health_marker_accepts_only_explicit_healthy_value() {
    let root = temp_dir("health");
    fs::create_dir_all(&root).unwrap();
    let marker = root.join("health.json");
    fs::write(&marker, r#"{ "pid": 42, "healthy": true }"#).unwrap();
    wait_for_health_with_limit(&marker, std::time::Duration::from_millis(1)).unwrap();
    fs::write(&marker, r#"{"healthy":false}"#).unwrap();
    let error = wait_for_health_with_limit(&marker, std::time::Duration::ZERO).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    fs::write(&marker, vec![b'x'; super::MAX_HEALTH_FILE_BYTES + 1]).unwrap();
    let error = wait_for_health_with_limit(&marker, std::time::Duration::ZERO).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn component_set_updates_native_and_ui_together() {
    let root = temp_dir("component-set");
    let install = root.join("install");
    let staging = root.join("staging");
    let state = root.join("state");
    write_components(&install, "old");
    fs::create_dir_all(staging.join("ui-bundle")).unwrap();
    fs::write(staging.join("EvoHime.exe"), "new:shell").unwrap();
    fs::write(staging.join("ui-bundle/index.html"), "new:ui").unwrap();
    fs::write(install.join("ui-active.json"), r#"{"version":"old"}"#).unwrap();
    let selected = vec!["EvoHime.exe".to_owned()];
    apply_component_set_staged(ComponentSetApply {
        staging: &staging,
        install_dir: &install,
        state_dir: &state,
        native_selected: &selected,
        ui_version: Some("new"),
        shell_host: false,
        wait_pid: None,
        relaunch: None,
        health_file: None,
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
        "new:shell"
    );
    assert_eq!(
        fs::read_to_string(install.join("ui-bundles/new/index.html")).unwrap(),
        "new:ui"
    );
    assert_eq!(
        fs::read_to_string(install.join("ui-active.json")).unwrap(),
        r#"{"version":"new"}"#
    );
    assert_eq!(
        fs::read_to_string(install.join("evohime-core.exe")).unwrap(),
        "old:evohime-core.exe"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn shell_host_component_replaces_app_asar_and_preserves_native_files() {
    let root = temp_dir("shell-host");
    let install = root.join("install");
    let staging = root.join("staging");
    let state = root.join("state");
    write_components(&install, "old");
    fs::create_dir_all(staging.join("shell-host/resources")).unwrap();
    fs::write(staging.join("shell-host/EvoHime.exe"), "new:shell").unwrap();
    fs::write(staging.join("shell-host/resources/app.asar"), "new:asar").unwrap();
    fs::write(staging.join("shell-host.zip"), "verified archive").unwrap();

    apply_component_set_staged(ComponentSetApply {
        staging: &staging,
        install_dir: &install,
        state_dir: &state,
        native_selected: &[],
        ui_version: None,
        shell_host: true,
        wait_pid: None,
        relaunch: None,
        health_file: None,
    })
    .unwrap();

    assert_eq!(
        fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
        "new:shell"
    );
    assert_eq!(
        fs::read_to_string(install.join("resources/app.asar")).unwrap(),
        "new:asar"
    );
    assert_eq!(
        fs::read_to_string(install.join("evohime-core.exe")).unwrap(),
        "old:evohime-core.exe"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn prepare_commit_removes_backup_and_state() {
    let root = temp_dir("commit");
    let install = root.join("install");
    let state = root.join("state");
    write_components(&install, "old");

    let transaction = UpdateTransaction::prepare(&install, &state).unwrap();
    assert!(transaction.backup_dir().exists());
    assert!(transaction.operation_id().starts_with("tx-"));
    assert!(transaction.state_path().exists());

    transaction.commit().unwrap();

    assert!(!transaction.backup_dir().exists());
    assert!(!transaction.state_path().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_initial_state_write_removes_orphan_backup() {
    let root = temp_dir("prepare-state-failure");
    let install = root.join("install");
    let state = root.join("state");
    write_components(&install, "old");
    fs::create_dir_all(&state).unwrap();
    fs::create_dir(state.join("transaction.json.tmp")).unwrap();

    assert!(UpdateTransaction::prepare(&install, &state).is_err());
    let backups = fs::read_dir(&state)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("backup-"))
        .count();
    assert_eq!(backups, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rollback_restores_components_after_partial_install() {
    let root = temp_dir("rollback");
    let install = root.join("install");
    let state = root.join("state");
    write_components(&install, "old");
    let transaction = UpdateTransaction::prepare(&install, &state).unwrap();

    fs::write(install.join("EvoHime.exe"), "new").unwrap();
    fs::remove_file(install.join("evohime-core.exe")).unwrap();
    transaction.rollback().unwrap();

    for component in UpdateTransaction::COMPONENTS {
        assert_eq!(
            fs::read_to_string(install.join(component)).unwrap(),
            format!("old:{component}")
        );
    }
    assert!(!transaction.state_path().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recover_rolls_back_leftover_installing_transaction() {
    let root = temp_dir("recover");
    let install = root.join("install");
    let state = root.join("state");
    write_components(&install, "old");
    let transaction = UpdateTransaction::prepare(&install, &state).unwrap();
    fs::write(install.join("EvoHime.exe"), "interrupted").unwrap();

    let result = UpdateTransaction::recover(&state).unwrap();

    assert!(result.recovered);
    assert_eq!(
        fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
        "old:EvoHime.exe"
    );
    assert!(!transaction.state_path().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_staged_replaces_installation_and_keeps_extra_files() {
    let root = temp_dir("staged");
    let install = root.join("install");
    let staging = root.join("staging");
    let state = root.join("state");
    write_components(&install, "old");
    fs::write(install.join("user-note.txt"), "keep me").unwrap();
    write_components(&staging, "new");
    fs::create_dir_all(staging.join("resources")).unwrap();
    fs::write(staging.join("resources").join("icon.ico"), "icon").unwrap();

    super::apply_staged(super::StagedApply {
        staging: &staging,
        install_dir: &install,
        state_dir: &state,
        wait_pid: None,
        relaunch: None,
        health_file: None,
    })
    .unwrap();

    assert_eq!(
        fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
        "new:EvoHime.exe"
    );
    assert_eq!(
        fs::read_to_string(install.join("resources").join("icon.ico")).unwrap(),
        "icon"
    );
    assert_eq!(
        fs::read_to_string(install.join("user-note.txt")).unwrap(),
        "keep me"
    );
    assert!(!state.join("transaction.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_staged_refuses_an_incomplete_package() {
    let root = temp_dir("staged-incomplete");
    let install = root.join("install");
    let staging = root.join("staging");
    let state = root.join("state");
    write_components(&install, "old");
    write_components(&staging, "new");
    fs::remove_file(staging.join("evohime-core.exe")).unwrap();

    let error = super::apply_staged(super::StagedApply {
        staging: &staging,
        install_dir: &install,
        state_dir: &state,
        wait_pid: None,
        relaunch: None,
        health_file: None,
    })
    .unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    // The installation was never touched, so no transaction is left behind.
    assert_eq!(
        fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
        "old:EvoHime.exe"
    );
    assert!(!state.join("transaction.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tree_rollback_restores_a_partially_replaced_installation() {
    let root = temp_dir("tree-rollback");
    let install = root.join("install");
    let state = root.join("state");
    write_components(&install, "old");
    fs::create_dir_all(install.join("resources")).unwrap();
    fs::write(install.join("resources").join("app.asar"), "old-asar").unwrap();

    let transaction = UpdateTransaction::prepare_tree(&install, &state).unwrap();
    fs::write(install.join("resources").join("app.asar"), "broken").unwrap();
    fs::remove_file(install.join("evohime-core.exe")).unwrap();

    transaction.rollback().unwrap();

    assert_eq!(
        fs::read_to_string(install.join("resources").join("app.asar")).unwrap(),
        "old-asar"
    );
    assert_eq!(
        fs::read_to_string(install.join("evohime-core.exe")).unwrap(),
        "old:evohime-core.exe"
    );
    assert!(!transaction.state_path().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recover_rolls_back_a_leftover_tree_transaction() {
    let root = temp_dir("tree-recover");
    let install = root.join("install");
    let state = root.join("state");
    write_components(&install, "old");
    let transaction = UpdateTransaction::prepare_tree(&install, &state).unwrap();
    fs::write(install.join("EvoHime.exe"), "interrupted").unwrap();

    assert!(UpdateTransaction::recover(&state).unwrap().recovered);

    assert_eq!(
        fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
        "old:EvoHime.exe"
    );
    assert!(!transaction.state_path().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(windows)]
fn a_locked_installation_is_waited_for_and_never_half_written() {
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::{Duration, Instant};

    let root = temp_dir("locked");
    let install = root.join("install");
    write_components(&install, "old");
    let locked_path = install.join("d3dcompiler_47.dll");
    fs::write(&locked_path, "old:d3dcompiler_47.dll").unwrap();

    // FILE_SHARE_READ is how Windows holds a running image: readable by
    // anyone, writable by no one. Electron's children keep the executable
    // open like this for a moment after the main process is gone.
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&locked_path)
        .unwrap();

    let started = Instant::now();
    let error = super::wait_until_writable(&install, Duration::from_millis(600)).unwrap_err();

    assert!(started.elapsed() >= Duration::from_millis(500));
    assert!(error.to_string().contains("still in use"), "{error}");
    assert_eq!(
        fs::read_to_string(&locked_path).unwrap(),
        "old:d3dcompiler_47.dll"
    );

    // Once the handle is gone the same check passes immediately.
    drop(held);
    super::wait_until_writable(&install, Duration::from_millis(600)).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn waiting_ignores_components_a_fresh_installation_has_not_written_yet() {
    let root = temp_dir("writable");
    write_components(&root, "old");
    fs::remove_file(root.join("evohime.manifest.json")).unwrap();

    super::wait_until_writable(&root, std::time::Duration::from_millis(100)).unwrap();

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn copy_tree_rejects_excessive_directory_depth() {
    let root = temp_dir("deep-tree");
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&source).unwrap();
    let mut nested = source.clone();
    for index in 0..=super::MAX_COPY_TREE_DEPTH {
        nested = nested.join(format!("level-{index}"));
        fs::create_dir_all(&nested).unwrap();
    }
    fs::write(nested.join("payload"), "payload").unwrap();

    let error =
        super::copy_tree(&source, &destination).expect_err("excessive tree depth must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(windows)]
fn a_locked_active_update_agent_is_ignored() {
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;

    let root = temp_dir("locked-updater");
    write_components(&root, "old");
    let locked_path = root.join("evohime-updater.exe");
    fs::write(&locked_path, "active-worker").unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&locked_path)
        .unwrap();

    super::wait_until_writable(&root, Duration::from_millis(100)).unwrap();

    drop(held);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn copy_tree_preserves_active_update_agent() {
    let root = temp_dir("active-updater");
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("evohime-updater.exe"), "new-worker").unwrap();
    fs::write(source.join("EvoHime.exe"), "new-shell").unwrap();
    fs::write(destination.join("evohime-updater.exe"), "active-worker").unwrap();

    super::copy_tree(&source, &destination).unwrap();

    assert_eq!(
        fs::read_to_string(destination.join("evohime-updater.exe")).unwrap(),
        "active-worker"
    );
    assert_eq!(
        fs::read_to_string(destination.join("EvoHime.exe")).unwrap(),
        "new-shell"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn verify_installation_rejects_missing_component() {
    let root = temp_dir("verify");
    write_components(&root, "installed");
    fs::remove_file(root.join("evohime-supervisor.exe")).unwrap();

    let error = verify_installation(&root).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn selected_apply_preserves_unselected_components() {
    let root = temp_dir("selected");
    let install = root.join("install");
    let staging = root.join("staging");
    let state = root.join("state");
    write_components(&install, "old");
    write_components(&staging, "new");
    let duplicate = vec!["EvoHime.exe".to_owned(), "EvoHime.exe".to_owned()];
    let error = super::apply_selected_staged(&staging, &install, &state, None, &duplicate)
        .expect_err("duplicate selection must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    super::apply_selected_staged(
        &staging,
        &install,
        &state,
        None,
        &["EvoHime.exe".to_owned()],
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
        "new:EvoHime.exe"
    );
    assert_eq!(
        fs::read_to_string(install.join("evohime-core.exe")).unwrap(),
        "old:evohime-core.exe"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn selected_apply_requires_marker_entry_for_every_selected_component() {
    let root = temp_dir("selected-marker");
    let staging = root.join("staging");
    fs::create_dir_all(&staging).unwrap();
    let bytes = b"new:EvoHime.exe";
    fs::write(staging.join("EvoHime.exe"), bytes).unwrap();
    let digest = sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let marker = component_manifest::Manifest {
        schema: component_manifest::SCHEMA.into(),
        product: "EvoHime".into(),
        release_id: "test".into(),
        os: "windows".into(),
        architecture: "x64".into(),
        release_commit: "a".repeat(40),
        components: vec![component_manifest::Component {
            id: "shell-host".into(),
            version: "1.0.0".into(),
            artifact: "EvoHime.exe".into(),
            path: "EvoHime.exe".into(),
            size: bytes.len() as u64,
            sha256: digest,
            dependencies: vec![],
            required: true,
            protocol: "desktop-ipc-v1".into(),
            restart: "shell".into(),
        }],
    };
    fs::write(
        staging.join("evohime.components.json"),
        serde_json::to_vec(&marker).unwrap(),
    )
    .unwrap();

    let selected = vec!["evohime-core.exe".to_owned()];
    let error = super::validate_component_marker_for(&staging, Some(&selected))
        .expect_err("selected component without marker entry must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_oversized_component_marker_before_parsing() {
    let root = temp_dir("oversized-marker");
    let staging = root.join("staging");
    fs::create_dir_all(&staging).unwrap();
    fs::write(
        staging.join("evohime.components.json"),
        vec![b'x'; super::MAX_COMPONENT_MARKER_BYTES + 1],
    )
    .unwrap();

    let error = super::validate_component_marker_for(&staging, None)
        .expect_err("oversized marker must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_oversized_transaction_state_before_parsing() {
    let root = temp_dir("oversized-state");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("transaction.json"),
        vec![b'x'; super::MAX_TRANSACTION_STATE_BYTES + 1],
    )
    .unwrap();

    let error = UpdateTransaction::recover(&root)
        .expect_err("oversized transaction state must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_oversized_ui_pointer_before_starting_transaction() {
    let root = temp_dir("oversized-pointer");
    let install = root.join("install");
    let staging = root.join("staging");
    let state = root.join("state");
    write_components(&install, "old");
    fs::create_dir_all(staging.join("ui-bundle")).unwrap();
    fs::write(staging.join("evohime-core.exe"), "new").unwrap();
    fs::write(staging.join("ui-bundle/index.html"), "new").unwrap();
    fs::write(
        install.join("ui-active.json"),
        vec![b'x'; super::MAX_UI_POINTER_BYTES + 1],
    )
    .unwrap();

    let selected = vec!["evohime-core.exe".to_owned()];
    let error = super::apply_component_set_staged(super::ComponentSetApply {
        staging: &staging,
        install_dir: &install,
        state_dir: &state,
        native_selected: &selected,
        ui_version: Some("1.0.0"),
        shell_host: false,
        wait_pid: None,
        relaunch: None,
        health_file: None,
    })
    .expect_err("oversized UI pointer must be rejected before transaction");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(
        fs::metadata(install.join("ui-active.json")).unwrap().len(),
        (super::MAX_UI_POINTER_BYTES + 1) as u64
    );
    assert!(!state.join("transaction.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_recovery_state_with_external_backup_path() {
    let root = temp_dir("external-backup");
    let state_dir = root.join("state");
    fs::create_dir_all(&state_dir).unwrap();
    let external_backup = root.join("external-backup");
    fs::create_dir_all(&external_backup).unwrap();
    fs::write(external_backup.join("sentinel"), "keep").unwrap();
    let mut state = super::TransactionState {
        operation_id: "tx-test".into(),
        install_dir: root.join("install"),
        backup_dir: external_backup.clone(),
        phase: super::TransactionPhase::Installing,
        scope: super::TransactionScope::Tree,
        components: Vec::new(),
        backed_up_components: None,
        ui_target: None,
        ui_previous_pointer: None,
    };
    fs::write(
        state_dir.join("transaction.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();

    let error =
        UpdateTransaction::recover(&state_dir).expect_err("external backup paths must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(external_backup.join("sentinel").is_file());

    let local_backup = state_dir.join("not-a-backup");
    fs::create_dir_all(&local_backup).unwrap();
    fs::write(local_backup.join("sentinel"), "keep").unwrap();
    state.backup_dir = local_backup.clone();
    fs::write(
        state_dir.join("transaction.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let error = UpdateTransaction::recover(&state_dir)
        .expect_err("unexpected backup names must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(local_backup.join("sentinel").is_file());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bootstrap_transaction_accepts_missing_components_and_rolls_back_new_files() {
    let root = temp_dir("bootstrap-transaction");
    let install = root.join("install");
    let staging = root.join("staging");
    let state = root.join("state");
    fs::create_dir_all(&install).unwrap();
    fs::write(install.join("user-data.txt"), "keep").unwrap();
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("EvoHime.exe"), "new-shell").unwrap();
    let selected = vec!["EvoHime.exe".to_owned()];
    let missing_shell = root.join("missing-shell.exe");

    let error = super::apply_component_set_staged(super::ComponentSetApply {
        staging: &staging,
        install_dir: &install,
        state_dir: &state,
        native_selected: &selected,
        ui_version: None,
        shell_host: false,
        wait_pid: None,
        relaunch: Some(&missing_shell),
        health_file: None,
    })
    .expect_err("failed bootstrap must roll back");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(!install.join("EvoHime.exe").exists());
    assert_eq!(
        fs::read_to_string(install.join("user-data.txt")).unwrap(),
        "keep"
    );
    assert!(!state.join("transaction.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bootstrap_transaction_commits_missing_components() {
    let root = temp_dir("bootstrap-commit");
    let install = root.join("install");
    let staging = root.join("staging");
    let state = root.join("state");
    fs::create_dir_all(&install).unwrap();
    fs::write(install.join("user-data.txt"), "keep").unwrap();
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("EvoHime.exe"), "new-shell").unwrap();
    let selected = vec!["EvoHime.exe".to_owned()];

    super::apply_component_set_staged(super::ComponentSetApply {
        staging: &staging,
        install_dir: &install,
        state_dir: &state,
        native_selected: &selected,
        ui_version: None,
        shell_host: false,
        wait_pid: None,
        relaunch: None,
        health_file: None,
    })
    .unwrap();
    assert_eq!(
        fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
        "new-shell"
    );
    assert_eq!(
        fs::read_to_string(install.join("user-data.txt")).unwrap(),
        "keep"
    );
    assert!(!state.join("transaction.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ui_bundle_apply_publishes_pointer_last() {
    let root = temp_dir("ui-apply");
    let staging = root.join("staging");
    let install = root.join("install");
    fs::create_dir_all(staging.join("ui-bundle/assets")).unwrap();
    fs::write(staging.join("ui-bundle/index.html"), "new").unwrap();
    fs::write(staging.join("ui-bundle/assets/app.js"), "app").unwrap();
    super::apply_ui_bundle_staged(&staging, &install, "1.2.3").unwrap();
    assert_eq!(
        fs::read_to_string(install.join("ui-active.json")).unwrap(),
        r#"{"version":"1.2.3"}"#
    );
    assert_eq!(
        fs::read_to_string(install.join("ui-bundles/1.2.3/index.html")).unwrap(),
        "new"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ui_bundle_apply_restores_pointer_when_relaunch_fails() {
    let root = temp_dir("ui-rollback");
    let staging = root.join("staging");
    let install = root.join("install");
    fs::create_dir_all(staging.join("ui-bundle")).unwrap();
    fs::write(staging.join("ui-bundle/index.html"), "new").unwrap();
    fs::create_dir_all(install.join("ui-bundles/old")).unwrap();
    fs::write(install.join("ui-bundles/old/index.html"), "old").unwrap();
    fs::write(install.join("ui-active.json"), r#"{"version":"old"}"#).unwrap();

    let error = super::apply_ui_bundle_staged_with_restart(
        &staging,
        &install,
        "1.2.3",
        None,
        Some(&root.join("missing-shell.exe")),
        None,
    )
    .expect_err("failed relaunch must roll back the UI pointer");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(
        fs::read_to_string(install.join("ui-active.json")).unwrap(),
        r#"{"version":"old"}"#
    );
    assert!(!install.join("ui-bundles/1.2.3").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ui_bundle_apply_rejects_existing_version_without_replacing_it() {
    let root = temp_dir("ui-duplicate");
    let staging = root.join("staging");
    let install = root.join("install");
    fs::create_dir_all(staging.join("ui-bundle")).unwrap();
    fs::write(staging.join("ui-bundle/index.html"), "new").unwrap();
    fs::create_dir_all(install.join("ui-bundles/1.2.3")).unwrap();
    fs::write(install.join("ui-bundles/1.2.3/index.html"), "published").unwrap();

    let error = super::apply_ui_bundle_staged(&staging, &install, "1.2.3")
        .expect_err("published UI versions must be immutable");
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(
        fs::read_to_string(install.join("ui-bundles/1.2.3/index.html")).unwrap(),
        "published"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restore_file_cleans_temporary_copy_when_destination_cannot_be_removed() {
    let root = temp_dir("restore-cleanup");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source");
    let destination = root.join("destination.txt");
    fs::write(&source, "old").unwrap();
    fs::create_dir(&destination).unwrap();

    assert!(restore_file(&source, &destination).is_err());
    assert!(!destination.with_extension("rollback.tmp").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ui_bundle_validation_failure_keeps_health_marker() {
    let root = temp_dir("ui-health-preserve");
    let staging = root.join("staging");
    let install = root.join("install");
    let health = root.join("health.json");
    fs::create_dir_all(staging.join("ui-bundle")).unwrap();
    fs::write(staging.join("ui-bundle/index.html"), "new").unwrap();
    fs::write(&health, r#"{"healthy":true}"#).unwrap();

    let error = super::apply_ui_bundle_staged_with_restart(
        &staging,
        &install,
        "../invalid",
        None,
        None,
        Some(&health),
    )
    .expect_err("invalid UI version must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(health.is_file());
    fs::remove_dir_all(root).unwrap();
}
