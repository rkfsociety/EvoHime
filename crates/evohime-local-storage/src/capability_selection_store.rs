//! Migration-neutral persistence contract for the capability-registry
//! selection the user pins or replaces for a task, so the choice survives
//! reconnect (matching the `research_store.rs` / `capability_store.rs`
//! pattern: this module owns SQL + record shape, not schema lifecycle).
//!
//! One row per `task_id`. `state_json` is the canonical JSON encoding of an
//! `evohime_core::capability_selection::CapabilitySelectionState`; the
//! `origin` column is a denormalized copy kept for cheap filtering (e.g.
//! "list all pinned selections") without deserializing every row.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAX_TASK_ID_BYTES: usize = 256;
const MAX_STATE_JSON_BYTES: usize = 64 * 1024;

/// Source of a task's active capability selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionOrigin {
    /// Automatically selected from the available capabilities.
    Auto,
    /// Explicitly pinned by the user for the task.
    Pinned,
    /// Explicitly replaced by the user for the task.
    Replaced,
}

impl SelectionOrigin {
    /// Returns the stable snake-case value stored in SQLite.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Pinned => "pinned",
            Self::Replaced => "replaced",
        }
    }

    fn parse(value: &str) -> Result<Self, CapabilitySelectionStoreError> {
        match value {
            "auto" => Ok(Self::Auto),
            "pinned" => Ok(Self::Pinned),
            "replaced" => Ok(Self::Replaced),
            _ => Err(CapabilitySelectionStoreError::InvalidOrigin),
        }
    }
}

/// Persisted selection state associated with one task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySelectionRecord {
    /// Task identity used as the row's unique key.
    pub task_id: String,
    /// Whether the selection was automatic, pinned, or replaced.
    pub origin: SelectionOrigin,
    /// Name of the selected capability manifest.
    pub manifest_name: String,
    /// Canonical JSON representation of the capability selection state.
    pub state_json: String,
}

impl CapabilitySelectionRecord {
    /// Checks that identifiers and state JSON are non-empty and within limits.
    ///
    /// ```
    /// use evohime_local_storage::capability_selection_store::{
    ///     CapabilitySelectionRecord, SelectionOrigin,
    /// };
    ///
    /// let record = CapabilitySelectionRecord {
    ///     task_id: "task-42".into(),
    ///     origin: SelectionOrigin::Pinned,
    ///     manifest_name: "rust-review".into(),
    ///     state_json: r#"{"selected":["rust-review"]}"#.into(),
    /// };
    /// assert!(record.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<(), CapabilitySelectionStoreError> {
        validate_text("task_id", &self.task_id, MAX_TASK_ID_BYTES)?;
        validate_text("manifest_name", &self.manifest_name, MAX_TASK_ID_BYTES)?;
        validate_text("state_json", &self.state_json, MAX_STATE_JSON_BYTES)?;
        Ok(())
    }
}

/// Validation and SQLite errors from capability-selection persistence.
#[derive(Debug, thiserror::Error)]
pub enum CapabilitySelectionStoreError {
    /// A required field contains only whitespace or is empty.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the required field that was empty.
        field: &'static str,
    },
    /// A field exceeds its byte-length limit.
    #[error("{field} exceeds {max} bytes")]
    Limit {
        /// Name of the field that exceeded its bound.
        field: &'static str,
        /// Maximum permitted field size in bytes.
        max: usize,
    },
    /// The stored origin value is not recognized.
    #[error("invalid selection origin")]
    InvalidOrigin,
    /// SQLite rejected the query or persistence operation.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl PartialEq for CapabilitySelectionStoreError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty { field: a }, Self::Empty { field: b }) => a == b,
            (
                Self::Limit {
                    field: a,
                    max: max_a,
                },
                Self::Limit {
                    field: b,
                    max: max_b,
                },
            ) => a == b && max_a == max_b,
            (Self::InvalidOrigin, Self::InvalidOrigin) => true,
            _ => false,
        }
    }
}

impl Eq for CapabilitySelectionStoreError {}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), CapabilitySelectionStoreError> {
    if value.trim().is_empty() {
        return Err(CapabilitySelectionStoreError::Empty { field });
    }
    if value.len() > max_bytes {
        return Err(CapabilitySelectionStoreError::Limit {
            field,
            max: max_bytes,
        });
    }
    Ok(())
}

/// SQL contract only; schema creation and migrations remain outside this API.
/// Constants and operations in this type expect `capability_selections` to exist.
pub struct CapabilitySelectionStoreSql;

impl CapabilitySelectionStoreSql {
    /// Upsert statement keyed by task identity.
    pub const INSERT_OR_REPLACE: &'static str = r#"
        INSERT INTO capability_selections
            (task_id, origin, manifest_name, state_json)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(task_id) DO UPDATE SET
            origin = excluded.origin,
            manifest_name = excluded.manifest_name,
            state_json = excluded.state_json
    "#;

    /// Select statement for one task's persisted selection.
    pub const SELECT_BY_TASK_ID: &'static str = r#"
        SELECT task_id, origin, manifest_name, state_json
        FROM capability_selections
        WHERE task_id = ?1
    "#;

    /// Delete statement for one task's persisted selection.
    pub const DELETE_BY_TASK_ID: &'static str =
        "DELETE FROM capability_selections WHERE task_id = ?1";

    /// Persists the user's pin/replace choice (or the latest auto match) so
    /// it survives reconnect. Upserts by `task_id`: a later pin/replace for
    /// the same task overwrites the prior stored choice, matching
    /// `capability_store::CapabilityStoreSql::insert`'s upsert-by-id shape.
    pub fn upsert(
        connection: &Connection,
        record: &CapabilitySelectionRecord,
    ) -> Result<(), CapabilitySelectionStoreError> {
        record.validate()?;
        connection.execute(
            Self::INSERT_OR_REPLACE,
            params![
                record.task_id,
                record.origin.as_str(),
                record.manifest_name,
                record.state_json,
            ],
        )?;
        Ok(())
    }

    /// Loads the saved selection for a task, if one exists.
    pub fn get_by_task_id(
        connection: &Connection,
        task_id: &str,
    ) -> Result<Option<CapabilitySelectionRecord>, CapabilitySelectionStoreError> {
        let record = connection
            .query_row(Self::SELECT_BY_TASK_ID, params![task_id], map_record)
            .optional()?;
        Ok(record)
    }

    /// Deletes a task selection and reports whether a row was removed.
    pub fn delete_by_task_id(
        connection: &Connection,
        task_id: &str,
    ) -> Result<bool, CapabilitySelectionStoreError> {
        Ok(connection.execute(Self::DELETE_BY_TASK_ID, params![task_id])? == 1)
    }
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CapabilitySelectionRecord> {
    Ok(CapabilitySelectionRecord {
        task_id: row.get(0)?,
        origin: SelectionOrigin::parse(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        manifest_name: row.get(2)?,
        state_json: row.get(3)?,
    })
}

fn to_sql_error(error: CapabilitySelectionStoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
}

#[cfg(test)]
#[path = "capability_selection_store_tests.rs"]
mod tests;
