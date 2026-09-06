use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 44 {
        crate::execution_policy_profiles_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 44;")?;
    }
    Ok(())
}
