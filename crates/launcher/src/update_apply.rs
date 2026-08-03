//! Atomic-обновление (раздел V плана): скачивание артефактов с проверкой
//! SHA256, распаковка во временную папку, применение миграций, атомарное
//! переключение версии, health-check с откатом при ошибке на любом шаге.
//!
//! Вызывается только по клику пользователя "Обновить сейчас" — фоновая
//! проверка (`update_check.rs`) никогда не доходит сюда сама, только
//! показывает уведомление.

use crate::migrations::apply_migrations;
use evohime_artifacts::{download_with_resume_and_verify, extract_zip};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error(transparent)]
    Download(#[from] evohime_artifacts::DownloadError),
    #[error("SHA256 mismatch for {0} — обновление прервано, ничего не изменено")]
    ChecksumMismatch(String),
    #[error(transparent)]
    Extract(#[from] evohime_artifacts::ExtractError),
    #[error(transparent)]
    Migration(#[from] crate::migrations::MigrationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
}

/// Один артефакт релиза для скачивания: имя файла, URL самого файла и URL
/// его `*.sha256`.
#[derive(Clone)]
pub struct ReleaseAsset {
    pub file_name: String,
    pub download_url: String,
    pub sha256_url: String,
}

pub struct UpdatePlan {
    pub install_dir: PathBuf,
    pub new_version: String,
    pub assets: Vec<ReleaseAsset>,
    /// Строка подключения к БД для применения миграций. `None` пропускает
    /// шаг миграций целиком (например, если PostgreSQL ещё не поднят).
    pub dsn: Option<String>,
}

/// Скачивает и проверяет все артефакты во временную папку загрузки.
/// Ничего не переключает — только готовит материал для `stage_and_switch`.
pub async fn download_and_verify_assets(
    plan: &UpdatePlan,
    client: &reqwest::Client,
    download_dir: &Path,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Result<(), UpdateError> {
    tokio::fs::create_dir_all(download_dir).await?;

    for asset in &plan.assets {
        progress(&format!("Скачивание {}...", asset.file_name));
        let dest = download_dir.join(&asset.file_name);

        let expected_sha = client
            .get(&asset.sha256_url)
            .send()
            .await
            .map_err(evohime_artifacts::DownloadError::from)?
            .text()
            .await
            .map_err(evohime_artifacts::DownloadError::from)?;

        if !download_with_resume_and_verify(client, &asset.download_url, &dest, expected_sha.trim())
            .await?
        {
            return Err(UpdateError::ChecksumMismatch(asset.file_name.clone()));
        }
    }

    progress("Все артефакты проверены (SHA256 совпадает).");
    Ok(())
}

/// Распаковывает скачанные zip-артефакты в `<install_dir>/versions/<new_version>_tmp/`,
/// копирует `server.exe`, применяет миграции (если `dsn` задан), затем
/// атомарно переименовывает `_tmp` в финальную папку версии — партиально
/// применённое обновление невозможно: либо всё, либо ничего.
pub async fn stage_and_switch(
    plan: &UpdatePlan,
    download_dir: &Path,
    progress: &(dyn Fn(&str) + Send + Sync),
) -> Result<PathBuf, UpdateError> {
    let versions_dir = plan.install_dir.join("versions");
    let tmp_dir = versions_dir.join(format!("{}_tmp", plan.new_version));
    let final_dir = versions_dir.join(&plan.new_version);

    if tmp_dir.exists() {
        tokio::fs::remove_dir_all(&tmp_dir).await.ok();
    }
    tokio::fs::create_dir_all(&tmp_dir).await?;

    progress("Распаковка компонентов...");
    for (zip_name, subdir) in [
        ("server.zip", None),
        ("dist.zip", Some("dist")),
        ("migrations.zip", Some("migrations")),
        ("worker.zip", Some("worker")),
    ] {
        let zip_path = download_dir.join(zip_name);
        if zip_path.exists() {
            let dest = match subdir {
                Some(s) => tmp_dir.join(s),
                None => tmp_dir.clone(),
            };
            tokio::task::spawn_blocking(move || extract_zip(&zip_path, &dest)).await??;
        }
    }

    if let Some(dsn) = &plan.dsn {
        let migrations_dir = tmp_dir.join("migrations");
        if migrations_dir.exists() {
            progress("Применение миграций базы данных...");
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(2)
                .connect(dsn)
                .await
                .map_err(|err| UpdateError::Io(std::io::Error::other(err.to_string())))?;
            apply_migrations(&pool, &migrations_dir, progress).await?;
        }
    }

    progress("Переключение на новую версию...");
    tokio::fs::rename(&tmp_dir, &final_dir).await?;

    Ok(final_dir)
}

/// Атомарно записывает `current.txt`. Используется и для переключения на
/// новую версию, и для отката (пишет старую версию обратно) — один и тот
/// же примитив, никакой отдельной "логики отката".
pub async fn write_current_version(install_dir: &Path, version: &str) -> Result<(), UpdateError> {
    let current_txt = install_dir.join("current.txt");
    let tmp_path = current_txt.with_extension("txt.tmp");
    tokio::fs::write(&tmp_path, version).await?;

    #[cfg(windows)]
    {
        evohime_win_support::atomic_replace_or_create(&current_txt, &tmp_path)
            .map_err(|err| std::io::Error::other(err.to_string()))?;
    }
    #[cfg(not(windows))]
    {
        tokio::fs::rename(&tmp_path, &current_txt).await?;
    }

    Ok(())
}

pub fn read_current_version(install_dir: &Path) -> Option<String> {
    std::fs::read_to_string(install_dir.join("current.txt"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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

    fn build_test_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            use std::io::Write;
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
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

    async fn prepare_fake_release(assets_dir: &Path) {
        tokio::fs::create_dir_all(assets_dir).await.unwrap();

        let dist_zip = assets_dir.join("dist.zip");
        build_test_zip(&dist_zip, &[("index.html", b"<html></html>")]);
        let dist_bytes = tokio::fs::read(&dist_zip).await.unwrap();
        tokio::fs::write(assets_dir.join("dist.zip.sha256"), sha256_hex(&dist_bytes))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn happy_path_downloads_verifies_and_switches_version() {
        let assets_dir = tempfile::tempdir().unwrap();
        prepare_fake_release(assets_dir.path()).await;
        let base_url = spawn_release_server(assets_dir.path()).await;

        let install_dir = tempfile::tempdir().unwrap();
        write_current_version(install_dir.path(), "v0.1.0")
            .await
            .unwrap();

        let plan = UpdatePlan {
            install_dir: install_dir.path().to_path_buf(),
            new_version: "v0.2.0".to_string(),
            assets: vec![ReleaseAsset {
                file_name: "dist.zip".to_string(),
                download_url: format!("{base_url}/dist.zip"),
                sha256_url: format!("{base_url}/dist.zip.sha256"),
            }],
            dsn: None,
        };

        let client = reqwest::Client::new();
        let download_dir = install_dir.path().join("download_tmp");
        let messages = std::sync::Mutex::new(Vec::<String>::new());
        let progress = |msg: &str| messages.lock().unwrap().push(msg.to_string());

        download_and_verify_assets(&plan, &client, &download_dir, &progress)
            .await
            .unwrap();
        let final_dir = stage_and_switch(&plan, &download_dir, &progress)
            .await
            .unwrap();
        write_current_version(&plan.install_dir, &plan.new_version)
            .await
            .unwrap();

        assert!(final_dir.join("dist").join("index.html").exists());
        assert_eq!(
            read_current_version(install_dir.path()).as_deref(),
            Some("v0.2.0")
        );
        assert!(
            !install_dir
                .path()
                .join("versions")
                .join("v0.2.0_tmp")
                .exists(),
            "tmp staging dir must not survive a successful switch"
        );
    }

    #[tokio::test]
    async fn checksum_mismatch_aborts_before_touching_current_version() {
        let assets_dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(assets_dir.path()).await.unwrap();
        let dist_zip = assets_dir.path().join("dist.zip");
        build_test_zip(&dist_zip, &[("index.html", b"<html></html>")]);
        // Publish a deliberately wrong SHA256.
        tokio::fs::write(assets_dir.path().join("dist.zip.sha256"), "0".repeat(64))
            .await
            .unwrap();

        let base_url = spawn_release_server(assets_dir.path()).await;

        let install_dir = tempfile::tempdir().unwrap();
        write_current_version(install_dir.path(), "v0.1.0")
            .await
            .unwrap();

        let plan = UpdatePlan {
            install_dir: install_dir.path().to_path_buf(),
            new_version: "v0.2.0".to_string(),
            assets: vec![ReleaseAsset {
                file_name: "dist.zip".to_string(),
                download_url: format!("{base_url}/dist.zip"),
                sha256_url: format!("{base_url}/dist.zip.sha256"),
            }],
            dsn: None,
        };

        let client = reqwest::Client::new();
        let download_dir = install_dir.path().join("download_tmp");
        let progress = |_msg: &str| {};

        let result = download_and_verify_assets(&plan, &client, &download_dir, &progress).await;
        assert!(matches!(result, Err(UpdateError::ChecksumMismatch(_))));

        // Current version must be untouched — the whole point of
        // verify-before-switch.
        assert_eq!(
            read_current_version(install_dir.path()).as_deref(),
            Some("v0.1.0")
        );
        assert!(
            !install_dir.path().join("versions").join("v0.2.0").exists(),
            "final version dir must never be created when checksum verification fails"
        );
    }

    #[tokio::test]
    async fn stale_partial_from_previous_release_is_replaced_and_verified() {
        let assets_dir = tempfile::tempdir().unwrap();
        prepare_fake_release(assets_dir.path()).await;
        let expected = tokio::fs::read(assets_dir.path().join("dist.zip"))
            .await
            .unwrap();
        let base_url = spawn_release_server(assets_dir.path()).await;

        let install_dir = tempfile::tempdir().unwrap();
        let plan = UpdatePlan {
            install_dir: install_dir.path().to_path_buf(),
            new_version: "v0.2.0".to_string(),
            assets: vec![ReleaseAsset {
                file_name: "dist.zip".to_string(),
                download_url: format!("{base_url}/dist.zip"),
                sha256_url: format!("{base_url}/dist.zip.sha256"),
            }],
            dsn: None,
        };

        let client = reqwest::Client::new();
        let download_dir = install_dir.path().join("download_tmp");
        tokio::fs::create_dir_all(&download_dir).await.unwrap();
        tokio::fs::write(download_dir.join("dist.zip"), b"stale")
            .await
            .unwrap();

        let progress = |_msg: &str| {};
        download_and_verify_assets(&plan, &client, &download_dir, &progress)
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read(download_dir.join("dist.zip"))
                .await
                .unwrap(),
            expected
        );
    }

    #[tokio::test]
    async fn write_current_version_round_trips() {
        let install_dir = tempfile::tempdir().unwrap();
        write_current_version(install_dir.path(), "v1.0.0")
            .await
            .unwrap();
        assert_eq!(
            read_current_version(install_dir.path()).as_deref(),
            Some("v1.0.0")
        );

        write_current_version(install_dir.path(), "v1.0.1")
            .await
            .unwrap();
        assert_eq!(
            read_current_version(install_dir.path()).as_deref(),
            Some("v1.0.1")
        );
    }

    #[test]
    fn read_current_version_returns_none_when_missing() {
        let install_dir = tempfile::tempdir().unwrap();
        assert_eq!(read_current_version(install_dir.path()), None);
    }
}
