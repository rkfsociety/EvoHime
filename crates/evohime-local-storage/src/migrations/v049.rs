use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 49 {
        crate::team_sop_protocols_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 49;")?;
    }
    Ok(())
}
