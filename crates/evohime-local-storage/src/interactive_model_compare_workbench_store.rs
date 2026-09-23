use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Creates the revisioned model-comparison workbench table transactionally.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS interactive_model_compare_workbench (id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(id,revision), UNIQUE(id,idempotency_key));")
}
/// Saves a comparison-workbench revision with idempotency-key validation.
///
/// Replaying the original revision and content hash is a no-op; a conflicting
/// replay is rejected.
///
/// # Errors
///
/// Returns a SQLite error for conflicts or failed writes.
pub fn save(
    c: &Connection,
    id: &str,
    r: u64,
    h: &str,
    j: &[u8],
    k: &str,
    n: i64,
) -> rusqlite::Result<()> {
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM interactive_model_compare_workbench WHERE id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "idempotency_conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO interactive_model_compare_workbench VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, n],
    )?;
    Ok(())
}
/// Returns the most recent serialized comparison-workbench snapshot.
///
/// # Errors
///
/// Returns a SQLite error if the query fails.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM interactive_model_compare_workbench WHERE id=?1 ORDER BY revision DESC LIMIT 1",params![id],|x|x.get(0)).optional()
}
