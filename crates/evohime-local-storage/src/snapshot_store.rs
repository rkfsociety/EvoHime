use rusqlite::OptionalExtension;

use crate::{LocalDatabase, SnapshotRecord, StorageError};

impl LocalDatabase {
    /// Persists a run snapshot and returns the stored record.
    ///
    /// `workspace_hash` binds the payload to the workspace state captured by
    /// the caller.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if insertion or readback fails.
    pub fn save_snapshot(
        &self,
        id: &str,
        run_id: &str,
        workspace_hash: &str,
        payload: &[u8],
    ) -> Result<SnapshotRecord, StorageError> {
        self.connection.execute(
            "INSERT INTO snapshots(id, run_id, workspace_hash, payload) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, run_id, workspace_hash, payload],
        )?;
        self.get_snapshot(id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Returns a snapshot by ID, or `None` when it does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the database query fails.
    pub fn get_snapshot(&self, id: &str) -> Result<Option<SnapshotRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, run_id, workspace_hash, payload, created_at FROM snapshots WHERE id = ?1",
                [id],
                |row| {
                    Ok(SnapshotRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        workspace_hash: row.get(2)?,
                        payload: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Returns the newest snapshot belonging to a task's run, if one exists.
    ///
    /// Ties use snapshot ID as a deterministic secondary order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the database query fails.
    pub fn latest_snapshot_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<SnapshotRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT s.id, s.run_id, s.workspace_hash, s.payload, s.created_at
             FROM snapshots s JOIN runs r ON r.id = s.run_id
             WHERE r.work_item_id = ?1 ORDER BY s.created_at DESC, s.id DESC LIMIT 1",
        )?;
        Ok(statement
            .query_row([task_id], |row| {
                Ok(SnapshotRecord {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    workspace_hash: row.get(2)?,
                    payload: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .optional()?)
    }
}
