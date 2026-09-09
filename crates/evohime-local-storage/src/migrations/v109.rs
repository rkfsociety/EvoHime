use rusqlite::Transaction;
pub(crate) fn apply(tx: &Transaction<'_>, current: u32) -> rusqlite::Result<()> { if current < 109 { crate::authorized_security_assessment_store::install_schema(tx)?; tx.execute_batch("PRAGMA user_version = 109;")?; } Ok(()) }
