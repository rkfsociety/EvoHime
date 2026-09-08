use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 97 {
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS verification_evidence_ledger (evidence_id TEXT PRIMARY KEY NOT NULL, target_id TEXT NOT NULL, lane_id TEXT NOT NULL, status TEXT NOT NULL, fingerprint TEXT NOT NULL, evidence_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_verification_ledger_target ON verification_evidence_ledger(target_id, lane_id);")?;
        transaction.execute_batch("PRAGMA user_version = 97;")?;
    }
    Ok(())
}
