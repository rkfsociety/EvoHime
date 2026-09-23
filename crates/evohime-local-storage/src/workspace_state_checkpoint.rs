//! Durable metadata for workspace-file checkpoints (plan 58).
//!
//! Snapshot bytes are kept in ArtifactStore. This store contains only bounded,
//! immutable manifests and the journal of restore intents/results.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Current version of the workspace checkpoint persistence schema.
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;
/// Maximum encoded size accepted for a checkpoint manifest.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

/// Persisted checkpoint metadata pointing to a snapshot held by the artifact store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCheckpointRecord {
    /// Stable identifier for this checkpoint.
    pub checkpoint_id: String,
    /// Workspace that owns the checkpoint.
    pub workspace_id: String,
    /// Optional task associated with the checkpoint.
    pub task_id: Option<String>,
    /// Content hash of the checkpoint snapshot.
    pub snapshot_hash: String,
    /// Serialized checkpoint manifest, bounded by [`MAX_MANIFEST_BYTES`].
    pub manifest_json: Vec<u8>,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: i64,
    /// Whether retention policy must preserve this checkpoint.
    pub pinned: bool,
}

/// Compact checkpoint metadata returned by workspace listing queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCheckpointSummary {
    /// Stable identifier for this checkpoint.
    pub checkpoint_id: String,
    /// Optional task associated with the checkpoint.
    pub task_id: Option<String>,
    /// Content hash of the checkpoint snapshot.
    pub snapshot_hash: String,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: i64,
    /// Whether retention policy must preserve this checkpoint.
    pub pinned: bool,
}

/// Durable journal entry describing a checkpoint restore operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreJournalRecord {
    /// Unique identifier for this restore operation.
    pub operation_id: String,
    /// Checkpoint the operation refers to.
    pub checkpoint_id: String,
    /// Operation kind recorded by the caller.
    pub operation: String,
    /// Current outcome or lifecycle state.
    pub state: String,
    /// Serialized operation details.
    pub detail_json: Vec<u8>,
    /// Record creation timestamp in Unix milliseconds.
    pub created_at_ms: i64,
}

/// Creates checkpoint and restore-journal tables and their lookup indexes if absent.
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

/// Inserts a checkpoint manifest, rejecting manifests larger than [`MAX_MANIFEST_BYTES`].
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

/// Loads a checkpoint by identifier, returning `None` when it does not exist.
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

/// Lists at most 256 checkpoint summaries for a workspace, newest first.
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

/// Appends a restore operation record to the journal.
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
#[path = "workspace_state_checkpoint_tests.rs"]
mod tests;
