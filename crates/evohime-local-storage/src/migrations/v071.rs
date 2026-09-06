use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 71 {
        crate::declarative_agent_component_registry_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 71;")?;
    }
    Ok(())
}
