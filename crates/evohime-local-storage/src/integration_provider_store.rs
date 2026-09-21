//! Durable metadata for Integration Provider SDK. Secret bytes never enter this store.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

pub const STORE_SCHEMA_VERSION: u32 = 1;
const MAX_DEPENDENCY_REPORT_ROWS: i64 = 256;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

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
    if json.len() > MAX_MANIFEST_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            "integration provider manifest exceeds 64 KiB".into(),
        ));
    }
    connection.execute("INSERT INTO integration_provider_manifests(provider_id,version,manifest_json,content_hash,updated_at_ms) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(provider_id,version) DO NOTHING", params![provider_id, version, json, hash, now_ms])?;
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
    let escaped_id = credential_id
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let mut statement = connection.prepare("SELECT owner_kind, owner_id FROM integration_provider_bindings WHERE binding_json LIKE '%' || ?1 || '%' ESCAPE '\\' ORDER BY owner_kind, owner_id LIMIT ?2")?;
    let rows = statement.query_map(params![escaped_id, MAX_DEPENDENCY_REPORT_ROWS], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    rows.collect()
}

#[cfg(test)]
#[path = "integration_provider_store_tests.rs"]
mod tests;
