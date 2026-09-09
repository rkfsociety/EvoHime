use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 160 {
        crate::ide_companion_bridge_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 160;")?;
    }
    Ok(())
}
