//! Task-scoped scratchpad (этап 01.2).
//!
//! Подтверждённой считается только атомарно записанная Core-запись, созданная
//! после provenance/policy-проверки, явного пользовательского подтверждения или
//! завершённой policy-операции. Перезапись подтверждённой записи допускается
//! только новой ревизией, а не silent override.

use evohime_context_budget::{
    item::{Privacy, ScratchpadStatus, Trust},
    scratchpad::{ConfirmationBasis, RecoveryPolicy, ScratchpadCategory, ScratchpadEntry},
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
        let mut entry = self.get(id)?.ok_or_else(|| {
            StorageError::Context(ScratchpadError::NotFound(id.to_string()).to_string())
        })?;
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
            .list(
                task_id,
                None,
                Some(ScratchpadStatus::Confirmed),
                SCRATCHPAD_READ_LIMIT,
            )?
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
#[path = "scratchpad_store_tests.rs"]
mod tests;
