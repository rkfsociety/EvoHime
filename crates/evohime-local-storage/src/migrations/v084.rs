use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 84 {
        crate::customization_inventory_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 84;")?;
    }
    Ok(())
}
