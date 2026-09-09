use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 153 {
        crate::native_computer_use_runtime_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 153;")?;
    }
    Ok(())
}
