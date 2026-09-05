//! Централизованные пути, которыми пользуется Core.

use std::path::PathBuf;

/// Имя каталога приложения внутри `%LOCALAPPDATA%`.
pub const APPLICATION_DIRECTORY_NAME: &str = "EvoHime";

/// Имя переменной окружения для переопределения каталога данных.
pub const DATA_DIRECTORY_ENV: &str = "EVOHIME_DATA_DIR";

/// Возвращает каталог данных Core.
///
/// Приоритет: `EVOHIME_DATA_DIR`, затем `%LOCALAPPDATA%\\EvoHime`, затем
/// локальный `.evohime`. Последний вариант нужен для portable/dev-запуска.
pub fn get_data_directory() -> PathBuf {
    std::env::var_os(DATA_DIRECTORY_ENV)
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|path| PathBuf::from(path).join(APPLICATION_DIRECTORY_NAME))
        })
        .unwrap_or_else(|| {
            tracing::warn!(
                "neither {} nor LOCALAPPDATA is set; using portable data directory",
                DATA_DIRECTORY_ENV
            );
            PathBuf::from(".evohime")
        })
}

#[cfg(test)]
mod tests {
    use super::get_data_directory;

    #[test]
    fn returns_a_non_empty_path() {
        assert!(!get_data_directory().as_os_str().is_empty());
    }
}
