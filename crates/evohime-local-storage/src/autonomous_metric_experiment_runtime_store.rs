use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Creates the revisioned runtime-state table for metric experiments.
///
/// # Errors
///
/// Returns the underlying SQLite schema error.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS autonomous_metric_experiment_runtime (id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(id,revision), UNIQUE(id,idempotency_key));")
}
/// Persists a metric-experiment runtime revision with idempotency checks.
///
/// Replaying a key with the same revision and hash succeeds; reusing it for a
/// different revision or content hash is rejected.
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
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM autonomous_metric_experiment_runtime WHERE id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "idempotency_conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO autonomous_metric_experiment_runtime VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, n],
    )?;
    Ok(())
}
/// Loads the JSON state at the greatest revision for an experiment ID.
///
/// # Errors
///
/// Returns a SQLite error if the query fails.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM autonomous_metric_experiment_runtime WHERE id=?1 ORDER BY revision DESC LIMIT 1",params![id],|x|x.get(0)).optional()
}
