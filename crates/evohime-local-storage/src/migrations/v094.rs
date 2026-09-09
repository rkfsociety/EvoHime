use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 94 {
        let has_table: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agent_git_change_sets'",
            [],
            |row| row.get(0),
        )?;
        if has_table == 0 {
            crate::agent_git_change_sets_store::install_schema(transaction)?;
        }
        let has_revision: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('agent_git_change_sets') WHERE name='revision'",
            [],
            |row| row.get(0),
        )?;
        if has_revision == 0 {
            transaction.execute(
                "ALTER TABLE agent_git_change_sets ADD COLUMN revision INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        crate::agent_git_change_sets_store::install_idempotency_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 94;")?;
    }
    Ok(())
}
