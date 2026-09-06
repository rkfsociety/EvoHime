use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 67 { crate::code_diagnostics_feedback_loop_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 67;")?; } Ok(()) }
