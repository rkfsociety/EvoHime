use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 66 {
        crate::experience_replay_library_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 66;")?;
    }
    Ok(())
}
