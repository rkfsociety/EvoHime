use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 39 {
        crate::refinement_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 39;")?;
    }
    Ok(())
}
