use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 74 {
        crate::capability_workbenches_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 74;")?;
    }
    Ok(())
}
