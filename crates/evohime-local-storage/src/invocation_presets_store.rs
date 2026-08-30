//! Durable, metadata-only storage for invocation presets.

use rusqlite::{params, Connection, OptionalExtension};

use crate::StorageError;

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

#[allow(clippy::too_many_arguments)]
pub fn save_revision(
    connection: &Connection,
    owner_scope: &str,
    id: &str,
    revision: u64,
    content_json: &str,
    content_hash: &str,
    state: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    let changed = connection.execute(
        "INSERT OR IGNORE INTO invocation_presets (id, owner_scope, revision, content_json, content_hash, state, created_at_ms, updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
        params![id, owner_scope, revision as i64, content_json, content_hash, state, now_ms],
    )?;
    Ok(changed == 1)
}

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
        assert!(save_revision(&connection, "o", "p", 1, "{}", "h", "ready", 1).unwrap());
        assert!(!save_revision(&connection, "o", "p", 1, "{bad}", "x", "ready", 2).unwrap());
        assert_eq!(
            read_revision(&connection, "o", "p", 1).unwrap().unwrap().1,
            "h"
        );
    }
}
