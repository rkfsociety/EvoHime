//! Загрузка артефактов релиза с докачкой через HTTP Range-запросы (раздел
//! VI/X плана: "обрыв интернета при загрузке" — докачка вместо перезапуска
//! с нуля). Используется для больших файлов (portable PostgreSQL, Python,
//! server.exe) на медленных/нестабильных соединениях.

use futures_util::StreamExt;
use reqwest::StatusCode;
use std::path::{Path, PathBuf};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("unexpected HTTP status {0}")]
    HttpStatus(u16),
}

/// Скачивает `url` в `dest`, докачивая с места обрыва если `dest` уже
/// частично существует (например, после сбоя сети на прошлой попытке).
///
/// Если сервер не поддерживает Range-запросы (отвечает `200 OK` вместо
/// `206 Partial Content` на запрос с частичным диапазоном), файл
/// перекачивается с нуля — существующий частичный файл усекается, чтобы не
/// получить дублирование/повреждение содержимого.
///
/// Если локальный файл уже не короче текущего удалённого артефакта и сервер
/// отвечает `416 Range Not Satisfiable`, загрузка также повторяется с нуля.
pub async fn download_with_resume(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
) -> Result<(), DownloadError> {
    let existing_len = tokio::fs::metadata(dest)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);

    let mut request = client.get(url);
    if existing_len > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing_len}-"));
    }

    let mut response = request.send().await?;
    if existing_len > 0 && response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        response = client.get(url).send().await?;
    }
    let status = response.status();

    if !status.is_success() {
        return Err(DownloadError::HttpStatus(status.as_u16()));
    }

    let server_honored_range = status == StatusCode::PARTIAL_CONTENT;
    let mut file = if existing_len > 0 && server_honored_range {
        OpenOptions::new().append(true).open(dest).await?
    } else {
        File::create(dest).await?
    };

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

/// Скачивает файл с поддержкой докачки и проверяет опубликованный SHA256.
///
/// Если докачанный файл не проходит проверку, он мог состоять из начала
/// старого релиза и хвоста нового. В таком случае файл удаляется и ровно
/// один раз скачивается целиком без `Range`.
pub async fn download_with_resume_and_verify(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: &str,
) -> Result<bool, DownloadError> {
    download_with_resume(client, url, dest).await?;
    if crate::sha256::verify_sha256(dest, expected_sha256).await? {
        return Ok(true);
    }

    tokio::fs::remove_file(dest).await?;
    download_with_resume(client, url, dest).await?;
    Ok(crate::sha256::verify_sha256(dest, expected_sha256).await?)
}

fn part_path(dest: &Path) -> PathBuf {
    let mut value = dest.as_os_str().to_os_string();
    value.push(".part");
    PathBuf::from(value)
}

async fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Всегда скачивает свежий файл целиком без HTTP Range, проверяет его во
/// временном `<dest>.part` и публикует под окончательным именем только после
/// совпадения с заново загруженным SHA256.
pub async fn download_fresh_and_verify(
    client: &reqwest::Client,
    url: &str,
    sha256_url: &str,
    dest: &Path,
) -> Result<bool, DownloadError> {
    let part = part_path(dest);
    remove_file_if_exists(dest).await?;
    remove_file_if_exists(&part).await?;

    let expected_sha256 = client
        .get(sha256_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let download_result = async {
        let response = client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(DownloadError::HttpStatus(status.as_u16()));
        }

        let mut file = File::create(&part).await?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }
        file.flush().await?;
        Ok::<(), DownloadError>(())
    }
    .await;

    if let Err(err) = download_result {
        let _ = remove_file_if_exists(&part).await;
        return Err(err);
    }

    let verified = match crate::sha256::verify_sha256(&part, expected_sha256.trim()).await {
        Ok(value) => value,
        Err(err) => {
            let _ = remove_file_if_exists(&part).await;
            return Err(err.into());
        }
    };
    if !verified {
        remove_file_if_exists(&part).await?;
        return Ok(false);
    }

    tokio::fs::rename(&part, dest).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        http::{header::RANGE, HeaderMap},
        response::IntoResponse,
        routing::get,
        Router,
    };
    use std::net::SocketAddr;
    use tower_http::services::ServeDir;

    const FRESH_CONTENT: &[u8] = b"fresh artifact";
    const FRESH_SHA256: &str = "fba70a783cecd8de271f147d7afabee99f3ee796d97a080293f0adc2fbfff0af";

    /// Spins up a real local HTTP server (axum + tower-http's `ServeDir`,
    /// which natively supports Range requests) serving `dir`, returning its
    /// base URL. Used to test resume logic against genuine Range/206
    /// semantics rather than a hand-rolled mock.
    async fn spawn_static_server(dir: &Path) -> String {
        let app = Router::new().nest_service("/", ServeDir::new(dir));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    async fn spawn_fresh_download_server(checksum: &'static str) -> String {
        let app = Router::new()
            .route(
                "/artifact.bin",
                get(|headers: HeaderMap| async move {
                    if headers.contains_key(RANGE) {
                        return StatusCode::BAD_REQUEST.into_response();
                    }
                    (StatusCode::OK, Bytes::from_static(FRESH_CONTENT)).into_response()
                }),
            )
            .route(
                "/artifact.bin.sha256",
                get(move || async move { (StatusCode::OK, checksum) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn fresh_download_replaces_old_files_without_range() {
        let base_url = spawn_fresh_download_server(FRESH_SHA256).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("artifact.bin");
        let part = dir.path().join("artifact.bin.part");
        tokio::fs::write(&dest, b"old release").await.unwrap();
        tokio::fs::write(&part, b"interrupted old release")
            .await
            .unwrap();

        let ok = download_fresh_and_verify(
            &reqwest::Client::new(),
            &format!("{base_url}/artifact.bin"),
            &format!("{base_url}/artifact.bin.sha256"),
            &dest,
        )
        .await
        .unwrap();

        assert!(ok);
        assert_eq!(tokio::fs::read(&dest).await.unwrap(), FRESH_CONTENT);
        assert!(!part.exists());
    }

    #[tokio::test]
    async fn fresh_download_removes_part_when_checksum_is_wrong() {
        let bad_sha = "0000000000000000000000000000000000000000000000000000000000000000";
        let base_url = spawn_fresh_download_server(bad_sha).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("artifact.bin");
        let part = dir.path().join("artifact.bin.part");
        tokio::fs::write(&dest, b"old release").await.unwrap();

        let ok = download_fresh_and_verify(
            &reqwest::Client::new(),
            &format!("{base_url}/artifact.bin"),
            &format!("{base_url}/artifact.bin.sha256"),
            &dest,
        )
        .await
        .unwrap();

        assert!(!ok);
        assert!(!dest.exists());
        assert!(!part.exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn fresh_download_propagates_failure_to_remove_old_file() {
        use std::os::windows::fs::OpenOptionsExt;

        let base_url = spawn_fresh_download_server(FRESH_SHA256).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("artifact.bin");
        let locked = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .share_mode(0)
            .open(&dest)
            .unwrap();

        let result = download_fresh_and_verify(
            &reqwest::Client::new(),
            &format!("{base_url}/artifact.bin"),
            &format!("{base_url}/artifact.bin.sha256"),
            &dest,
        )
        .await;

        assert!(matches!(result, Err(DownloadError::Io(_))));
        drop(locked);
    }

    #[tokio::test]
    async fn downloads_full_file_when_nothing_exists_yet() {
        let source_dir = tempfile::tempdir().unwrap();
        let content = b"the quick brown fox jumps over the lazy dog".repeat(1000);
        tokio::fs::write(source_dir.path().join("artifact.bin"), &content)
            .await
            .unwrap();

        let base_url = spawn_static_server(source_dir.path()).await;

        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("artifact.bin");

        let client = reqwest::Client::new();
        download_with_resume(&client, &format!("{base_url}/artifact.bin"), &dest)
            .await
            .unwrap();

        let downloaded = tokio::fs::read(&dest).await.unwrap();
        assert_eq!(downloaded, content);
    }

    #[tokio::test]
    async fn resumes_partial_download_without_corruption() {
        let source_dir = tempfile::tempdir().unwrap();
        let content = b"0123456789".repeat(10_000); // 100_000 bytes, deterministic pattern
        tokio::fs::write(source_dir.path().join("artifact.bin"), &content)
            .await
            .unwrap();

        let base_url = spawn_static_server(source_dir.path()).await;

        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("artifact.bin");

        // Simulate a prior attempt that got cut off after the first half.
        let half = content.len() / 2;
        tokio::fs::write(&dest, &content[..half]).await.unwrap();

        let client = reqwest::Client::new();
        download_with_resume(&client, &format!("{base_url}/artifact.bin"), &dest)
            .await
            .unwrap();

        let downloaded = tokio::fs::read(&dest).await.unwrap();
        assert_eq!(
            downloaded.len(),
            content.len(),
            "resumed file must match original length exactly, no duplication"
        );
        assert_eq!(
            downloaded, content,
            "resumed file content must exactly match source"
        );
    }

    #[tokio::test]
    async fn restarts_download_when_existing_file_is_larger_than_remote() {
        let source_dir = tempfile::tempdir().unwrap();
        let content = b"current release artifact";
        tokio::fs::write(source_dir.path().join("artifact.bin"), content)
            .await
            .unwrap();

        let base_url = spawn_static_server(source_dir.path()).await;

        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("artifact.bin");
        tokio::fs::write(&dest, b"stale release artifact that is larger")
            .await
            .unwrap();

        let client = reqwest::Client::new();
        download_with_resume(&client, &format!("{base_url}/artifact.bin"), &dest)
            .await
            .unwrap();

        let downloaded = tokio::fs::read(&dest).await.unwrap();
        assert_eq!(downloaded, content);
    }

    #[tokio::test]
    async fn returns_error_for_missing_file() {
        let source_dir = tempfile::tempdir().unwrap();
        let base_url = spawn_static_server(source_dir.path()).await;

        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("missing.bin");

        let client = reqwest::Client::new();
        let result = download_with_resume(&client, &format!("{base_url}/nope.bin"), &dest).await;
        assert!(matches!(result, Err(DownloadError::HttpStatus(404))));
    }
}
