use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 87 {
        crate::checkpoint_forking_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 87;")?;
    }
    Ok(())
}
