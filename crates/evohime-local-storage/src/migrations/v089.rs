use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 89 {
        crate::conversation_bridge_adapters_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 89;")?;
    }
    Ok(())
}
