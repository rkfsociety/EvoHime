use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 43 {
        crate::agent_middleware_pipeline_store::install_schema(t)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        t.execute_batch("PRAGMA user_version = 43;")?;
    }
    Ok(())
}
