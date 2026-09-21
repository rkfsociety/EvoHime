use super::*;
use crate::LocalDatabase;
use std::path::PathBuf;

fn paths(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "evohime-backup-{name}-{}-{}",
        std::process::id(),
        now_unix_ms()
    ));
    (
        root.join("events.db"),
        root.join("backup.evohime"),
        root.join("pre-restore.evohime"),
    )
}

fn cleanup(paths: &(PathBuf, PathBuf, PathBuf)) {
    if let Some(root) = paths.0.parent() {
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn backup_rotation_drops_only_expired_evohime_containers() {
    let paths = paths("retention");
    let database = LocalDatabase::open(&paths.0).expect("database opens");
    database
        .create_backup(&paths.1, "1.0.0", |_| {})
        .expect("backup writes");
    let directory = paths.1.parent().expect("backup directory").to_path_buf();

    // A foreign file in the same directory is never ours to delete.
    let foreign = directory.join("notes.txt");
    fs::write(&foreign, b"user data").expect("foreign file writes");

    let created = LocalDatabase::preview_backup(&paths.1)
        .expect("preview reads")
        .created_at_unix_ms;
    let retention_ms = 7 * 24 * 60 * 60 * 1000;

    // Inside the retention window nothing is removed.
    assert!(LocalDatabase::purge_expired_backups(
        &directory,
        retention_ms,
        created + retention_ms - 1
    )
    .expect("sweep runs")
    .is_empty());
    assert!(paths.1.exists());

    // Once the container ages past the window it is rotated out.
    let removed =
        LocalDatabase::purge_expired_backups(&directory, retention_ms, created + retention_ms + 1)
            .expect("sweep runs");
    assert_eq!(removed, vec!["backup.evohime".to_owned()]);
    assert!(!paths.1.exists());
    assert!(foreign.exists(), "unrelated files must survive rotation");

    // A missing directory is not an error: there is nothing to rotate.
    assert!(
        LocalDatabase::purge_expired_backups(directory.join("absent"), retention_ms, 0)
            .expect("missing directory is fine")
            .is_empty()
    );
    cleanup(&paths);
}

#[test]
fn preview_reads_safe_manifest_without_database_rows() {
    let paths = paths("preview");
    let database = LocalDatabase::open(&paths.0).expect("database opens");
    database
        .connection()
        .execute("CREATE TABLE marker(value TEXT NOT NULL)", [])
        .expect("marker creates");
    database
        .connection()
        .execute("INSERT INTO marker VALUES ('not in preview')", [])
        .expect("marker writes");
    database
        .create_backup(&paths.1, "core-test", |_| {})
        .expect("backup creates");
    drop(database);

    let preview = LocalDatabase::preview_backup(&paths.1).expect("preview reads");
    assert_eq!(preview.format_version, BACKUP_FORMAT_VERSION);
    assert_eq!(preview.schema_version, crate::SCHEMA_VERSION);
    assert_eq!(preview.app_version, "core-test");
    assert_eq!(preview.source_name, "events.db");
    let serialized = serde_json::to_string(&preview).expect("preview serializes");
    assert!(!serialized.contains("not in preview"));
    assert_eq!(preview.checksum_sha256.len(), 64);
    cleanup(&paths);
}

#[test]
fn cancellation_does_not_publish_partial_backup() {
    let paths = paths("cancel");
    let database = LocalDatabase::open(&paths.0).expect("database opens");
    let error = database
        .create_backup_with_cancel(&paths.1, "core-test", |_| {}, || true)
        .expect_err("backup must be cancelled");
    assert!(matches!(error, StorageError::BackupCancelled));
    assert!(!paths.1.exists());
    let partials = fs::read_dir(paths.1.parent().expect("parent"))
        .expect("parent reads")
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().contains("partial"));
    assert!(!partials);
    cleanup(&paths);
}

#[test]
fn checksum_failure_leaves_working_database_untouched() {
    let paths = paths("checksum");
    let database = LocalDatabase::open(&paths.0).expect("database opens");
    database
        .connection()
        .execute("CREATE TABLE marker(value TEXT NOT NULL)", [])
        .expect("marker creates");
    database
        .connection()
        .execute("INSERT INTO marker VALUES ('before')", [])
        .expect("marker writes");
    database
        .create_backup(&paths.1, "core-test", |_| {})
        .expect("backup creates");
    drop(database);
    let mut bytes = fs::read(&paths.1).expect("backup reads");
    let index = bytes.len() - 1;
    bytes[index] ^= 0x40;
    fs::write(&paths.1, bytes).expect("tampered backup writes");

    let mut database = LocalDatabase::open(&paths.0).expect("database reopens");
    let error = database
        .restore_backup(&paths.1, &paths.2, "core-test", |_| {})
        .expect_err("tampered backup rejects");
    assert!(matches!(error, StorageError::BackupChecksumMismatch { .. }));
    let marker: String = database
        .connection()
        .query_row("SELECT value FROM marker", [], |row| row.get(0))
        .expect("marker survives");
    assert_eq!(marker, "before");
    assert!(!paths.2.exists());
    cleanup(&paths);
}

#[test]
fn restore_creates_safety_backup_and_replaces_database_state() {
    let paths = paths("restore");
    let mut database = LocalDatabase::open(&paths.0).expect("database opens");
    database
        .connection()
        .execute("CREATE TABLE marker(value TEXT NOT NULL)", [])
        .expect("marker creates");
    database
        .connection()
        .execute("INSERT INTO marker VALUES ('before')", [])
        .expect("marker writes");
    database
        .create_backup(&paths.1, "core-test", |_| {})
        .expect("backup creates");
    database
        .connection()
        .execute("UPDATE marker SET value = 'after'", [])
        .expect("marker changes");

    let result = database
        .restore_backup(&paths.1, &paths.2, "core-test", |_| {})
        .expect("restore succeeds");
    assert_eq!(result.safety_backup_name, "pre-restore.evohime");
    assert!(paths.2.exists());
    let marker: String = database
        .connection()
        .query_row("SELECT value FROM marker", [], |row| row.get(0))
        .expect("restored marker reads");
    assert_eq!(marker, "before");
    cleanup(&paths);
}
