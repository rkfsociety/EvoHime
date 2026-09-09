use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 164 {
        crate::semantic_activity_motion_system_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 164;")?;
    }
    Ok(())
}
