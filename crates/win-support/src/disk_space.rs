//! Проверка свободного места на диске через `GetDiskFreeSpaceExW` — нужна
//! Installer'у перед распаковкой PostgreSQL/Python/релиза и Launcher'у перед
//! каждым обновлением (раздел X плана: предупреждение при <1 ГБ свободно).

#[cfg(windows)]
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DiskSpaceError {
    #[error("GetDiskFreeSpaceExW failed for path {path}: OS error {code}")]
    QueryFailed { path: String, code: u32 },
}

/// Возвращает количество свободных байт, доступных текущему пользователю
/// на диске, где расположен `path` (учитывает дисковые квоты — как и
/// нужно, поскольку Installer/Launcher всегда работают от имени обычного
/// пользователя, без прав администратора).
#[cfg(windows)]
pub fn free_bytes_available(path: &Path) -> Result<u64, DiskSpaceError> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide = HSTRING::from(path.as_os_str());
    let mut free_bytes_available_to_caller: u64 = 0;

    // SAFETY: стандартный вызов GetDiskFreeSpaceExW; передаём только
    // первый out-параметр (доступно вызывающему), остальные два (общий
    // размер, всего свободно) не нужны и передаются как None.
    let result = unsafe {
        GetDiskFreeSpaceExW(&wide, Some(&mut free_bytes_available_to_caller), None, None)
    };

    result.map_err(|err| DiskSpaceError::QueryFailed {
        path: path.display().to_string(),
        code: err.code().0 as u32,
    })?;

    Ok(free_bytes_available_to_caller)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn reports_nonzero_free_space_for_temp_dir() {
        let temp_dir = std::env::temp_dir();
        let free = free_bytes_available(&temp_dir).expect("should query real disk");
        // Любая рабочая машина CI/разработчика имеет больше нуля свободного
        // места — тест ловит грубые ошибки маппинга WinAPI-параметров, а
        // не проверяет конкретное число.
        assert!(free > 0, "expected nonzero free space, got {free}");
    }
}
