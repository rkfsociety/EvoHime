use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 82 {
        crate::reasoning_operator_library_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 82;")?;
    }
    Ok(())
}
