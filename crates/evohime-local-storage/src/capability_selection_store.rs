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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionOrigin {
    Auto,
    Pinned,
    Replaced,
}

impl SelectionOrigin {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySelectionRecord {
    pub task_id: String,
    pub origin: SelectionOrigin,
    pub manifest_name: String,
    pub state_json: String,
}

impl CapabilitySelectionRecord {
    pub fn validate(&self) -> Result<(), CapabilitySelectionStoreError> {
        validate_text("task_id", &self.task_id, MAX_TASK_ID_BYTES)?;
        validate_text("manifest_name", &self.manifest_name, MAX_TASK_ID_BYTES)?;
        validate_text("state_json", &self.state_json, MAX_STATE_JSON_BYTES)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CapabilitySelectionStoreError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("invalid selection origin")]
    InvalidOrigin,
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
pub struct CapabilitySelectionStoreSql;

impl CapabilitySelectionStoreSql {
    pub const INSERT_OR_REPLACE: &'static str = r#"
        INSERT INTO capability_selections
            (task_id, origin, manifest_name, state_json)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(task_id) DO UPDATE SET
            origin = excluded.origin,
            manifest_name = excluded.manifest_name,
            state_json = excluded.state_json
    "#;

    pub const SELECT_BY_TASK_ID: &'static str = r#"
        SELECT task_id, origin, manifest_name, state_json
        FROM capability_selections
        WHERE task_id = ?1
    "#;

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

    pub fn get_by_task_id(
        connection: &Connection,
        task_id: &str,
    ) -> Result<Option<CapabilitySelectionRecord>, CapabilitySelectionStoreError> {
        let record = connection
            .query_row(Self::SELECT_BY_TASK_ID, params![task_id], map_record)
            .optional()?;
        Ok(record)
    }

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
mod tests {
    use super::*;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE capability_selections (
                    task_id TEXT PRIMARY KEY NOT NULL,
                    origin TEXT NOT NULL,
                    manifest_name TEXT NOT NULL,
                    state_json BLOB NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );",
            )
            .expect("contract fixture creates");
    }

    fn record(
        task_id: &str,
        origin: SelectionOrigin,
        manifest_name: &str,
    ) -> CapabilitySelectionRecord {
        CapabilitySelectionRecord {
            task_id: task_id.into(),
            origin,
            manifest_name: manifest_name.into(),
            state_json: r#"{"selection":{"manifest_name":"reviewer"}}"#.into(),
        }
    }

    #[test]
    fn round_trips_pinned_selection_and_survives_reconnect_simulation() {
        let path = std::env::temp_dir().join(format!(
            "evohime-capability-selection-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        {
            let connection = Connection::open(&path).expect("sqlite opens");
            schema(&connection);
            CapabilitySelectionStoreSql::upsert(
                &connection,
                &record("task-1", SelectionOrigin::Pinned, "reviewer"),
            )
            .expect("pin persists");
        }

        // Simulate reconnect: reopen the same file with a fresh connection.
        let connection = Connection::open(&path).expect("sqlite reopens");
        let loaded = CapabilitySelectionStoreSql::get_by_task_id(&connection, "task-1")
            .expect("selection reads")
            .expect("selection present after reconnect");
        assert_eq!(loaded.origin, SelectionOrigin::Pinned);
        assert_eq!(loaded.manifest_name, "reviewer");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn upsert_overwrites_prior_choice_for_same_task() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        CapabilitySelectionStoreSql::upsert(
            &connection,
            &record("task-1", SelectionOrigin::Auto, "reviewer"),
        )
        .unwrap();
        CapabilitySelectionStoreSql::upsert(
            &connection,
            &record("task-1", SelectionOrigin::Replaced, "planner"),
        )
        .unwrap();

        let loaded = CapabilitySelectionStoreSql::get_by_task_id(&connection, "task-1")
            .unwrap()
            .unwrap();
        assert_eq!(loaded.origin, SelectionOrigin::Replaced);
        assert_eq!(loaded.manifest_name, "planner");
    }

    #[test]
    fn deletes_and_rejects_unbounded_or_empty_fields() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        CapabilitySelectionStoreSql::upsert(
            &connection,
            &record("task-1", SelectionOrigin::Pinned, "reviewer"),
        )
        .unwrap();
        assert!(CapabilitySelectionStoreSql::delete_by_task_id(&connection, "task-1").unwrap());
        assert!(
            CapabilitySelectionStoreSql::get_by_task_id(&connection, "task-1")
                .unwrap()
                .is_none()
        );

        let mut invalid = record("task-1", SelectionOrigin::Auto, "reviewer");
        invalid.task_id.clear();
        assert_eq!(
            invalid.validate(),
            Err(CapabilitySelectionStoreError::Empty { field: "task_id" })
        );

        invalid = record("task-1", SelectionOrigin::Auto, "reviewer");
        invalid.state_json = "x".repeat(MAX_STATE_JSON_BYTES + 1);
        assert!(matches!(
            invalid.validate(),
            Err(CapabilitySelectionStoreError::Limit {
                field: "state_json",
                ..
            })
        ));
    }
}
