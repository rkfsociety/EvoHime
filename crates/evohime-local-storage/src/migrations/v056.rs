use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 56 { crate::plan_artifact::PlanArtifactStore::install_schema(t)?; t.execute_batch("PRAGMA user_version = 56;")?; } Ok(()) }
