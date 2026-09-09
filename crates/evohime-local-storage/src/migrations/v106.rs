use rusqlite::Transaction;
pub(crate) fn apply(tx:&Transaction<'_>,current:u32)->rusqlite::Result<()> {if current<106{crate::context_loadout_store::install_schema(tx)?;tx.execute_batch("PRAGMA user_version = 106;")?;}Ok(())}
