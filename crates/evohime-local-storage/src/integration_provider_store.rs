//! Durable metadata for Integration Provider SDK. Secret bytes never enter this store.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

pub const STORE_SCHEMA_VERSION: u32 = 1;

pub fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS integration_provider_manifests (
           provider_id TEXT NOT NULL, version INTEGER NOT NULL, manifest_json TEXT NOT NULL,
           content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL,
           PRIMARY KEY(provider_id, version));
         CREATE TABLE IF NOT EXISTS integration_provider_credentials (
           credential_id TEXT PRIMARY KEY, provider_id TEXT NOT NULL, metadata_json TEXT NOT NULL,
           status TEXT NOT NULL, version INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS integration_provider_bindings (
           binding_id TEXT PRIMARY KEY, owner_kind TEXT NOT NULL, owner_id TEXT NOT NULL,
           binding_json TEXT NOT NULL, status TEXT NOT NULL, version INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL);
         CREATE INDEX IF NOT EXISTS idx_integration_provider_bindings_owner
           ON integration_provider_bindings(owner_kind, owner_id);
         CREATE TABLE IF NOT EXISTS integration_provider_events (
           event_id TEXT PRIMARY KEY, entity_id TEXT NOT NULL, event_type TEXT NOT NULL,
           payload_json TEXT NOT NULL, created_at_ms INTEGER NOT NULL);",
    )
}

pub fn put_manifest<T: Serialize>(
    connection: &Connection,
    provider_id: &str,
    version: u32,
    manifest: &T,
    hash: &str,
    now_ms: i64,
) -> Result<(), rusqlite::Error> {
    let json = serde_json::to_string(manifest)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    connection.execute("INSERT INTO integration_provider_manifests(provider_id,version,manifest_json,content_hash,updated_at_ms) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(provider_id,version) DO UPDATE SET manifest_json=excluded.manifest_json,content_hash=excluded.content_hash,updated_at_ms=excluded.updated_at_ms", params![provider_id, version, json, hash, now_ms])?;
    Ok(())
}

pub fn get_manifest<T: DeserializeOwned>(
    connection: &Connection,
    provider_id: &str,
    version: u32,
) -> Result<Option<T>, rusqlite::Error> {
    connection.query_row("SELECT manifest_json FROM integration_provider_manifests WHERE provider_id=?1 AND version=?2", params![provider_id, version], |row| row.get::<_, String>(0)).optional()?.map(|json| serde_json::from_str(&json).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))).transpose()
}

pub fn dependency_report(
    connection: &Connection,
    credential_id: &str,
) -> Result<Vec<(String, String)>, rusqlite::Error> {
    let mut statement = connection.prepare("SELECT owner_kind, owner_id FROM integration_provider_bindings WHERE binding_json LIKE '%' || ?1 || '%' ORDER BY owner_kind, owner_id")?;
    let rows = statement.query_map(params![credential_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_round_trips_without_secret_column() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        put_manifest(
            &connection,
            "fixture.echo",
            1,
            &serde_json::json!({"id":"fixture.echo"}),
            "hash",
            1,
        )
        .unwrap();
        let value: Option<serde_json::Value> =
            get_manifest(&connection, "fixture.echo", 1).unwrap();
        assert_eq!(value.unwrap()["id"], "fixture.echo");
        let columns: Vec<String> = connection
            .prepare("PRAGMA table_info(integration_provider_credentials)")
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!columns.iter().any(|column| column.contains("secret")));
    }
}
