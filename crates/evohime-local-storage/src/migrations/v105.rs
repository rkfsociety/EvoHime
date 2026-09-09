use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 105 {
        super::super::static_analysis_pack_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 105;")?;
    }
    Ok(())
}
