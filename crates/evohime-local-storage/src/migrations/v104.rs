use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 104 {
        crate::code_review_lane_store::CodeReviewStore::install_schema(t)?;
        t.execute_batch("PRAGMA user_version = 104;")?;
    }
    Ok(())
}
