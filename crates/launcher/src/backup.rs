use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("pg_dump exited with status {0}")]
    DumpFailed(std::process::ExitStatus),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Runs `pg_dump` against `dsn`, writing a timestamped SQL backup into
/// `backup_dir`. Called automatically before every migration batch (раздел
/// III/XII плана) so a botched migration always has a restore path — code
/// rollback alone does not undo data corruption caused by a bad migration.
///
/// `dsn` may embed the database name (e.g. `postgres://user:pass@host:port/db`);
/// `pg_dump` accepts a full connection URI as its positional argument.
pub async fn create_backup(
    pg_dump_exe: &Path,
    dsn: &str,
    backup_dir: &Path,
    version_label: &str,
) -> Result<PathBuf, BackupError> {
    tokio::fs::create_dir_all(backup_dir).await?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let file_name = format!("backup-{version_label}-{timestamp}.sql");
    let out_path = backup_dir.join(&file_name);

    let output = Command::new(pg_dump_exe)
        .arg("--no-owner")
        .arg("--file")
        .arg(&out_path)
        .arg(dsn)
        .output()
        .await?;

    if !output.status.success() {
        return Err(BackupError::DumpFailed(output.status));
    }
    Ok(out_path)
}

/// Keeps only the `keep` most recently modified `*.sql` backups in `backup_dir`,
/// deleting older ones. Best-effort: a failure to delete an individual file is
/// logged and skipped rather than propagated — the same reasoning as the
/// retry-then-skip approach for `remove_dir_all` in раздел IX: a file
/// temporarily locked by an antivirus scan should not block normal operation.
pub async fn prune_backups(backup_dir: &Path, keep: usize) -> Result<(), BackupError> {
    let mut entries = tokio::fs::read_dir(backup_dir).await?;
    let mut files = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("sql") {
            let metadata = entry.metadata().await?;
            if let Ok(modified) = metadata.modified() {
                files.push((modified, path));
            }
        }
    }
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified)); // newest first
    for (_, path) in files.into_iter().skip(keep) {
        if let Err(err) = tokio::fs::remove_file(&path).await {
            tracing::warn!(?path, %err, "failed to prune old backup, leaving in place");
        }
    }
    Ok(())
}
