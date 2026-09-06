use rusqlite::Transaction;
pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 6 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS run_leases (run_id TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE, lease_id TEXT NOT NULL UNIQUE, owner_id TEXT NOT NULL, generation INTEGER NOT NULL, lease_expires_at TEXT NOT NULL, heartbeat_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS run_reconciliations (effect_id TEXT PRIMARY KEY REFERENCES run_effects(effect_id) ON DELETE CASCADE, state TEXT NOT NULL, verifier TEXT NOT NULL, evidence_json BLOB NOT NULL, reconciled_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); PRAGMA user_version = 6;")?;
    }
    Ok(())
}
