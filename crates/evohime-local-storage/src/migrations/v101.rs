use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 101 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS design_intent_reviews (review_id TEXT PRIMARY KEY NOT NULL, intent_id TEXT NOT NULL, revision INTEGER NOT NULL, scope TEXT NOT NULL, intent_hash TEXT NOT NULL, verdict TEXT NOT NULL, evidence_refs_json BLOB NOT NULL, reviewer_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL);")?;
        t.execute_batch("PRAGMA user_version = 101;")?;
    }
    Ok(())
}
