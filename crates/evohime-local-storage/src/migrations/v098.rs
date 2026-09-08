use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 98 {
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS compact_context_blocks (compact_hash TEXT PRIMARY KEY NOT NULL, source_ref TEXT NOT NULL, source_hash TEXT NOT NULL, kind TEXT NOT NULL, loss_class TEXT NOT NULL, omitted_json BLOB NOT NULL, incomplete INTEGER NOT NULL, decision TEXT NOT NULL, created_at_ms INTEGER NOT NULL);")?;
        transaction.execute_batch("PRAGMA user_version = 98;")?;
    }
    Ok(())
}
