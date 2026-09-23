//! Durable Workspace Set definitions and version-fenced updates.
//!
//! Create a set and advance it with an optimistic version check:
//!
//! ```
//! use evohime_local_storage::workspace_sets_store;
//! use rusqlite::Connection;
//!
//! let connection = Connection::open_in_memory()?;
//! workspace_sets_store::install_schema(&connection)?;
//! assert!(workspace_sets_store::create(&connection, "default", b"v1", "hash-v1", 1)?);
//! assert!(workspace_sets_store::update(
//!     &connection, "default", 1, 2, b"v2", "hash-v2", 2,
//! )?);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use rusqlite::{params, Connection, OptionalExtension};

/// Creates the Workspace Set, idempotency, and run-binding tables when absent.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_sets (
           set_id TEXT PRIMARY KEY NOT NULL,
           version INTEGER NOT NULL,
           content_hash TEXT NOT NULL,
           set_json BLOB NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS workspace_sets_idempotency (
           idempotency_key TEXT PRIMARY KEY NOT NULL,
           result_json BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS workspace_set_run_bindings (
           task_id TEXT PRIMARY KEY NOT NULL,
           set_id TEXT NOT NULL,
           set_version INTEGER NOT NULL,
           binding_json BLOB NOT NULL,
           status TEXT NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );",
    )
}

/// Inserts a Workspace Set at version 1; duplicate IDs return `false`.
pub fn create(
    connection: &Connection,
    set_id: &str,
    set_json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(connection.execute(
        "INSERT OR IGNORE INTO workspace_sets(set_id,version,content_hash,set_json,updated_at_ms) VALUES (?1,1,?2,?3,?4)",
        params![set_id, hash, set_json, now],
    )? == 1)
}

/// Returns the serialized Workspace Set definition, if it exists.
pub fn get(connection: &Connection, set_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT set_json FROM workspace_sets WHERE set_id=?1",
            [set_id],
            |row| row.get(0),
        )
        .optional()
}

/// Replaces a set only when its stored version equals `expected_version`.
/// The caller supplies `next_version`; a stale version returns `false`.
pub fn update(
    connection: &Connection,
    set_id: &str,
    expected_version: u64,
    next_version: u64,
    set_json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(connection.execute(
        "UPDATE workspace_sets SET version=?1,content_hash=?2,set_json=?3,updated_at_ms=?4 WHERE set_id=?5 AND version=?6",
        params![next_version as i64, hash, set_json, now, set_id, expected_version as i64],
    )? == 1)
}

/// Returns the previously stored result for an idempotency key, if present.
pub fn get_idempotency(connection: &Connection, key: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT result_json FROM workspace_sets_idempotency WHERE idempotency_key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()
}

/// Stores the first result associated with `key`; later writes leave it unchanged.
pub fn put_idempotency(
    connection: &Connection,
    key: &str,
    result_json: &[u8],
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO workspace_sets_idempotency(idempotency_key,result_json) VALUES (?1,?2)",
        params![key, result_json],
    )?;
    Ok(())
}

/// Pins a task to a Workspace Set version and stores its serialized binding.
pub fn bind_run(
    connection: &Connection,
    task_id: &str,
    set_id: &str,
    version: u64,
    binding_json: &[u8],
    now: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO workspace_set_run_bindings(task_id,set_id,set_version,binding_json,status,updated_at_ms) VALUES (?1,?2,?3,?4,'pinned',?5) ON CONFLICT(task_id) DO UPDATE SET set_id=excluded.set_id,set_version=excluded.set_version,binding_json=excluded.binding_json,status='pinned',updated_at_ms=excluded.updated_at_ms",
        params![task_id, set_id, version as i64, binding_json, now],
    )?;
    Ok(())
}

/// Returns the pinned serialized binding for a task, if one exists.
pub fn get_run_binding(
    connection: &Connection,
    task_id: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection.query_row(
        "SELECT binding_json FROM workspace_set_run_bindings WHERE task_id=?1 AND status='pinned'",
        [task_id],
        |row| row.get(0),
    ).optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn create_is_duplicate_safe_and_update_is_version_fenced() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        assert!(create(&connection, "s", b"one", "h1", 1).unwrap());
        assert!(!create(&connection, "s", b"two", "h2", 2).unwrap());
        assert!(!update(&connection, "s", 0, 2, b"bad", "h2", 2).unwrap());
        assert!(update(&connection, "s", 1, 2, b"two", "h2", 2).unwrap());
        assert_eq!(get(&connection, "s").unwrap(), Some(b"two".to_vec()));
    }
}
