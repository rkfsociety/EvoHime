//! Персистентный конфиг подключения к портативному PostgreSQL
//! (`launcher-data/config.json`) — Installer пишет его один раз сразу после
//! успешного `initdb`, Launcher читает его при каждом запуске, чтобы
//! построить DSN через [`crate::dsn::build_dsn`] и передать её в
//! `server.exe`/`UpdatePlan`, вместо хардкода несуществующей БД по
//! умолчанию.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbConfig {
    pub user: String,
    pub password: String,
    pub port: u16,
    pub db_name: String,
}

fn config_path(install_dir: &Path) -> PathBuf {
    install_dir.join("launcher-data").join("config.json")
}

/// Читает конфиг БД. `None`, если файла нет (свежая/незавершённая
/// установка) или он повреждён — вызывающая сторона должна работать без
/// DSN в этом случае, а не падать.
pub fn load(install_dir: &Path) -> Option<DbConfig> {
    let contents = std::fs::read_to_string(config_path(install_dir)).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Атомарно записывает конфиг БД — тот же паттерн `tmp` →
/// `atomic_replace_or_create`, что и `write_current_version`
/// (см. `update_apply.rs`), чтобы не оставить `config.json` в битом
/// состоянии при сбое посреди записи.
pub async fn save(install_dir: &Path, config: &DbConfig) -> std::io::Result<()> {
    let path = config_path(install_dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(config)?;
    tokio::fs::write(&tmp_path, json).await?;

    #[cfg(windows)]
    {
        evohime_win_support::atomic_replace_or_create(&path, &tmp_path)
            .map_err(|err| std::io::Error::other(err.to_string()))?;
    }
    #[cfg(not(windows))]
    {
        tokio::fs::rename(&tmp_path, &path).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DbConfig {
        DbConfig {
            user: "roman".to_string(),
            password: "s3cr3t".to_string(),
            port: 55432,
            db_name: "evohime".to_string(),
        }
    }

    #[tokio::test]
    async fn saves_and_loads_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let config = sample();

        save(dir.path(), &config).await.unwrap();

        assert_eq!(load(dir.path()), Some(config));
    }

    #[tokio::test]
    async fn overwrites_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &sample()).await.unwrap();

        let mut updated = sample();
        updated.password = "new-password".to_string();
        save(dir.path(), &updated).await.unwrap();

        assert_eq!(load(dir.path()), Some(updated));
    }

    #[test]
    fn returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), None);
    }

    #[test]
    fn returns_none_when_corrupted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("launcher-data")).unwrap();
        std::fs::write(
            dir.path().join("launcher-data").join("config.json"),
            "not json",
        )
        .unwrap();

        assert_eq!(load(dir.path()), None);
    }
}
