use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 63 { crate::team_coordination_policies_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 63;")?; } Ok(()) }
