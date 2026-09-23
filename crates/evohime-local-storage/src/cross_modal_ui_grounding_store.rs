use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Creates the revisioned cross-modal UI grounding state table.
///
/// # Errors
///
/// Returns the underlying SQLite schema error.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS cross_modal_ui_grounding (id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(id,revision), UNIQUE(id,idempotency_key));")
}
/// Saves one cross-modal grounding revision with idempotent replay checks.
///
/// Replaying an idempotency key is accepted only when both revision and content
/// hash match the stored write.
///
/// # Errors
///
/// Returns a SQLite error for conflicting key reuse or write failure.
pub fn save(
    c: &Connection,
    id: &str,
    r: u64,
    h: &str,
    j: &[u8],
    k: &str,
    n: i64,
) -> rusqlite::Result<()> {
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM cross_modal_ui_grounding WHERE id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "idempotency_conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO cross_modal_ui_grounding VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, n],
    )?;
    Ok(())
}
/// Loads the serialized grounding state at the highest revision for `id`.
///
/// # Errors
///
/// Returns a SQLite error if the query fails.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT json FROM cross_modal_ui_grounding WHERE id=?1 ORDER BY revision DESC LIMIT 1",
        params![id],
        |x| x.get(0),
    )
    .optional()
}
