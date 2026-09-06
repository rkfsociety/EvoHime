use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 86 {
        crate::approval_policy_profiles_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 86;")?;
    }
    Ok(())
}
