//! Idempotent typed-memory schema extension.

use rusqlite::Connection;

use crate::memory_store::MemoryStoreError;

/// Installs the v31 typed-memory columns on every database open. It is
/// intentionally idempotent and independent of the legacy migration ladder.
pub fn install_schema(connection: &Connection) -> Result<(), MemoryStoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'memory_entries')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let mut statement = connection.prepare("PRAGMA table_info(memory_entries)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|name| name == "record_version") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN record_version INTEGER NOT NULL DEFAULT 1",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "evidence_refs") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN evidence_refs TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "execution_event_refs") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN execution_event_refs TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "authority") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN authority TEXT NOT NULL DEFAULT 'user_asserted'",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "durability") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN durability TEXT NOT NULL DEFAULT 'durable'",
            [],
        )?;
    }
    if !columns.iter().any(|name| name == "confidence") {
        connection.execute(
            "ALTER TABLE memory_entries ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0",
            [],
        )?;
    }
    Ok(())
}
