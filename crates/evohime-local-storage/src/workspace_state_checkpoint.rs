//! Durable metadata for workspace-file checkpoints (plan 58).
//!
//! Snapshot bytes are kept in ArtifactStore. This store contains only bounded,
//! immutable manifests and the journal of restore intents/results.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCheckpointRecord {
    pub checkpoint_id: String,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub snapshot_hash: String,
    pub manifest_json: Vec<u8>,
    pub created_at_ms: i64,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCheckpointSummary {
    pub checkpoint_id: String,
    pub task_id: Option<String>,
    pub snapshot_hash: String,
    pub created_at_ms: i64,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreJournalRecord {
    pub operation_id: String,
    pub checkpoint_id: String,
    pub operation: String,
    pub state: String,
    pub detail_json: Vec<u8>,
    pub created_at_ms: i64,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_state_checkpoints (
            checkpoint_id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            task_id TEXT,
            snapshot_hash TEXT NOT NULL,
            manifest_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0,1))
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_state_checkpoints_workspace
            ON workspace_state_checkpoints(workspace_id, created_at_ms DESC);
        CREATE TABLE IF NOT EXISTS workspace_state_restore_journal (
            operation_id TEXT PRIMARY KEY,
            checkpoint_id TEXT NOT NULL,
            operation TEXT NOT NULL,
            state TEXT NOT NULL,
            detail_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            FOREIGN KEY(checkpoint_id) REFERENCES workspace_state_checkpoints(checkpoint_id)
        );
        CREATE INDEX IF NOT EXISTS idx_workspace_state_restore_journal_checkpoint
            ON workspace_state_restore_journal(checkpoint_id, created_at_ms DESC);",
    )
}

pub fn insert_checkpoint(
    connection: &Connection,
    record: &WorkspaceCheckpointRecord,
) -> rusqlite::Result<()> {
    if record.manifest_json.len() > MAX_MANIFEST_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "manifest too large"),
        )));
    }
    connection.execute(
        "INSERT INTO workspace_state_checkpoints
         (checkpoint_id,workspace_id,task_id,snapshot_hash,manifest_json,created_at_ms,pinned)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            record.checkpoint_id,
            record.workspace_id,
            record.task_id,
            record.snapshot_hash,
            record.manifest_json,
            record.created_at_ms,
            i64::from(record.pinned)
        ],
    )?;
    Ok(())
}

pub fn get_checkpoint(
    connection: &Connection,
    checkpoint_id: &str,
) -> rusqlite::Result<Option<WorkspaceCheckpointRecord>> {
    connection
        .query_row(
            "SELECT checkpoint_id,workspace_id,task_id,snapshot_hash,manifest_json,created_at_ms,pinned
             FROM workspace_state_checkpoints WHERE checkpoint_id=?1",
            [checkpoint_id],
            |row| {
                Ok(WorkspaceCheckpointRecord {
                    checkpoint_id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    task_id: row.get(2)?,
                    snapshot_hash: row.get(3)?,
                    manifest_json: row.get(4)?,
                    created_at_ms: row.get(5)?,
                    pinned: row.get::<_, i64>(6)? != 0,
                })
            },
        )
        .optional()
}

pub fn list_checkpoint_summaries(
    connection: &Connection,
    workspace_id: &str,
) -> rusqlite::Result<Vec<WorkspaceCheckpointSummary>> {
    let mut statement = connection.prepare(
        "SELECT checkpoint_id,task_id,snapshot_hash,created_at_ms,pinned
         FROM workspace_state_checkpoints
         WHERE workspace_id=?1 ORDER BY created_at_ms DESC LIMIT 256",
    )?;
    let rows = statement.query_map([workspace_id], |row| {
        Ok(WorkspaceCheckpointSummary {
            checkpoint_id: row.get(0)?,
            task_id: row.get(1)?,
            snapshot_hash: row.get(2)?,
            created_at_ms: row.get(3)?,
            pinned: row.get::<_, i64>(4)? != 0,
        })
    })?;
    rows.collect()
}

pub fn append_restore_journal(
    connection: &Connection,
    record: &RestoreJournalRecord,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO workspace_state_restore_journal
         (operation_id,checkpoint_id,operation,state,detail_json,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            record.operation_id,
            record.checkpoint_id,
            record.operation,
            record.state,
            record.detail_json,
            record.created_at_ms
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_and_restore_journal_round_trip() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let record = WorkspaceCheckpointRecord {
            checkpoint_id: "cp-1".into(),
            workspace_id: "ws-1".into(),
            task_id: Some("t-1".into()),
            snapshot_hash: "a".repeat(64),
            manifest_json: br#"{"version":1}"#.to_vec(),
            created_at_ms: 1,
            pinned: false,
        };
        insert_checkpoint(&connection, &record).unwrap();
        assert_eq!(get_checkpoint(&connection, "cp-1").unwrap(), Some(record));
        append_restore_journal(
            &connection,
            &RestoreJournalRecord {
                operation_id: "op-1".into(),
                checkpoint_id: "cp-1".into(),
                operation: "workspace".into(),
                state: "completed".into(),
                detail_json: b"{}".to_vec(),
                created_at_ms: 2,
            },
        )
        .unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM workspace_state_restore_journal",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
}
