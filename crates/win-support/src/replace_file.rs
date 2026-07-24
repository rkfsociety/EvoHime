//! Атомарная замена файла через WinAPI `ReplaceFileW` — устойчивее к
//! блокировке антивирусом, чем стандартный `fs::rename`/`fs::rename`-based
//! паттерн `tmp` → `rename` (раздел IX плана): `config.json`, `current.txt`
//! и `history.json` пишутся именно так.

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReplaceFileError {
    #[error("ReplaceFileW failed: OS error {0}")]
    ReplaceFailed(u32),
    #[error("fallback rename failed: {0}")]
    RenameFailed(#[from] std::io::Error),
}

/// Атомарно заменяет `target` содержимым `replacement`.
///
/// `ReplaceFileW` требует, чтобы `target` уже существовал (иначе
/// `ERROR_FILE_NOT_FOUND`) — поэтому для первой записи файла (когда
/// `target` ещё не создан) используется обычный `fs::rename` как fallback;
/// он тоже атомарен на NTFS для случая "файла ещё нет, конфликтовать не с
/// чем".
#[cfg(windows)]
pub fn atomic_replace_or_create(target: &Path, replacement: &Path) -> Result<(), ReplaceFileError> {
    if !target.exists() {
        std::fs::rename(replacement, target)?;
        return Ok(());
    }

    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{ReplaceFileW, REPLACE_FILE_FLAGS};

    let target_w = HSTRING::from(target.as_os_str());
    let replacement_w = HSTRING::from(replacement.as_os_str());

    // SAFETY: стандартный вызов ReplaceFileW с валидными wide-строками
    // путей; backup-файл не запрашивается (None), доп. флаги не нужны.
    let result = unsafe {
        ReplaceFileW(
            &target_w,
            &replacement_w,
            None,
            REPLACE_FILE_FLAGS(0),
            None,
            None,
        )
    };

    result.map_err(|err| ReplaceFileError::ReplaceFailed(err.code().0 as u32))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn creates_target_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("current.txt");
        let replacement = dir.path().join("current.txt.tmp");
        fs::write(&replacement, b"v0.1.0").unwrap();

        atomic_replace_or_create(&target, &replacement).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "v0.1.0");
        assert!(
            !replacement.exists(),
            "tmp file should have been consumed by rename"
        );
    }

    #[test]
    fn replaces_existing_target_content() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("current.txt");
        fs::write(&target, b"v0.1.0").unwrap();

        let replacement = dir.path().join("current.txt.tmp");
        fs::write(&replacement, b"v0.2.0").unwrap();

        atomic_replace_or_create(&target, &replacement).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "v0.2.0");
        assert!(
            !replacement.exists(),
            "tmp file should have been consumed by ReplaceFileW"
        );
    }
}
