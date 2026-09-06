use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 48 {
        crate::skill_trust_pipeline_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 48;")?;
    }
    Ok(())
}
