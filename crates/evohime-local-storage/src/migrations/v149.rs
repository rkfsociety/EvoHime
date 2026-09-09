use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 149 {
        crate::interactive_model_compare_workbench_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 149;")?;
    }
    Ok(())
}
