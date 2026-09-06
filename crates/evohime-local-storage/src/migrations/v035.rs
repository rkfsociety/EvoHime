use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 35 {
        let columns = t.prepare("PRAGMA table_info(continuation_runs)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if !columns.iter().any(|column| column == "idempotency_key") {
            t.execute_batch("ALTER TABLE continuation_runs ADD COLUMN idempotency_key TEXT NOT NULL DEFAULT '';")?;
        }
        if !columns.iter().any(|column| column == "task_id") {
            t.execute_batch("ALTER TABLE continuation_runs ADD COLUMN task_id TEXT NOT NULL DEFAULT '';")?;
        }
        t.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS idx_continuation_runs_idempotency ON continuation_runs(owner_scope, idempotency_key); CREATE INDEX IF NOT EXISTS idx_continuation_runs_task ON continuation_runs(task_id, state, updated_at_ms); PRAGMA user_version = 35;")?;
    }
    Ok(())
}
