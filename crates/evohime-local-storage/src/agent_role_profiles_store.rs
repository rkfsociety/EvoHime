//! Durable metadata-only catalog for Agent Role Profiles (schema v47).

use rusqlite::{params, Connection, OptionalExtension};

pub fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_role_profiles (id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, profile_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS agent_role_profile_revisions (profile_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, profile_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(profile_id, revision));")
}

pub fn save_revision(
    connection: &Connection,
    id: &str,
    revision: u64,
    content_hash: &str,
    profile_json: &[u8],
    now_ms: i64,
) -> Result<bool, rusqlite::Error> {
    let tx = connection.unchecked_transaction()?;
    let current: Option<u64> = tx
        .query_row(
            "SELECT revision FROM agent_role_profiles WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some_and(|value| value >= revision) {
        return Ok(false);
    }
    tx.execute("INSERT INTO agent_role_profile_revisions(profile_id, revision, content_hash, profile_json, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, revision as i64, content_hash, profile_json, now_ms])?;
    tx.execute("INSERT INTO agent_role_profiles(id, revision, content_hash, profile_json, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision, content_hash=excluded.content_hash, profile_json=excluded.profile_json, updated_at_ms=excluded.updated_at_ms", params![id, revision as i64, content_hash, profile_json, now_ms])?;
    tx.commit()?;
    Ok(true)
}

pub fn load_json(
    connection: &Connection,
    id: &str,
    revision: u64,
) -> Result<Option<Vec<u8>>, rusqlite::Error> {
    connection.query_row("SELECT profile_json FROM agent_role_profile_revisions WHERE profile_id = ?1 AND revision = ?2", params![id, revision as i64], |row| row.get(0)).optional()
}

pub fn load_all_json(connection: &Connection) -> Result<Vec<Vec<u8>>, rusqlite::Error> {
    let mut statement =
        connection.prepare("SELECT profile_json FROM agent_role_profiles ORDER BY id")?;
    let rows = statement.query_map([], |row| row.get(0))?.collect();
    rows
}
