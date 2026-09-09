use rusqlite::Transaction;
pub(crate) fn apply(tx:&Transaction<'_>,current:u32)->rusqlite::Result<()>{if current<112{crate::project_knowledge_notebook_store::install_schema(tx)?;tx.execute_batch("PRAGMA user_version = 112;")?;}Ok(())}
