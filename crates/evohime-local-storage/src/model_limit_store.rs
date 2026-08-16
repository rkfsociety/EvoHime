//! Migration-neutral persistence contract for the per-model context limits the
//! provider reports (matching the `capability_selection_store.rs` pattern: this
//! module owns SQL + record shape, not schema lifecycle).
//!
//! One row per model identifier. The limits outlive a single catalogue refresh
//! on purpose: the context planner and the plan review both need a window
//! before any catalogue request has happened in the current session, and a
//! provider that is rate-limited must not silently downgrade every model to
//! "window unknown".
//!
//! A missing limit is stored as `NULL` and read back as `None`. Callers must
//! treat that as "unknown", never as "unlimited" — plain OpenAI does not report
//! a window at all, and guessing one would be worse than admitting ignorance.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAX_MODEL_BYTES: usize = 256;
const MAX_PROVIDER_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelLimitRecord {
    pub model: String,
    pub provider: String,
    pub context_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

impl ModelLimitRecord {
    pub fn validate(&self) -> Result<(), ModelLimitStoreError> {
        validate_text("model", &self.model, MAX_MODEL_BYTES)?;
        validate_text("provider", &self.provider, MAX_PROVIDER_BYTES)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModelLimitStoreError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), ModelLimitStoreError> {
    if value.trim().is_empty() {
        return Err(ModelLimitStoreError::Empty { field });
    }
    if value.len() > max_bytes {
        return Err(ModelLimitStoreError::Limit {
            field,
            max: max_bytes,
        });
    }
    Ok(())
}

/// SQL contract only; schema creation and migrations remain outside this API.
pub struct ModelLimitStoreSql;

impl ModelLimitStoreSql {
    pub const INSERT_OR_REPLACE: &'static str = r#"
        INSERT INTO model_context_limits
            (model, provider, context_tokens, max_output_tokens, fetched_at)
        VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        ON CONFLICT(model) DO UPDATE SET
            provider = excluded.provider,
            context_tokens = excluded.context_tokens,
            max_output_tokens = excluded.max_output_tokens,
            fetched_at = excluded.fetched_at
    "#;

    pub const SELECT_BY_MODEL: &'static str = r#"
        SELECT model, provider, context_tokens, max_output_tokens
        FROM model_context_limits
        WHERE model = ?1
    "#;

    pub const SELECT_ALL: &'static str = r#"
        SELECT model, provider, context_tokens, max_output_tokens
        FROM model_context_limits
        ORDER BY model
    "#;

    /// Записывает лимиты одной пачкой: каталог приходит целиком, и половина
    /// обновлённых строк хуже, чем ни одной.
    pub fn upsert_all(
        connection: &Connection,
        records: &[ModelLimitRecord],
    ) -> Result<usize, ModelLimitStoreError> {
        for record in records {
            record.validate()?;
        }
        let transaction = connection.unchecked_transaction()?;
        {
            let mut statement = transaction.prepare(Self::INSERT_OR_REPLACE)?;
            for record in records {
                statement.execute(params![
                    record.model,
                    record.provider,
                    record.context_tokens,
                    record.max_output_tokens,
                ])?;
            }
        }
        transaction.commit()?;
        Ok(records.len())
    }

    pub fn get(
        connection: &Connection,
        model: &str,
    ) -> Result<Option<ModelLimitRecord>, ModelLimitStoreError> {
        Ok(connection
            .query_row(Self::SELECT_BY_MODEL, params![model], map_record)
            .optional()?)
    }

    pub fn list(connection: &Connection) -> Result<Vec<ModelLimitRecord>, ModelLimitStoreError> {
        let mut statement = connection.prepare(Self::SELECT_ALL)?;
        let rows = statement.query_map([], map_record)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelLimitRecord> {
    Ok(ModelLimitRecord {
        model: row.get(0)?,
        provider: row.get(1)?,
        context_tokens: row.get(2)?,
        max_output_tokens: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE model_context_limits (
                    model TEXT PRIMARY KEY,
                    provider TEXT NOT NULL,
                    context_tokens INTEGER,
                    max_output_tokens INTEGER,
                    fetched_at TEXT NOT NULL
                 );",
            )
            .expect("schema is created");
    }

    fn record(model: &str, context: Option<u32>) -> ModelLimitRecord {
        ModelLimitRecord {
            model: model.into(),
            provider: "literouter".into(),
            context_tokens: context,
            max_output_tokens: Some(32_768),
        }
    }

    #[test]
    fn stores_and_reads_back_a_window() {
        let connection = Connection::open_in_memory().expect("memory database opens");
        schema(&connection);

        ModelLimitStoreSql::upsert_all(&connection, &[record("a:free", Some(128_000))])
            .expect("limits are stored");

        let stored = ModelLimitStoreSql::get(&connection, "a:free").expect("read succeeds");
        assert_eq!(stored, Some(record("a:free", Some(128_000))));
    }

    /// Провайдер может не сообщить окно — тогда строка есть, а лимита нет, и
    /// это должно читаться как «неизвестно», а не как ноль.
    #[test]
    fn an_unknown_window_reads_back_as_none() {
        let connection = Connection::open_in_memory().expect("memory database opens");
        schema(&connection);

        ModelLimitStoreSql::upsert_all(&connection, &[record("a", None)]).expect("limits stored");

        let stored = ModelLimitStoreSql::get(&connection, "a").expect("read succeeds");
        assert_eq!(stored.and_then(|record| record.context_tokens), None);
    }

    #[test]
    fn a_later_catalogue_overwrites_the_previous_limits() {
        let connection = Connection::open_in_memory().expect("memory database opens");
        schema(&connection);

        ModelLimitStoreSql::upsert_all(&connection, &[record("a", Some(8_000))]).expect("stored");
        ModelLimitStoreSql::upsert_all(&connection, &[record("a", Some(256_000))]).expect("stored");

        let stored = ModelLimitStoreSql::list(&connection).expect("read succeeds");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].context_tokens, Some(256_000));
    }

    #[test]
    fn an_empty_model_identifier_is_refused() {
        let connection = Connection::open_in_memory().expect("memory database opens");
        schema(&connection);

        let outcome = ModelLimitStoreSql::upsert_all(&connection, &[record("  ", Some(1_000))]);
        assert!(matches!(
            outcome,
            Err(ModelLimitStoreError::Empty { field: "model" })
        ));
    }
}
