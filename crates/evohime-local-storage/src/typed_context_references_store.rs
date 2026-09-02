use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS typed_context_references (ref_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, ref_json BLOB NOT NULL, resolved_json BLOB, content_hash TEXT, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    revision: u64,
    json: &[u8],
    hash: Option<&str>,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO typed_context_references(ref_id,revision,ref_json,content_hash,updated_at_ms) VALUES(?1,?2,?3,?4,?5)",params![id,revision,json,hash,now])?==1)
}
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT ref_json FROM typed_context_references WHERE ref_id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}
