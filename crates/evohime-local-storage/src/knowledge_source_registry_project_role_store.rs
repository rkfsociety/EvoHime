//! Durable Knowledge Source Registry metadata.
use rusqlite::{params, Connection, OptionalExtension};

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS knowledge_sources (source_id TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL, content_hash TEXT NOT NULL, source_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS knowledge_bindings (binding_id TEXT PRIMARY KEY NOT NULL, source_id TEXT NOT NULL, binding_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS knowledge_manifests (source_id TEXT PRIMARY KEY NOT NULL, manifest_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS knowledge_chunks (chunk_id TEXT PRIMARY KEY NOT NULL, source_id TEXT NOT NULL, source_revision INTEGER NOT NULL, ordinal INTEGER NOT NULL, locator TEXT NOT NULL, chunk_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS knowledge_collections (collection_id TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL, content_hash TEXT NOT NULL, collection_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}

pub fn put_collection(
    c: &Connection,
    id: &str,
    version: u64,
    hash: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT INTO knowledge_collections(collection_id,version,content_hash,collection_json,updated_at_ms) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(collection_id) DO UPDATE SET version=excluded.version,content_hash=excluded.content_hash,collection_json=excluded.collection_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.version > knowledge_collections.version", params![id, version as i64, hash, json, now])? > 0)
}

pub fn get_collection(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT collection_json FROM knowledge_collections WHERE collection_id=?1",
        [id],
        |row| row.get(0),
    )
    .optional()
}
pub fn put_source(
    c: &Connection,
    id: &str,
    version: u64,
    hash: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT INTO knowledge_sources(source_id,version,content_hash,source_json,updated_at_ms) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(source_id) DO UPDATE SET version=excluded.version,content_hash=excluded.content_hash,source_json=excluded.source_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.version > knowledge_sources.version", params![id, version as i64, hash, json, now])? > 0)
}
pub fn get_source(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT source_json FROM knowledge_sources WHERE source_id=?1",
        [id],
        |row| row.get(0),
    )
    .optional()
}
pub fn put_binding(
    c: &Connection,
    id: &str,
    source_id: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<()> {
    c.execute("INSERT INTO knowledge_bindings(binding_id,source_id,binding_json,updated_at_ms) VALUES (?1,?2,?3,?4) ON CONFLICT(binding_id) DO UPDATE SET source_id=excluded.source_id,binding_json=excluded.binding_json,updated_at_ms=excluded.updated_at_ms", params![id, source_id, json, now])?;
    Ok(())
}
pub fn put_manifest(
    c: &Connection,
    source_id: &str,
    hash: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<()> {
    c.execute("INSERT INTO knowledge_manifests(source_id,manifest_json,content_hash,updated_at_ms) VALUES (?1,?2,?3,?4) ON CONFLICT(source_id) DO UPDATE SET manifest_json=excluded.manifest_json,content_hash=excluded.content_hash,updated_at_ms=excluded.updated_at_ms", params![source_id, json, hash, now])?;
    Ok(())
}

#[derive(Clone, Copy)]
pub struct PutChunkInput<'a> {
    pub id: &'a str,
    pub source_id: &'a str,
    pub revision: u64,
    pub ordinal: u32,
    pub locator: &'a str,
    pub json: &'a [u8],
    pub now_ms: i64,
}

pub fn put_chunk(c: &Connection, input: PutChunkInput<'_>) -> rusqlite::Result<()> {
    c.execute("INSERT INTO knowledge_chunks(chunk_id,source_id,source_revision,ordinal,locator,chunk_json,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(chunk_id) DO UPDATE SET source_id=excluded.source_id,source_revision=excluded.source_revision,ordinal=excluded.ordinal,locator=excluded.locator,chunk_json=excluded.chunk_json,updated_at_ms=excluded.updated_at_ms", params![input.id, input.source_id, input.revision as i64, input.ordinal as i64, input.locator, input.json, input.now_ms])?;
    Ok(())
}

pub fn list_chunks(
    c: &Connection,
    source_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = c.prepare(
        "SELECT chunk_json FROM knowledge_chunks WHERE source_id=?1 ORDER BY ordinal LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![source_id, limit as i64], |row| row.get(0))?
        .collect();
    rows
}

pub fn list_bindings(
    c: &Connection,
    source_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = c.prepare("SELECT binding_json FROM knowledge_bindings WHERE source_id=?1 ORDER BY binding_id LIMIT ?2")?;
    let rows = statement
        .query_map(params![source_id, limit as i64], |row| row.get(0))?
        .collect();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_revision_is_monotonic() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_source(&c, "s", 2, "h2", b"two", 2).unwrap());
        assert!(!put_source(&c, "s", 1, "h1", b"one", 3).unwrap());
        assert_eq!(get_source(&c, "s").unwrap(), Some(b"two".to_vec()));
    }

    #[test]
    fn collection_revision_is_monotonic_and_survives_reopen() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_collection(&c, "c", 2, "h2", b"two", 2).unwrap());
        assert!(!put_collection(&c, "c", 1, "h1", b"one", 3).unwrap());
        assert_eq!(get_collection(&c, "c").unwrap(), Some(b"two".to_vec()));
    }
}
