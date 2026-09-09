use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 103 {
        t.execute_batch(
            "CREATE TABLE IF NOT EXISTS automation_queues (queue_id TEXT PRIMARY KEY NOT NULL, owner_scope TEXT NOT NULL, revision INTEGER NOT NULL, max_active INTEGER NOT NULL, max_queued INTEGER NOT NULL, priority TEXT NOT NULL, overflow_policy TEXT NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS automation_waits (run_id TEXT PRIMARY KEY NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE, revision INTEGER NOT NULL, condition_json BLOB NOT NULL, wake_at_ms INTEGER, state TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS automation_wakeups (wake_key TEXT PRIMARY KEY NOT NULL, run_id TEXT NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE, wake_at_ms INTEGER NOT NULL, kind TEXT NOT NULL, active INTEGER NOT NULL, created_at_ms INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS automation_attempts (attempt_id TEXT PRIMARY KEY NOT NULL, run_id TEXT NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE, generation INTEGER NOT NULL, dispatcher_id TEXT NOT NULL, state TEXT NOT NULL, outcome_code TEXT NOT NULL, started_at_ms INTEGER, ended_at_ms INTEGER, created_at_ms INTEGER NOT NULL);
             CREATE INDEX IF NOT EXISTS idx_automation_wakeups_due ON automation_wakeups(active, wake_at_ms);
             CREATE INDEX IF NOT EXISTS idx_automation_attempts_run ON automation_attempts(run_id, created_at_ms);
             CREATE INDEX IF NOT EXISTS idx_automation_attempts_active ON automation_attempts(state, created_at_ms);
             PRAGMA user_version = 103;",
        )?;
    }
    Ok(())
}
