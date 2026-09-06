use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 76 {
        crate::project_instruction_stack_store::install_schema(t)?;
        crate::batch_invocation_runtime_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 76;")?;
    }
    Ok(())
}
