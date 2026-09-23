use crate::{LocalDatabase, RunLeaseRecord, StorageError};
use rusqlite::OptionalExtension;

impl LocalDatabase {
    /// Acquires a run lease, replacing an expired lease or renewing the same lease ID.
    pub fn acquire_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
        ttl_seconds: u64,
    ) -> Result<RunLeaseRecord, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO run_leases(run_id, lease_id, owner_id, generation, lease_expires_at, heartbeat_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now', '+' || ?5 || ' seconds'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            rusqlite::params![run_id, lease_id, owner_id, generation, ttl_seconds as i64],
        )?;
        let updated = transaction.execute(
            "UPDATE run_leases SET lease_id = ?1, owner_id = ?2, generation = ?3,
             lease_expires_at = datetime('now', '+' || ?4 || ' seconds'),
             heartbeat_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE run_id = ?5 AND (lease_id = ?1 OR lease_expires_at <= datetime('now'))",
            rusqlite::params![lease_id, owner_id, generation, ttl_seconds as i64, run_id],
        )?;
        if updated == 0 {
            return Err(StorageError::InvalidRunEffect(
                "run lease is held by another owner".into(),
            ));
        }
        transaction.commit()?;
        self.get_run_lease(run_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Loads the current lease for a run, if present.
    pub fn get_run_lease(&self, run_id: &str) -> Result<Option<RunLeaseRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT run_id, lease_id, owner_id, generation, lease_expires_at, heartbeat_at
                 FROM run_leases WHERE run_id = ?1",
                [run_id],
                |row| {
                    Ok(RunLeaseRecord {
                        run_id: row.get(0)?,
                        lease_id: row.get(1)?,
                        owner_id: row.get(2)?,
                        generation: row.get(3)?,
                        lease_expires_at: row.get(4)?,
                        heartbeat_at: row.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    /// Renews an unexpired run lease when its ID, owner, and generation all match.
    pub fn heartbeat_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
        ttl_seconds: u64,
    ) -> Result<RunLeaseRecord, StorageError> {
        let changed = self.connection.execute(
            "UPDATE run_leases SET lease_expires_at = datetime('now', '+' || ?1 || ' seconds'), heartbeat_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE run_id = ?2 AND lease_id = ?3 AND owner_id = ?4 AND generation = ?5 AND lease_expires_at > datetime('now')",
            rusqlite::params![ttl_seconds as i64, run_id, lease_id, owner_id, generation],
        )?;
        if changed == 0 {
            return Err(StorageError::InvalidRunEffect(
                "run lease heartbeat rejected".into(),
            ));
        }
        self.get_run_lease(run_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Releases a run lease only when its ID, owner, and generation match.
    pub fn release_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM run_leases WHERE run_id = ?1 AND lease_id = ?2 AND owner_id = ?3 AND generation = ?4",
            rusqlite::params![run_id, lease_id, owner_id, generation],
        )?;
        Ok(())
    }

    /// Acquires an agent-run lease, replacing an expired lease or renewing the same lease ID.
    pub fn acquire_agent_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
        ttl_seconds: u64,
    ) -> Result<RunLeaseRecord, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO agent_run_leases(
                run_id, lease_id, owner_id, generation, lease_expires_at, heartbeat_at
             ) VALUES (?1, ?2, ?3, ?4, datetime('now', '+' || ?5 || ' seconds'),
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            rusqlite::params![run_id, lease_id, owner_id, generation, ttl_seconds as i64],
        )?;
        let updated = transaction.execute(
            "UPDATE agent_run_leases SET lease_id = ?1, owner_id = ?2, generation = ?3,
             lease_expires_at = datetime('now', '+' || ?4 || ' seconds'),
             heartbeat_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE run_id = ?5 AND (lease_id = ?1 OR lease_expires_at <= datetime('now'))",
            rusqlite::params![lease_id, owner_id, generation, ttl_seconds as i64, run_id],
        )?;
        if updated == 0 {
            return Err(StorageError::InvalidRunEffect(
                "agent run lease is held by another owner".into(),
            ));
        }
        transaction.commit()?;
        self.get_agent_run_lease(run_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Loads the current agent-run lease, if present.
    pub fn get_agent_run_lease(
        &self,
        run_id: &str,
    ) -> Result<Option<RunLeaseRecord>, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT run_id, lease_id, owner_id, generation, lease_expires_at, heartbeat_at
                 FROM agent_run_leases WHERE run_id = ?1",
                [run_id],
                |row| {
                    Ok(RunLeaseRecord {
                        run_id: row.get(0)?,
                        lease_id: row.get(1)?,
                        owner_id: row.get(2)?,
                        generation: row.get(3)?,
                        lease_expires_at: row.get(4)?,
                        heartbeat_at: row.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    /// Renews an unexpired agent-run lease when its fencing fields all match.
    pub fn heartbeat_agent_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
        ttl_seconds: u64,
    ) -> Result<RunLeaseRecord, StorageError> {
        let changed = self.connection.execute(
            "UPDATE agent_run_leases SET lease_expires_at = datetime('now', '+' || ?1 || ' seconds'),
             heartbeat_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE run_id = ?2 AND lease_id = ?3 AND owner_id = ?4 AND generation = ?5
             AND lease_expires_at > datetime('now')",
            rusqlite::params![ttl_seconds as i64, run_id, lease_id, owner_id, generation],
        )?;
        if changed == 0 {
            return Err(StorageError::InvalidRunEffect(
                "agent run lease heartbeat rejected".into(),
            ));
        }
        self.get_agent_run_lease(run_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Releases an agent-run lease only when its fencing fields match.
    pub fn release_agent_run_lease(
        &self,
        run_id: &str,
        lease_id: &str,
        owner_id: &str,
        generation: u64,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM agent_run_leases
             WHERE run_id = ?1 AND lease_id = ?2 AND owner_id = ?3 AND generation = ?4",
            rusqlite::params![run_id, lease_id, owner_id, generation],
        )?;
        Ok(())
    }
}
