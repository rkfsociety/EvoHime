use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 79 {
        crate::agent_git_change_sets_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 79;")?;
    }
    Ok(())
}
