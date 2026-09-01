use evohime_core::workspace_state_checkpoints::{capture, compare, restore, CheckpointError};
use std::{fs, path::PathBuf};

fn workspace() -> PathBuf {
    let path = std::env::temp_dir().join(format!("evohime-plan58-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn restart_like_reload_preserves_checkpoint_and_detects_conflict() {
    let path = workspace();
    fs::write(path.join("file.txt"), b"before").unwrap();
    let checkpoint = capture(&path, "checkpoint-1", "workspace-1", Some("task-1".into())).unwrap();
    let encoded = serde_json::to_vec(&checkpoint).unwrap();
    let reloaded = serde_json::from_slice(&encoded).unwrap();
    assert!(compare(&path, &reloaded).unwrap().is_empty());
    fs::write(path.join("file.txt"), b"user change").unwrap();
    assert!(matches!(
        restore(&path, &reloaded),
        Err(CheckpointError::Conflicts(_))
    ));
    fs::remove_dir_all(path).unwrap();
}
