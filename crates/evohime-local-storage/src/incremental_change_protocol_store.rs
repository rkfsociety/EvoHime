//! Durable, metadata-only storage for Incremental Change Protocol runs.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const MAX_JSON_BYTES: usize = 64 * 1024;
pub const MAX_EVIDENCE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncrementalChangeRunRecord {
    pub run_id: String,
    pub version: u64,
    pub state: String,
    pub plan_artifact_id: String,
    pub plan_revision: u64,
    pub plan_content_hash: String,
    pub checkpoint_id: String,
    pub checkpoint_snapshot_hash: String,
    pub baseline_fingerprint: String,
    pub impact_json: Vec<u8>,
    pub change_plan_json: Vec<u8>,
    pub evidence_json: Vec<u8>,
    pub idempotency_key: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS incremental_change_runs (
            run_id TEXT PRIMARY KEY,
            version INTEGER NOT NULL,
            state TEXT NOT NULL,
            plan_artifact_id TEXT NOT NULL,
            plan_revision INTEGER NOT NULL,
            plan_content_hash TEXT NOT NULL,
            checkpoint_id TEXT NOT NULL,
            checkpoint_snapshot_hash TEXT NOT NULL,
            baseline_fingerprint TEXT NOT NULL,
            impact_json BLOB NOT NULL,
            change_plan_json BLOB NOT NULL,
            evidence_json BLOB NOT NULL,
            idempotency_key TEXT NOT NULL UNIQUE,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_incremental_change_runs_updated
            ON incremental_change_runs(updated_at_ms DESC);",
    )
}

pub fn create(
    connection: &Connection,
    record: &IncrementalChangeRunRecord,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "INSERT OR IGNORE INTO incremental_change_runs
         (run_id,version,state,plan_artifact_id,plan_revision,plan_content_hash,
          checkpoint_id,checkpoint_snapshot_hash,baseline_fingerprint,impact_json,
          change_plan_json,evidence_json,idempotency_key,created_at_ms,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?14)",
        params![
            record.run_id,
            record.version as i64,
            record.state,
            record.plan_artifact_id,
            record.plan_revision as i64,
            record.plan_content_hash,
            record.checkpoint_id,
            record.checkpoint_snapshot_hash,
            record.baseline_fingerprint,
            record.impact_json,
            record.change_plan_json,
            record.evidence_json,
            record.idempotency_key,
            record.created_at_ms,
        ],
    )?;
    Ok(changed == 1)
}

pub fn get(
    connection: &Connection,
    run_id: &str,
) -> rusqlite::Result<Option<IncrementalChangeRunRecord>> {
    connection
        .query_row(
            "SELECT run_id,version,state,plan_artifact_id,plan_revision,plan_content_hash,
                    checkpoint_id,checkpoint_snapshot_hash,baseline_fingerprint,impact_json,
                    change_plan_json,evidence_json,idempotency_key,created_at_ms,updated_at_ms
             FROM incremental_change_runs WHERE run_id=?1",
            params![run_id],
            |row| {
                Ok(IncrementalChangeRunRecord {
                    run_id: row.get(0)?,
                    version: row.get::<_, i64>(1)? as u64,
                    state: row.get(2)?,
                    plan_artifact_id: row.get(3)?,
                    plan_revision: row.get::<_, i64>(4)? as u64,
                    plan_content_hash: row.get(5)?,
                    checkpoint_id: row.get(6)?,
                    checkpoint_snapshot_hash: row.get(7)?,
                    baseline_fingerprint: row.get(8)?,
                    impact_json: row.get(9)?,
                    change_plan_json: row.get(10)?,
                    evidence_json: row.get(11)?,
                    idempotency_key: row.get(12)?,
                    created_at_ms: row.get(13)?,
                    updated_at_ms: row.get(14)?,
                })
            },
        )
        .optional()
}

pub fn get_by_idempotency(
    connection: &Connection,
    idempotency_key: &str,
) -> rusqlite::Result<Option<IncrementalChangeRunRecord>> {
    let run_id = connection
        .query_row(
            "SELECT run_id FROM incremental_change_runs WHERE idempotency_key=?1",
            params![idempotency_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    run_id.map_or(Ok(None), |id| get(connection, &id))
}

pub fn transition(
    connection: &Connection,
    run_id: &str,
    expected_version: u64,
    state: &str,
    observed_fingerprint: &str,
    evidence_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "UPDATE incremental_change_runs
         SET version=version+1,state=?1,evidence_json=?2,updated_at_ms=?3
         WHERE run_id=?4 AND version=?5 AND baseline_fingerprint=?6",
        params![
            state,
            evidence_json,
            now_ms,
            run_id,
            expected_version as i64,
            observed_fingerprint
        ],
    )?;
    Ok(changed == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_and_stale_transitions_are_fenced() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let record = IncrementalChangeRunRecord {
            run_id: "run".into(),
            version: 1,
            state: "planned".into(),
            plan_artifact_id: "a".into(),
            plan_revision: 1,
            plan_content_hash: "a".repeat(64),
            checkpoint_id: "c".into(),
            checkpoint_snapshot_hash: "b".repeat(64),
            baseline_fingerprint: "scope".into(),
            impact_json: b"{}".to_vec(),
            change_plan_json: b"{}".to_vec(),
            evidence_json: b"{}".to_vec(),
            idempotency_key: "idem".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        assert!(create(&connection, &record).unwrap());
        assert!(!create(&connection, &record).unwrap());
        assert!(!transition(&connection, "run", 1, "applied", "other", b"{}", 2).unwrap());
        assert!(transition(&connection, "run", 1, "applied", "scope", b"{}", 2).unwrap());
        assert!(!transition(&connection, "run", 1, "applied", "scope", b"{}", 3).unwrap());
    }
}
