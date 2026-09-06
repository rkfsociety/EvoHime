use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 3 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS snapshots (id TEXT PRIMARY KEY, run_id TEXT NOT NULL, workspace_hash TEXT NOT NULL, payload BLOB NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_snapshots_run ON snapshots(run_id); PRAGMA user_version = 3;")?;
    }
    Ok(())
}
