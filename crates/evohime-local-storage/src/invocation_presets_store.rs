//! Durable, metadata-only storage for invocation presets.

use rusqlite::{params, Connection, OptionalExtension};

use crate::StorageError;

/// Creates the immutable, owner-scoped invocation preset revision table.
pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS invocation_presets (
           id TEXT NOT NULL, owner_scope TEXT NOT NULL, revision INTEGER NOT NULL,
           content_json TEXT NOT NULL, content_hash TEXT NOT NULL,
           state TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
           PRIMARY KEY(id, revision), UNIQUE(owner_scope, id, revision)
         );
         CREATE INDEX IF NOT EXISTS idx_invocation_presets_owner ON invocation_presets(owner_scope, id, revision);",
    )?;
    Ok(())
}

/// Values required to insert one immutable invocation preset revision.
#[derive(Clone, Copy)]
pub struct SaveRevisionInput<'a> {
    /// Scope that owns the preset.
    pub owner_scope: &'a str,
    /// Stable preset identifier.
    pub id: &'a str,
    /// Immutable revision number.
    pub revision: u64,
    /// Serialized preset definition.
    pub content_json: &'a str,
    /// Hash of the serialized definition.
    pub content_hash: &'a str,
    /// Lifecycle state associated with this revision.
    pub state: &'a str,
    /// Creation/update timestamp in milliseconds.
    pub now_ms: i64,
}

/// Inserts a preset revision once; existing `(owner_scope, id, revision)` rows are unchanged.
pub fn save_revision(
    connection: &Connection,
    input: SaveRevisionInput<'_>,
) -> Result<bool, StorageError> {
    let SaveRevisionInput {
        owner_scope,
        id,
        revision,
        content_json,
        content_hash,
        state,
        now_ms,
    } = input;
    let changed = connection.execute(
        "INSERT OR IGNORE INTO invocation_presets (id, owner_scope, revision, content_json, content_hash, state, created_at_ms, updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
        params![id, owner_scope, revision as i64, content_json, content_hash, state, now_ms],
    )?;
    Ok(changed == 1)
}

/// Reads an immutable revision's JSON, content hash, and state for its owner.
pub fn read_revision(
    connection: &Connection,
    owner_scope: &str,
    id: &str,
    revision: u64,
) -> Result<Option<(String, String, String)>, StorageError> {
    Ok(connection.query_row(
        "SELECT content_json, content_hash, state FROM invocation_presets WHERE owner_scope=?1 AND id=?2 AND revision=?3",
        params![owner_scope, id, revision as i64],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_are_immutable_and_idempotent() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        assert!(save_revision(
            &connection,
            SaveRevisionInput {
                owner_scope: "o",
                id: "p",
                revision: 1,
                content_json: "{}",
                content_hash: "h",
                state: "ready",
                now_ms: 1,
            },
        )
        .unwrap());
        assert!(!save_revision(
            &connection,
            SaveRevisionInput {
                owner_scope: "o",
                id: "p",
                revision: 1,
                content_json: "{bad}",
                content_hash: "x",
                state: "ready",
                now_ms: 2,
            },
        )
        .unwrap());
        assert_eq!(
            read_revision(&connection, "o", "p", 1).unwrap().unwrap().1,
            "h"
        );
    }
}
