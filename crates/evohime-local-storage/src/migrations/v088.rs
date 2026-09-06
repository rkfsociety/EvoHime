use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 88 {
        crate::privacy_telemetry_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 88;")?;
    }
    Ok(())
}
