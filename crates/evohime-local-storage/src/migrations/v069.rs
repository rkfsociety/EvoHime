use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> { if c < 69 { crate::core_topic_subscription_event_bus_store::install_schema(t)?; t.execute_batch("PRAGMA user_version = 69;")?; } Ok(()) }
