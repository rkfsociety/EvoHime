use rusqlite::{params, Connection, OptionalExtension};

pub fn put(
    connection: &Connection,
    evidence_id: &str,
    lane_id: &str,
    status: &str,
    evidence_json: &[u8],
    target_id: &str,
    fingerprint: &str,
    now_ms: i64,
) -> Result<bool, &'static str> {
    if evidence_id.trim().is_empty()
        || evidence_id.len() > 256
        || lane_id.trim().is_empty()
        || lane_id.len() > 256
        || status.trim().is_empty()
        || status.len() > 64
        || evidence_json.is_empty()
        || target_id.trim().is_empty()
        || fingerprint.trim().is_empty()
        || now_ms <= 0
    {
        return Err("invalid ledger scope");
    }
    if evidence_json.len() > 64 * 1024 {
        return Err("evidence too large");
    }
    connection.execute("INSERT OR IGNORE INTO verification_evidence_ledger (evidence_id,target_id,lane_id,status,fingerprint,evidence_json,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![evidence_id, target_id, lane_id, status, fingerprint, evidence_json, now_ms]).map(|count| count == 1).map_err(|_| "sqlite")
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
