use rusqlite::Transaction;

pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 110 {
        crate::runtime_service_graph_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 110;")?;
    }
    Ok(())
}
