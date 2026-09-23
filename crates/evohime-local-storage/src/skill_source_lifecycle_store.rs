use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Creates the skill-installation lifecycle snapshot table transactionally.
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS skill_source_lifecycle (installation_id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, UNIQUE(installation_id,idempotency_key));")
}
/// Appends the next lifecycle revision for a skill installation.
///
/// The requested revision must be exactly one greater than the stored revision
/// (or `1` for a new installation); gaps and stale updates are rejected.
///
/// # Errors
///
/// Returns a SQLite error for revision conflicts or database failures.
pub fn save(
    c: &Connection,
    id: &str,
    rev: u64,
    hash: &str,
    json: &[u8],
    key: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let cur: Option<u64> = c
        .query_row(
            "SELECT revision FROM skill_source_lifecycle WHERE installation_id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    if rev != cur.unwrap_or(0) + 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "skill lifecycle revision conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO skill_source_lifecycle VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, rev, hash, json, key, now],
    )?;
    Ok(())
}
/// Returns the current serialized lifecycle state for an installation.
///
/// # Errors
///
/// Returns a SQLite error if the query fails.
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT json FROM skill_source_lifecycle WHERE installation_id=?1",
        params![id],
        |r| r.get(0),
    )
    .optional()
}
