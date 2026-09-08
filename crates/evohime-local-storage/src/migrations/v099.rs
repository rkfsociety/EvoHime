use rusqlite::Transaction;
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 99 {
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS project_quality_contracts (contract_id TEXT NOT NULL, revision INTEGER NOT NULL, content_hash TEXT NOT NULL, status TEXT NOT NULL, contract_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(contract_id, revision));")?;
        transaction.execute_batch("PRAGMA user_version = 99;")?;
    }
    Ok(())
}
