//! Защита от прерванной установки (BSOD/сбой питания на любом шаге) через
//! файл-маркер `.setup_complete` (раздел VI плана).
//!
//! Финальный шаг успешной установки — создание пустого файла-маркера в
//! корне папки установки. Если папка существует, но маркера нет — установка
//! считается "грязной" (прервана на полпути), и Installer должен полностью
//! удалить её и начать распаковку заново, не пытаясь угадать, что уже
//! успело установиться.

use std::path::Path;

const MARKER_FILE_NAME: &str = ".setup_complete";

/// `true`, если папка установки существует, но не завершена успешно
/// (маркер отсутствует) — вызывающий код должен удалить её перед повторной
/// установкой.
pub fn is_installation_dirty(install_dir: &Path) -> bool {
    install_dir.exists() && !marker_path(install_dir).exists()
}

/// Полностью удаляет незавершённую установку. Ошибка удаления возвращается
/// вызывающему коду: продолжать поверх частично очищенного каталога нельзя.
pub async fn clear_dirty_installation(install_dir: &Path) -> std::io::Result<bool> {
    if !is_installation_dirty(install_dir) {
        return Ok(false);
    }
    tokio::fs::remove_dir_all(install_dir).await?;
    Ok(true)
}

/// Помечает установку как успешно завершённую. Должен вызываться строго
/// последним шагом — после ярлыка, миграций и всего остального.
pub async fn mark_setup_complete(install_dir: &Path) -> std::io::Result<()> {
    tokio::fs::write(marker_path(install_dir), b"").await
}

fn marker_path(install_dir: &Path) -> std::path::PathBuf {
    install_dir.join(MARKER_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_nonexistent_dir_is_not_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let install_dir = dir.path().join("EvoHime");
        assert!(!install_dir.exists());
        assert!(!is_installation_dirty(&install_dir));
    }

    #[test]
    fn dir_without_marker_is_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let install_dir = dir.path().join("EvoHime");
        std::fs::create_dir_all(&install_dir).unwrap();
        // Simulate a partially unpacked install: some files present, but
        // the process was interrupted before the marker step.
        std::fs::write(install_dir.join("server.exe"), b"partial").unwrap();

        assert!(is_installation_dirty(&install_dir));
    }

    #[tokio::test]
    async fn dir_with_marker_is_not_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let install_dir = dir.path().join("EvoHime");
        std::fs::create_dir_all(&install_dir).unwrap();

        mark_setup_complete(&install_dir).await.unwrap();

        assert!(!is_installation_dirty(&install_dir));
    }
}
