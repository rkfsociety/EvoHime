use crate::{
    LocalDatabase, RecoveryState, RecoveryTransitionInput, RunRecoveryRecord, StorageError,
};
use rusqlite::OptionalExtension;

impl LocalDatabase {
    /// Persists the next verified recovery decision for a workflow run.
    ///
    /// Transitions follow `Recovering → Reconciling → terminal`; each record
    /// includes bounded verifier/evidence data and an idempotency key. A
    /// matching replay returns the original record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] for invalid transitions, conflicting key reuse,
    /// invalid fields, or transaction failure.
    pub fn transition_recovery(
        &self,
        input: RecoveryTransitionInput<'_>,
    ) -> Result<RunRecoveryRecord, StorageError> {
        let RecoveryTransitionInput {
            run_id,
            next,
            effect_id,
            idempotency_key,
            verifier,
            evidence_json,
            decision,
        } = input;
        const MAX_TEXT: usize = 256;
        const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
        for (field, value) in [
            ("run_id", run_id),
            ("effect_id", effect_id),
            ("idempotency_key", idempotency_key),
            ("verifier", verifier),
            ("decision", decision),
        ] {
            if value.trim().is_empty() || value.chars().count() > MAX_TEXT {
                return Err(StorageError::InvalidRecovery(format!(
                    "{field} is empty or exceeds {MAX_TEXT} characters"
                )));
            }
        }
        if evidence_json.len() > MAX_EVIDENCE_BYTES {
            return Err(StorageError::InvalidRecovery(format!(
                "evidence exceeds {MAX_EVIDENCE_BYTES} bytes"
            )));
        }

        let transaction = self.connection.unchecked_transaction()?;
        let current = transaction
            .query_row(
                "SELECT id, run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision, created_at
                 FROM run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
                [run_id],
                |row| {
                    Ok(RunRecoveryRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        state: RecoveryState::parse(&row.get::<_, String>(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
                        effect_id: row.get(3)?,
                        idempotency_key: row.get(4)?,
                        verifier: row.get(5)?,
                        evidence_json: row.get(6)?,
                        decision: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()?;
        if let Some(record) = &current {
            if record.idempotency_key == idempotency_key {
                if record.state == next {
                    transaction.commit()?;
                    return Ok(record.clone());
                }
                return Err(StorageError::InvalidRecovery(format!(
                    "idempotency key {} was already used for {:?}",
                    idempotency_key, record.state
                )));
            }
        }
        let current_state = current.as_ref().map(|record| record.state);
        let valid = match (current_state, next) {
            (None, RecoveryState::Recovering) => true,
            (Some(RecoveryState::Recovering), RecoveryState::Reconciling) => true,
            (Some(RecoveryState::Reconciling), state) if state.is_terminal() => true,
            (Some(state), next) if state == next && state.is_terminal() => true,
            _ => false,
        };
        if !valid {
            return Err(StorageError::InvalidRecovery(format!(
                "cannot transition from {:?} to {:?}",
                current_state, next
            )));
        }

        transaction.execute(
            "INSERT INTO run_recovery(run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                run_id,
                next.as_str(),
                effect_id,
                idempotency_key,
                verifier,
                evidence_json,
                decision,
            ],
        )?;
        let work_item_id: String = transaction.query_row(
            "SELECT work_item_id FROM runs WHERE id = ?1",
            [run_id],
            |row| row.get(0),
        )?;
        let payload = serde_json::to_vec(&serde_json::json!({
            "run_id": run_id,
            "effect_id": effect_id,
            "idempotency_key": idempotency_key,
            "verifier": verifier,
            "evidence": serde_json::from_slice::<serde_json::Value>(evidence_json)
                .unwrap_or_else(|_| serde_json::json!({"raw_bytes": evidence_json})),
            "decision": decision,
            "state": next.as_str(),
        }))?;
        transaction.execute(
            "INSERT INTO events(task_id, event_type, payload) VALUES (?1, 'run.recovery.decision', ?2)",
            rusqlite::params![work_item_id, payload],
        )?;
        let record = transaction.query_row(
            "SELECT id, run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision, created_at
             FROM run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
            [run_id],
            |row| {
                Ok(RunRecoveryRecord {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    state: RecoveryState::parse(&row.get::<_, String>(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
                    effect_id: row.get(3)?,
                    idempotency_key: row.get(4)?,
                    verifier: row.get(5)?,
                    evidence_json: row.get(6)?,
                    decision: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )?;
        transaction.commit()?;
        Ok(record)
    }

    /// Persists a verified recovery decision for an agent-run effect.
    ///
    /// Uses the agent recovery ledger and the same bounded, idempotent state
    /// transitions as [`Self::transition_recovery`].
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] for invalid transitions, conflicting key reuse,
    /// invalid fields, or transaction failure.
    pub fn transition_agent_recovery(
        &self,
        input: RecoveryTransitionInput<'_>,
    ) -> Result<RunRecoveryRecord, StorageError> {
        let RecoveryTransitionInput {
            run_id,
            next,
            effect_id,
            idempotency_key,
            verifier,
            evidence_json,
            decision,
        } = input;
        const MAX_TEXT: usize = 256;
        const MAX_EVIDENCE_BYTES: usize = 64 * 1024;
        for (field, value) in [
            ("run_id", run_id),
            ("effect_id", effect_id),
            ("idempotency_key", idempotency_key),
            ("verifier", verifier),
            ("decision", decision),
        ] {
            if value.trim().is_empty() || value.chars().count() > MAX_TEXT {
                return Err(StorageError::InvalidRecovery(format!(
                    "{field} is empty or exceeds {MAX_TEXT} characters"
                )));
            }
        }
        if evidence_json.len() > MAX_EVIDENCE_BYTES {
            return Err(StorageError::InvalidRecovery(format!(
                "evidence exceeds {MAX_EVIDENCE_BYTES} bytes"
            )));
        }

        let transaction = self.connection.unchecked_transaction()?;
        let current = transaction
            .query_row(
                "SELECT id, run_id, state, effect_id, idempotency_key, verifier,
                 evidence_json, decision, created_at
                 FROM agent_run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
                [run_id],
                |row| {
                    Ok(RunRecoveryRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        state: RecoveryState::parse(&row.get::<_, String>(2)?)
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        effect_id: row.get(3)?,
                        idempotency_key: row.get(4)?,
                        verifier: row.get(5)?,
                        evidence_json: row.get(6)?,
                        decision: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()?;
        if let Some(record) = &current {
            if record.idempotency_key == idempotency_key {
                if record.state == next {
                    transaction.commit()?;
                    return Ok(record.clone());
                }
                return Err(StorageError::InvalidRecovery(format!(
                    "idempotency key {} was already used for {:?}",
                    idempotency_key, record.state
                )));
            }
        }
        let current_state = current.as_ref().map(|record| record.state);
        let valid = match (current_state, next) {
            (None, RecoveryState::Recovering) => true,
            (Some(RecoveryState::Recovering), RecoveryState::Reconciling) => true,
            (Some(RecoveryState::Reconciling), state) if state.is_terminal() => true,
            (Some(state), next) if state == next && state.is_terminal() => true,
            _ => false,
        };
        if !valid {
            return Err(StorageError::InvalidRecovery(format!(
                "cannot transition agent run from {:?} to {:?}",
                current_state, next
            )));
        }

        transaction.execute(
            "INSERT INTO agent_run_recovery(
                run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                run_id,
                next.as_str(),
                effect_id,
                idempotency_key,
                verifier,
                evidence_json,
                decision,
            ],
        )?;
        let task_id: String = transaction.query_row(
            "SELECT task_id FROM agent_run_effects WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )?;
        let payload = serde_json::to_vec(&serde_json::json!({
            "run_id": run_id,
            "effect_id": effect_id,
            "idempotency_key": idempotency_key,
            "verifier": verifier,
            "evidence": serde_json::from_slice::<serde_json::Value>(evidence_json)
                .unwrap_or_else(|_| serde_json::json!({"raw_bytes": evidence_json})),
            "decision": decision,
            "state": next.as_str(),
        }))?;
        transaction.execute(
            "INSERT INTO events(task_id, event_type, payload) VALUES (?1, 'run.recovery.decision', ?2)",
            rusqlite::params![task_id, payload],
        )?;
        let record = transaction.query_row(
            "SELECT id, run_id, state, effect_id, idempotency_key, verifier,
             evidence_json, decision, created_at
             FROM agent_run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
            [run_id],
            |row| {
                Ok(RunRecoveryRecord {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    state: RecoveryState::parse(&row.get::<_, String>(2)?)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    effect_id: row.get(3)?,
                    idempotency_key: row.get(4)?,
                    verifier: row.get(5)?,
                    evidence_json: row.get(6)?,
                    decision: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )?;
        transaction.commit()?;
        Ok(record)
    }

    /// Returns the latest agent-effect recovery record for a run, if present.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the SQLite query fails.
    pub fn latest_agent_recovery(
        &self,
        run_id: &str,
    ) -> Result<Option<RunRecoveryRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, run_id, state, effect_id, idempotency_key, verifier,
                 evidence_json, decision, created_at
                 FROM agent_run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
                [run_id],
                |row| {
                    Ok(RunRecoveryRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        state: RecoveryState::parse(&row.get::<_, String>(2)?)
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        effect_id: row.get(3)?,
                        idempotency_key: row.get(4)?,
                        verifier: row.get(5)?,
                        evidence_json: row.get(6)?,
                        decision: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Returns the latest workflow-effect recovery record for a run, if present.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if the SQLite query fails.
    pub fn latest_recovery(&self, run_id: &str) -> Result<Option<RunRecoveryRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, run_id, state, effect_id, idempotency_key, verifier, evidence_json, decision, created_at
                 FROM run_recovery WHERE run_id = ?1 ORDER BY id DESC LIMIT 1",
                [run_id],
                |row| {
                    Ok(RunRecoveryRecord {
                        id: row.get(0)?,
                        run_id: row.get(1)?,
                        state: RecoveryState::parse(&row.get::<_, String>(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
                        effect_id: row.get(3)?,
                        idempotency_key: row.get(4)?,
                        verifier: row.get(5)?,
                        evidence_json: row.get(6)?,
                        decision: row.get(7)?,
                        created_at: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}
