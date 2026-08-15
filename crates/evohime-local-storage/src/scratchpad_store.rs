//! Task-scoped scratchpad (этап 01.2).
//!
//! Подтверждённой считается только атомарно записанная Core-запись, созданная
//! после provenance/policy-проверки, явного пользовательского подтверждения или
//! завершённой policy-операции. Перезапись подтверждённой записи допускается
//! только новой ревизией, а не silent override.

use evohime_context_budget::{
    item::{Privacy, ScratchpadStatus, Trust},
    scratchpad::{
        ConfirmationBasis, RecoveryPolicy, ScratchpadCategory, ScratchpadEntry,
    },
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::StorageError;

/// Базовый лимит bounded-чтения scratchpad.
pub const SCRATCHPAD_READ_LIMIT: usize = 100;

/// Bounded проекция записи для UI: содержимое усечено по границе строки, факт
/// усечения помечен явно.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScratchpadProjection {
    pub id: String,
    pub category: String,
    pub status: String,
    pub trust: String,
    pub revision: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub preview: String,
    pub truncated: bool,
    pub artifact_locator: Option<String>,
}

/// Ошибка операции над scratchpad.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScratchpadError {
    #[error("confirmed entry {0} cannot be overwritten in place; write a new revision")]
    ConfirmedOverwrite(String),
    #[error("entry {0} was not found")]
    NotFound(String),
}

/// Хранилище scratchpad поверх общей миграции базы.
pub struct ScratchpadStore<'a> {
    connection: &'a Connection,
}

impl<'a> ScratchpadStore<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Запись новой заметки или новой ревизии. Существующая подтверждённая
    /// запись не перезаписывается: попытка даёт ошибку.
    pub fn upsert(&self, entry: &ScratchpadEntry) -> Result<(), StorageError> {
        if let Some(existing) = self.get(&entry.id)? {
            if existing.status == ScratchpadStatus::Confirmed
                && existing.content_hash != entry.content_hash
            {
                return Err(StorageError::Context(
                    ScratchpadError::ConfirmedOverwrite(entry.id.clone()).to_string(),
                ));
            }
        }
        self.connection.execute(
            "INSERT OR REPLACE INTO task_scratchpad (
                id, task_id, session_id, category, status, trust, privacy, revision,
                parent_id, content, content_hash, created_at, updated_at, ttl_ms,
                confirmation, artifact_locator, recovered_at_step
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            rusqlite::params![
                entry.id,
                entry.task_id,
                entry.session_id,
                entry.category.as_str(),
                entry.status.as_str(),
                entry.trust.as_str(),
                entry.privacy.as_str(),
                entry.revision,
                entry.parent_id,
                entry.content,
                entry.content_hash,
                entry.created_at,
                entry.updated_at,
                entry.ttl_ms,
                entry.confirmation.map(|basis| basis.as_str()),
                entry.artifact_locator,
                entry.recovered_at_step,
            ],
        )?;
        Ok(())
    }

    /// Подтверждение записи с явным основанием.
    pub fn confirm(
        &self,
        id: &str,
        basis: ConfirmationBasis,
        now: i64,
    ) -> Result<ScratchpadEntry, StorageError> {
        let mut entry = self
            .get(id)?
            .ok_or_else(|| StorageError::Context(ScratchpadError::NotFound(id.to_string()).to_string()))?;
        entry.confirm(basis, now);
        self.upsert(&entry)?;
        Ok(entry)
    }

    pub fn get(&self, id: &str) -> Result<Option<ScratchpadEntry>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, task_id, session_id, category, status, trust, privacy, revision,
                        parent_id, content, content_hash, created_at, updated_at, ttl_ms,
                        confirmation, artifact_locator, recovered_at_step
                 FROM task_scratchpad WHERE id = ?1",
                [id],
                read_entry,
            )
            .optional()?)
    }

    /// Полное чтение записей задачи с фильтром по категории и статусу.
    pub fn list(
        &self,
        task_id: &str,
        category: Option<ScratchpadCategory>,
        status: Option<ScratchpadStatus>,
        limit: usize,
    ) -> Result<Vec<ScratchpadEntry>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, session_id, category, status, trust, privacy, revision,
                    parent_id, content, content_hash, created_at, updated_at, ttl_ms,
                    confirmation, artifact_locator, recovered_at_step
             FROM task_scratchpad
             WHERE task_id = ?1
               AND (?2 IS NULL OR category = ?2)
               AND (?3 IS NULL OR status = ?3)
             ORDER BY created_at ASC, id ASC
             LIMIT ?4",
        )?;
        let rows = statement.query_map(
            rusqlite::params![
                task_id,
                category.map(ScratchpadCategory::as_str),
                status.map(ScratchpadStatus::as_str),
                i64::try_from(limit.min(SCRATCHPAD_READ_LIMIT)).unwrap_or(i64::MAX),
            ],
            read_entry,
        )?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Bounded проекция для UI: без полного содержимого.
    pub fn projection(
        &self,
        task_id: &str,
        category: Option<ScratchpadCategory>,
        status: Option<ScratchpadStatus>,
        limit: usize,
        preview_chars: usize,
    ) -> Result<Vec<ScratchpadProjection>, StorageError> {
        Ok(self
            .list(task_id, category, status, limit)?
            .into_iter()
            .map(|entry| {
                let preview: String = entry.content.chars().take(preview_chars).collect();
                let truncated = preview.chars().count() < entry.content.chars().count();
                ScratchpadProjection {
                    id: entry.id,
                    category: entry.category.as_str().to_string(),
                    status: entry.status.as_str().to_string(),
                    trust: entry.trust.as_str().to_string(),
                    revision: entry.revision,
                    created_at: entry.created_at,
                    updated_at: entry.updated_at,
                    preview,
                    truncated,
                    artifact_locator: entry.artifact_locator,
                }
            })
            .collect())
    }

    /// Очистка scratchpad задачи. Возвращает число удалённых записей.
    pub fn clear_task(&self, task_id: &str) -> Result<usize, StorageError> {
        Ok(self
            .connection
            .execute("DELETE FROM task_scratchpad WHERE task_id = ?1", [task_id])?)
    }

    /// Удаление одной записи вместе с её производными ревизиями.
    pub fn forget(&self, id: &str) -> Result<usize, StorageError> {
        let removed = self.connection.execute(
            "DELETE FROM task_scratchpad WHERE id = ?1 OR parent_id = ?1",
            [id],
        )?;
        Ok(removed)
    }

    /// Восстановление после restart: `confirmed` возвращаются в рабочий контекст,
    /// остальные переводятся в recovery view.
    pub fn recover(
        &self,
        task_id: &str,
        now: i64,
        current_step: u32,
    ) -> Result<(Vec<ScratchpadEntry>, Vec<ScratchpadEntry>), StorageError> {
        let entries = self.list(task_id, None, None, SCRATCHPAD_READ_LIMIT)?;
        let mut restored = Vec::new();
        let mut isolated = Vec::new();
        for mut entry in entries {
            match entry.status {
                ScratchpadStatus::Confirmed => restored.push(entry),
                ScratchpadStatus::Draft => {
                    // `draft` не восстанавливается.
                    self.connection
                        .execute("DELETE FROM task_scratchpad WHERE id = ?1", [&entry.id])?;
                }
                ScratchpadStatus::Recovered => {
                    entry.trust = Trust::Unverified;
                    entry.updated_at = now;
                    if entry.recovered_at_step.is_none() {
                        entry.recovered_at_step = Some(current_step);
                    }
                    self.upsert(&entry)?;
                    isolated.push(entry);
                }
            }
        }
        Ok((restored, isolated))
    }

    /// Перевод незавершённых записей в `recovered` перед выключением: после
    /// restart они попадут в изолированный recovery view.
    pub fn mark_unconfirmed_as_recovered(
        &self,
        task_id: &str,
        now: i64,
        current_step: u32,
    ) -> Result<usize, StorageError> {
        Ok(self.connection.execute(
            "UPDATE task_scratchpad
             SET status = 'recovered', trust = 'unverified', updated_at = ?2,
                 recovered_at_step = COALESCE(recovered_at_step, ?3)
             WHERE task_id = ?1 AND status = 'draft'",
            rusqlite::params![task_id, now, current_step],
        )?)
    }

    /// Удаление recovered-записей, исчерпавших policy изоляции.
    pub fn discard_expired_recovered(
        &self,
        task_id: &str,
        policy: RecoveryPolicy,
        now: i64,
        current_step: u32,
    ) -> Result<usize, StorageError> {
        let entries = self.list(
            task_id,
            None,
            Some(ScratchpadStatus::Recovered),
            SCRATCHPAD_READ_LIMIT,
        )?;
        let mut removed = 0;
        for entry in entries {
            if policy.should_discard(&entry, now, current_step) {
                self.connection
                    .execute("DELETE FROM task_scratchpad WHERE id = ?1", [&entry.id])?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Самые старые `confirmed` записи задачи — кандидаты на выгрузку в artifact
    /// store при переполнении бюджета scratchpad. `open_questions` текущего шага
    /// не вытесняются.
    pub fn offload_candidates(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<ScratchpadEntry>, StorageError> {
        Ok(self
            .list(task_id, None, Some(ScratchpadStatus::Confirmed), SCRATCHPAD_READ_LIMIT)?
            .into_iter()
            .filter(|entry| {
                entry.category != ScratchpadCategory::OpenQuestions
                    && entry.artifact_locator.is_none()
            })
            .take(limit)
            .collect())
    }
}

fn read_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScratchpadEntry> {
    let category: String = row.get(3)?;
    let status: String = row.get(4)?;
    let trust: String = row.get(5)?;
    let privacy: String = row.get(6)?;
    let confirmation: Option<String> = row.get(14)?;
    Ok(ScratchpadEntry {
        id: row.get(0)?,
        task_id: row.get(1)?,
        session_id: row.get(2)?,
        category: ScratchpadCategory::parse(&category).unwrap_or(ScratchpadCategory::Facts),
        status: match status.as_str() {
            "confirmed" => ScratchpadStatus::Confirmed,
            "recovered" => ScratchpadStatus::Recovered,
            _ => ScratchpadStatus::Draft,
        },
        trust: match trust.as_str() {
            "core_owned" => Trust::CoreOwned,
            "confirmed" => Trust::Confirmed,
            "external" => Trust::External,
            _ => Trust::Unverified,
        },
        privacy: match privacy.as_str() {
            "secret" => Privacy::Secret,
            "sensitive" => Privacy::Sensitive,
            _ => Privacy::Workspace,
        },
        revision: row.get(7)?,
        parent_id: row.get(8)?,
        content: row.get(9)?,
        content_hash: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        ttl_ms: row.get(13)?,
        confirmation: confirmation.as_deref().and_then(|basis| match basis {
            "tool_provenance_verified" => Some(ConfirmationBasis::ToolProvenanceVerified),
            "user_confirmed" => Some(ConfirmationBasis::UserConfirmed),
            "policy_operation_completed" => Some(ConfirmationBasis::PolicyOperationCompleted),
            _ => None,
        }),
        artifact_locator: row.get(15)?,
        recovered_at_step: row.get(16)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalDatabase;

    fn database(name: &str) -> LocalDatabase {
        let path = std::env::temp_dir().join(format!(
            "evohime-scratchpad-{name}-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        LocalDatabase::open(&path).expect("database opens")
    }

    fn draft(id: &str, category: ScratchpadCategory) -> ScratchpadEntry {
        ScratchpadEntry::draft(id, "task", "session", category, format!("заметка {id}"), 1_000)
    }

    #[test]
    fn an_entry_round_trips_through_sqlite() {
        let database = database("round-trip");
        let store = ScratchpadStore::new(database.connection());
        let entry = draft("s1", ScratchpadCategory::Facts);
        store.upsert(&entry).expect("write succeeds");
        assert_eq!(store.get("s1").expect("read").expect("entry"), entry);
    }

    #[test]
    fn confirmation_requires_an_explicit_basis_and_is_persisted() {
        let database = database("confirm");
        let store = ScratchpadStore::new(database.connection());
        store
            .upsert(&draft("s1", ScratchpadCategory::Facts))
            .expect("write succeeds");
        let confirmed = store
            .confirm("s1", ConfirmationBasis::ToolProvenanceVerified, 2_000)
            .expect("confirm succeeds");
        assert_eq!(confirmed.status, ScratchpadStatus::Confirmed);
        assert_eq!(
            store.get("s1").expect("read").expect("entry").confirmation,
            Some(ConfirmationBasis::ToolProvenanceVerified)
        );
    }

    #[test]
    fn a_confirmed_entry_cannot_be_overwritten_in_place() {
        let database = database("no-overwrite");
        let store = ScratchpadStore::new(database.connection());
        store
            .upsert(&draft("s1", ScratchpadCategory::Facts))
            .expect("write succeeds");
        let confirmed = store
            .confirm("s1", ConfirmationBasis::UserConfirmed, 2_000)
            .expect("confirm succeeds");

        let mut silent_override = confirmed.clone();
        silent_override.content = "подменённое содержимое".to_string();
        silent_override.content_hash = "other-hash".to_string();
        assert!(store.upsert(&silent_override).is_err());

        // Новая ревизия — допустимый путь.
        let revision = confirmed.revise("s2", "новое содержимое", 3_000);
        store.upsert(&revision).expect("revision is accepted");
        assert_eq!(store.get("s1").expect("read").expect("entry"), confirmed);
        assert_eq!(store.get("s2").expect("read").expect("entry").revision, 2);
    }

    #[test]
    fn only_confirmed_entries_return_after_restart() {
        let database = database("restart");
        let store = ScratchpadStore::new(database.connection());
        store
            .upsert(&draft("draft", ScratchpadCategory::Facts))
            .expect("write");
        store
            .upsert(&draft("confirmed", ScratchpadCategory::Facts))
            .expect("write");
        store
            .confirm("confirmed", ConfirmationBasis::UserConfirmed, 1_500)
            .expect("confirm");
        let mut recovered = draft("recovered", ScratchpadCategory::Facts);
        recovered.status = ScratchpadStatus::Recovered;
        store.upsert(&recovered).expect("write");

        let (restored, isolated) = store.recover("task", 9_000, 4).expect("recovery runs");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].id, "confirmed");
        assert_eq!(isolated.len(), 1);
        assert_eq!(isolated[0].trust, Trust::Unverified);
        assert_eq!(isolated[0].recovered_at_step, Some(4));
        // Черновик не восстанавливается и не остаётся в хранилище.
        assert!(store.get("draft").expect("read").is_none());
    }

    #[test]
    fn unconfirmed_entries_become_recovered_on_shutdown() {
        let database = database("shutdown");
        let store = ScratchpadStore::new(database.connection());
        store
            .upsert(&draft("s1", ScratchpadCategory::Facts))
            .expect("write");
        let marked = store
            .mark_unconfirmed_as_recovered("task", 5_000, 7)
            .expect("marking runs");
        assert_eq!(marked, 1);
        let entry = store.get("s1").expect("read").expect("entry");
        assert_eq!(entry.status, ScratchpadStatus::Recovered);
        assert_eq!(entry.trust, Trust::Unverified);
        assert_eq!(entry.recovered_at_step, Some(7));
    }

    #[test]
    fn expired_recovered_entries_are_discarded_by_policy() {
        let database = database("recovery-policy");
        let store = ScratchpadStore::new(database.connection());
        let mut recovered = draft("s1", ScratchpadCategory::Facts);
        recovered.status = ScratchpadStatus::Recovered;
        recovered.updated_at = 0;
        recovered.recovered_at_step = Some(0);
        store.upsert(&recovered).expect("write");

        let policy = RecoveryPolicy::default();
        assert_eq!(
            store
                .discard_expired_recovered("task", policy, 1_000, 1)
                .expect("policy runs"),
            0
        );
        assert_eq!(
            store
                .discard_expired_recovered("task", policy, policy.max_age_ms, 1)
                .expect("policy runs"),
            1
        );
    }

    #[test]
    fn listing_filters_by_category_and_status() {
        let database = database("filter");
        let store = ScratchpadStore::new(database.connection());
        store
            .upsert(&draft("fact", ScratchpadCategory::Facts))
            .expect("write");
        store
            .upsert(&draft("question", ScratchpadCategory::OpenQuestions))
            .expect("write");
        store
            .confirm("fact", ConfirmationBasis::UserConfirmed, 2_000)
            .expect("confirm");

        let facts = store
            .list("task", Some(ScratchpadCategory::Facts), None, 50)
            .expect("list");
        assert_eq!(facts.len(), 1);
        let confirmed = store
            .list("task", None, Some(ScratchpadStatus::Confirmed), 50)
            .expect("list");
        assert_eq!(confirmed.len(), 1);
        assert_eq!(confirmed[0].id, "fact");
    }

    #[test]
    fn projection_is_bounded_and_marks_truncation() {
        let database = database("projection");
        let store = ScratchpadStore::new(database.connection());
        let mut long = draft("s1", ScratchpadCategory::Facts);
        long.content = "я".repeat(500);
        store.upsert(&long).expect("write");
        let projection = store
            .projection("task", None, None, 50, 40)
            .expect("projection");
        assert_eq!(projection[0].preview.chars().count(), 40);
        assert!(projection[0].truncated);
    }

    #[test]
    fn forgetting_an_entry_removes_its_revisions_too() {
        let database = database("forget");
        let store = ScratchpadStore::new(database.connection());
        let mut base = draft("s1", ScratchpadCategory::Facts);
        base.confirm(ConfirmationBasis::UserConfirmed, 1_500);
        store.upsert(&base).expect("write");
        store
            .upsert(&base.revise("s2", "новая ревизия", 2_000))
            .expect("write");
        assert_eq!(store.forget("s1").expect("forget"), 2);
        assert!(store.get("s2").expect("read").is_none());
    }

    #[test]
    fn clearing_a_task_removes_only_that_task() {
        let database = database("clear");
        let store = ScratchpadStore::new(database.connection());
        store.upsert(&draft("s1", ScratchpadCategory::Facts)).expect("write");
        let mut other = draft("s2", ScratchpadCategory::Facts);
        other.task_id = "other-task".to_string();
        store.upsert(&other).expect("write");
        assert_eq!(store.clear_task("task").expect("clear"), 1);
        assert!(store.get("s2").expect("read").is_some());
    }

    #[test]
    fn open_questions_are_never_offload_candidates() {
        let database = database("offload-candidates");
        let store = ScratchpadStore::new(database.connection());
        for (id, category) in [
            ("fact", ScratchpadCategory::Facts),
            ("question", ScratchpadCategory::OpenQuestions),
        ] {
            store.upsert(&draft(id, category)).expect("write");
            store
                .confirm(id, ConfirmationBasis::UserConfirmed, 2_000)
                .expect("confirm");
        }
        let candidates = store.offload_candidates("task", 10).expect("candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "fact");
    }
}
