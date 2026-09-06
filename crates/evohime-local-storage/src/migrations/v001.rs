use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 1 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS events (sequence_id INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT NOT NULL, event_type TEXT NOT NULL, payload BLOB NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_events_task_sequence ON events(task_id, sequence_id); PRAGMA user_version = 1;")?;
    }
    Ok(())
}
