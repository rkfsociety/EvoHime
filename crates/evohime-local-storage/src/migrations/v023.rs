use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 23 {
        t.execute_batch("PRAGMA user_version = 23;")?;
    }
    Ok(())
}
