use crate::{LocalDatabase, RecoveredRunRecord, StorageError};

impl LocalDatabase {
    pub fn update_run_status(&self, run_id: &str, status: &str) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE runs SET status = ?1 WHERE id = ?2",
            rusqlite::params![status, run_id],
        )?;
        Ok(())
    }

    pub fn recover_unknown_effects(&self) -> Result<Vec<RecoveredRunRecord>, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut statement = transaction.prepare(
            "SELECT e.run_id, r.work_item_id, e.effect_id, e.kind FROM run_effects e
             JOIN runs r ON r.id = e.run_id WHERE e.state = 'executing'",
        )?;
        let mut records = statement
            .query_map([], |row| {
                Ok(RecoveredRunRecord {
                    run_id: row.get(0)?,
                    work_item_id: row.get(1)?,
                    effect_id: row.get(2)?,
                    kind: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let mut agent_statement = transaction.prepare(
            "SELECT run_id, task_id, effect_id, kind FROM agent_run_effects
             WHERE state = 'executing'",
        )?;
        records.extend(
            agent_statement
                .query_map([], |row| {
                    Ok(RecoveredRunRecord {
                        run_id: row.get(0)?,
                        work_item_id: row.get(1)?,
                        effect_id: row.get(2)?,
                        kind: row.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?,
        );
        drop(agent_statement);
        for record in &records {
            if record.kind == "agent_task" {
                transaction.execute(
                    "DELETE FROM agent_run_leases WHERE run_id = ?1",
                    [&record.run_id],
                )?;
                transaction.execute(
                    "UPDATE agent_run_effects SET state = 'unknown',
                     completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE effect_id = ?1 AND state = 'executing'",
                    [&record.effect_id],
                )?;
            } else {
                transaction
                    .execute("DELETE FROM run_leases WHERE run_id = ?1", [&record.run_id])?;
                transaction.execute(
                    "UPDATE run_effects SET state = 'unknown', completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE effect_id = ?1 AND state = 'executing'",
                    [&record.effect_id],
                )?;
                transaction.execute(
                    "UPDATE runs SET status = 'blocked' WHERE id = ?1 AND status = 'running'",
                    [&record.run_id],
                )?;
            }
            let payload = serde_json::to_vec(&serde_json::json!({
                "run_id": record.run_id, "effect_id": record.effect_id,
                "reason": "recovery_unknown_effect"
            }))?;
            transaction.execute(
                "INSERT INTO events(task_id, event_type, payload) VALUES (?1, 'run.recovery.blocked', ?2)",
                rusqlite::params![record.work_item_id, payload],
            )?;
        }
        transaction.commit()?;
        Ok(records)
    }
}
