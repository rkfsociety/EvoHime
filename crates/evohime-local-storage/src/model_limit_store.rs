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
const MAX_MODELS: i64 = 256;

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
        LIMIT ?1
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
        let rows = statement.query_map([MAX_MODELS], map_record)?;
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
#[path = "model_limit_store_tests.rs"]
mod tests;
