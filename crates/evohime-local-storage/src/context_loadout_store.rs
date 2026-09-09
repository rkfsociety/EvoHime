use rusqlite::{params, Connection, OptionalExtension, Transaction};
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS context_loadout_revisions (profile_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(profile_id,revision), UNIQUE(profile_id,idempotency_key)); CREATE INDEX IF NOT EXISTS idx_context_loadout_current ON context_loadout_revisions(profile_id,revision DESC);")
}
pub fn save(
    c: &Connection,
    id: &str,
    rev: u64,
    hash: &str,
    json: &[u8],
    key: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let old:Option<(String,Vec<u8>)>=c.query_row("SELECT content_hash,json FROM context_loadout_revisions WHERE profile_id=?1 AND idempotency_key=?2",params![id,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((h, j)) = old {
        if h == hash && j == json {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "context loadout idempotency conflict".into(),
        ));
    }
    let cur: Option<u64> = c
        .query_row(
            "SELECT MAX(revision) FROM context_loadout_revisions WHERE profile_id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if rev != cur.unwrap_or(0) + 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "context loadout revision conflict".into(),
        ));
    }
    c.execute("INSERT INTO context_loadout_revisions(profile_id,revision,content_hash,json,idempotency_key,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6)",params![id,rev,hash,json,key,now])?;
    Ok(())
}
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM context_loadout_revisions WHERE profile_id=?1 ORDER BY revision DESC LIMIT 1",params![id],|r|r.get(0)).optional()
}
