use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 60 {
        crate::team_resource_budget_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 60;")?;
    }
    Ok(())
}
