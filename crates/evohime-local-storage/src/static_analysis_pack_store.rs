use rusqlite::{params, Connection, OptionalExtension, Transaction};
pub const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS static_analysis_pack_revisions (pack_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(pack_id, revision), UNIQUE(pack_id, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_static_analysis_pack_current ON static_analysis_pack_revisions(pack_id, revision DESC);")
}
pub fn save(
    connection: &Connection,
    pack_id: &str,
    revision: u64,
    content_hash: &str,
    json: &[u8],
    idempotency_key: &str,
    now_ms: i64,
) -> rusqlite::Result<()> {
    if json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::InvalidParameterName(
            "static analysis pack JSON too large".into(),
        ));
    }
    if let Some((old_hash, old_json)) = connection.query_row("SELECT content_hash,json FROM static_analysis_pack_revisions WHERE pack_id=?1 AND idempotency_key=?2", params![pack_id, idempotency_key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))).optional()? { if old_hash == content_hash && old_json == json { return Ok(()); } return Err(rusqlite::Error::InvalidParameterName("static analysis pack idempotency conflict".into())); }
    let current: Option<u64> = connection
        .query_row(
            "SELECT MAX(revision) FROM static_analysis_pack_revisions WHERE pack_id=?1",
            params![pack_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if revision != current.unwrap_or(0) + 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "static analysis pack revision conflict".into(),
        ));
    }
    connection.execute("INSERT INTO static_analysis_pack_revisions(pack_id,revision,content_hash,json,idempotency_key,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6)", params![pack_id, revision, content_hash, json, idempotency_key, now_ms])?;
    Ok(())
}
pub fn load_current(connection: &Connection, pack_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection.query_row("SELECT json FROM static_analysis_pack_revisions WHERE pack_id=?1 ORDER BY revision DESC LIMIT 1", params![pack_id], |row| row.get(0)).optional()
}
