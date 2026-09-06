use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 18 {
        t.execute_batch(
            "CREATE TABLE IF NOT EXISTS context_pins (
                task_id TEXT NOT NULL,
                item_id TEXT NOT NULL,
                pinned INTEGER NOT NULL DEFAULT 1,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (task_id, item_id)
            );
            CREATE TABLE IF NOT EXISTS context_command_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                command TEXT NOT NULL,
                subject TEXT,
                outcome TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_context_command_audit_rate
                ON context_command_audit(task_id, command, created_at);
            PRAGMA user_version = 18;",
        )?;
    }
    Ok(())
}
