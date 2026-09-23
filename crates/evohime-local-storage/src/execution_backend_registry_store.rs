use crate::StorageError;
use rusqlite::{params, Connection, OptionalExtension};
const MAX_BACKENDS: i64 = 256;
const MAX_CAPABILITIES_BYTES: usize = 64 * 1024;

/// Creates backend registry, event, and metadata tables.
pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS execution_backends (id TEXT PRIMARY KEY, kind TEXT NOT NULL, endpoint TEXT, auth_ref TEXT, enabled INTEGER NOT NULL, capabilities_json TEXT NOT NULL, version INTEGER NOT NULL, health TEXT NOT NULL, health_failure TEXT, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS execution_backend_events (id INTEGER PRIMARY KEY AUTOINCREMENT, backend_id TEXT NOT NULL, operation TEXT NOT NULL, version INTEGER NOT NULL, outcome TEXT NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, UNIQUE(backend_id, operation, idempotency_key)); CREATE TABLE IF NOT EXISTS execution_backend_registry_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
    Ok(())
}

/// Values used to insert or revision-update one execution backend.
#[derive(Clone, Copy)]
pub struct UpsertInput<'a> {
    /// Stable backend identifier.
    pub id: &'a str,
    /// Backend implementation kind.
    pub kind: &'a str,
    /// Optional endpoint for remote backend kinds.
    pub endpoint: Option<&'a str>,
    /// Optional reference to locally managed authentication material.
    pub auth_ref: Option<&'a str>,
    /// Serialized capability declarations, limited to 64 KiB.
    pub capabilities_json: &'a str,
    /// Backend configuration revision.
    pub version: u64,
    /// Current health state.
    pub health: &'a str,
    /// Last update time in Unix milliseconds.
    pub now_ms: i64,
}

/// Inserts or updates a backend only when its supplied version is newer.
///
/// Returns `false` when the capabilities JSON exceeds the configured size bound.
pub fn upsert(connection: &Connection, input: UpsertInput<'_>) -> Result<bool, StorageError> {
    if input.capabilities_json.len() > MAX_CAPABILITIES_BYTES {
        return Ok(false);
    }
    Ok(connection.execute("INSERT INTO execution_backends(id,kind,endpoint,auth_ref,enabled,capabilities_json,version,health,updated_at_ms) VALUES (?1,?2,?3,?4,1,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,endpoint=excluded.endpoint,auth_ref=excluded.auth_ref,capabilities_json=excluded.capabilities_json,version=excluded.version,health=excluded.health,updated_at_ms=excluded.updated_at_ms WHERE excluded.version > execution_backends.version", params![input.id,input.kind,input.endpoint,input.auth_ref,input.capabilities_json,input.version as i64,input.health,input.now_ms])? == 1)
}

/// Persisted backend fields returned by registry listing.
pub struct BackendRow {
    /// Stable backend identifier.
    pub id: String,
    /// Backend implementation kind.
    pub kind: String,
    /// Optional backend endpoint.
    pub endpoint: Option<String>,
    /// Optional local credential reference.
    pub auth_ref: Option<String>,
    /// Serialized backend capability declarations.
    pub capabilities_json: String,
    /// Stored configuration version.
    pub version: i64,
    /// Current health state.
    pub health: String,
}

/// Lists at most 256 backends ordered by identifier.
pub fn list(connection: &Connection) -> Result<Vec<BackendRow>, StorageError> {
    let mut stmt=connection.prepare("SELECT id,kind,endpoint,auth_ref,capabilities_json,version,health FROM execution_backends ORDER BY id LIMIT ?1")?;
    let rows = stmt
        .query_map([MAX_BACKENDS], |row| {
            Ok(BackendRow {
                id: row.get(0)?,
                kind: row.get(1)?,
                endpoint: row.get(2)?,
                auth_ref: row.get(3)?,
                capabilities_json: row.get(4)?,
                version: row.get(5)?,
                health: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Removes a backend by identifier and reports whether a row was deleted.
pub fn remove(connection: &Connection, id: &str) -> Result<bool, StorageError> {
    Ok(connection.execute("DELETE FROM execution_backends WHERE id=?1", [id])? == 1)
}

/// Enables or disables a backend and updates its health label.
pub fn set_enabled(connection: &Connection, id: &str, enabled: bool) -> Result<bool, StorageError> {
    Ok(connection.execute(
        "UPDATE execution_backends SET enabled=?2,health=?3 WHERE id=?1",
        params![
            id,
            enabled as i64,
            if enabled { "registered" } else { "disabled" }
        ],
    )? == 1)
}

/// Stores the registry's default backend identifier.
pub fn set_default(connection: &Connection, id: &str) -> Result<(), StorageError> {
    connection.execute("INSERT INTO execution_backend_registry_meta(key,value) VALUES ('default_backend_id',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [id])?;
    Ok(())
}

/// Loads the default backend identifier, if one has been configured.
pub fn default_id(connection: &Connection) -> Result<Option<String>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT value FROM execution_backend_registry_meta WHERE key='default_backend_id'",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

#[cfg(test)]
#[path = "execution_backend_registry_store_tests.rs"]
mod tests;
