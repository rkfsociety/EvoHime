use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 68 {
        crate::workflow_optimization_lab_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 68;")?;
    }
    Ok(())
}
