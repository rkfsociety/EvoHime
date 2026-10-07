//! Централизованные пути, которыми пользуется Core.

use std::path::PathBuf;

/// Имя каталога приложения в системном каталоге данных пользователя.
pub const APPLICATION_DIRECTORY_NAME: &str = "EvoHime";

/// Имя переменной окружения для переопределения каталога данных.
pub const DATA_DIRECTORY_ENV: &str = "EVOHIME_DATA_DIR";

/// Возвращает каталог данных Core.
///
/// Приоритет: `EVOHIME_DATA_DIR`, затем системный каталог данных пользователя,
/// затем локальный `.evohime` для portable/dev-запуска.
pub fn get_data_directory() -> PathBuf {
    if let Some(path) = std::env::var_os(DATA_DIRECTORY_ENV) {
        return PathBuf::from(path);
    }

    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join(APPLICATION_DIRECTORY_NAME);
    }

    #[cfg(target_os = "linux")]
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        if path.is_absolute() {
            return path.join("evohime");
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home);
        if path.is_absolute() {
            return path.join(".local/share/evohime");
        }
    }

    tracing::warn!(
        "neither {} nor a platform data directory is available; using portable data directory",
        DATA_DIRECTORY_ENV
    );
    PathBuf::from(".evohime")
}

#[cfg(test)]
mod tests {
    use super::get_data_directory;

    #[test]
    fn returns_a_non_empty_path() {
        assert!(!get_data_directory().as_os_str().is_empty());
    }
}
