use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 37 { crate::retained_child_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 37;")?; } Ok(()) }
