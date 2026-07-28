//! Загрузка артефактов релиза с докачкой через HTTP Range-запросы (раздел
//! VI/X плана: "обрыв интернета при загрузке" — докачка вместо перезапуска
//! с нуля). Используется для больших файлов (portable PostgreSQL, Python,
//! server.exe) на медленных/нестабильных соединениях.

use futures_util::StreamExt;
use reqwest::StatusCode;
use std::path::Path;
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use std::net::SocketAddr;
    use tower_http::services::ServeDir;

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
