use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 81 {
        crate::event_visualizer_registry_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 81;")?;
    }
    Ok(())
}
