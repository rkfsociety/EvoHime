use rusqlite::Transaction;

/// Adds metadata-only sensitive egress decision history.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 184 {
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS sensitive_egress_decisions (
               request_id TEXT PRIMARY KEY NOT NULL,
               payload_hash TEXT NOT NULL,
               transformed_payload_hash TEXT,
               destination TEXT NOT NULL,
               policy_hash TEXT NOT NULL,
               detector_pack_version INTEGER NOT NULL,
               findings_json TEXT NOT NULL,
               disposition TEXT NOT NULL,
               reason_code TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_sensitive_egress_created
               ON sensitive_egress_decisions(created_at_ms);
             PRAGMA user_version = 184;",
        )?;
    }
    Ok(())
}
