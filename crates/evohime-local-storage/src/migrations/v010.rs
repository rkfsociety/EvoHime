use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 10 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS child_handoffs (handoff_id TEXT PRIMARY KEY NOT NULL, task_id TEXT NOT NULL, kind TEXT NOT NULL, status TEXT NOT NULL, from_role TEXT NOT NULL, to_role TEXT NOT NULL, sequence INTEGER NOT NULL, envelope_json BLOB NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_child_handoffs_task ON child_handoffs(task_id); CREATE TABLE IF NOT EXISTS child_task_requests (child_task_id TEXT PRIMARY KEY NOT NULL, parent_task_id TEXT NOT NULL, role TEXT NOT NULL, kind TEXT NOT NULL, request_json BLOB NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_child_task_requests_parent ON child_task_requests(parent_task_id); CREATE TABLE IF NOT EXISTS child_reports (child_task_id TEXT PRIMARY KEY NOT NULL, parent_task_id TEXT NOT NULL, status TEXT NOT NULL, confidence_percent INTEGER NOT NULL, report_json BLOB NOT NULL, accepted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_child_reports_parent ON child_reports(parent_task_id); PRAGMA user_version = 10;")?;
    }
    Ok(())
}
