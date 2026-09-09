use rusqlite::{params, Connection, OptionalExtension, Transaction};
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS deterministic_review_plan (plan_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(plan_id,revision), UNIQUE(plan_id,idempotency_key)); CREATE TABLE IF NOT EXISTS deterministic_review_run (run_id TEXT PRIMARY KEY, plan_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, pinned_at_ms INTEGER NOT NULL);")
}
pub fn save(
    c: &Connection,
    id: &str,
    r: u64,
    h: &str,
    j: &[u8],
    k: &str,
    n: i64,
) -> rusqlite::Result<()> {
    let old:Option<(u64,String)>=c.query_row("SELECT revision,content_hash FROM deterministic_review_plan WHERE plan_id=?1 AND idempotency_key=?2",params![id,k],|x|Ok((x.get(0)?,x.get(1)?))).optional()?;
    if let Some((or, oh)) = old {
        if or == r && oh == h {
            return Ok(());
        }
        return Err(rusqlite::Error::InvalidParameterName(
            "review idempotency conflict".into(),
        ));
    }
    let latest: Option<u64> = c
        .query_row(
            "SELECT MAX(revision) FROM deterministic_review_plan WHERE plan_id=?1",
            params![id],
            |x| x.get(0),
        )
        .optional()?
        .flatten();
    if r != latest.unwrap_or(0) + 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "review revision conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO deterministic_review_plan VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, r, h, j, k, n],
    )?;
    Ok(())
}
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM deterministic_review_plan WHERE plan_id=?1 ORDER BY revision DESC LIMIT 1",params![id],|x|x.get(0)).optional()
}
pub fn pin(c: &Connection, run: &str, id: &str, r: u64, h: &str, n: i64) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO deterministic_review_run VALUES(?1,?2,?3,?4,?5)",
        params![run, id, r, h, n],
    )
    .map(|_| ())
    .or_else(|e| {
        if matches!(e, rusqlite::Error::SqliteFailure(_, _)) {
            Err(e)
        } else {
            Err(e)
        }
    })
}
