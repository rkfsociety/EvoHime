use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 53 {
        crate::human_work_items_store::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 53;")?;
    }
    Ok(())
}
