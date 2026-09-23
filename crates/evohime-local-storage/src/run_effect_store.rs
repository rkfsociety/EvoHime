use crate::{LocalDatabase, RunCheckpointRecord, RunEffectRecord, RunRecord, StorageError};
use rusqlite::OptionalExtension;

impl LocalDatabase {
    /// Atomically ensures the run, checkpoint, and effect exist before returning the stored effect.
    pub fn prepare_run_effect(
        &self,
        run: &RunRecord,
        checkpoint: &RunCheckpointRecord,
        effect: &RunEffectRecord,
    ) -> Result<RunEffectRecord, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO runs(id, work_item_id, status, policy_snapshot, role_snapshot,
             skill_snapshot, model_route_snapshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                run.id,
                run.work_item_id,
                run.status,
                run.policy_snapshot,
                run.role_snapshot,
                run.skill_snapshot,
                run.model_route_snapshot
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO run_checkpoints(run_id, checkpoint_id, stage, node_id, attempt,
             input_hash, state_json, pending_effects_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                checkpoint.run_id,
                checkpoint.checkpoint_id,
                checkpoint.stage,
                checkpoint.node_id,
                checkpoint.attempt,
                checkpoint.input_hash,
                checkpoint.state_json,
                checkpoint.pending_effects_json
            ],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO run_effects(effect_id, run_id, node_id, kind, idempotency_key,
             immutable_intent_hash, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                effect.effect_id,
                effect.run_id,
                effect.node_id,
                effect.kind,
                effect.idempotency_key,
                effect.immutable_intent_hash,
                effect.state
            ],
        )?;
        transaction.commit()?;
        self.get_run_effect(&effect.effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Loads a durable run effect by its stable effect ID.
    pub fn get_run_effect(&self, effect_id: &str) -> Result<Option<RunEffectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT effect_id, run_id, node_id, kind, idempotency_key, immutable_intent_hash,
             state, started_at, completed_at, result_hash FROM run_effects WHERE effect_id = ?1",
        )?;
        Ok(statement
            .query_row([effect_id], |row| {
                Ok(RunEffectRecord {
                    effect_id: row.get(0)?,
                    run_id: row.get(1)?,
                    node_id: row.get(2)?,
                    kind: row.get(3)?,
                    idempotency_key: row.get(4)?,
                    immutable_intent_hash: row.get(5)?,
                    state: row.get(6)?,
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    result_hash: row.get(9)?,
                })
            })
            .optional()?)
    }

    /// Moves a prepared run effect into execution and returns its persisted state.
    pub fn mark_effect_executing(&self, effect_id: &str) -> Result<RunEffectRecord, StorageError> {
        self.connection.execute(
            "UPDATE run_effects SET state = 'executing', started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE effect_id = ?1 AND state = 'prepared'",
            [effect_id],
        )?;
        self.get_run_effect(effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Completes an executing effect with success/failure and an optional result hash.
    pub fn complete_run_effect(
        &self,
        effect_id: &str,
        success: bool,
        result_hash: Option<&str>,
    ) -> Result<RunEffectRecord, StorageError> {
        let state = if success {
            "completed_success"
        } else {
            "completed_failure"
        };
        self.connection.execute(
            "UPDATE run_effects SET state = ?1, completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), result_hash = ?2
             WHERE effect_id = ?3 AND state = 'executing'",
            rusqlite::params![state, result_hash, effect_id],
        )?;
        self.get_run_effect(effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Stores an agent effect once and returns the persisted record.
    pub fn prepare_agent_run_effect(
        &self,
        effect: &RunEffectRecord,
        task_id: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO agent_run_effects(
                effect_id, run_id, task_id, node_id, kind, idempotency_key,
                immutable_intent_hash, state
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                effect.effect_id,
                effect.run_id,
                task_id,
                effect.node_id,
                effect.kind,
                effect.idempotency_key,
                effect.immutable_intent_hash,
                effect.state,
            ],
        )?;
        transaction.commit()?;
        self.get_agent_run_effect(&effect.effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Loads a durable agent effect by its stable effect ID.
    pub fn get_agent_run_effect(
        &self,
        effect_id: &str,
    ) -> Result<Option<RunEffectRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT effect_id, run_id, node_id, kind, idempotency_key,
             immutable_intent_hash, state, started_at, completed_at, result_hash
             FROM agent_run_effects WHERE effect_id = ?1",
        )?;
        Ok(statement
            .query_row([effect_id], |row| {
                Ok(RunEffectRecord {
                    effect_id: row.get(0)?,
                    run_id: row.get(1)?,
                    node_id: row.get(2)?,
                    kind: row.get(3)?,
                    idempotency_key: row.get(4)?,
                    immutable_intent_hash: row.get(5)?,
                    state: row.get(6)?,
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                    result_hash: row.get(9)?,
                })
            })
            .optional()?)
    }

    /// Moves a prepared agent effect into execution and returns its persisted state.
    pub fn mark_agent_effect_executing(
        &self,
        effect_id: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        self.connection.execute(
            "UPDATE agent_run_effects SET state = 'executing',
             started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE effect_id = ?1 AND state = 'prepared'",
            [effect_id],
        )?;
        self.get_agent_run_effect(effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }

    /// Completes an executing agent effect with success/failure and an optional result hash.
    pub fn complete_agent_run_effect(
        &self,
        effect_id: &str,
        success: bool,
        result_hash: Option<&str>,
    ) -> Result<RunEffectRecord, StorageError> {
        let state = if success {
            "completed_success"
        } else {
            "completed_failure"
        };
        self.connection.execute(
            "UPDATE agent_run_effects SET state = ?1,
             completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), result_hash = ?2
             WHERE effect_id = ?3 AND state = 'executing'",
            rusqlite::params![state, result_hash, effect_id],
        )?;
        self.get_agent_run_effect(effect_id)?
            .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows.into())
    }
}
