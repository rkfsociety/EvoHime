use sqlx::migrate::{MigrateError, Migrator};
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
///
/// Локальная dev-БД переиспользуется между релизами лаунчера, поэтому уже
/// применённая миграция может позже получить безобидную правку (например,
/// добавление `IF NOT EXISTS` для идемпотентности) — sqlx хранит чек-сумму
/// файла на момент применения и в этом случае отказывается продолжать
/// (`VersionMismatch`). Раз лаунчер сам это обнаружил, он же безопасно
/// сбрасывает запись о такой миграции в `_sqlx_migrations` и переигрывает её
/// заново, вместо того чтобы требовать ручного вмешательства в БД.
pub async fn apply_migrations(
    pool: &PgPool,
    migrations_dir: &Path,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Result<(), MigrationError> {
    let migrator = Migrator::new(migrations_dir)
        .await
        .map_err(|source| MigrationError::Load {
            path: migrations_dir.display().to_string(),
            source,
        })?;

    let max_attempts = migrator.migrations.len() + 1;
    for _ in 0..max_attempts {
        match migrator.run(pool).await {
            Ok(()) => return Ok(()),
            Err(MigrateError::VersionMismatch(version)) => {
                progress(&format!(
                    "Миграция {version} была изменена после применения — сбрасываю её статус и переигрываю…"
                ));
                sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
                    .bind(version)
                    .execute(pool)
                    .await
                    .map_err(MigrateError::Execute)?;
            }
            Err(err) => return Err(err.into()),
        }
    }

    Err(MigrateError::Execute(sqlx::Error::Configuration(
        "не удалось согласовать чек-суммы миграций за разумное число попыток".into(),
    ))
    .into())
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
