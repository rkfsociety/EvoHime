use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 58 { crate::incremental_change_protocol_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 58;")?; } Ok(()) }
