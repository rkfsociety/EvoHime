use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 70 { crate::dependency_aware_task_graph_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 70;")?; } Ok(()) }
