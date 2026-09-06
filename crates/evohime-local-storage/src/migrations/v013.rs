use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 13 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS capability_selections (task_id TEXT PRIMARY KEY NOT NULL, origin TEXT NOT NULL, manifest_name TEXT NOT NULL, state_json BLOB NOT NULL, updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); PRAGMA user_version = 13;")?;
    }
    Ok(())
}
