use crate::{LocalDatabase, RunReconciliationRecord, StorageError};

impl LocalDatabase {
    /// Records the verified outcome of an agent-run side effect.
    ///
    /// The first reconciliation is stored transactionally. A successful
    /// reconciliation advances an `unknown` effect to `completed_success`;
    /// a blocked outcome leaves it unresolved. Repeating the operation returns
    /// the existing reconciliation record without replacing its evidence.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the database transaction or record lookup
    /// fails.
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

    /// Records the verified outcome of a workflow-run side effect.
    ///
    /// The first reconciliation is stored transactionally. A successful
    /// reconciliation advances an `unknown` effect to `completed_success`;
    /// a blocked outcome remains unresolved. Existing evidence is preserved
    /// when the same effect is reconciled again.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the database transaction or record lookup
    /// fails.
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
