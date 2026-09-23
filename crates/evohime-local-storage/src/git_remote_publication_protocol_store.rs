use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Creates the revisioned Git remote-publication protocol table.
///
/// # Errors
///
/// Returns the underlying SQLite schema error.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS git_remote_publication_protocol (protocol_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(protocol_id,revision), UNIQUE(protocol_id,idempotency_key));")
}
/// Appends the next remote-publication protocol revision.
///
/// Replays with the same key, revision, and content hash are idempotent;
/// revisions must be contiguous and key reuse with other content is rejected.
///
/// # Errors
///
/// Returns a SQLite error for revision or idempotency conflicts and failed
/// writes.
pub fn save(
    c: &Connection,
    id: &str,
    r: u64,
    h: &str,
    j: &[u8],
    k: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM git_remote_publication_protocol WHERE protocol_id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "publication idempotency conflict".into(),
        ));
    }
    let cur: Option<u64> = c
        .query_row(
            "SELECT MAX(revision) FROM git_remote_publication_protocol WHERE protocol_id=?1",
            params![id],
            |x| x.get(0),
        )
        .optional()?
        .flatten();
    if r != cur.unwrap_or(0) + 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "publication revision conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO git_remote_publication_protocol VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, now],
    )?;
    Ok(())
}
/// Loads the protocol JSON at the highest revision for the protocol ID.
///
/// # Errors
///
/// Returns a SQLite error if the query fails.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM git_remote_publication_protocol WHERE protocol_id=?1 ORDER BY revision DESC LIMIT 1",params![id],|x|x.get(0)).optional()
}
