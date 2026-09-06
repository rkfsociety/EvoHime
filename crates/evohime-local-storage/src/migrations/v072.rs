use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 72 { crate::typed_context_references_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 72;")?; } Ok(()) }
