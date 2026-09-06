use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 85 {
        crate::standing_approval_profiles_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 85;")?;
    }
    Ok(())
}
