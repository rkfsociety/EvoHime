use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Adds the versioned acquisition-runtime table to the schema transaction.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS external_source_acquisition_runtime (id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(id,revision), UNIQUE(id,idempotency_key));")
}
/// Persists an external-source acquisition runtime revision.
///
/// An idempotency key may be replayed only with the original revision and
/// content hash; conflicting reuse returns an error.
///
/// # Errors
///
/// Returns a SQLite error for a conflict or failed database operation.
pub fn save(
    c: &Connection,
    id: &str,
    r: u64,
    h: &str,
    j: &[u8],
    k: &str,
    n: i64,
) -> rusqlite::Result<()> {
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM external_source_acquisition_runtime WHERE id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "idempotency_conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO external_source_acquisition_runtime VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, n],
    )?;
    Ok(())
}
/// Loads the acquisition-runtime JSON at the greatest revision for `id`.
///
/// # Errors
///
/// Returns a SQLite error if the query fails.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM external_source_acquisition_runtime WHERE id=?1 ORDER BY revision DESC LIMIT 1",params![id],|x|x.get(0)).optional()
}
