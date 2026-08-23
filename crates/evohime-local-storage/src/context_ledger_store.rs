//! Хранилище `context_ledger` (этап 01.1).
//!
//! Записи immutable: при апгрейде Core старые записи читаются по своей
//! `schema_version` без перезаписи и без пересчёта hash. Фактический usage
//! провайдера пишется в отдельную append-only таблицу, поэтому запись остаётся
//! hash-стабильной.

use std::{thread::sleep, time::Duration};

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
        let recent_sessions: Vec<String> = statement
            .query_map(
                [i64::try_from(LEDGER_RETAINED_SESSIONS).unwrap_or(i64::MAX)],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<_, _>>()?;
        drop(statement);

        let mut removable = self
            .connection
            .prepare("SELECT id, session_id FROM context_ledger WHERE created_at < ?1")?;
        let candidates: Vec<(String, String)> = removable
            .query_map([age_cutoff], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<_, _>>()?;
        drop(removable);

        let mut removed = 0_u64;
        for (id, session_id) in candidates {
            if recent_sessions.contains(&session_id) {
                continue;
            }
            let pinned_by_receipt: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM context_ledger_receipts
                 WHERE ledger_id = ?1 AND exported = 0",
                [&id],
                |row| row.get(0),
            )?;
            if pinned_by_receipt > 0 {
                continue;
            }
            // Запись удаляется целиком, вместе со строками usage.
            self.connection.execute(
                "DELETE FROM context_ledger_usage WHERE ledger_id = ?1",
                [&id],
            )?;
            self.connection.execute(
                "DELETE FROM context_ledger_receipts WHERE ledger_id = ?1",
                [&id],
            )?;
            self.connection
                .execute("DELETE FROM context_ledger WHERE id = ?1", [&id])?;
            removed += 1;
        }
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
mod tests {
    use super::*;
    use evohime_context_budget::{
        budget::{BudgetUnavailableStage, MandatoryPart},
        ledger::CONTEXT_LEDGER_SCHEMA_VERSION,
    };

    use crate::LocalDatabase;

    fn database(name: &str) -> LocalDatabase {
        let path = std::env::temp_dir().join(format!(
            "evohime-ledger-{name}-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        LocalDatabase::open(&path).expect("database opens")
    }

    fn entry(id: &str, session: &str, created_at: i64) -> ContextLedgerEntry {
        let mut entry = ContextLedgerEntry {
            id: id.to_string(),
            schema_version: CONTEXT_LEDGER_SCHEMA_VERSION,
            task_id: "task".to_string(),
            session_id: session.to_string(),
            model_call_id: format!("call-{id}"),
            created_at,
            provider: "literouter".to_string(),
            model: "gpt-4o-mini".to_string(),
            profile_version: "profile-1".to_string(),
            profile_snapshot: "{}".to_string(),
            tokenizer_version: "heuristic-1".to_string(),
            normalizer_version: "norm-1".to_string(),
            strategy_version: "strategy-1".to_string(),
            mandatory_tokens: 100,
            selected_optional_tokens: 200,
            reserves_tokens: 300,
            estimated_prompt_tokens: 300,
            selected_items: vec![SelectedItemRecord {
                id: "a".to_string(),
                estimated_tokens: 100,
            }],
            dropped_items: vec![DroppedItemRecord {
                id: "b".to_string(),
                drop_reason: DropReason::LowPriority,
            }],
            mandatory_parts: vec![MandatoryPartRecord {
                part: MandatoryPart::SafetyPolicy,
                items: 1,
                tokens: 100,
            }],
            ladder_levels_applied: vec![LadderLevel::LowPriorityOptional],
            compression: Vec::new(),
            loadout: None,
            fallback_estimator: false,
            replan_of: None,
            outcome: LedgerOutcome::Sent,
            budget_unavailable: None,
            context_ledger_hash: String::new(),
        };
        entry.finalize_hash();
        entry
    }

    #[test]
    fn an_entry_round_trips_without_changing_its_hash() {
        let database = database("round-trip");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        let written = entry("ledger-1", "session-1", 1_000);
        store.append(&written).expect("append succeeds");
        let read = store
            .get("ledger-1")
            .expect("read succeeds")
            .expect("entry exists");
        assert_eq!(read, written);
        assert_eq!(read.context_ledger_hash, read.compute_hash());
    }

    #[test]
    fn usage_is_recorded_without_touching_the_immutable_entry() {
        let database = database("usage");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        let written = entry("ledger-1", "session-1", 1_000);
        store.append(&written).expect("append succeeds");
        store
            .record_usage(&ContextLedgerUsage {
                ledger_id: "ledger-1".to_string(),
                actual_prompt_tokens: 290,
                actual_completion_tokens: 50,
                estimator_drift: 0.034,
                recorded_at: 2_000,
            })
            .expect("usage recorded");
        let read = store.get("ledger-1").expect("read").expect("entry");
        assert_eq!(read.context_ledger_hash, written.context_ledger_hash);
        let usage_rows: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM context_ledger_usage", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(usage_rows, 1);
    }

    #[test]
    fn lookup_by_hash_finds_the_recorded_entry() {
        let database = database("by-hash");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        let written = entry("ledger-1", "session-1", 1_000);
        store.append(&written).expect("append succeeds");
        let found = store
            .find_by_hash(&written.context_ledger_hash)
            .expect("read")
            .expect("entry");
        assert_eq!(found.id, "ledger-1");
    }

    #[test]
    fn projection_is_bounded_and_marks_truncation() {
        let database = database("projection");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        let mut written = entry("ledger-1", "session-1", 1_000);
        written.selected_items = (0..250)
            .map(|index| SelectedItemRecord {
                id: format!("item-{index:03}"),
                estimated_tokens: 1,
            })
            .collect();
        written.finalize_hash();
        store.append(&written).expect("append succeeds");
        let projections = store.projection("task", 10).expect("projection");
        assert_eq!(projections.len(), 1);
        assert_eq!(projections[0].selected_item_ids.len(), BOUNDED_ID_LIMIT);
        assert!(projections[0].truncated);
    }

    #[test]
    fn a_refused_assembly_keeps_its_stage_in_the_projection() {
        let database = database("refusal");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        let mut written = entry("ledger-1", "session-1", 1_000);
        written.outcome = LedgerOutcome::BudgetUnavailable;
        written.budget_unavailable = Some(
            BudgetUnavailable::new(
                BudgetUnavailableStage::MandatoryOverflow,
                1_000,
                500,
                "profile-1",
                "heuristic-1",
            )
            .with_missing_part(Some(MandatoryPart::UserPrompt)),
        );
        written.finalize_hash();
        store.append(&written).expect("append succeeds");
        let projection = store.projection("task", 10).expect("projection");
        let refusal = projection[0]
            .budget_unavailable
            .as_ref()
            .expect("refusal is visible");
        assert_eq!(refusal.stage, BudgetUnavailableStage::MandatoryOverflow);
        assert_eq!(refusal.missing_part, Some(MandatoryPart::UserPrompt));
        assert_eq!(projection[0].outcome, "budget_unavailable");
    }

    #[test]
    fn rotation_removes_old_entries_together_with_their_usage_rows() {
        let database = database("rotation");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        let now = 1_800_000_000_000_i64;
        let old = now - 40 * 24 * 60 * 60 * 1000;
        // Свежая запись новой сессии: старая сессия перестаёт быть «последней».
        for index in 0..(LEDGER_RETAINED_SESSIONS + 1) {
            let mut fresh = entry(&format!("fresh-{index}"), &format!("session-{index}"), now);
            fresh.finalize_hash();
            store.append(&fresh).expect("append succeeds");
        }
        let stale = entry("stale", "session-stale", old);
        store.append(&stale).expect("append succeeds");
        store
            .record_usage(&ContextLedgerUsage {
                ledger_id: "stale".to_string(),
                actual_prompt_tokens: 1,
                actual_completion_tokens: 1,
                estimator_drift: 0.0,
                recorded_at: old,
            })
            .expect("usage recorded");

        let removed = store.prune(now).expect("prune runs");
        assert_eq!(removed, 1);
        assert!(store.get("stale").expect("read").is_none());
        let usage_rows: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM context_ledger_usage WHERE ledger_id = 'stale'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(usage_rows, 0);
    }

    #[test]
    fn rotation_keeps_entries_referenced_by_an_unexported_receipt() {
        let database = database("receipt");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        let now = 1_800_000_000_000_i64;
        let old = now - 40 * 24 * 60 * 60 * 1000;
        for index in 0..(LEDGER_RETAINED_SESSIONS + 1) {
            store
                .append(&entry(
                    &format!("fresh-{index}"),
                    &format!("session-{index}"),
                    now,
                ))
                .expect("append succeeds");
        }
        store
            .append(&entry("pinned", "session-pinned", old))
            .expect("append succeeds");
        store
            .register_receipt("pinned", "receipt-1", false)
            .expect("receipt registered");

        assert_eq!(store.prune(now).expect("prune runs"), 0);
        assert!(store.get("pinned").expect("read").is_some());

        store
            .register_receipt("pinned", "receipt-1", true)
            .expect("receipt exported");
        assert_eq!(store.prune(now).expect("prune runs"), 1);
    }

    #[test]
    fn recent_sessions_survive_the_age_cutoff() {
        let database = database("recent-session");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        let now = 1_800_000_000_000_i64;
        let old = now - 40 * 24 * 60 * 60 * 1000;
        store
            .append(&entry("old-but-recent-session", "session-1", old))
            .expect("append succeeds");
        assert_eq!(store.prune(now).expect("prune runs"), 0);
    }

    #[test]
    fn a_golden_entry_of_the_previous_schema_version_reads_without_rewrite() {
        let database = database("golden");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        // «Золотая» запись предыдущей версии: неизвестный `drop_reason` и
        // отсутствующее необязательное поле.
        database
            .connection()
            .execute(
                "INSERT INTO context_ledger (
                    id, schema_version, task_id, session_id, model_call_id, created_at,
                    provider, model, profile_version, profile_snapshot, tokenizer_version,
                    normalizer_version, strategy_version, mandatory_tokens,
                    selected_optional_tokens, reserves_tokens, estimated_prompt_tokens,
                    selected_items, dropped_items, mandatory_parts, ladder_levels_applied,
                    compression, loadout, fallback_estimator, replan_of, outcome,
                    budget_unavailable, context_ledger_hash
                 ) VALUES (
                    'golden', 0, 'task', 'session-1', 'call', 1000,
                    'literouter', 'model', 'profile-0', '{}', 'tok-0',
                    'norm-0', 'strategy-0', 10, 20, 30, 30,
                    '[{\"id\":\"a\",\"estimated_tokens\":10}]',
                    '[{\"id\":\"b\",\"drop_reason\":\"future_reason\"}]',
                    '[]', '[]', '[]', NULL, 0, NULL, 'sent', NULL, 'golden-hash'
                 )",
                [],
            )
            .expect("golden row inserted");

        let read = store.get("golden").expect("read").expect("entry exists");
        assert_eq!(read.schema_version, 0);
        assert_eq!(read.context_ledger_hash, "golden-hash");
        // Неизвестный `drop_reason` не роняет чтение и не переписывает запись.
        let stored_hash: String = database
            .connection()
            .query_row(
                "SELECT context_ledger_hash FROM context_ledger WHERE id = 'golden'",
                [],
                |row| row.get(0),
            )
            .expect("hash still readable");
        assert_eq!(stored_hash, "golden-hash");
    }

    #[test]
    fn concurrent_appends_stay_atomic_and_hash_stable() {
        let database = database("concurrent");
        let path = database.path().to_path_buf();
        drop(database);
        let entries: Vec<ContextLedgerEntry> = (0..8)
            .map(|index| {
                entry(
                    &format!("ledger-{index}"),
                    &format!("session-{index}"),
                    1_000,
                )
            })
            .collect();
        let expected: Vec<String> = entries
            .iter()
            .map(|entry| entry.context_ledger_hash.clone())
            .collect();

        let handles: Vec<_> = entries
            .into_iter()
            .map(|entry| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let database = LocalDatabase::open(&path).expect("database opens");
                    let store =
                        ContextLedgerStore::new(database.connection()).expect("store opens");
                    store.append(&entry).expect("append succeeds");
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread completes");
        }

        let database = LocalDatabase::open(&path).expect("database opens");
        let store = ContextLedgerStore::new(database.connection()).expect("store opens");
        assert_eq!(store.count().expect("count"), 8);
        for (index, hash) in expected.iter().enumerate() {
            let read = store
                .get(&format!("ledger-{index}"))
                .expect("read")
                .expect("entry");
            // Hash не зависит от порядка коммитов соседних задач.
            assert_eq!(&read.context_ledger_hash, hash);
        }
    }
}
