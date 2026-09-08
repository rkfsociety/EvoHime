use rusqlite::Transaction;
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 93 {
        crate::execution_environment_profiles_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 93;")?;
    }
    Ok(())
}
