use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 55 { crate::artifact_handoff_registry_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 55;")?; } Ok(()) }
