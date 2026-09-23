use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Adds the revisioned voice-output adapter table within the schema transaction.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS optional_voice_output_adapter (id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(id,revision), UNIQUE(id,idempotency_key));")
}
/// Stores a voice-output adapter revision with idempotent replay protection.
///
/// A repeated key succeeds only when revision and content hash are unchanged.
///
/// # Errors
///
/// Returns a SQLite error for conflicting key reuse or database failures.
pub fn save(
    c: &Connection,
    id: &str,
    r: u64,
    h: &str,
    j: &[u8],
    k: &str,
    n: i64,
) -> rusqlite::Result<()> {
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM optional_voice_output_adapter WHERE id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "idempotency_conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO optional_voice_output_adapter VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, n],
    )?;
    Ok(())
}
/// Returns the serialized adapter configuration at the highest revision.
///
/// # Errors
///
/// Returns a SQLite error if the query fails.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT json FROM optional_voice_output_adapter WHERE id=?1 ORDER BY revision DESC LIMIT 1",
        params![id],
        |x| x.get(0),
    )
    .optional()
}
