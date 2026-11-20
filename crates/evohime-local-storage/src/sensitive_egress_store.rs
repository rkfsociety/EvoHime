//! Metadata-only persistence for sensitive egress decisions.
//!
//! This store deliberately has no payload, finding value, span, context,
//! credential, or reversible mapping column. It is suitable for replay and
//! diagnostics without becoming a PII database.

use crate::StorageError;
use rusqlite::{params, Connection};

/// Bounded metadata written for one final egress evaluation.
#[derive(Debug, Clone, Copy)]
pub struct EgressDecisionInput<'a> {
    /// Stable request identifier.
    pub request_id: &'a str,
    /// Exact payload digest.
    pub payload_hash: &'a str,
    /// Transformed payload digest, if a transform occurred.
    pub transformed_payload_hash: Option<&'a str>,
    /// Route or destination reference.
    pub destination: &'a str,
    /// Policy snapshot digest.
    pub policy_hash: &'a str,
    /// Detector pack version.
    pub detector_pack_version: u32,
    /// Metadata-only finding categories/count projection.
    pub findings_json: &'a str,
    /// Terminal disposition.
    pub disposition: &'a str,
    /// Stable reason code.
    pub created_at_ms: i64,
}

/// Creates the append-only metadata table.
pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
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
           ON sensitive_egress_decisions(created_at_ms);",
    )?;
    Ok(())
}

/// Inserts one bounded metadata record idempotently.
pub fn record(
    connection: &Connection,
    input: EgressDecisionInput<'_>,
) -> Result<bool, StorageError> {
    if input.request_id.is_empty()
        || input.request_id.len() > 128
        || input.payload_hash.len() > 128
        || input.destination.len() > 128
        || input.policy_hash.len() > 128
        || input.findings_json.len() > 8 * 1024
        || input.disposition.len() > 64
    {
        return Err(StorageError::InvalidInput(
            "sensitive egress metadata exceeds bound".into(),
        ));
    }
    Ok(connection.execute(
        "INSERT OR IGNORE INTO sensitive_egress_decisions
         (request_id,payload_hash,transformed_payload_hash,destination,policy_hash,
          detector_pack_version,findings_json,disposition,reason_code,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            input.request_id,
            input.payload_hash,
            input.transformed_payload_hash,
            input.destination,
            input.policy_hash,
            input.detector_pack_version as i64,
            input.findings_json,
            input.disposition,
            "metadata_only",
            input.created_at_ms,
        ],
    )? == 1)
}
