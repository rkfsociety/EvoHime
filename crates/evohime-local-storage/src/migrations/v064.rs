use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 64 {
        crate::typed_agent_handoff_contract_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 64;")?;
    }
    Ok(())
}
