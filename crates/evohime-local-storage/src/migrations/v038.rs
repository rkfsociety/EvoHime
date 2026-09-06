use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 38 { crate::analysis_kernel::install_schema(t)?; t.execute_batch("PRAGMA user_version = 38;")?; } Ok(()) }
