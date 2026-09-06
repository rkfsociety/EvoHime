use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 32 {
        crate::task_checkpoint::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 32;")?;
    }
    Ok(())
}
