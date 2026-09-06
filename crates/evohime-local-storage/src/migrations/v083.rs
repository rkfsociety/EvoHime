use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 83 {
        crate::output_guardrail_pipeline_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 83;")?;
    }
    Ok(())
}
