use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 54 {
        crate::browser_session_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 54;")?;
    }
    Ok(())
}
