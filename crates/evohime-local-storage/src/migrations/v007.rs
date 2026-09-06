use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 7 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS run_recovery (id INTEGER PRIMARY KEY AUTOINCREMENT, run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE, state TEXT NOT NULL, effect_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, verifier TEXT NOT NULL, evidence_json BLOB NOT NULL, decision TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_run_recovery_run ON run_recovery(run_id, id); PRAGMA user_version = 7;")?;
    }
    Ok(())
}
