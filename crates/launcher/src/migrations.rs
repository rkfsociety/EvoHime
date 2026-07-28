use sqlx::migrate::Migrator;
use sqlx::PgPool;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("failed to load migrations from {path}: {source}")]
    Load {
        path: String,
        #[source]
        source: sqlx::migrate::MigrateError,
    },
    #[error("failed to apply migrations: {0}")]
    Apply(#[from] sqlx::migrate::MigrateError),
}

/// Applies every `.sql` file found in `migrations_dir` against `pool`.
///
/// This uses the runtime `Migrator::new(path)` API rather than the compile-time
/// `sqlx::migrate!` macro used by `evohime-server`, because the Launcher applies
/// migrations from a directory extracted at runtime from a downloaded
/// `migrations.zip` (раздел III плана) — the path is not known until the
/// version is unpacked, so it cannot be baked in at compile time. This also
/// means the user's machine never needs `sqlx-cli` installed.
pub async fn apply_migrations(pool: &PgPool, migrations_dir: &Path) -> Result<(), MigrationError> {
    let migrator = Migrator::new(migrations_dir)
        .await
        .map_err(|source| MigrationError::Load {
            path: migrations_dir.display().to_string(),
            source,
        })?;
    migrator.run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn errors_cleanly_on_missing_directory() {
        let missing = Path::new("this/path/does/not/exist");
        let result = Migrator::new(missing).await;
        assert!(result.is_err());
    }
}
