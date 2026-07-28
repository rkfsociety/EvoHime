#![cfg(windows)]

use evohime_installer::clear_dirty_installation;
use std::os::windows::fs::OpenOptionsExt;

#[tokio::test]
async fn removes_dirty_installation_before_continuing() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    tokio::fs::create_dir_all(&install_dir).await.unwrap();
    tokio::fs::write(install_dir.join("partial.bin"), b"partial")
        .await
        .unwrap();

    assert!(clear_dirty_installation(&install_dir).await.unwrap());
    assert!(!install_dir.exists());
}

#[tokio::test]
async fn reports_error_when_dirty_installation_cannot_be_removed() {
    let root = tempfile::tempdir().unwrap();
    let install_dir = root.path().join("EvoHime");
    tokio::fs::create_dir_all(&install_dir).await.unwrap();
    let locked_path = install_dir.join("locked.bin");
    let locked = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0)
        .open(&locked_path)
        .unwrap();

    let result = clear_dirty_installation(&install_dir).await;

    assert!(result.is_err());
    assert!(install_dir.exists());
    drop(locked);
}
