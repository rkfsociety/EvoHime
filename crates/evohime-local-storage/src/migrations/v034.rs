use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 34 {
        crate::continuation_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 34;")?;
    }
    Ok(())
}
