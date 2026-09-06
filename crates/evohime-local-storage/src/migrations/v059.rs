use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 59 {
        crate::task_worktree_isolation_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 59;")?;
    }
    Ok(())
}
