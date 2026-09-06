use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 80 {
        crate::architect_editor_model_pipeline_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 80;")?;
    }
    Ok(())
}
