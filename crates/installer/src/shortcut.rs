//! Создание ярлыка "EvoHime Launcher" на рабочем столе — финальный
//! пользовательский шаг установки (раздел VI плана).
//!
//! Реализовано через встроенный PowerShell (`WScript.Shell` COM-объект),
//! а не через прямые COM-биндинги `IShellLinkW`/`IPersistFile` — PowerShell
//! гарантированно присутствует на любой Windows-машине, а сценарий короткий
//! и легко проверяем без риска ошибиться в низкоуровневом COM-протоколе.

use std::path::Path;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum ShortcutError {
    #[error("powershell exited with status {status}: {stderr}")]
    CommandFailed {
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Создаёт `.lnk`-ярлык `shortcut_path`, указывающий на `target_exe`, с
/// рабочей директорией — папкой, содержащей `target_exe`.
pub async fn create_shortcut(shortcut_path: &Path, target_exe: &Path) -> Result<(), ShortcutError> {
    let working_dir = target_exe
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $sc = $ws.CreateShortcut('{}'); \
         $sc.TargetPath = '{}'; \
         $sc.WorkingDirectory = '{}'; \
         $sc.Save()",
        escape_single_quotes(&shortcut_path.display().to_string()),
        escape_single_quotes(&target_exe.display().to_string()),
        escape_single_quotes(&working_dir),
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .await?;

    if !output.status.success() {
        return Err(ShortcutError::CommandFailed {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

/// PowerShell escapes a literal single quote inside a single-quoted string
/// by doubling it (`''`) — standard PowerShell quoting convention.
fn escape_single_quotes(input: &str) -> String {
    input.replace('\'', "''")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_a_real_lnk_file() {
        let dir = tempfile::tempdir().unwrap();
        let target_exe = dir.path().join("evohime-launcher.exe");
        tokio::fs::write(&target_exe, b"fake exe contents")
            .await
            .unwrap();

        let shortcut_path = dir.path().join("EvoHime Launcher.lnk");

        create_shortcut(&shortcut_path, &target_exe)
            .await
            .expect("real powershell/WScript.Shell invocation should succeed");

        let metadata = tokio::fs::metadata(&shortcut_path)
            .await
            .expect("shortcut file should exist");
        assert!(metadata.len() > 0, "a real .lnk file should be nonempty");
    }
}
