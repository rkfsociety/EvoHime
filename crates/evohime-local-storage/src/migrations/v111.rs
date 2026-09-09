use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 111 {
        crate::agent_program_optimizer_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 111;")?;
    }
    Ok(())
}
