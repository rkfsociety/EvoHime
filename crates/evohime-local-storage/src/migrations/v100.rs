use rusqlite::Transaction;
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 100 {
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS provider_reliability_metadata (provider_id TEXT NOT NULL, model_id TEXT NOT NULL, revision INTEGER NOT NULL, free_state TEXT NOT NULL, reliability_json BLOB NOT NULL, observed_at_ms INTEGER NOT NULL, PRIMARY KEY(provider_id, model_id, revision));")?;
        transaction.execute_batch("PRAGMA user_version = 100;")?;
    }
    Ok(())
}
