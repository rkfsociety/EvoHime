use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 61 { crate::composable_termination_conditions_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 61;")?; } Ok(()) }
