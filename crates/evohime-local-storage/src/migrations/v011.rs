use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 11 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS run_tool_metrics (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT NOT NULL, tool_name TEXT NOT NULL, iteration INTEGER NOT NULL, ok INTEGER NOT NULL, failure_kind TEXT, recovery_hint INTEGER NOT NULL, escalated INTEGER NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_run_tool_metrics_task ON run_tool_metrics(task_id, id); CREATE INDEX IF NOT EXISTS idx_run_tool_metrics_tool ON run_tool_metrics(task_id, tool_name, id); PRAGMA user_version = 11;")?;
    }
    Ok(())
}
