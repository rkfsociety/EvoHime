use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 62 { crate::workspace_bootstrap_manifest_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 62;")?; } Ok(()) }
