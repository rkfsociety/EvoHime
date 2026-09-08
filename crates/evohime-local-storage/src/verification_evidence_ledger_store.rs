use rusqlite::{params, Connection, OptionalExtension};

pub fn put(
    connection: &Connection,
    evidence: &crate::verification_evidence_ledger::VerificationEvidence,
    target_id: &str,
    fingerprint: &str,
    now_ms: i64,
) -> Result<bool, &'static str> {
    evidence.validate().map_err(|_| "invalid evidence")?;
    if target_id.trim().is_empty() || fingerprint.trim().is_empty() || now_ms <= 0 {
        return Err("invalid ledger scope");
    }
    let json = serde_json::to_vec(evidence).map_err(|_| "serialization")?;
    if json.len() > 64 * 1024 {
        return Err("evidence too large");
    }
    connection.execute("INSERT OR IGNORE INTO verification_evidence_ledger (evidence_id,target_id,lane_id,status,fingerprint,evidence_json,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![evidence.evidence_id, target_id, evidence.lane_id, serde_json::to_string(&evidence.status).map_err(|_| "serialization")?, fingerprint, json, now_ms]).map(|count| count == 1).map_err(|_| "sqlite")
}

pub fn get(connection: &Connection, evidence_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT evidence_json FROM verification_evidence_ledger WHERE evidence_id=?1",
            params![evidence_id],
            |row| row.get(0),
        )
        .optional()
}
