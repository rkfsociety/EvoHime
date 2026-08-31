use crate::StorageError;
use rusqlite::{params, Connection, OptionalExtension};

pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS execution_backends (id TEXT PRIMARY KEY, kind TEXT NOT NULL, endpoint TEXT, auth_ref TEXT, enabled INTEGER NOT NULL, capabilities_json TEXT NOT NULL, version INTEGER NOT NULL, health TEXT NOT NULL, health_failure TEXT, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS execution_backend_events (id INTEGER PRIMARY KEY AUTOINCREMENT, backend_id TEXT NOT NULL, operation TEXT NOT NULL, version INTEGER NOT NULL, outcome TEXT NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, UNIQUE(backend_id, operation, idempotency_key)); CREATE TABLE IF NOT EXISTS execution_backend_registry_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn upsert(
    connection: &Connection,
    id: &str,
    kind: &str,
    endpoint: Option<&str>,
    auth_ref: Option<&str>,
    capabilities_json: &str,
    version: u64,
    health: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    Ok(connection.execute("INSERT INTO execution_backends(id,kind,endpoint,auth_ref,enabled,capabilities_json,version,health,updated_at_ms) VALUES (?1,?2,?3,?4,1,?5,?6,?7,?8) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,endpoint=excluded.endpoint,auth_ref=excluded.auth_ref,capabilities_json=excluded.capabilities_json,version=excluded.version,health=excluded.health,updated_at_ms=excluded.updated_at_ms", params![id,kind,endpoint,auth_ref,capabilities_json,version as i64,health,now_ms])? == 1)
}

#[allow(clippy::type_complexity)]
pub fn list(
    connection: &Connection,
) -> Result<
    Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        String,
    )>,
    StorageError,
> {
    let mut stmt=connection.prepare("SELECT id,kind,endpoint,auth_ref,capabilities_json,version,health FROM execution_backends ORDER BY id")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn remove(connection: &Connection, id: &str) -> Result<bool, StorageError> {
    Ok(connection.execute("DELETE FROM execution_backends WHERE id=?1", [id])? == 1)
}

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

pub fn set_default(connection: &Connection, id: &str) -> Result<(), StorageError> {
    connection.execute("INSERT INTO execution_backend_registry_meta(key,value) VALUES ('default_backend_id',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [id])?;
    Ok(())
}

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
mod tests {
    use super::*;
    #[test]
    fn metadata_store_is_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(upsert(&c, "local.core", "local", None, None, "[]", 1, "healthy", 1).unwrap());
        assert!(upsert(&c, "local.core", "local", None, None, "[]", 1, "healthy", 2).unwrap());
        assert_eq!(list(&c).unwrap().len(), 1);
    }
}
