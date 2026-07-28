//! Общая логика работы с артефактами GitHub Releases: SHA256-верификация,
//! докачка по HTTP Range, безопасная распаковка zip. Используется и
//! Installer'ом (первая установка), и Launcher'ом (обновления) — вынесена
//! в отдельный крейт, чтобы избежать циклической зависимости между ними
//! (`evohime-installer` уже зависит от `evohime-launcher` за программные
//! миграции/бэкапы/pg_auth/dsn).

pub mod downloader;
pub mod extract;
pub mod sha256;

pub use downloader::{download_with_resume, download_with_resume_and_verify, DownloadError};
pub use extract::{extract_zip, ExtractError};
pub use sha256::{compute_sha256, verify_sha256};
