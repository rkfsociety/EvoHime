use rusqlite::{params, Connection, OptionalExtension, Transaction};
pub fn install_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS authorized_security_assessment (assessment_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, json BLOB NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(assessment_id,revision), UNIQUE(assessment_id,idempotency_key));")
}
pub fn save(
    c: &Connection,
    id: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    key: &str,
    now: i64,
) -> rusqlite::Result<()> {
    let current: Option<u64> = c
        .query_row(
            "SELECT MAX(revision) FROM authorized_security_assessment WHERE assessment_id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if revision != current.unwrap_or(0) + 1 {
        return Err(rusqlite::Error::InvalidParameterName(
            "assessment revision conflict".into(),
        ));
    }
    c.execute(
        "INSERT INTO authorized_security_assessment VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, revision, hash, json, key, now],
    )?;
    Ok(())
}
pub fn current(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row("SELECT json FROM authorized_security_assessment WHERE assessment_id=?1 ORDER BY revision DESC LIMIT 1", params![id], |r| r.get(0)).optional()
}
