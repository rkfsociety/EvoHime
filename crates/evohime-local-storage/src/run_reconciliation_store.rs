use crate::{LocalDatabase, RunReconciliationRecord, StorageError};

impl LocalDatabase {
    pub fn reconcile_agent_run_effect(
        &self,
        effect_id: &str,
        success: bool,
        verifier: &str,
        evidence_json: &[u8],
    ) -> Result<RunReconciliationRecord, StorageError> {
        let state = if success {
            "reconciled_success"
        } else {
            "reconciled_blocked"
        };
        let transaction = self.connection.unchecked_transaction()?;
        let inserted = transaction.execute(
            "INSERT INTO agent_run_reconciliations(
                effect_id, state, verifier, evidence_json
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(effect_id) DO NOTHING",
            rusqlite::params![effect_id, state, verifier, evidence_json],
        )? == 1;
        if inserted && success {
            transaction.execute(
                "UPDATE agent_run_effects SET state = 'completed_success',
                 result_hash = ?1 WHERE effect_id = ?2 AND state = 'unknown'",
                rusqlite::params![verifier, effect_id],
            )?;
        }
        transaction.commit()?;
        self.connection
            .query_row(
                "SELECT effect_id, state, verifier, evidence_json, reconciled_at
                 FROM agent_run_reconciliations WHERE effect_id = ?1",
                [effect_id],
                |row| {
                    Ok(RunReconciliationRecord {
                        effect_id: row.get(0)?,
                        state: row.get(1)?,
                        verifier: row.get(2)?,
                        evidence_json: row.get(3)?,
                        reconciled_at: row.get(4)?,
                    })
                },
            )
            .map_err(Into::into)
    }

    pub fn reconcile_run_effect(
        &self,
        effect_id: &str,
        success: bool,
        verifier: &str,
        evidence_json: &[u8],
    ) -> Result<RunReconciliationRecord, StorageError> {
        let state = if success {
            "reconciled_success"
        } else {
            "reconciled_blocked"
        };
        let transaction = self.connection.unchecked_transaction()?;
        let inserted = transaction.execute(
            "INSERT INTO run_reconciliations(effect_id, state, verifier, evidence_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(effect_id) DO NOTHING",
            rusqlite::params![effect_id, state, verifier, evidence_json],
        )? == 1;
        if inserted && success {
            transaction.execute(
                "UPDATE run_effects SET state = 'completed_success', result_hash = ?1
                 WHERE effect_id = ?2 AND state = 'unknown'",
                rusqlite::params![verifier, effect_id],
            )?;
        }
        transaction.commit()?;
        self.connection
            .query_row(
                "SELECT effect_id, state, verifier, evidence_json, reconciled_at
                 FROM run_reconciliations WHERE effect_id = ?1",
                [effect_id],
                |row| {
                    Ok(RunReconciliationRecord {
                        effect_id: row.get(0)?,
                        state: row.get(1)?,
                        verifier: row.get(2)?,
                        evidence_json: row.get(3)?,
                        reconciled_at: row.get(4)?,
                    })
                },
            )
            .map_err(Into::into)
    }
}
