use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS declarative_component_registries (registry_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, registry_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS declarative_component_migrations (migration_id TEXT PRIMARY KEY, provider_id TEXT NOT NULL, from_version INTEGER NOT NULL, to_version INTEGER NOT NULL, result_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    revision: u64,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO declarative_component_registries(registry_id,revision,registry_json,content_hash,updated_at_ms) VALUES(?1,?2,?3,?4,?5)",params![id,revision,json,hash,now])?==1)
}
pub fn replace(
    c: &Connection,
    id: &str,
    expected: u64,
    revision: u64,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("UPDATE declarative_component_registries SET revision=?3,registry_json=?4,content_hash=?5,updated_at_ms=?6 WHERE registry_id=?1 AND revision=?2",params![id,expected,revision,json,hash,now])?==1)
}
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT registry_json FROM declarative_component_registries WHERE registry_id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}
