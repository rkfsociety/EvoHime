//! Библиотека Installer'а EvoHime (Фаза 3 плана Installer/Launcher/Update).
//!
//! Содержит логику, не зависящую от GUI: проверку целостности скачанных
//! артефактов (SHA256), докачку с докачкой через HTTP Range, защиту от
//! прерванной установки (`.setup_complete`), фикс прав на `data\` через
//! `icacls`. GUI-обвязка (egui) в `src/main.rs` вызывает эти модули и
//! переиспользует `evohime_launcher` (программные миграции, генерация
//! пароля PostgreSQL, построение DSN) и `evohime_win_support` (именованный
//! Mutex, проверка свободного места на диске).

pub mod downloader;
pub mod extract;
pub mod icacls;
pub mod setup_marker;
pub mod sha256;
pub mod shortcut;

pub use downloader::{download_with_resume, DownloadError};
pub use extract::{extract_zip, ExtractError};
pub use icacls::{restrict_to_current_user, IcaclsError};
pub use setup_marker::{is_installation_dirty, mark_setup_complete};
pub use sha256::{compute_sha256, verify_sha256};
pub use shortcut::{create_shortcut, ShortcutError};
