//! История обновлений — JSON-файл, не SQLite (раздел I плана: 50-100
//! записей не требуют сложных запросов; JSON проще бэкапить, читать и
//! чинить руками, чем разбираться со схемой БД). Атомарная запись через
//! `ReplaceFileW` (`evohime_win_support`, раздел IX плана) — обычный
//! `fs::rename` может упасть с `Access Denied`, если файл в этот момент
//! открыт на чтение антивирусом.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateOutcome {
    Success,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateHistoryEntry {
    /// RFC3339 timestamp — передаётся вызывающим кодом (не генерируется
    /// здесь), чтобы модуль оставался чистым и легко тестируемым без
    /// системных часов.
    pub timestamp: String,
    pub from_version: String,
    pub to_version: String,
    pub outcome: UpdateOutcome,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateHistory {
    pub entries: Vec<UpdateHistoryEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[cfg(windows)]
    #[error(transparent)]
    Replace(#[from] evohime_win_support::ReplaceFileError),
}

/// Читает `history.json`; файл отсутствующий, битый или с неожиданной
/// схемой не считается фатальной ошибкой — возвращается пустая история,
/// чтобы повреждённый файл не ронял весь Launcher (тот же принцип, что и
/// fallback для `current.txt` по mtime, раздел VII плана).
pub fn load_history(path: &Path) -> UpdateHistory {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

/// Дописывает запись и атомарно сохраняет весь файл.
pub async fn append_and_save(path: &Path, entry: UpdateHistoryEntry) -> Result<(), HistoryError> {
    let mut history = load_history(path);
    history.entries.push(entry);
    save_history(path, &history).await
}

/// Атомарно перезаписывает `history.json`: сериализация во временный файл
/// рядом, затем `ReplaceFileW` (или обычный rename вне Windows — этот
/// крейт всё равно Windows-only по назначению, но не хочется падать при
/// `cargo test` на других платформах).
pub async fn save_history(path: &Path, history: &UpdateHistory) -> Result<(), HistoryError> {
    let json = serde_json::to_string_pretty(history)?;
    let tmp_path = path.with_extension("json.tmp");
    tokio::fs::write(&tmp_path, json).await?;

    #[cfg(windows)]
    {
        evohime_win_support::atomic_replace_or_create(path, &tmp_path)?;
    }
    #[cfg(not(windows))]
    {
        tokio::fs::rename(&tmp_path, path).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(to_version: &str, outcome: UpdateOutcome) -> UpdateHistoryEntry {
        UpdateHistoryEntry {
            timestamp: "2026-07-25T12:00:00Z".to_string(),
            from_version: "v0.4.1".to_string(),
            to_version: to_version.to_string(),
            outcome,
            message: String::new(),
        }
    }

    #[test]
    fn load_history_returns_empty_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let history = load_history(&path);
        assert!(history.entries.is_empty());
    }

    #[test]
    fn load_history_returns_empty_for_corrupted_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        std::fs::write(&path, b"{ this is not valid json").unwrap();
        let history = load_history(&path);
        assert!(history.entries.is_empty());
    }

    #[tokio::test]
    async fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");

        let mut history = UpdateHistory::default();
        history
            .entries
            .push(sample_entry("v0.4.2", UpdateOutcome::Success));

        save_history(&path, &history).await.unwrap();
        let loaded = load_history(&path);
        assert_eq!(loaded, history);
    }

    #[tokio::test]
    async fn append_and_save_accumulates_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");

        append_and_save(&path, sample_entry("v0.4.2", UpdateOutcome::Success))
            .await
            .unwrap();
        append_and_save(&path, sample_entry("v0.4.3", UpdateOutcome::Failed))
            .await
            .unwrap();

        let loaded = load_history(&path);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].to_version, "v0.4.2");
        assert_eq!(loaded.entries[1].to_version, "v0.4.3");
        assert_eq!(loaded.entries[1].outcome, UpdateOutcome::Failed);
    }

    #[tokio::test]
    async fn save_overwrites_existing_file_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");

        let mut first = UpdateHistory::default();
        first
            .entries
            .push(sample_entry("v0.4.1", UpdateOutcome::Success));
        save_history(&path, &first).await.unwrap();

        let mut second = UpdateHistory::default();
        second
            .entries
            .push(sample_entry("v0.4.2", UpdateOutcome::RolledBack));
        save_history(&path, &second).await.unwrap();

        let loaded = load_history(&path);
        assert_eq!(loaded, second);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "tmp file should be consumed"
        );
    }
}
