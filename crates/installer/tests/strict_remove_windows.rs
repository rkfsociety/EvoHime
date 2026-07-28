#![cfg(windows)]

use evohime_installer::{remove_tree_once, remove_tree_with_retries};
use std::os::windows::fs::{symlink_dir, OpenOptionsExt};
use std::time::Duration;

#[test]
fn reports_exact_nested_path_that_cannot_be_deleted() {
    let root = tempfile::tempdir().unwrap();
    let dirty = root.path().join("EvoHime");
    let nested = dirty.join("pg16").join("data").join("base");
    std::fs::create_dir_all(&nested).unwrap();
    let locked_path = nested.join("locked.bin");
    let locked = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0)
        .open(&locked_path)
        .unwrap();

    let error = remove_tree_once(&dirty).unwrap_err();

    assert_eq!(error.path(), locked_path);
    assert!(
        matches!(error.source_error().raw_os_error(), Some(5 | 32)),
        "expected Windows access/share violation, got: {error}"
    );
    drop(locked);
}

#[tokio::test]
async fn retries_until_a_locked_file_is_released() {
    let root = tempfile::tempdir().unwrap();
    let dirty = root.path().join("EvoHime");
    std::fs::create_dir_all(&dirty).unwrap();
    let locked_path = dirty.join("locked.bin");
    let locked = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0)
        .open(&locked_path)
        .unwrap();
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(locked);
    });

    let removed = remove_tree_with_retries(&dirty, 5, Duration::from_millis(150))
        .await
        .unwrap();

    releaser.join().unwrap();
    assert!(removed);
    assert!(!dirty.exists());
}

#[test]
fn removes_junction_without_touching_its_external_target() {
    let root = tempfile::tempdir().unwrap();
    let dirty = root.path().join("EvoHime");
    let external = root.path().join("external");
    let junction = dirty.join("linked-data");
    std::fs::create_dir_all(&dirty).unwrap();
    std::fs::create_dir_all(&external).unwrap();
    let external_file = external.join("keep.txt");
    std::fs::write(&external_file, b"keep").unwrap();

    symlink_dir(&external, &junction).unwrap();

    assert!(remove_tree_once(&dirty).unwrap());

    assert!(!dirty.exists());
    assert!(external_file.exists());
}
