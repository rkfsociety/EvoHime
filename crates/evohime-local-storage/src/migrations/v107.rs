use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 107 {
        crate::skill_source_lifecycle_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 107;")?;
    }
    Ok(())
}
