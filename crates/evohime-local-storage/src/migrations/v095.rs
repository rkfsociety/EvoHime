use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 95 {
        crate::grounded_research_store::GroundedResearchStore::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 95;")?;
    }
    Ok(())
}
