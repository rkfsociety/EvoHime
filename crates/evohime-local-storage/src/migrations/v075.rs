use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 75 {
        crate::team_coordinator_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 75;")?;
    }
    Ok(())
}
