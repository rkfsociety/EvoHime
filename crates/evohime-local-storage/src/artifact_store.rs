//! Core-owned task artifact store (этап 01.2).
//!
//! Содержимое адресуется по `content_hash` из 01.1: повторный offload того же
//! содержимого переиспользует существующий артефакт и добавляет ссылку, а не
//! копию. Store общий на уровне Core, но пространство имён — per-task.

use evohime_context_budget::{
    artifact::{
        access_allowed, bounded_summary, dedup_hit_allowed, plan_eviction, ArtifactError,
        ArtifactQuota, ArtifactRef, ArtifactRefStatus, ArtifactTombstone, EvictionCandidate,
    },
    hash::{content_hash, ContentForm},
    item::Privacy,
};
use rusqlite::{Connection, OptionalExtension};

use crate::StorageError;

/// Максимальный размер bounded summary, остающегося в контексте.
pub const ARTIFACT_SUMMARY_CHARS: usize = 512;
pub const ARTIFACT_SUMMARY_LINES: usize = 8;

/// Результат выгрузки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffloadResult {
    pub reference: ArtifactRef,
    /// Было ли содержимое переиспользовано по `content_hash`.
    pub deduplicated: bool,
}

/// Хранилище артефактов поверх общей миграции базы.
pub struct ArtifactStore<'a> {
    connection: &'a Connection,
    quota: ArtifactQuota,
}

impl<'a> ArtifactStore<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self {
            connection,
            quota: ArtifactQuota::default(),
        }
    }

    pub fn with_quota(connection: &'a Connection, quota: ArtifactQuota) -> Self {
        Self { connection, quota }
    }

    pub fn quota(&self) -> ArtifactQuota {
        self.quota
    }

    /// Выгрузка ограниченного бинарного результата (например, PNG) без
    /// промежуточной записи в workspace.
    pub fn offload_bytes(
        &self,
        kind: &str,
        task_id: &str,
        owner_task_id: &str,
        content: &[u8],
        privacy: Privacy,
        now: i64,
    ) -> Result<OffloadResult, StorageError> {
        if !privacy.allows_offload() {
            return Err(StorageError::Context(
                "artifact privacy forbids offload".into(),
            ));
        }
        let hash = content_hash(kind, &ContentForm::Binary(content));
        let bytes = content.len() as u64;
        self.ensure_quota(task_id, bytes, now)?;
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let outcome = (|| -> Result<ArtifactRef, StorageError> {
            self.connection.execute("INSERT OR REPLACE INTO task_artifacts(content_hash,bytes,content,created_at,last_access_at) VALUES (?1,?2,?3,?4,?4)", rusqlite::params![hash, bytes as i64, content, now])?;
            let reference = ArtifactRef {
                locator: format!("artifact://{owner_task_id}/{hash}"),
                content_hash: hash,
                task_id: task_id.into(),
                owner_task_id: owner_task_id.into(),
                bytes,
                privacy,
                status: ArtifactRefStatus::Live,
                created_at: now,
                last_access_at: now,
                ttl_ms: Some(self.quota.default_ttl_ms),
                summary: format!("binary artifact ({bytes} bytes)"),
            };
            self.write_ref(&reference)?;
            Ok(reference)
        })();
        match outcome {
            Ok(reference) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(OffloadResult {
                    reference,
                    deduplicated: false,
                })
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// Выгрузка содержимого. Запись артефакта и обновление ссылок атомарны:
    /// конкурентный offload одинакового содержимого из двух задач даёт один
    /// артефакт и две ссылки, а не гонку.
    pub fn offload(
        &self,
        kind: &str,
        task_id: &str,
        owner_task_id: &str,
        content: &str,
        privacy: Privacy,
        now: i64,
    ) -> Result<OffloadResult, StorageError> {
        if !privacy.allows_offload() {
            return Err(StorageError::Context(
                ArtifactError::PrivacyForbidsOffload(privacy.as_str()).to_string(),
            ));
        }
        let hash = content_hash(kind, &ContentForm::Text(content));
        let bytes = content.len() as u64;

        let tombstoned = self.is_tombstoned(&hash)?;
        let existing_status = self.existing_status(&hash)?;
        let deduplicated = existing_status
            .is_some_and(|status| dedup_hit_allowed(status, tombstoned))
            && self.content_exists(&hash)?;

        if !deduplicated {
            self.ensure_quota(task_id, bytes, now)?;
        }

        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let outcome = (|| -> Result<ArtifactRef, StorageError> {
            if !deduplicated {
                self.connection.execute(
                    "INSERT OR REPLACE INTO task_artifacts
                        (content_hash, bytes, content, created_at, last_access_at)
                     VALUES (?1, ?2, ?3, ?4, ?4)",
                    rusqlite::params![hash, bytes as i64, content.as_bytes(), now],
                )?;
                // Новое содержимое снимает tombstone: hash снова доступен.
                self.connection.execute(
                    "DELETE FROM artifact_tombstones WHERE content_hash = ?1",
                    [&hash],
                )?;
            } else {
                self.connection.execute(
                    "UPDATE task_artifacts SET last_access_at = ?2 WHERE content_hash = ?1",
                    rusqlite::params![hash, now],
                )?;
            }
            let locator = format!("artifact://{owner_task_id}/{hash}");
            let reference = ArtifactRef {
                locator,
                content_hash: hash,
                task_id: task_id.to_string(),
                owner_task_id: owner_task_id.to_string(),
                bytes,
                privacy,
                status: ArtifactRefStatus::Live,
                created_at: now,
                last_access_at: now,
                ttl_ms: Some(self.quota.default_ttl_ms),
                summary: bounded_summary(content, ARTIFACT_SUMMARY_CHARS, ARTIFACT_SUMMARY_LINES),
            };
            self.write_ref(&reference)?;
            Ok(reference)
        })();

        match outcome {
            Ok(reference) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(OffloadResult {
                    reference,
                    deduplicated,
                })
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// Чтение полного содержимого по locator с повторной проверкой доступа и
    /// hash. Расхождение означает повреждение или подмену: содержимое не
    /// попадает в контекст, ссылка помечается `invalid`.
    pub fn read(
        &self,
        locator: &str,
        task_id: &str,
        parent_chain: &[String],
        kind: &str,
        now: i64,
    ) -> Result<String, StorageError> {
        let reference = self
            .get_ref(locator)?
            .ok_or_else(|| StorageError::Context(format!("artifact {locator} was not found")))?;
        if !access_allowed(&reference, task_id, parent_chain) {
            return Err(StorageError::Context(
                ArtifactError::AccessDenied {
                    locator: locator.to_string(),
                    task_id: task_id.to_string(),
                }
                .to_string(),
            ));
        }
        if !reference.is_readable() {
            return Err(StorageError::Context(
                ArtifactError::NotReadable {
                    locator: locator.to_string(),
                    status: reference.status.as_str().to_string(),
                }
                .to_string(),
            ));
        }
        let content: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT content FROM task_artifacts WHERE content_hash = ?1",
                [&reference.content_hash],
                |row| row.get(0),
            )
            .optional()?;
        let Some(content) = content else {
            self.set_ref_status(locator, ArtifactRefStatus::Expired)?;
            return Err(StorageError::Context(
                ArtifactError::NotReadable {
                    locator: locator.to_string(),
                    status: ArtifactRefStatus::Expired.as_str().to_string(),
                }
                .to_string(),
            ));
        };
        let text = String::from_utf8_lossy(&content).to_string();
        let actual = content_hash(kind, &ContentForm::Text(&text));
        if actual != reference.content_hash {
            self.set_ref_status(locator, ArtifactRefStatus::Invalid)?;
            return Err(StorageError::Context(
                ArtifactError::HashMismatch {
                    locator: locator.to_string(),
                    expected: reference.content_hash,
                    actual,
                }
                .to_string(),
            ));
        }
        self.connection.execute(
            "UPDATE task_artifacts SET last_access_at = ?2 WHERE content_hash = ?1",
            rusqlite::params![reference.content_hash, now],
        )?;
        self.connection.execute(
            "UPDATE task_artifact_refs SET last_access_at = ?2 WHERE locator = ?1",
            rusqlite::params![locator, now],
        )?;
        Ok(text)
    }

    /// Чтение бинарного объекта с теми же проверками владения и hash, что и
    /// текстовое чтение. Возвращает только содержимое ArtifactStore.
    pub fn read_bytes(
        &self,
        locator: &str,
        task_id: &str,
        parent_chain: &[String],
        kind: &str,
        now: i64,
    ) -> Result<Vec<u8>, StorageError> {
        let reference = self
            .get_ref(locator)?
            .ok_or_else(|| StorageError::Context("artifact not found".into()))?;
        if !access_allowed(&reference, task_id, parent_chain) || !reference.is_readable() {
            return Err(StorageError::Context("artifact access denied".into()));
        }
        let content: Vec<u8> = self
            .connection
            .query_row(
                "SELECT content FROM task_artifacts WHERE content_hash=?1",
                [&reference.content_hash],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StorageError::Context("artifact content missing".into()))?;
        if content_hash(kind, &ContentForm::Binary(&content)) != reference.content_hash {
            self.set_ref_status(locator, ArtifactRefStatus::Invalid)?;
            return Err(StorageError::Context("artifact hash mismatch".into()));
        }
        self.connection.execute(
            "UPDATE task_artifacts SET last_access_at=?2 WHERE content_hash=?1",
            rusqlite::params![reference.content_hash, now],
        )?;
        Ok(content)
    }

    pub fn get_ref(&self, locator: &str) -> Result<Option<ArtifactRef>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT locator, content_hash, task_id, owner_task_id, bytes, privacy,
                        status, created_at, last_access_at, ttl_ms, summary
                 FROM task_artifact_refs WHERE locator = ?1",
                [locator],
                read_ref,
            )
            .optional()?)
    }

    /// Все ссылки задачи.
    pub fn list_refs(&self, task_id: &str) -> Result<Vec<ArtifactRef>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT locator, content_hash, task_id, owner_task_id, bytes, privacy,
                    status, created_at, last_access_at, ttl_ms, summary
             FROM task_artifact_refs WHERE task_id = ?1 ORDER BY created_at ASC, locator ASC",
        )?;
        let rows = statement.query_map([task_id], read_ref)?;
        let mut refs = Vec::new();
        for row in rows {
            refs.push(row?);
        }
        Ok(refs)
    }

    /// Каскадное удаление ссылок задачи. Содержимое удаляется, только если на
    /// него больше нет живых ссылок; hash остаётся tombstone для аудита.
    pub fn forget_task_artifacts(
        &self,
        task_id: &str,
        now: i64,
        reason: &str,
    ) -> Result<usize, StorageError> {
        let refs = self.list_refs(task_id)?;
        let transaction = self.connection.unchecked_transaction()?;
        let mut delete_ref =
            transaction.prepare_cached("DELETE FROM task_artifact_refs WHERE locator = ?1")?;
        let mut count_refs = transaction
            .prepare_cached("SELECT COUNT(*) FROM task_artifact_refs WHERE content_hash = ?1")?;
        let mut delete_content =
            transaction.prepare_cached("DELETE FROM task_artifacts WHERE content_hash = ?1")?;
        let mut insert_tombstone = transaction.prepare_cached(
            "INSERT OR REPLACE INTO artifact_tombstones (content_hash, bytes, removed_at, reason)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        let mut removed = 0;
        for reference in refs {
            delete_ref.execute([&reference.locator])?;
            removed += 1;
            let remaining: i64 =
                count_refs.query_row([&reference.content_hash], |row| row.get(0))?;
            if remaining == 0 {
                delete_content.execute([&reference.content_hash])?;
                insert_tombstone.execute(rusqlite::params![
                    reference.content_hash,
                    reference.bytes as i64,
                    now,
                    reason,
                ])?;
            }
        }
        drop(insert_tombstone);
        drop(delete_content);
        drop(count_refs);
        drop(delete_ref);
        transaction.commit()?;
        Ok(removed)
    }

    /// Вытеснение по TTL и последнему обращению до освобождения `needed_bytes`.
    pub fn evict(&self, needed_bytes: u64, now: i64) -> Result<u64, StorageError> {
        let candidates = self.eviction_candidates(now)?;
        let plan = plan_eviction(&candidates, needed_bytes);
        let transaction = self.connection.unchecked_transaction()?;
        let mut get_ref = transaction.prepare_cached(
            "SELECT locator, content_hash, task_id, owner_task_id, bytes, privacy,
                    status, created_at, last_access_at, ttl_ms, summary
             FROM task_artifact_refs WHERE locator = ?1",
        )?;
        let mut set_ref_status = transaction
            .prepare_cached("UPDATE task_artifact_refs SET status = ?2 WHERE locator = ?1")?;
        let mut delete_ref =
            transaction.prepare_cached("DELETE FROM task_artifact_refs WHERE locator = ?1")?;
        let mut count_live_refs = transaction.prepare_cached(
            "SELECT COUNT(*) FROM task_artifact_refs
             WHERE content_hash = ?1 AND status = 'live'",
        )?;
        let mut delete_content =
            transaction.prepare_cached("DELETE FROM task_artifacts WHERE content_hash = ?1")?;
        let mut insert_tombstone = transaction.prepare_cached(
            "INSERT OR REPLACE INTO artifact_tombstones (content_hash, bytes, removed_at, reason)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for locator in &plan.evicted {
            let Some(reference) = get_ref.query_row([locator], read_ref).optional()? else {
                continue;
            };
            if plan.marked_expired.contains(locator) {
                // Ссылка из живого ledger entry или confirmed scratchpad:
                // помечается `expired` с сохранением hash и размера.
                set_ref_status.execute(rusqlite::params![
                    locator,
                    ArtifactRefStatus::Expired.as_str(),
                ])?;
            } else {
                delete_ref.execute([locator])?;
            }
            let live_refs: i64 =
                count_live_refs.query_row([&reference.content_hash], |row| row.get(0))?;
            if live_refs == 0 {
                delete_content.execute([&reference.content_hash])?;
                insert_tombstone.execute(rusqlite::params![
                    reference.content_hash,
                    reference.bytes as i64,
                    now,
                    "evicted",
                ])?;
            }
        }
        drop(insert_tombstone);
        drop(delete_content);
        drop(count_live_refs);
        drop(delete_ref);
        drop(set_ref_status);
        drop(get_ref);
        transaction.commit()?;
        Ok(plan.freed_bytes)
    }

    /// Суммарный размер содержимого, занятого задачей.
    pub fn task_bytes(&self, task_id: &str) -> Result<u64, StorageError> {
        let bytes: i64 = self.connection.query_row(
            "SELECT COALESCE(SUM(bytes), 0) FROM task_artifact_refs
             WHERE task_id = ?1 AND status = 'live'",
            [task_id],
            |row| row.get(0),
        )?;
        Ok(bytes.max(0) as u64)
    }

    /// Суммарный размер содержимого на диске.
    pub fn total_bytes(&self) -> Result<u64, StorageError> {
        let bytes: i64 = self.connection.query_row(
            "SELECT COALESCE(SUM(bytes), 0) FROM task_artifacts",
            [],
            |row| row.get(0),
        )?;
        Ok(bytes.max(0) as u64)
    }

    pub fn tombstone(&self, content_hash: &str) -> Result<Option<ArtifactTombstone>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT content_hash, bytes, removed_at, reason FROM artifact_tombstones
                 WHERE content_hash = ?1",
                [content_hash],
                |row| {
                    Ok(ArtifactTombstone {
                        content_hash: row.get(0)?,
                        bytes: row.get::<_, i64>(1)?.max(0) as u64,
                        removed_at: row.get(2)?,
                        reason: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    fn eviction_candidates(&self, now: i64) -> Result<Vec<EvictionCandidate>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT locator, content_hash, task_id, owner_task_id, bytes, privacy,
                    status, created_at, last_access_at, ttl_ms, summary,
                    EXISTS(
                        SELECT 1 FROM task_scratchpad
                        WHERE artifact_locator = task_artifact_refs.locator
                          AND status = 'confirmed'
                    )
             FROM task_artifact_refs WHERE status = 'live'",
        )?;
        let rows = statement.query_map([], |row| Ok((read_ref(row)?, row.get(11)?)))?;
        let mut candidates = Vec::new();
        for row in rows {
            let (reference, referenced): (ArtifactRef, bool) = row?;
            candidates.push(EvictionCandidate {
                ttl_expired: reference.ttl_expired(now),
                locator: reference.locator,
                bytes: reference.bytes,
                last_access_at: reference.last_access_at,
                referenced,
            });
        }
        Ok(candidates)
    }

    fn ensure_quota(&self, task_id: &str, bytes: u64, now: i64) -> Result<(), StorageError> {
        let task_used = self.task_bytes(task_id)?;
        if task_used.saturating_add(bytes) > self.quota.per_task_bytes {
            let needed = task_used.saturating_add(bytes) - self.quota.per_task_bytes;
            let freed = self.evict(needed, now)?;
            if freed < needed {
                return Err(StorageError::Context(
                    ArtifactError::QuotaExceeded {
                        scope: "task",
                        needed: bytes,
                        available: self.quota.per_task_bytes.saturating_sub(task_used),
                    }
                    .to_string(),
                ));
            }
        }
        let total_used = self.total_bytes()?;
        if total_used.saturating_add(bytes) > self.quota.total_bytes {
            let needed = total_used.saturating_add(bytes) - self.quota.total_bytes;
            let freed = self.evict(needed, now)?;
            if freed < needed {
                return Err(StorageError::Context(
                    ArtifactError::QuotaExceeded {
                        scope: "disk",
                        needed: bytes,
                        available: self.quota.total_bytes.saturating_sub(total_used),
                    }
                    .to_string(),
                ));
            }
        }
        Ok(())
    }

    fn write_ref(&self, reference: &ArtifactRef) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT OR REPLACE INTO task_artifact_refs (
                locator, content_hash, task_id, owner_task_id, bytes, privacy,
                status, created_at, last_access_at, ttl_ms, summary
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                reference.locator,
                reference.content_hash,
                reference.task_id,
                reference.owner_task_id,
                reference.bytes as i64,
                reference.privacy.as_str(),
                reference.status.as_str(),
                reference.created_at,
                reference.last_access_at,
                reference.ttl_ms,
                reference.summary,
            ],
        )?;
        Ok(())
    }

    fn set_ref_status(&self, locator: &str, status: ArtifactRefStatus) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE task_artifact_refs SET status = ?2 WHERE locator = ?1",
            rusqlite::params![locator, status.as_str()],
        )?;
        Ok(())
    }

    fn is_tombstoned(&self, content_hash: &str) -> Result<bool, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM artifact_tombstones WHERE content_hash = ?1",
            [content_hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    fn content_exists(&self, content_hash: &str) -> Result<bool, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM task_artifacts WHERE content_hash = ?1",
            [content_hash],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    fn existing_status(
        &self,
        content_hash: &str,
    ) -> Result<Option<ArtifactRefStatus>, StorageError> {
        let status: Option<String> = self
            .connection
            .query_row(
                "SELECT status FROM task_artifact_refs WHERE content_hash = ?1
                 ORDER BY CASE status WHEN 'live' THEN 0 ELSE 1 END LIMIT 1",
                [content_hash],
                |row| row.get(0),
            )
            .optional()?;
        Ok(status.as_deref().map(ArtifactRefStatus::parse))
    }
}

fn read_ref(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactRef> {
    let privacy: String = row.get(5)?;
    let status: String = row.get(6)?;
    Ok(ArtifactRef {
        locator: row.get(0)?,
        content_hash: row.get(1)?,
        task_id: row.get(2)?,
        owner_task_id: row.get(3)?,
        bytes: row.get::<_, i64>(4)?.max(0) as u64,
        privacy: match privacy.as_str() {
            "secret" => Privacy::Secret,
            "sensitive" => Privacy::Sensitive,
            _ => Privacy::Workspace,
        },
        status: ArtifactRefStatus::parse(&status),
        created_at: row.get(7)?,
        last_access_at: row.get(8)?,
        ttl_ms: row.get(9)?,
        summary: row.get(10)?,
    })
}

#[cfg(test)]
#[path = "artifact_store_tests.rs"]
mod tests;
