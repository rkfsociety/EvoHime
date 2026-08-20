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
                locator: locator.clone(),
                content_hash: hash.clone(),
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
                    expected: reference.content_hash.clone(),
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
        let mut removed = 0;
        for reference in refs {
            self.connection.execute(
                "DELETE FROM task_artifact_refs WHERE locator = ?1",
                [&reference.locator],
            )?;
            removed += 1;
            let remaining: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM task_artifact_refs WHERE content_hash = ?1",
                [&reference.content_hash],
                |row| row.get(0),
            )?;
            if remaining == 0 {
                self.remove_content(&reference.content_hash, reference.bytes, now, reason)?;
            }
        }
        Ok(removed)
    }

    /// Вытеснение по TTL и последнему обращению до освобождения `needed_bytes`.
    pub fn evict(&self, needed_bytes: u64, now: i64) -> Result<u64, StorageError> {
        let candidates = self.eviction_candidates(now)?;
        let plan = plan_eviction(&candidates, needed_bytes);
        for locator in &plan.evicted {
            let Some(reference) = self.get_ref(locator)? else {
                continue;
            };
            if plan.marked_expired.contains(locator) {
                // Ссылка из живого ledger entry или confirmed scratchpad:
                // помечается `expired` с сохранением hash и размера.
                self.set_ref_status(locator, ArtifactRefStatus::Expired)?;
            } else {
                self.connection.execute(
                    "DELETE FROM task_artifact_refs WHERE locator = ?1",
                    [locator],
                )?;
            }
            let live_refs: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM task_artifact_refs
                 WHERE content_hash = ?1 AND status = 'live'",
                [&reference.content_hash],
                |row| row.get(0),
            )?;
            if live_refs == 0 {
                self.remove_content(&reference.content_hash, reference.bytes, now, "evicted")?;
            }
        }
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
                    status, created_at, last_access_at, ttl_ms, summary
             FROM task_artifact_refs WHERE status = 'live'",
        )?;
        let rows = statement.query_map([], read_ref)?;
        let mut candidates = Vec::new();
        for row in rows {
            let reference = row?;
            let referenced: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM task_scratchpad
                 WHERE artifact_locator = ?1 AND status = 'confirmed'",
                [&reference.locator],
                |row| row.get(0),
            )?;
            candidates.push(EvictionCandidate {
                ttl_expired: reference.ttl_expired(now),
                locator: reference.locator,
                bytes: reference.bytes,
                last_access_at: reference.last_access_at,
                referenced: referenced > 0,
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

    fn remove_content(
        &self,
        content_hash: &str,
        bytes: u64,
        now: i64,
        reason: &str,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM task_artifacts WHERE content_hash = ?1",
            [content_hash],
        )?;
        // Hash сохраняется как tombstone только для аудита и не считается
        // доступным dedup-hit для нового offload.
        self.connection.execute(
            "INSERT OR REPLACE INTO artifact_tombstones (content_hash, bytes, removed_at, reason)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![content_hash, bytes as i64, now, reason],
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
mod tests {
    use super::*;
    use crate::LocalDatabase;

    fn database(name: &str) -> LocalDatabase {
        let path = std::env::temp_dir().join(format!(
            "evohime-artifact-{name}-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        LocalDatabase::open(&path).expect("database opens")
    }

    const KIND: &str = "tool_result";

    #[test]
    fn a_large_output_is_stored_and_summarized_for_the_context() {
        let database = database("offload");
        let store = ArtifactStore::new(database.connection());
        let content = (1..=50)
            .map(|index| format!("строка {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = store
            .offload(KIND, "task", "task", &content, Privacy::Workspace, 1_000)
            .expect("offload succeeds");
        assert!(!result.deduplicated);
        assert!(result.reference.summary.contains("ещё"));
        assert!(result.reference.summary.chars().count() <= ARTIFACT_SUMMARY_CHARS + 64);
        assert_eq!(result.reference.bytes, content.len() as u64);
        let read = store
            .read(&result.reference.locator, "task", &[], KIND, 2_000)
            .expect("read succeeds");
        assert_eq!(read, content);
    }

    #[test]
    fn repeated_offload_of_the_same_content_reuses_the_artifact() {
        let database = database("dedup");
        let store = ArtifactStore::new(database.connection());
        let first = store
            .offload(
                KIND,
                "task-a",
                "task-a",
                "одно и то же",
                Privacy::Workspace,
                1_000,
            )
            .expect("offload succeeds");
        let second = store
            .offload(
                KIND,
                "task-b",
                "task-b",
                "одно и то же",
                Privacy::Workspace,
                2_000,
            )
            .expect("offload succeeds");
        assert!(!first.deduplicated);
        assert!(second.deduplicated);
        assert_eq!(first.reference.content_hash, second.reference.content_hash);
        assert_ne!(first.reference.locator, second.reference.locator);
        let stored: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM task_artifacts", [], |row| row.get(0))
            .expect("count");
        assert_eq!(stored, 1, "содержимое хранится один раз, ссылок — две");
    }

    #[test]
    fn privacy_labels_forbid_offload() {
        let database = database("privacy");
        let store = ArtifactStore::new(database.connection());
        for privacy in [Privacy::Sensitive, Privacy::Secret] {
            assert!(store
                .offload(KIND, "task", "task", "секрет", privacy, 1_000)
                .is_err());
        }
    }

    #[test]
    fn locator_access_is_limited_to_the_owner_and_its_children() {
        let database = database("access");
        let store = ArtifactStore::new(database.connection());
        let result = store
            .offload(
                KIND,
                "parent",
                "parent",
                "содержимое",
                Privacy::Workspace,
                1_000,
            )
            .expect("offload succeeds");
        assert!(store
            .read(&result.reference.locator, "parent", &[], KIND, 2_000)
            .is_ok());
        assert!(store
            .read(
                &result.reference.locator,
                "child",
                &["parent".to_string()],
                KIND,
                2_000
            )
            .is_ok());
        assert!(store
            .read(&result.reference.locator, "stranger", &[], KIND, 2_000)
            .is_err());
    }

    #[test]
    fn a_corrupted_artifact_is_marked_invalid_and_never_enters_the_context() {
        let database = database("corruption");
        let store = ArtifactStore::new(database.connection());
        let result = store
            .offload(
                KIND,
                "task",
                "task",
                "исходное содержимое",
                Privacy::Workspace,
                1_000,
            )
            .expect("offload succeeds");
        // Подмена содержимого мимо store.
        database
            .connection()
            .execute(
                "UPDATE task_artifacts SET content = ?2 WHERE content_hash = ?1",
                rusqlite::params![result.reference.content_hash, "подменённое".as_bytes()],
            )
            .expect("tampering succeeds");

        let error = store
            .read(&result.reference.locator, "task", &[], KIND, 2_000)
            .expect_err("hash check fails");
        assert!(error.to_string().contains("hash check"));
        assert_eq!(
            store
                .get_ref(&result.reference.locator)
                .expect("read")
                .expect("ref")
                .status,
            ArtifactRefStatus::Invalid
        );
    }

    #[test]
    fn quota_overflow_evicts_by_ttl_and_last_access_without_losing_referenced_links() {
        let database = database("quota");
        let quota = ArtifactQuota {
            per_task_bytes: 200,
            total_bytes: 200,
            default_ttl_ms: 1_000,
        };
        let store = ArtifactStore::with_quota(database.connection(), quota);
        let first = store
            .offload(
                KIND,
                "task",
                "task",
                &"a".repeat(90),
                Privacy::Workspace,
                1_000,
            )
            .expect("offload succeeds");
        // Ссылка из confirmed scratchpad: удалять содержимое молча нельзя.
        database
            .connection()
            .execute(
                "INSERT INTO task_scratchpad (
                    id, task_id, session_id, category, status, trust, privacy, revision,
                    parent_id, content, content_hash, created_at, updated_at, ttl_ms,
                    confirmation, artifact_locator, recovered_at_step
                 ) VALUES ('s1','task','session','facts','confirmed','confirmed','workspace',1,
                    NULL,'заметка','hash',1000,1000,NULL,'user_confirmed',?1,NULL)",
                [&first.reference.locator],
            )
            .expect("scratchpad link inserted");

        store
            .offload(
                KIND,
                "task",
                "task",
                &"b".repeat(90),
                Privacy::Workspace,
                2_000,
            )
            .expect("offload succeeds");
        // Третья выгрузка не помещается: сработает вытеснение.
        store
            .offload(
                KIND,
                "task",
                "task",
                &"c".repeat(90),
                Privacy::Workspace,
                5_000,
            )
            .expect("offload succeeds after eviction");

        let referenced = store
            .get_ref(&first.reference.locator)
            .expect("read")
            .expect("ref still exists");
        assert_eq!(referenced.status, ArtifactRefStatus::Expired);
        assert_eq!(referenced.bytes, 90, "размер сохраняется после вытеснения");
        assert_eq!(referenced.content_hash, first.reference.content_hash);
    }

    #[test]
    fn tombstoned_content_is_not_reused_as_a_dedup_hit() {
        let database = database("tombstone");
        let store = ArtifactStore::new(database.connection());
        let first = store
            .offload(
                KIND,
                "task",
                "task",
                "содержимое",
                Privacy::Workspace,
                1_000,
            )
            .expect("offload succeeds");
        store
            .forget_task_artifacts("task", 2_000, "forget memory")
            .expect("cascade delete succeeds");
        let tombstone = store
            .tombstone(&first.reference.content_hash)
            .expect("read")
            .expect("tombstone exists");
        assert_eq!(tombstone.bytes, first.reference.bytes);

        let second = store
            .offload(
                KIND,
                "task",
                "task",
                "содержимое",
                Privacy::Workspace,
                3_000,
            )
            .expect("offload succeeds");
        assert!(
            !second.deduplicated,
            "tombstone не считается доступным dedup-hit"
        );
    }

    #[test]
    fn cascade_delete_removes_refs_and_content_without_live_links() {
        let database = database("cascade");
        let store = ArtifactStore::new(database.connection());
        store
            .offload(KIND, "task", "task", "первое", Privacy::Workspace, 1_000)
            .expect("offload succeeds");
        store
            .offload(KIND, "task", "task", "второе", Privacy::Workspace, 1_100)
            .expect("offload succeeds");
        assert_eq!(
            store
                .forget_task_artifacts("task", 2_000, "forget memory")
                .expect("cascade"),
            2
        );
        assert!(store.list_refs("task").expect("list").is_empty());
        assert_eq!(store.total_bytes().expect("bytes"), 0);
    }

    #[test]
    fn an_expired_reference_is_not_readable_but_keeps_its_hash() {
        let database = database("expired-read");
        let store = ArtifactStore::new(database.connection());
        let result = store
            .offload(
                KIND,
                "task",
                "task",
                "содержимое",
                Privacy::Workspace,
                1_000,
            )
            .expect("offload succeeds");
        store
            .set_ref_status(&result.reference.locator, ArtifactRefStatus::Expired)
            .expect("status updated");
        assert!(store
            .read(&result.reference.locator, "task", &[], KIND, 2_000)
            .is_err());
        let reference = store
            .get_ref(&result.reference.locator)
            .expect("read")
            .expect("ref");
        assert_eq!(reference.content_hash, result.reference.content_hash);
    }

    #[test]
    fn concurrent_offload_of_identical_content_yields_one_artifact_and_two_refs() {
        let database = database("concurrent");
        let path = database.path().to_path_buf();
        drop(database);
        let handles: Vec<_> = ["task-a", "task-b"]
            .into_iter()
            .map(|task| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let database = LocalDatabase::open(&path).expect("database opens");
                    database
                        .connection()
                        .busy_timeout(std::time::Duration::from_millis(5_000))
                        .expect("timeout set");
                    let store = ArtifactStore::new(database.connection());
                    store
                        .offload(
                            KIND,
                            task,
                            task,
                            "общее содержимое",
                            Privacy::Workspace,
                            1_000,
                        )
                        .expect("offload succeeds");
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread completes");
        }
        let database = LocalDatabase::open(&path).expect("database opens");
        let artifacts: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM task_artifacts", [], |row| row.get(0))
            .expect("count");
        let refs: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM task_artifact_refs", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(artifacts, 1);
        assert_eq!(refs, 2);
    }
}
