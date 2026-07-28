//! Поддерживающая библиотека Launcher'а EvoHime (Фаза 2 плана Installer/Launcher/Update).
//!
//! Содержит логику, не зависящую от GUI/трея (те появятся в Фазе 4): применение
//! миграций без `sqlx-cli`, автоматические бэкапы БД перед миграциями, генерацию
//! пароля суперпользователя PostgreSQL и патч `pg_hba.conf`, а также построение
//! DSN без хардкода имени пользователя.

pub mod backup;
pub mod config;
pub mod dsn;
pub mod github_api;
pub mod history;
pub mod migrations;
pub mod observed_command;
pub mod pg_auth;
pub mod postgres;
pub mod process_manager;
pub mod safe_mode;
pub mod self_update;
pub mod static_server;
pub mod status_server;
pub mod token;
pub mod update_apply;
pub mod update_check;

pub use backup::{create_backup, prune_backups, BackupError};
pub use config::DbConfig;
pub use dsn::build_dsn;
pub use github_api::fetch_latest_release;
pub use history::{
    append_and_save, load_history, save_history, UpdateHistory, UpdateHistoryEntry, UpdateOutcome,
};
pub use migrations::{apply_migrations, MigrationError};
pub use pg_auth::{generate_password, patch_pg_hba_trust_local, PgHbaError};
pub use process_manager::ManagedProcess;
pub use self_update::{
    build_updater_args, download_new_launcher, spawn_updater, SelfUpdateError, UpdaterArgs,
};
pub use static_server::build_static_router;
pub use status_server::{build_status_router, ComponentStatus, LauncherStatus, StatusServerState};
pub use token::{generate_session_token, tokens_equal};
pub use update_check::{is_update_available, latest_version_from_atom, releases_atom_url};
