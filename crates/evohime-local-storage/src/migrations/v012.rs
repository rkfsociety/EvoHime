use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 12 {
        t.execute_batch("ALTER TABLE memory_entries ADD COLUMN confirmations INTEGER NOT NULL DEFAULT 1; ALTER TABLE memory_entries ADD COLUMN lesson_key TEXT; CREATE INDEX IF NOT EXISTS idx_memory_entries_lesson ON memory_entries(scope_kind, scope_id, lesson_key); PRAGMA user_version = 12;")?;
    }
    Ok(())
}
