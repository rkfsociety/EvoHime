use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 15 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS agent_run_effects (effect_id TEXT PRIMARY KEY, run_id TEXT NOT NULL UNIQUE, task_id TEXT NOT NULL, node_id TEXT NOT NULL, kind TEXT NOT NULL, idempotency_key TEXT NOT NULL UNIQUE, immutable_intent_hash TEXT NOT NULL, state TEXT NOT NULL, started_at TEXT, completed_at TEXT, result_hash TEXT); CREATE INDEX IF NOT EXISTS idx_agent_run_effects_task ON agent_run_effects(task_id, started_at); CREATE TABLE IF NOT EXISTS agent_run_leases (run_id TEXT PRIMARY KEY REFERENCES agent_run_effects(run_id) ON DELETE CASCADE, lease_id TEXT NOT NULL UNIQUE, owner_id TEXT NOT NULL, generation INTEGER NOT NULL, lease_expires_at TEXT NOT NULL, heartbeat_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS agent_run_reconciliations (effect_id TEXT PRIMARY KEY REFERENCES agent_run_effects(effect_id) ON DELETE CASCADE, state TEXT NOT NULL, verifier TEXT NOT NULL, evidence_json BLOB NOT NULL, reconciled_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE TABLE IF NOT EXISTS agent_run_recovery (id INTEGER PRIMARY KEY AUTOINCREMENT, run_id TEXT NOT NULL, state TEXT NOT NULL, effect_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, verifier TEXT NOT NULL, evidence_json BLOB NOT NULL, decision TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_agent_run_recovery_run ON agent_run_recovery(run_id, id); PRAGMA user_version = 15;")?;
    }
    Ok(())
}
