use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 20 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS model_context_limits (model TEXT PRIMARY KEY, provider TEXT NOT NULL, context_tokens INTEGER, max_output_tokens INTEGER, fetched_at TEXT NOT NULL); PRAGMA user_version = 20;")?;
    }
    Ok(())
}
