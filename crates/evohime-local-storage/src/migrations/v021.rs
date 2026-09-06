use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 21 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS receipt_key_transitions (sequence INTEGER PRIMARY KEY AUTOINCREMENT, transition_id TEXT NOT NULL UNIQUE, transition_hash TEXT NOT NULL UNIQUE, previous_key_id TEXT, new_key_id TEXT NOT NULL, continuity TEXT NOT NULL, canonical_json BLOB NOT NULL, created_at TEXT NOT NULL); CREATE INDEX IF NOT EXISTS idx_receipt_key_transitions_new_key ON receipt_key_transitions(new_key_id, sequence); CREATE TABLE IF NOT EXISTS receipt_key_audit (event_id TEXT PRIMARY KEY, transition_id TEXT NOT NULL UNIQUE, event_type TEXT NOT NULL, old_key_id TEXT, new_key_id TEXT, transition_hash TEXT NOT NULL, reason TEXT NOT NULL, actor TEXT NOT NULL, outcome TEXT NOT NULL, error_code TEXT, created_at TEXT NOT NULL); PRAGMA user_version = 21;")?;
    }
    Ok(())
}
