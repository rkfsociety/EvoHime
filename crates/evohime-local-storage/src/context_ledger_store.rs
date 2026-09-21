//! Хранилище `context_ledger` (этап 01.1).
//!
//! Записи immutable: при апгрейде Core старые записи читаются по своей
//! `schema_version` без перезаписи и без пересчёта hash. Фактический usage
//! провайдера пишется в отдельную append-only таблицу, поэтому запись остаётся
//! hash-стабильной.

use std::{collections::HashSet, thread::sleep, time::Duration};

use evohime_context_budget::{
    budget::BudgetUnavailable,
    item::DropReason,
    ladder::LadderLevel,
    ledger::{
        CompressionRecord, ContextLedgerEntry, ContextLedgerUsage, DroppedItemRecord,
        LedgerOutcome, LoadoutRecord, MandatoryPartRecord, SelectedItemRecord,
        LEDGER_RETAINED_SESSIONS, LEDGER_RETENTION_DAYS,
    },
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::StorageError;

/// Таймаут ожидания блокировки SQLite.
pub const BUSY_TIMEOUT_MS: u32 = 5_000;

/// Задержки повторов записи при `SQLITE_BUSY` после истечения timeout.
const RETRY_BACKOFF_MS: [u64; 3] = [50, 100, 200];

/// Максимум одновременных сборок контекста. Превышение ставит задачу в очередь,
/// а не расширяет параллельность.
pub const MAX_CONCURRENT_MODEL_CALLS: usize = 4;

/// Базовые лимиты bounded-вывода из 01.1.
pub const BOUNDED_ID_LIMIT: usize = 100;
pub const BOUNDED_REASON_CHARS: usize = 200;

/// Диагностика неудачной записи ledger. Model call при этом не выполняется.
pub const LEDGER_WRITE_FAILED: &str = "ledger_write_failed";

pub const COMPACTION_OPERATION_KEY_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionOperation {
    pub operation_key: String,
    pub scope_id: String,
    pub snapshot_revision: i64,
    pub state: String,
    pub summary_id: Option<String>,
    pub fallback: bool,
    pub fallback_reason: Option<String>,
}

/// Идемпотентный durable state compaction. Уникальность operation key
/// обеспечивается SQLite, а не только проверкой в памяти вызывающего кода.
pub fn install_compaction_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS context_compaction_operations (
            operation_key TEXT PRIMARY KEY NOT NULL,
            scope_id TEXT NOT NULL,
            snapshot_revision INTEGER NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('planned','running','cancelled','committed','failed')),
            summary_id TEXT,
            fallback INTEGER NOT NULL DEFAULT 0 CHECK(fallback IN (0,1)),
            fallback_reason TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_context_compaction_scope
            ON context_compaction_operations(scope_id, snapshot_revision);
        CREATE TABLE IF NOT EXISTS context_compaction_provenance (
            summary_id TEXT NOT NULL,
            source_item_id TEXT NOT NULL,
            sequence_id INTEGER,
            provenance_status TEXT NOT NULL CHECK(provenance_status IN ('complete','incomplete')),
            PRIMARY KEY(summary_id, source_item_id)
        );
        CREATE TABLE IF NOT EXISTS context_compaction_projections (
            summary_id TEXT PRIMARY KEY NOT NULL,
            schema_version INTEGER NOT NULL,
            payload_version INTEGER NOT NULL,
            snapshot_revision INTEGER NOT NULL,
            operation_key TEXT NOT NULL UNIQUE,
            summarizer_version TEXT NOT NULL,
            payload TEXT NOT NULL,
            committed_at INTEGER NOT NULL
        );",
    )?;
    Ok(())
}

/// Bounded read-only projection записи ledger для IPC и UI (этап 01.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextLedgerProjection {
    pub id: String,
    pub schema_version: u32,
    pub task_id: String,
    pub model_call_id: String,
    pub created_at: i64,
    pub provider: String,
    pub model: String,
    pub profile_version: String,
    pub tokenizer_version: String,
    pub context_ledger_hash: String,
    pub outcome: String,
    pub mandatory_tokens: u32,
    pub selected_optional_tokens: u32,
    pub reserves_tokens: u32,
    pub estimated_prompt_tokens: u32,
    /// Не более [`BOUNDED_ID_LIMIT`] элементов.
    pub selected_item_ids: Vec<String>,
    /// Не более [`BOUNDED_ID_LIMIT`] элементов.
    pub dropped_items: Vec<DroppedProjection>,
    /// Факт усечения любого из списков.
    pub truncated: bool,
    pub ladder_levels_applied: Vec<String>,
    pub compression: Vec<CompressionProjection>,
    pub loadout: Option<LoadoutRecord>,
    pub fallback_estimator: bool,
    pub budget_unavailable: Option<BudgetUnavailable>,
}

/// Отброшенный item в projection: только id и причина.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedProjection {
    pub id: String,
    pub drop_reason: String,
}

/// Compression-решение в projection: без текста summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionProjection {
    pub summary_id: String,
    pub source_count: usize,
    pub compression_ratio: f64,
    pub summarizer_version: String,
    pub fallback: bool,
    /// Bounded причина fallback, не более [`BOUNDED_REASON_CHARS`] символов.
    pub fallback_reason: Option<String>,
}

fn bounded_reason(reason: &str) -> String {
    reason.chars().take(BOUNDED_REASON_CHARS).collect()
}

/// Хранилище ledger поверх общей миграции базы.
pub struct ContextLedgerStore<'a> {
    connection: &'a Connection,
}

impl<'a> ContextLedgerStore<'a> {
    pub fn new(connection: &'a Connection) -> Result<Self, StorageError> {
        // WAL и busy_timeout: чтения диагностики идут из WAL-снимка и не
        // блокируют писателей.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.busy_timeout(Duration::from_millis(u64::from(BUSY_TIMEOUT_MS)))?;
        Ok(Self { connection })
    }

    pub fn begin_compaction(
        &self,
        operation_key: &str,
        scope_id: &str,
        snapshot_revision: i64,
    ) -> Result<CompactionOperation, StorageError> {
        if operation_key.is_empty() || operation_key.len() > COMPACTION_OPERATION_KEY_BYTES {
            return Err(StorageError::InvalidInput(
                "invalid compaction operation key".into(),
            ));
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO context_compaction_operations
             (operation_key, scope_id, snapshot_revision, state)
             VALUES (?1, ?2, ?3, 'planned')",
            rusqlite::params![operation_key, scope_id, snapshot_revision],
        )?;
        self.connection.execute(
            "UPDATE context_compaction_operations SET state = 'running'
             WHERE operation_key = ?1 AND state = 'planned'",
            [operation_key],
        )?;
        self.compaction_operation(operation_key)
    }

    pub fn finish_compaction(
        &self,
        operation_key: &str,
        summary_id: &str,
        fallback: bool,
        fallback_reason: Option<&str>,
    ) -> Result<CompactionOperation, StorageError> {
        self.connection.execute(
            "UPDATE context_compaction_operations
             SET state = 'committed', summary_id = ?2, fallback = ?3, fallback_reason = ?4
             WHERE operation_key = ?1 AND state = 'running'",
            rusqlite::params![
                operation_key,
                summary_id,
                i32::from(fallback),
                fallback_reason
            ],
        )?;
        self.compaction_operation(operation_key)
    }

    pub fn cancel_compaction(
        &self,
        operation_key: &str,
    ) -> Result<CompactionOperation, StorageError> {
        self.connection.execute(
            "UPDATE context_compaction_operations SET state = 'cancelled'
             WHERE operation_key = ?1 AND state IN ('planned', 'running')",
            [operation_key],
        )?;
        self.compaction_operation(operation_key)
    }

    pub fn compaction_operation(
        &self,
        operation_key: &str,
    ) -> Result<CompactionOperation, StorageError> {
        Ok(self.connection.query_row(
            "SELECT operation_key, scope_id, snapshot_revision, state, summary_id,
                    fallback, fallback_reason
             FROM context_compaction_operations WHERE operation_key = ?1",
            [operation_key],
            |row| {
                Ok(CompactionOperation {
                    operation_key: row.get(0)?,
                    scope_id: row.get(1)?,
                    snapshot_revision: row.get(2)?,
                    state: row.get(3)?,
                    summary_id: row.get(4)?,
                    fallback: row.get::<_, i32>(5)? != 0,
                    fallback_reason: row.get(6)?,
                })
            },
        )?)
    }

    /// Запись ledger одной транзакцией `BEGIN IMMEDIATE`: либо появляется полная
    /// запись с hash, либо не появляется ничего. При `SQLITE_BUSY` запись
    /// повторяется до трёх раз с экспоненциальной задержкой; повтор записи в БД
    /// не является запрещённым retry model call.
    pub fn append(&self, entry: &ContextLedgerEntry) -> Result<(), StorageError> {
        let mut attempt = 0_usize;
        loop {
            match self.append_once(entry) {
                Ok(()) => return Ok(()),
                Err(error) if attempt < RETRY_BACKOFF_MS.len() && is_busy(&error) => {
                    sleep(Duration::from_millis(RETRY_BACKOFF_MS[attempt]));
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn append_once(&self, entry: &ContextLedgerEntry) -> Result<(), StorageError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.connection.execute(
            "INSERT OR REPLACE INTO context_ledger (
                id, schema_version, task_id, session_id, model_call_id, created_at,
                provider, model, profile_version, profile_snapshot, tokenizer_version,
                normalizer_version, strategy_version, mandatory_tokens,
                selected_optional_tokens, reserves_tokens, estimated_prompt_tokens,
                selected_items, dropped_items, mandatory_parts, ladder_levels_applied,
                compression, loadout, fallback_estimator, replan_of, outcome,
                budget_unavailable, context_ledger_hash
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28
             )",
            rusqlite::params![
                entry.id,
                entry.schema_version,
                entry.task_id,
                entry.session_id,
                entry.model_call_id,
                entry.created_at,
                entry.provider,
                entry.model,
                entry.profile_version,
                entry.profile_snapshot,
                entry.tokenizer_version,
                entry.normalizer_version,
                entry.strategy_version,
                entry.mandatory_tokens,
                entry.selected_optional_tokens,
                entry.reserves_tokens,
                entry.estimated_prompt_tokens,
                serde_json::to_string(&entry.selected_items)?,
                serde_json::to_string(&entry.dropped_items)?,
                serde_json::to_string(&entry.mandatory_parts)?,
                serde_json::to_string(&entry.ladder_levels_applied)?,
                serde_json::to_string(&entry.compression)?,
                entry
                    .loadout
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                i32::from(entry.fallback_estimator),
                entry.replan_of,
                entry.outcome.as_str(),
                entry
                    .budget_unavailable
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                entry.context_ledger_hash,
            ],
        );
        match result {
            Ok(_) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(StorageError::Sqlite(error))
            }
        }
    }

    /// Фактический usage провайдера. Пишется append-only и не меняет запись ledger.
    pub fn record_usage(&self, usage: &ContextLedgerUsage) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT INTO context_ledger_usage (
                ledger_id, actual_prompt_tokens, actual_completion_tokens,
                estimator_drift, recorded_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                usage.ledger_id,
                usage.actual_prompt_tokens,
                usage.actual_completion_tokens,
                usage.estimator_drift,
                usage.recorded_at,
            ],
        )?;
        Ok(())
    }

    /// Регистрация ссылки receipt из 03.4. Записи с неэкспортированным receipt
    /// не удаляются ротацией.
    pub fn register_receipt(
        &self,
        ledger_id: &str,
        receipt_id: &str,
        exported: bool,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "INSERT OR REPLACE INTO context_ledger_receipts (ledger_id, receipt_id, exported)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![ledger_id, receipt_id, i32::from(exported)],
        )?;
        Ok(())
    }

    /// Чтение полной записи. Записи со старой `schema_version` читаются своим
    /// reader'ом: неизвестные значения справочников не являются ошибкой.
    pub fn get(&self, id: &str) -> Result<Option<ContextLedgerEntry>, StorageError> {
        let entry = self
            .connection
            .query_row(
                "SELECT id, schema_version, task_id, session_id, model_call_id, created_at,
                        provider, model, profile_version, profile_snapshot, tokenizer_version,
                        normalizer_version, strategy_version, mandatory_tokens,
                        selected_optional_tokens, reserves_tokens, estimated_prompt_tokens,
                        selected_items, dropped_items, mandatory_parts, ladder_levels_applied,
                        compression, loadout, fallback_estimator, replan_of, outcome,
                        budget_unavailable, context_ledger_hash
                 FROM context_ledger WHERE id = ?1",
                [id],
                read_entry,
            )
            .optional()?;
        Ok(entry)
    }

    /// Поиск записи по hash: валидация upstream сравнивает hash с уже записанным
    /// ledger entry, а не пересчитывает контекст.
    pub fn find_by_hash(&self, hash: &str) -> Result<Option<ContextLedgerEntry>, StorageError> {
        let entry = self
            .connection
            .query_row(
                "SELECT id, schema_version, task_id, session_id, model_call_id, created_at,
                        provider, model, profile_version, profile_snapshot, tokenizer_version,
                        normalizer_version, strategy_version, mandatory_tokens,
                        selected_optional_tokens, reserves_tokens, estimated_prompt_tokens,
                        selected_items, dropped_items, mandatory_parts, ladder_levels_applied,
                        compression, loadout, fallback_estimator, replan_of, outcome,
                        budget_unavailable, context_ledger_hash
                 FROM context_ledger WHERE context_ledger_hash = ?1
                 ORDER BY created_at DESC LIMIT 1",
                [hash],
                read_entry,
            )
            .optional()?;
        Ok(entry)
    }

    /// Bounded projection последних записей задачи для UI.
    pub fn projection(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<ContextLedgerProjection>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, schema_version, task_id, session_id, model_call_id, created_at,
                    provider, model, profile_version, profile_snapshot, tokenizer_version,
                    normalizer_version, strategy_version, mandatory_tokens,
                    selected_optional_tokens, reserves_tokens, estimated_prompt_tokens,
                    selected_items, dropped_items, mandatory_parts, ladder_levels_applied,
                    compression, loadout, fallback_estimator, replan_of, outcome,
                    budget_unavailable, context_ledger_hash
             FROM context_ledger WHERE task_id = ?1
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(
            rusqlite::params![task_id, i64::try_from(limit).unwrap_or(i64::MAX)],
            read_entry,
        )?;
        let mut projections = Vec::new();
        for row in rows {
            projections.push(project(&row?));
        }
        Ok(projections)
    }

    /// Ротация: запись хранится, пока выполняется хотя бы одно условие — возраст
    /// менее 30 дней или принадлежность одной из последних 200 сессий. Записи,
    /// на которые ссылается неэкспортированный receipt, не удаляются.
    pub fn prune(&self, now: i64) -> Result<u64, StorageError> {
        let age_cutoff = now - LEDGER_RETENTION_DAYS * 24 * 60 * 60 * 1000;
        let mut statement = self.connection.prepare(
            "SELECT session_id FROM context_ledger
             GROUP BY session_id ORDER BY MAX(created_at) DESC LIMIT ?1",
        )?;
        let recent_sessions: HashSet<String> = statement
            .query_map(
                [i64::try_from(LEDGER_RETAINED_SESSIONS).unwrap_or(i64::MAX)],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<_, _>>()?;
        drop(statement);

        // Filter pinned ledgers in SQL instead of issuing one receipt lookup
        // per candidate below. Retention can see many old rows, so the
        // previous N+1 query pattern made a routine sweep increasingly costly.
        let mut removable = self.connection.prepare(
            "SELECT id, session_id FROM context_ledger
             WHERE created_at < ?1
               AND NOT EXISTS (
                 SELECT 1 FROM context_ledger_receipts
                 WHERE context_ledger_receipts.ledger_id = context_ledger.id
                   AND context_ledger_receipts.exported = 0
               )",
        )?;
        let candidates: Vec<(String, String)> = removable
            .query_map([age_cutoff], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<_, _>>()?;
        drop(removable);

        let transaction = self.connection.unchecked_transaction()?;
        let mut delete_usage =
            transaction.prepare_cached("DELETE FROM context_ledger_usage WHERE ledger_id = ?1")?;
        let mut delete_receipts = transaction
            .prepare_cached("DELETE FROM context_ledger_receipts WHERE ledger_id = ?1")?;
        let mut delete_ledger =
            transaction.prepare_cached("DELETE FROM context_ledger WHERE id = ?1")?;
        let mut removed = 0_u64;
        for (id, session_id) in candidates {
            if recent_sessions.contains(&session_id) {
                continue;
            }
            // Запись удаляется целиком, вместе со строками usage.
            delete_usage.execute([&id])?;
            delete_receipts.execute([&id])?;
            delete_ledger.execute([&id])?;
            removed += 1;
        }
        drop(delete_ledger);
        drop(delete_receipts);
        drop(delete_usage);
        transaction.commit()?;
        Ok(removed)
    }

    /// Число записей — нужно тестам и диагностике.
    pub fn count(&self) -> Result<i64, StorageError> {
        Ok(self
            .connection
            .query_row("SELECT COUNT(*) FROM context_ledger", [], |row| row.get(0))?)
    }
}

fn is_busy(error: &StorageError) -> bool {
    matches!(
        error,
        StorageError::Sqlite(rusqlite::Error::SqliteFailure(inner, _))
            if inner.code == rusqlite::ErrorCode::DatabaseBusy
                || inner.code == rusqlite::ErrorCode::DatabaseLocked
    )
}

fn read_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextLedgerEntry> {
    let selected_items: String = row.get(17)?;
    let dropped_items: String = row.get(18)?;
    let mandatory_parts: String = row.get(19)?;
    let ladder_levels: String = row.get(20)?;
    let compression: String = row.get(21)?;
    let loadout: Option<String> = row.get(22)?;
    let budget_unavailable: Option<String> = row.get(26)?;
    let outcome: String = row.get(25)?;
    Ok(ContextLedgerEntry {
        id: row.get(0)?,
        schema_version: row.get(1)?,
        task_id: row.get(2)?,
        session_id: row.get(3)?,
        model_call_id: row.get(4)?,
        created_at: row.get(5)?,
        provider: row.get(6)?,
        model: row.get(7)?,
        profile_version: row.get(8)?,
        profile_snapshot: row.get(9)?,
        tokenizer_version: row.get(10)?,
        normalizer_version: row.get(11)?,
        strategy_version: row.get(12)?,
        mandatory_tokens: row.get(13)?,
        selected_optional_tokens: row.get(14)?,
        reserves_tokens: row.get(15)?,
        estimated_prompt_tokens: row.get(16)?,
        selected_items: parse_json::<Vec<SelectedItemRecord>>(&selected_items),
        dropped_items: parse_json::<Vec<DroppedItemRecord>>(&dropped_items),
        mandatory_parts: parse_json::<Vec<MandatoryPartRecord>>(&mandatory_parts),
        ladder_levels_applied: parse_json::<Vec<LadderLevel>>(&ladder_levels),
        compression: parse_json::<Vec<CompressionRecord>>(&compression),
        loadout: loadout
            .as_deref()
            .and_then(|json| serde_json::from_str::<LoadoutRecord>(json).ok()),
        fallback_estimator: row.get::<_, i32>(23)? != 0,
        replan_of: row.get(24)?,
        outcome: if outcome == LedgerOutcome::BudgetUnavailable.as_str() {
            LedgerOutcome::BudgetUnavailable
        } else {
            LedgerOutcome::Sent
        },
        budget_unavailable: budget_unavailable
            .as_deref()
            .and_then(|json| serde_json::from_str::<BudgetUnavailable>(json).ok()),
        context_ledger_hash: row.get(27)?,
    })
}

/// Разбор JSON-поля. Неизвестные значения справочника не роняют чтение: потеря
/// одного поля не должна делать всю запись нечитаемой.
fn parse_json<T: serde::de::DeserializeOwned + Default>(json: &str) -> T {
    serde_json::from_str(json).unwrap_or_default()
}

fn project(entry: &ContextLedgerEntry) -> ContextLedgerProjection {
    let selected_truncated = entry.selected_items.len() > BOUNDED_ID_LIMIT;
    let dropped_truncated = entry.dropped_items.len() > BOUNDED_ID_LIMIT;
    ContextLedgerProjection {
        id: entry.id.clone(),
        schema_version: entry.schema_version,
        task_id: entry.task_id.clone(),
        model_call_id: entry.model_call_id.clone(),
        created_at: entry.created_at,
        provider: entry.provider.clone(),
        model: entry.model.clone(),
        profile_version: entry.profile_version.clone(),
        tokenizer_version: entry.tokenizer_version.clone(),
        context_ledger_hash: entry.context_ledger_hash.clone(),
        outcome: entry.outcome.as_str().to_string(),
        mandatory_tokens: entry.mandatory_tokens,
        selected_optional_tokens: entry.selected_optional_tokens,
        reserves_tokens: entry.reserves_tokens,
        estimated_prompt_tokens: entry.estimated_prompt_tokens,
        selected_item_ids: entry
            .selected_items
            .iter()
            .take(BOUNDED_ID_LIMIT)
            .map(|item| item.id.clone())
            .collect(),
        dropped_items: entry
            .dropped_items
            .iter()
            .take(BOUNDED_ID_LIMIT)
            .map(|item| DroppedProjection {
                id: item.id.clone(),
                drop_reason: reason_label(item.drop_reason),
            })
            .collect(),
        truncated: selected_truncated || dropped_truncated,
        ladder_levels_applied: entry
            .ladder_levels_applied
            .iter()
            .map(|level| level.as_str().to_string())
            .collect(),
        compression: entry
            .compression
            .iter()
            .map(|record| CompressionProjection {
                summary_id: record.summary_id.clone(),
                source_count: record.source_ids.len(),
                compression_ratio: record.compression_ratio,
                summarizer_version: record.summarizer_version.clone(),
                fallback: record.fallback,
                fallback_reason: record.fallback_reason.as_deref().map(bounded_reason),
            })
            .collect(),
        loadout: entry.loadout.clone(),
        fallback_estimator: entry.fallback_estimator,
        budget_unavailable: entry.budget_unavailable.clone(),
    }
}

fn reason_label(reason: DropReason) -> String {
    reason.as_str().to_string()
}

#[cfg(test)]
#[path = "context_ledger_store_tests.rs"]
mod tests;
