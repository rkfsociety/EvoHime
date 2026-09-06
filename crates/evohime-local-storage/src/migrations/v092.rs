use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 92 {
        crate::persistent_agent_registry_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 92;")?;
    }
    Ok(())
}
