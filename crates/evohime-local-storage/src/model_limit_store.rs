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
//!
//! ```
//! use evohime_local_storage::model_limit_store::{ModelLimitRecord, ModelLimitStoreSql};
//! let connection = rusqlite::Connection::open_in_memory()?;
//! connection.execute_batch("CREATE TABLE model_context_limits (model TEXT PRIMARY KEY, provider TEXT NOT NULL, context_tokens INTEGER, max_output_tokens INTEGER, fetched_at TEXT NOT NULL);")?;
//! ModelLimitStoreSql::upsert_all(&connection, &[ModelLimitRecord {
//!     model: "example-model".into(),
//!     provider: "example-provider".into(),
//!     context_tokens: Some(32_000),
//!     max_output_tokens: None,
//! }])?;
//! assert_eq!(ModelLimitStoreSql::get(&connection, "example-model")?.unwrap().context_tokens, Some(32_000));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAX_MODEL_BYTES: usize = 256;
const MAX_PROVIDER_BYTES: usize = 64;
const MAX_MODELS: i64 = 256;

/// Context and output limits reported for one model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelLimitRecord {
    /// Model identifier used as the storage key.
    pub model: String,
    /// Provider that reported the limits.
    pub provider: String,
    /// Context window size; `None` means unknown, not unlimited.
    pub context_tokens: Option<u32>,
    /// Maximum output tokens; `None` means unknown.
    pub max_output_tokens: Option<u32>,
}

impl ModelLimitRecord {
    /// Validates model and provider identifiers against their byte bounds.
    pub fn validate(&self) -> Result<(), ModelLimitStoreError> {
        validate_text("model", &self.model, MAX_MODEL_BYTES)?;
        validate_text("provider", &self.provider, MAX_PROVIDER_BYTES)?;
        Ok(())
    }
}

/// Validation or SQLite failure while storing model limits.
#[derive(Debug, thiserror::Error)]
pub enum ModelLimitStoreError {
    /// A required identifier is empty or whitespace-only.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the required field that was empty.
        field: &'static str,
    },
    /// A text value exceeds the allowed byte length.
    #[error("{field} exceeds {max} bytes")]
    Limit {
        /// Name of the field that exceeded its limit.
        field: &'static str,
        /// Maximum allowed length in bytes.
        max: usize,
    },
    /// The underlying SQLite operation failed.
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
///
/// Callers must create a compatible `model_context_limits` table.
pub struct ModelLimitStoreSql;

impl ModelLimitStoreSql {
    /// Upsert statement used by [`Self::upsert_all`].
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

    /// Select statement used by [`Self::get`].
    pub const SELECT_BY_MODEL: &'static str = r#"
        SELECT model, provider, context_tokens, max_output_tokens
        FROM model_context_limits
        WHERE model = ?1
    "#;

    /// Ordered, bounded select statement used by [`Self::list`].
    pub const SELECT_ALL: &'static str = r#"
        SELECT model, provider, context_tokens, max_output_tokens
        FROM model_context_limits
        ORDER BY model
        LIMIT ?1
    "#;

    /// Записывает лимиты одной пачкой: каталог приходит целиком, и половина
    /// обновлённых строк хуже, чем ни одной.
    ///
    /// All records are validated before the transaction writes any row.
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

    /// Returns the limits for a model, or `None` when no record is stored.
    pub fn get(
        connection: &Connection,
        model: &str,
    ) -> Result<Option<ModelLimitRecord>, ModelLimitStoreError> {
        Ok(connection
            .query_row(Self::SELECT_BY_MODEL, params![model], map_record)
            .optional()?)
    }

    /// Lists at most 256 model records ordered by model identifier.
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
