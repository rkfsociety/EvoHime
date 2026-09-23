use crate::{LocalDatabase, RunCheckpointRecord, StorageError};
use rusqlite::OptionalExtension;

impl LocalDatabase {
    /// Persists a checkpoint snapshot for a workflow run.
    ///
    /// Checkpoints retain the run stage, attempt, input hash, serialized state,
    /// pending effects, and commit time so recovery can resume from a known
    /// boundary.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the checkpoint cannot be inserted.
    pub fn create_checkpoint(
        &self,
        checkpoint: &RunCheckpointRecord,
    ) -> Result<RunCheckpointRecord, StorageError> {
        self.connection.execute(
            "INSERT INTO run_checkpoints(run_id, checkpoint_id, stage, node_id, attempt, input_hash,
             state_json, pending_effects_json, committed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                checkpoint.run_id,
                checkpoint.checkpoint_id,
                checkpoint.stage,
                checkpoint.node_id,
                checkpoint.attempt,
                checkpoint.input_hash,
                checkpoint.state_json,
                checkpoint.pending_effects_json,
                checkpoint.committed_at
            ],
        )?;
        Ok(checkpoint.clone())
    }

    /// Returns the most recently inserted checkpoint for `run_id`, if present.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the checkpoint query fails.
    pub fn latest_checkpoint(
        &self,
        run_id: &str,
    ) -> Result<Option<RunCheckpointRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT run_id, checkpoint_id, stage, node_id, attempt, input_hash, state_json,
             pending_effects_json, committed_at FROM run_checkpoints
             WHERE run_id = ?1 ORDER BY rowid DESC LIMIT 1",
        )?;
        Ok(statement
            .query_row([run_id], |row| {
                Ok(RunCheckpointRecord {
                    run_id: row.get(0)?,
                    checkpoint_id: row.get(1)?,
                    stage: row.get(2)?,
                    node_id: row.get(3)?,
                    attempt: row.get(4)?,
                    input_hash: row.get(5)?,
                    state_json: row.get(6)?,
                    pending_effects_json: row.get(7)?,
                    committed_at: row.get(8)?,
                })
            })
            .optional()?)
    }
}
