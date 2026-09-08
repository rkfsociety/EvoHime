use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 96 {
        crate::local_model_performance_calibration_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 96;")?;
    }
    Ok(())
}
