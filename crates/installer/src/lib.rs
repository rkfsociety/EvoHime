//! Библиотека Installer'а EvoHime (Фаза 3 плана Installer/Launcher/Update).
//!
//! Содержит логику, не зависящую от GUI: защиту от прерванной установки
//! (`.setup_complete`), фикс прав на `data\` через `icacls`, создание
//! ярлыка. SHA256-верификация, докачка по HTTP Range и распаковка zip
//! переиспользуются из `evohime_artifacts` (общие с Launcher'ом, чтобы
//! избежать циклической зависимости installer↔launcher). GUI-обвязка
//! (egui) в `src/main.rs` также использует `evohime_launcher` (программные
//! миграции, генерация пароля PostgreSQL, построение DSN) и
//! `evohime_win_support` (именованный Mutex, проверка свободного места).

pub mod dirty_cleanup;
pub mod icacls;
pub mod log_safety;
pub mod setup_marker;
pub mod shortcut;
#[cfg(windows)]
pub mod strict_remove;
pub mod timing;
pub mod ui;

pub use dirty_cleanup::{
    clear_dirty_installation_safely, clear_dirty_installation_safely_observed,
    clear_dirty_installation_safely_with_progress, terminate_owned_processes, DirtyCleanupError,
};
pub use icacls::{
    restore_deletable_permissions, restore_deletable_permissions_observed,
    restrict_to_current_user, restrict_to_current_user_observed, IcaclsError,
};
pub use log_safety::redact_command_event;
pub use setup_marker::{clear_dirty_installation, is_installation_dirty, mark_setup_complete};
pub use shortcut::{create_shortcut, create_shortcut_observed, ShortcutError};
#[cfg(windows)]
pub use strict_remove::{remove_tree_once, remove_tree_with_retries, StrictRemoveError};
pub use timing::{format_elapsed, InstallationElapsed, InstallationTiming};
