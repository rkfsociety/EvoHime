use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 154 {
        crate::project_execution_board_store::install_schema(tx)?;
        tx.execute_batch("PRAGMA user_version = 154;")?;
    }
    Ok(())
}
