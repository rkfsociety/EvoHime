//! Распаковка zip-артефактов (`dist.zip`, `migrations.zip`, `worker.zip`,
//! portable PostgreSQL/Python архивов) в целевую папку. Синхронная (`zip`
//! крейт не async), поэтому вызывается через `tokio::task::spawn_blocking`
//! из асинхронного кода Installer'а, чтобы не блокировать executor.

use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error("zip entry has an unsafe path: {0}")]
    UnsafePath(String),
}

/// Распаковывает `zip_path` в `dest_dir`, создавая её при необходимости.
///
/// Проверяет каждый путь внутри архива на попытки выйти за пределы
/// `dest_dir` (zip slip) — секция про целостность обновлений (раздел
/// IX/XIV плана) требует не доверять слепо содержимому скачанных
/// артефактов, даже прошедших проверку SHA256 (SHA256 подтверждает
/// целостность байтов файла, но не безопасность путей внутри архива).
pub fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<(), ExtractError> {
    std::fs::create_dir_all(dest_dir)?;
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(relative_path) = entry.enclosed_name() else {
            return Err(ExtractError::UnsafePath(entry.name().to_string()));
        };
        let out_path = safe_join(dest_dir, &relative_path)?;

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }

    Ok(())
}

/// Соединяет `dest_dir` с `relative`, отклоняя результаты, вышедшие за
/// пределы `dest_dir` (защита от zip slip, даже если `enclosed_name()`
/// внутри `zip` крейта уже частично защищает от `..`-компонентов — второй
/// независимый барьер дешевле, чем инцидент).
fn safe_join(dest_dir: &Path, relative: &Path) -> Result<PathBuf, ExtractError> {
    let joined = dest_dir.join(relative);
    if !joined.starts_with(dest_dir) {
        return Err(ExtractError::UnsafePath(relative.display().to_string()));
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_test_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn extracts_flat_files() {
        let work_dir = tempfile::tempdir().unwrap();
        let zip_path = work_dir.path().join("test.zip");
        build_test_zip(
            &zip_path,
            &[("version.txt", b"v0.1.0"), ("server.exe", b"fake binary")],
        );

        let dest = work_dir.path().join("extracted");
        extract_zip(&zip_path, &dest).unwrap();

        assert_eq!(std::fs::read(dest.join("version.txt")).unwrap(), b"v0.1.0");
        assert_eq!(
            std::fs::read(dest.join("server.exe")).unwrap(),
            b"fake binary"
        );
    }

    #[test]
    fn extracts_nested_directories() {
        let work_dir = tempfile::tempdir().unwrap();
        let zip_path = work_dir.path().join("test.zip");
        build_test_zip(
            &zip_path,
            &[
                ("worker/worker.py", b"print('hi')"),
                (
                    "worker/wheels/jsonschema-4.0.0-py3-none-any.whl",
                    b"fake wheel",
                ),
            ],
        );

        let dest = work_dir.path().join("extracted");
        extract_zip(&zip_path, &dest).unwrap();

        assert_eq!(
            std::fs::read(dest.join("worker/worker.py")).unwrap(),
            b"print('hi')"
        );
        assert_eq!(
            std::fs::read(dest.join("worker/wheels/jsonschema-4.0.0-py3-none-any.whl")).unwrap(),
            b"fake wheel"
        );
    }
}
