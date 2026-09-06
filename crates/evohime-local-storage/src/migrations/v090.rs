use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 90 {
        crate::declarative_runtime_components_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 90;")?;
    }
    Ok(())
}
