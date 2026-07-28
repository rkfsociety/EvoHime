//! Безопасная очистка незавершённой установки на Windows.
//!
//! Portable PostgreSQL переживает аварийно завершившийся Installer, потому
//! что `postgres.exe` не является его дочерним процессом. Перед строгим
//! удалением каталога останавливаем только экземпляр, который launcher
//! подтвердил по выделенному порту и точному пути исполняемого файла.

use crate::{
    is_installation_dirty, remove_tree_once, restore_deletable_permissions, IcaclsError,
    StrictRemoveError,
};
use evohime_launcher::postgres;
use evohime_win_support::{processes_in_directory, terminate_and_wait, ProcessCleanupError};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum DirtyCleanupError {
    #[error("не удалось остановить PostgreSQL незавершённой установки: {0}")]
    PostgresStop(String),
    #[error("не удалось найти процессы незавершённой установки: {0}")]
    ProcessSnapshot(#[source] std::io::Error),
    #[error(transparent)]
    Process(#[from] ProcessCleanupError),
    #[error(transparent)]
    Permissions(#[from] IcaclsError),
    #[error(transparent)]
    Remove(#[from] StrictRemoveError),
}

pub async fn clear_dirty_installation_safely(
    install_dir: &Path,
) -> Result<bool, DirtyCleanupError> {
    clear_dirty_installation_safely_with_progress(install_dir, |_| {}).await
}

pub async fn clear_dirty_installation_safely_with_progress<F>(
    install_dir: &Path,
    mut progress: F,
) -> Result<bool, DirtyCleanupError>
where
    F: FnMut(&str),
{
    if !is_installation_dirty(install_dir) {
        return Ok(false);
    }

    progress("Закрываю оставшиеся процессы EvoHime...");
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

    terminate_owned_processes(install_dir)?;

    progress("Восстанавливаю права незавершённой установки...");
    restore_deletable_permissions(install_dir).await?;

    progress("Очищаю незавершённую установку...");
    let mut last_error = None;
    for attempt in 0..5 {
        terminate_owned_processes(install_dir)?;
        match remove_tree_once(install_dir) {
            Ok(_) => return Ok(true),
            Err(error) => last_error = Some(error),
        }
        if attempt < 4 {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    Err(DirtyCleanupError::Remove(last_error.expect(
        "five removal attempts always produce a final error",
    )))
}

fn terminate_owned_processes(install_dir: &Path) -> Result<(), DirtyCleanupError> {
    if !install_dir.exists() {
        return Ok(());
    }
    let processes = processes_in_directory(install_dir, std::process::id())
        .map_err(DirtyCleanupError::ProcessSnapshot)?;
    if processes.is_empty() {
        return Ok(());
    }
    terminate_and_wait(&processes, Duration::from_secs(5))?;
    Ok(())
}
