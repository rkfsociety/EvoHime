//! Проверка целостности скачанных артефактов (раздел IV/VI плана): каждый
//! файл релиза сверяется с опубликованным `*.sha256` перед распаковкой —
//! при несовпадении установка/обновление прерывается, ничего не трогая.

use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};

/// Вычисляет SHA256 файла потоково (не грузя весь файл в память — важно
/// для сотен мегабайт portable PostgreSQL/Python).
pub async fn compute_sha256(path: &Path) -> std::io::Result<String> {
    let file = File::open(path).await?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];

    loop {
        let read = reader.read(&mut buf).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(hex_encode(&hasher.finalize()))
}

/// Сравнивает вычисленный SHA256 файла с ожидаемым (регистронезависимо,
/// как публикуют большинство инструментов — `sha256sum` пишет в нижнем
/// регистре, но чужие инструменты могут использовать верхний).
pub async fn verify_sha256(path: &Path, expected_hex: &str) -> std::io::Result<bool> {
    let actual = compute_sha256(path).await?;
    Ok(actual.eq_ignore_ascii_case(expected_hex.trim()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn computes_known_sha256_for_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.bin");
        tokio::fs::File::create(&path).await.unwrap();

        let digest = compute_sha256(&path).await.unwrap();
        // SHA256 of an empty input, cross-checked against `sha256sum` on
        // this machine (independent oracle, not just re-testing sha2 with
        // itself).
        assert_eq!(
            digest,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn computes_known_sha256_for_abc() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.bin");
        let mut file = tokio::fs::File::create(&path).await.unwrap();
        file.write_all(b"abc").await.unwrap();
        file.flush().await.unwrap();

        let digest = compute_sha256(&path).await.unwrap();
        // SHA256("abc"), cross-checked against `sha256sum` on this machine.
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn verify_accepts_case_insensitive_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.bin");
        let mut file = tokio::fs::File::create(&path).await.unwrap();
        file.write_all(b"abc").await.unwrap();
        file.flush().await.unwrap();

        let uppercase_expected = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
        assert!(verify_sha256(&path, uppercase_expected).await.unwrap());
    }

    #[tokio::test]
    async fn verify_rejects_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.bin");
        let mut file = tokio::fs::File::create(&path).await.unwrap();
        file.write_all(b"abc").await.unwrap();
        file.flush().await.unwrap();

        assert!(!verify_sha256(
            &path,
            "0000000000000000000000000000000000000000000000000000000000000"
        )
        .await
        .unwrap());
    }
}
