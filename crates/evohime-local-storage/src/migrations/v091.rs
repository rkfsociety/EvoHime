use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 91 {
        crate::guided_calibration_sessions_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 91;")?;
    }
    Ok(())
}
