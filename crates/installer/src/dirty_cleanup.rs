//! Безопасная очистка незавершённой установки на Windows.
//!
//! Portable PostgreSQL переживает аварийно завершившийся Installer, потому
//! что `postgres.exe` не является его дочерним процессом. Перед строгим
//! удалением каталога останавливаем только экземпляр, который launcher
//! подтвердил по выделенному порту и точному пути исполняемого файла.

use crate::{clear_dirty_installation, is_installation_dirty};
use evohime_launcher::postgres;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DirtyCleanupError {
    #[error("не удалось остановить PostgreSQL незавершённой установки: {0}")]
    PostgresStop(String),
    #[error(transparent)]
    Remove(#[from] std::io::Error),
}

pub async fn clear_dirty_installation_safely(
    install_dir: &Path,
) -> Result<bool, DirtyCleanupError> {
    if !is_installation_dirty(install_dir) {
        return Ok(false);
    }

    let pg_bin_dir = install_dir.join("pg16").join("bin");
    let pg_data_dir = install_dir.join("pg16").join("data");
    if pg_bin_dir.is_dir()
        && pg_data_dir.is_dir()
        && postgres::is_running(&pg_bin_dir, postgres::PG_PORT)
    {
        postgres::stop(&pg_bin_dir, &pg_data_dir)
            .await
            .map_err(|error| DirtyCleanupError::PostgresStop(error.to_string()))?;
    }

    clear_dirty_installation(install_dir)
        .await
        .map_err(DirtyCleanupError::Remove)
}
