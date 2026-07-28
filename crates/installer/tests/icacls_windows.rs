#![cfg(windows)]

use evohime_installer::{restore_deletable_permissions, restrict_to_current_user};

#[tokio::test]
async fn restricts_real_temp_directory_without_error() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("pgdata");
    std::fs::create_dir_all(&target).unwrap();

    let result = restrict_to_current_user(&target).await;
    assert!(
        result.is_ok(),
        "icacls should succeed on a real writable dir: {result:?}"
    );
}

#[tokio::test]
async fn grants_current_user_access_to_nested_directories() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    std::fs::create_dir(&data).unwrap();

    restrict_to_current_user(&data).await.unwrap();

    let acl = std::process::Command::new("icacls")
        .arg(&data)
        .output()
        .unwrap();
    assert!(acl.status.success());
    let acl_text = String::from_utf8_lossy(&acl.stdout);
    assert!(
        acl_text.contains("(OI)(CI)(F)"),
        "expected inheritable full-control grant, got: {acl_text}"
    );

    let nested = data.join("nested");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("probe.txt"), b"ok").unwrap();
}

#[tokio::test]
async fn restores_inherited_permissions_before_dirty_tree_removal() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let nested = data.join("base").join("1");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("probe.bin"), b"locked-down").unwrap();

    restrict_to_current_user(&data).await.unwrap();
    restore_deletable_permissions(&data).await.unwrap();

    let acl = std::process::Command::new("icacls")
        .arg(&nested)
        .output()
        .unwrap();
    assert!(acl.status.success());
    let acl_text = String::from_utf8_lossy(&acl.stdout);
    assert!(
        acl_text.contains("(I)"),
        "expected inherited permissions after reset, got: {acl_text}"
    );

    std::fs::remove_dir_all(&data).unwrap();
    assert!(!data.exists());
}
