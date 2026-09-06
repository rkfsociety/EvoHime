use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 57 { crate::workspace_state_checkpoint::install_schema(t)?; t.execute_batch("PRAGMA user_version = 57;")?; } Ok(()) }
