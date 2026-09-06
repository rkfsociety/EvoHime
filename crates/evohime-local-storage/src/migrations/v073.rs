use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 73 { crate::safe_ui_extension_framework_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 73;")?; } Ok(()) }
