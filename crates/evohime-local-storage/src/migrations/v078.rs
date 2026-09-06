use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 78 {
        crate::knowledge_source_registry_project_role_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 78;")?;
    }
    Ok(())
}
