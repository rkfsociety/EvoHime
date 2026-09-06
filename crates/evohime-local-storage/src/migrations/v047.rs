use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 47 { crate::agent_role_profiles_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 47;")?; } Ok(()) }
