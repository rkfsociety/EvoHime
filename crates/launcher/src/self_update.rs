//! Самообновление Launcher'а через `updater.exe` (раздел VIII плана):
//! Launcher не может заменить собственный исполняемый файл, пока сам
//! работает (файл заблокирован ОС), поэтому передаёт эстафету отдельному
//! вспомогательному процессу и завершается сам.

use evohime_win_support::process_start_time;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, thiserror::Error)]
pub enum SelfUpdateError {
    #[error(transparent)]
    Download(#[from] evohime_artifacts::DownloadError),
    #[error("SHA256 mismatch for launcher.exe — обновление отменено")]
    ChecksumMismatch,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("cannot determine own process start time")]
    UnknownStartTime,
}

/// Скачивает новый `launcher.exe` во временный путь, проверяя SHA256.
/// Несовпадение — файл удаляется, апдейт не продолжается.
pub async fn download_new_launcher(
    client: &reqwest::Client,
    download_url: &str,
    sha256_url: &str,
    dest: &Path,
) -> Result<(), SelfUpdateError> {
    evohime_artifacts::download_with_resume(client, download_url, dest).await?;

    let expected_sha = client
        .get(sha256_url)
        .send()
        .await
        .map_err(evohime_artifacts::DownloadError::from)?
        .text()
        .await
        .map_err(evohime_artifacts::DownloadError::from)?;

    if !evohime_artifacts::verify_sha256(dest, expected_sha.trim()).await? {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(SelfUpdateError::ChecksumMismatch);
    }
    Ok(())
}

/// Аргументы командной строки для `updater.exe`, построенные отдельно от
/// самого вызова `Command::spawn` — чтобы формирование аргументов можно
/// было проверить юнит-тестом без реального запуска процесса.
#[derive(Debug, PartialEq, Eq)]
pub struct UpdaterArgs {
    pub old_exe: PathBuf,
    pub new_exe: PathBuf,
    pub pid: u32,
    pub started_at_millis: u128,
}

impl UpdaterArgs {
    pub fn to_cli_args(&self) -> Vec<String> {
        vec![
            "--old-exe".to_string(),
            self.old_exe.display().to_string(),
            "--new-exe".to_string(),
            self.new_exe.display().to_string(),
            "--pid".to_string(),
            self.pid.to_string(),
            "--started-at".to_string(),
            self.started_at_millis.to_string(),
        ]
    }
}

/// Строит аргументы для `updater.exe`, описывающие текущий (исходный)
/// процесс Launcher'а — PID, путь к своему exe, время своего старта
/// (раздел VIII плана: все три сверяются `updater.exe`, чтобы не спутать
/// исходный процесс с другим, случайно получившим тот же PID).
pub fn build_updater_args(
    own_exe: PathBuf,
    own_pid: u32,
    new_launcher_exe: PathBuf,
) -> Result<UpdaterArgs, SelfUpdateError> {
    let own_start = process_start_time(own_pid).ok_or(SelfUpdateError::UnknownStartTime)?;
    let started_at_millis = own_start
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    Ok(UpdaterArgs {
        old_exe: own_exe,
        new_exe: new_launcher_exe,
        pid: own_pid,
        started_at_millis,
    })
}

/// Запускает `updater.exe` с посчитанными аргументами. Вызывающий код
/// должен сразу после успешного вызова завершить процесс Launcher'а —
/// именно после этого `updater.exe` начинает ждать его завершения.
pub fn spawn_updater(
    updater_exe: &Path,
    args: &UpdaterArgs,
) -> std::io::Result<std::process::Child> {
    std::process::Command::new(updater_exe)
        .args(args.to_cli_args())
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use std::net::SocketAddr;
    use tower_http::services::ServeDir;

    async fn spawn_release_server(assets_dir: &Path) -> String {
        let app = Router::new().nest_service("/", ServeDir::new(assets_dir));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn to_cli_args_produces_expected_flags() {
        let args = UpdaterArgs {
            old_exe: PathBuf::from(r"C:\EvoHime\launcher.exe"),
            new_exe: PathBuf::from(r"C:\EvoHime\launcher_new.exe"),
            pid: 4242,
            started_at_millis: 1700000000000,
        };
        assert_eq!(
            args.to_cli_args(),
            vec![
                "--old-exe",
                r"C:\EvoHime\launcher.exe",
                "--new-exe",
                r"C:\EvoHime\launcher_new.exe",
                "--pid",
                "4242",
                "--started-at",
                "1700000000000",
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn build_updater_args_uses_own_real_process_state() {
        let own_pid = std::process::id();
        let args = build_updater_args(PathBuf::from("old.exe"), own_pid, PathBuf::from("new.exe"))
            .expect("should resolve own start time");

        assert_eq!(args.pid, own_pid);
        assert!(args.started_at_millis > 0);
    }

    #[tokio::test]
    async fn download_new_launcher_succeeds_with_matching_checksum() {
        let assets_dir = tempfile::tempdir().unwrap();
        let content = b"fake launcher.exe bytes";
        tokio::fs::write(assets_dir.path().join("launcher.exe"), content)
            .await
            .unwrap();
        tokio::fs::write(
            assets_dir.path().join("launcher.exe.sha256"),
            sha256_hex(content),
        )
        .await
        .unwrap();

        let base_url = spawn_release_server(assets_dir.path()).await;
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("launcher_new.exe");

        let client = reqwest::Client::new();
        download_new_launcher(
            &client,
            &format!("{base_url}/launcher.exe"),
            &format!("{base_url}/launcher.exe.sha256"),
            &dest,
        )
        .await
        .unwrap();

        assert_eq!(tokio::fs::read(&dest).await.unwrap(), content);
    }

    #[tokio::test]
    async fn download_new_launcher_removes_file_on_checksum_mismatch() {
        let assets_dir = tempfile::tempdir().unwrap();
        tokio::fs::write(assets_dir.path().join("launcher.exe"), b"content")
            .await
            .unwrap();
        tokio::fs::write(
            assets_dir.path().join("launcher.exe.sha256"),
            "0".repeat(64),
        )
        .await
        .unwrap();

        let base_url = spawn_release_server(assets_dir.path()).await;
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("launcher_new.exe");

        let client = reqwest::Client::new();
        let result = download_new_launcher(
            &client,
            &format!("{base_url}/launcher.exe"),
            &format!("{base_url}/launcher.exe.sha256"),
            &dest,
        )
        .await;

        assert!(matches!(result, Err(SelfUpdateError::ChecksumMismatch)));
        assert!(!dest.exists(), "mismatched download must be removed");
    }
}
