use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 65 { crate::schema_driven_agent_configuration_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 65;")?; } Ok(()) }
