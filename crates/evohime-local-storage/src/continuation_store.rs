//! Durable storage for Continuation Policy v1.
//!
//! The store deliberately keeps canonical contract bytes and bounded metadata;
//! it does not interpret model text or execute a gate. Core owns validation and
//! decisions, while this module provides transactional persistence, dedup and
//! budget reservation primitives.

use rusqlite::{params, Connection, OptionalExtension};

/// Immutable continuation policy revision and its scoped owner metadata.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PolicyRecord {
    /// Stable policy identifier.
    pub policy_id: String,
    /// Immutable policy revision.
    pub revision: i64,
    /// Owner scope to which the policy applies.
    pub owner_scope: String,
    /// Actor that created or last updated this revision.
    pub actor: String,
    /// Whether the policy is enabled.
    pub enabled: bool,
    /// Canonical serialized policy definition.
    pub canonical_json: Vec<u8>,
    /// Hash of the canonical policy definition.
    pub content_hash: String,
    /// Revision creation time in milliseconds.
    pub created_at_ms: i64,
    /// Most recent metadata update time in milliseconds.
    pub updated_at_ms: i64,
}

/// Continuation execution state, budgets, and captured policy/goal revisions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RunRecord {
    /// Stable continuation run identifier.
    pub run_id: String,
    /// Key used to deduplicate run creation.
    pub idempotency_key: String,
    /// Task being continued.
    pub task_id: String,
    /// Optional prompt captured for the run.
    pub prompt: Option<String>,
    /// Optional workspace path captured for the run.
    pub workspace_path: Option<String>,
    /// Owner scope of the run.
    pub owner_scope: String,
    /// Policy identifier selected for this run.
    pub policy_id: String,
    /// Policy revision captured by this run.
    pub policy_revision: i64,
    /// Policy content hash captured by this run.
    pub policy_hash: String,
    /// Optional goal associated with the run.
    pub goal_id: Option<String>,
    /// Optional goal version captured by the run.
    pub goal_version: Option<i64>,
    /// Current run lifecycle state.
    pub state: String,
    /// Number of continuation steps already performed.
    pub continuation_index: i64,
    /// Maximum continuation steps allowed.
    pub max_continuations: i64,
    /// Maximum model turns allowed.
    pub max_model_turns: i64,
    /// Model turns consumed so far.
    pub used_model_turns: i64,
    /// Optional token budget.
    pub token_budget: Option<i64>,
    /// Tokens consumed so far.
    pub token_used: i64,
    /// Optional cost budget in micros.
    pub cost_budget_micros: Option<i64>,
    /// Cost consumed so far in micros.
    pub cost_used_micros: i64,
    /// Reason the run stopped, if it has stopped.
    pub stop_reason: Option<String>,
    /// Run creation timestamp in milliseconds.
    pub created_at_ms: i64,
    /// Most recent run update timestamp in milliseconds.
    pub updated_at_ms: i64,
}

/// One persisted gate attempt for a continuation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    /// Continuation run that owns the attempt.
    pub run_id: String,
    /// Attempt sequence number within the run.
    pub attempt_index: i64,
    /// Gate evaluated by the attempt.
    pub gate_id: String,
    /// Fingerprint used to deduplicate equivalent attempts.
    pub fingerprint: String,
    /// Attempt lifecycle state.
    pub state: String,
    /// Serialized gate result payload.
    pub result_json: Vec<u8>,
    /// Attempt creation timestamp in milliseconds.
    pub created_at_ms: i64,
}

/// Gate outcome recorded for one continuation attempt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GateResultRecord {
    /// Continuation run that owns the result.
    pub run_id: String,
    /// Gate that produced the result.
    pub gate_id: String,
    /// Attempt sequence associated with the result.
    pub attempt_index: i64,
    /// Gate outcome status.
    pub status: String,
    /// Optional reference to the evidence supporting the outcome.
    pub evidence_ref: Option<String>,
    /// Optional stable error code when evaluation failed.
    pub error_code: Option<String>,
    /// Result creation timestamp in milliseconds.
    pub created_at_ms: i64,
}

/// Creates continuation policies, runs, actions, attempts, and gate-result tables.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS continuation_policies (
            policy_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            owner_scope TEXT NOT NULL,
            actor TEXT NOT NULL,
            enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
            canonical_json BLOB NOT NULL,
            content_hash TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(policy_id, revision, owner_scope)
        );
        CREATE INDEX IF NOT EXISTS idx_continuation_policies_scope
            ON continuation_policies(owner_scope, policy_id, revision);
        CREATE TABLE IF NOT EXISTS continuation_runs (
            run_id TEXT PRIMARY KEY NOT NULL,
            idempotency_key TEXT NOT NULL,
            task_id TEXT NOT NULL,
            prompt TEXT,
            workspace_path TEXT,
            owner_scope TEXT NOT NULL,
            policy_id TEXT NOT NULL,
            policy_revision INTEGER NOT NULL,
            policy_hash TEXT NOT NULL,
            goal_id TEXT,
            goal_version INTEGER,
            state TEXT NOT NULL,
            continuation_index INTEGER NOT NULL DEFAULT 0 CHECK(continuation_index >= 0),
            max_continuations INTEGER NOT NULL CHECK(max_continuations >= 0),
            max_model_turns INTEGER NOT NULL CHECK(max_model_turns >= 0),
            used_model_turns INTEGER NOT NULL DEFAULT 0 CHECK(used_model_turns >= 0),
            token_budget INTEGER CHECK(token_budget IS NULL OR token_budget >= 0),
            token_used INTEGER NOT NULL DEFAULT 0 CHECK(token_used >= 0),
            cost_budget_micros INTEGER CHECK(cost_budget_micros IS NULL OR cost_budget_micros >= 0),
            cost_used_micros INTEGER NOT NULL DEFAULT 0 CHECK(cost_used_micros >= 0),
            stop_reason TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            UNIQUE(owner_scope, idempotency_key),
            FOREIGN KEY(policy_id, policy_revision, owner_scope)
                REFERENCES continuation_policies(policy_id, revision, owner_scope)
        );
        CREATE INDEX IF NOT EXISTS idx_continuation_runs_scope
            ON continuation_runs(owner_scope, updated_at_ms);
        CREATE TABLE IF NOT EXISTS continuation_attempts (
            run_id TEXT NOT NULL REFERENCES continuation_runs(run_id) ON DELETE CASCADE,
            attempt_index INTEGER NOT NULL CHECK(attempt_index >= 0),
            gate_id TEXT NOT NULL,
            fingerprint TEXT NOT NULL,
            state TEXT NOT NULL,
            result_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(run_id, attempt_index),
            UNIQUE(run_id, fingerprint)
        );
        CREATE INDEX IF NOT EXISTS idx_continuation_attempts_run
            ON continuation_attempts(run_id, created_at_ms);
        CREATE TABLE IF NOT EXISTS continuation_actions (
            run_id TEXT NOT NULL REFERENCES continuation_runs(run_id) ON DELETE CASCADE,
            idempotency_key TEXT NOT NULL,
            action TEXT NOT NULL,
            result_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(run_id, idempotency_key)
        );
        CREATE TABLE IF NOT EXISTS continuation_gate_results (
            run_id TEXT NOT NULL REFERENCES continuation_runs(run_id) ON DELETE CASCADE,
            gate_id TEXT NOT NULL,
            attempt_index INTEGER NOT NULL,
            status TEXT NOT NULL,
            evidence_ref TEXT,
            error_code TEXT,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(run_id, gate_id, attempt_index)
        );
        ",
    )
}

/// Inputs for an idempotent, state-fenced continuation action.
#[derive(Clone, Copy)]
pub struct TransitionActionInput<'a> {
    /// Run to transition.
    pub run_id: &'a str,
    /// Key used to make action retries return the original outcome.
    pub idempotency_key: &'a str,
    /// Action name recorded in the result.
    pub action: &'a str,
    /// State the run must currently have.
    pub expected_state: &'a str,
    /// State to persist when the fence matches.
    pub next_state: &'a str,
    /// Stop reason stored with the new state.
    pub stop_reason: &'a str,
    /// Update timestamp in milliseconds.
    pub now_ms: i64,
}

/// Applies a state-fenced transition exactly once and returns its serialized outcome.
pub fn apply_transition_action(
    connection: &mut Connection,
    input: TransitionActionInput<'_>,
) -> rusqlite::Result<Vec<u8>> {
    let transaction = connection.transaction()?;
    if let Some(result) = transaction
        .query_row(
            "SELECT result_json FROM continuation_actions
             WHERE run_id=?1 AND idempotency_key=?2",
            params![input.run_id, input.idempotency_key],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
    {
        return Ok(result);
    }
    let applied = transaction.execute(
        "UPDATE continuation_runs SET state=?3,stop_reason=?4,updated_at_ms=?5
         WHERE run_id=?1 AND state=?2",
        params![
            input.run_id,
            input.expected_state,
            input.next_state,
            input.stop_reason,
            input.now_ms
        ],
    )? == 1;
    let result = serde_json::to_vec(&serde_json::json!({
        "run_id": input.run_id,
        "action": input.action,
        "applied": applied,
        "deduplicated": false,
        "error_code": if applied { "" } else { "stale_action" }
    }))
    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    transaction.execute(
        "INSERT INTO continuation_actions
         (run_id,idempotency_key,action,result_json,created_at_ms)
         VALUES (?1,?2,?3,?4,?5)",
        params![
            input.run_id,
            input.idempotency_key,
            input.action,
            result,
            input.now_ms
        ],
    )?;
    transaction.commit()?;
    Ok(result)
}

/// Persists a scoped policy revision; conflicting content for the same revision is ignored.
pub fn save_policy(connection: &Connection, record: &PolicyRecord) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO continuation_policies
         (policy_id,revision,owner_scope,actor,enabled,canonical_json,content_hash,created_at_ms,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(policy_id,revision,owner_scope) DO UPDATE SET
           actor=excluded.actor, enabled=excluded.enabled,
           canonical_json=excluded.canonical_json, content_hash=excluded.content_hash,
           updated_at_ms=excluded.updated_at_ms
         WHERE continuation_policies.content_hash=excluded.content_hash",
        params![
            record.policy_id,
            record.revision,
            record.owner_scope,
            record.actor,
            record.enabled,
            record.canonical_json,
            record.content_hash,
            record.created_at_ms,
            record.updated_at_ms,
        ],
    )?;
    Ok(())
}

/// Loads one policy revision by ID, revision, and owner scope.
pub fn get_policy(
    connection: &Connection,
    policy_id: &str,
    revision: i64,
    owner_scope: &str,
) -> rusqlite::Result<Option<PolicyRecord>> {
    connection
        .query_row(
            "SELECT policy_id,revision,owner_scope,actor,enabled,canonical_json,content_hash,
                    created_at_ms,updated_at_ms
             FROM continuation_policies
             WHERE policy_id=?1 AND revision=?2 AND owner_scope=?3",
            params![policy_id, revision, owner_scope],
            |row| {
                Ok(PolicyRecord {
                    policy_id: row.get(0)?,
                    revision: row.get(1)?,
                    owner_scope: row.get(2)?,
                    actor: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    canonical_json: row.get(5)?,
                    content_hash: row.get(6)?,
                    created_at_ms: row.get(7)?,
                    updated_at_ms: row.get(8)?,
                })
            },
        )
        .optional()
}

/// Inserts a continuation run using the run's scoped idempotency key.
pub fn create_run(connection: &Connection, record: &RunRecord) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO continuation_runs
         (run_id,idempotency_key,task_id,prompt,workspace_path,owner_scope,policy_id,policy_revision,policy_hash,goal_id,goal_version,state,
          continuation_index,max_continuations,max_model_turns,used_model_turns,token_budget,
          token_used,cost_budget_micros,cost_used_micros,stop_reason,created_at_ms,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",
        params![
            record.run_id,
            record.idempotency_key,
            record.task_id,
            record.prompt,
            record.workspace_path,
            record.owner_scope,
            record.policy_id,
            record.policy_revision,
            record.policy_hash,
            record.goal_id,
            record.goal_version,
            record.state,
            record.continuation_index,
            record.max_continuations,
            record.max_model_turns,
            record.used_model_turns,
            record.token_budget,
            record.token_used,
            record.cost_budget_micros,
            record.cost_used_micros,
            record.stop_reason,
            record.created_at_ms,
            record.updated_at_ms,
        ],
    )?;
    Ok(())
}

/// Loads a continuation run by its stable ID.
pub fn get_run(connection: &Connection, run_id: &str) -> rusqlite::Result<Option<RunRecord>> {
    connection
        .query_row(
            "SELECT run_id,idempotency_key,task_id,prompt,workspace_path,owner_scope,policy_id,policy_revision,policy_hash,goal_id,goal_version,
                    state,continuation_index,max_continuations,max_model_turns,used_model_turns,
                    token_budget,token_used,cost_budget_micros,cost_used_micros,stop_reason,
                    created_at_ms,updated_at_ms
             FROM continuation_runs WHERE run_id=?1",
            [run_id],
            |row| {
                Ok(RunRecord {
                    run_id: row.get(0)?,
                    idempotency_key: row.get(1)?,
                    task_id: row.get(2)?,
                    prompt: row.get(3)?,
                    workspace_path: row.get(4)?,
                    owner_scope: row.get(5)?,
                    policy_id: row.get(6)?,
                    policy_revision: row.get(7)?,
                    policy_hash: row.get(8)?,
                    goal_id: row.get(9)?,
                    goal_version: row.get(10)?,
                    state: row.get(11)?,
                    continuation_index: row.get(12)?,
                    max_continuations: row.get(13)?,
                    max_model_turns: row.get(14)?,
                    used_model_turns: row.get(15)?,
                    token_budget: row.get(16)?,
                    token_used: row.get(17)?,
                    cost_budget_micros: row.get(18)?,
                    cost_used_micros: row.get(19)?,
                    stop_reason: row.get(20)?,
                    created_at_ms: row.get(21)?,
                    updated_at_ms: row.get(22)?,
                })
            },
        )
        .optional()
}

/// Loads a run by owner scope and idempotency key.
pub fn get_run_by_idempotency(
    connection: &Connection,
    owner_scope: &str,
    idempotency_key: &str,
) -> rusqlite::Result<Option<RunRecord>> {
    let run_id = connection
        .query_row(
            "SELECT run_id FROM continuation_runs
             WHERE owner_scope=?1 AND idempotency_key=?2",
            params![owner_scope, idempotency_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    run_id.map_or(Ok(None), |id| get_run(connection, &id))
}

/// Loads a run associated with the specified task and owner scope.
pub fn get_run_by_task(
    connection: &Connection,
    task_id: &str,
) -> rusqlite::Result<Option<RunRecord>> {
    let run_id = connection
        .query_row(
            "SELECT run_id FROM continuation_runs WHERE task_id=?1 AND state='running'
             ORDER BY updated_at_ms DESC LIMIT 1",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    run_id.map_or(Ok(None), |id| get_run(connection, &id))
}

/// Attaches task prompt/workspace context to a run that has not advanced past its initial state.
pub fn attach_task_context(
    connection: &Connection,
    task_id: &str,
    prompt: &str,
    workspace_path: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "UPDATE continuation_runs SET prompt=?2,workspace_path=?3,updated_at_ms=?4
         WHERE task_id=?1 AND state='running' AND prompt IS NULL",
        params![task_id, prompt, workspace_path, now_ms],
    )?;
    Ok(changed == 1)
}

/// Lists continuation runs currently in a running state.
pub fn list_running_runs(connection: &Connection) -> rusqlite::Result<Vec<RunRecord>> {
    let mut statement = connection.prepare(
        "SELECT run_id FROM continuation_runs WHERE state='running'
         ORDER BY updated_at_ms ASC LIMIT 256",
    )?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.into_iter()
        .map(|id| get_run(connection, &id))
        .try_fold(Vec::new(), |mut runs, run| {
            if let Some(run) = run? {
                runs.push(run);
            }
            Ok(runs)
        })
}

/// Reserves one model turn and bounded resource units in one transaction.
/// The same idempotency fingerprint returns the existing attempt without
/// charging a second time.
pub fn reserve_attempt(
    connection: &mut Connection,
    run_id: &str,
    gate_id: &str,
    fingerprint: &str,
    token_reservation: i64,
    cost_reservation_micros: i64,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    if token_reservation < 0 || cost_reservation_micros < 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "negative reservation".into(),
        ));
    }
    let tx = connection.transaction()?;
    if tx
        .query_row(
            "SELECT 1 FROM continuation_attempts WHERE run_id=?1 AND fingerprint=?2",
            params![run_id, fingerprint],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(false);
    }
    let current: (i64, i64, i64, Option<i64>, i64, Option<i64>, i64, String) = tx.query_row(
        "SELECT continuation_index,used_model_turns,max_model_turns,token_budget,token_used,
                cost_budget_micros,cost_used_micros,state FROM continuation_runs WHERE run_id=?1",
        [run_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        },
    )?;
    if current.7 != "running" {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if current.1 >= current.2
        || current
            .3
            .is_some_and(|budget| current.4.saturating_add(token_reservation) > budget)
        || current
            .5
            .is_some_and(|budget| current.6.saturating_add(cost_reservation_micros) > budget)
    {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    let next_index = current.0.saturating_add(1);
    tx.execute(
        "UPDATE continuation_runs SET continuation_index=?2,used_model_turns=used_model_turns+1,
         token_used=token_used+?3,cost_used_micros=cost_used_micros+?4,updated_at_ms=?5
         WHERE run_id=?1 AND state='running'",
        params![
            run_id,
            next_index,
            token_reservation,
            cost_reservation_micros,
            now_ms
        ],
    )?;
    tx.execute(
        "INSERT INTO continuation_attempts
         (run_id,attempt_index,gate_id,fingerprint,state,result_json,created_at_ms)
         VALUES (?1,?2,?3,?4,'reserved',X'',?5)",
        params![run_id, next_index, gate_id, fingerprint, now_ms],
    )?;
    tx.commit()?;
    Ok(true)
}

/// Finishes a reserved attempt with its final state and serialized result.
pub fn finish_attempt(
    connection: &Connection,
    run_id: &str,
    attempt_index: i64,
    state: &str,
    result_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "UPDATE continuation_attempts SET state=?3,result_json=?4,created_at_ms=?5
         WHERE run_id=?1 AND attempt_index=?2 AND state='reserved'",
        params![run_id, attempt_index, state, result_json, now_ms],
    )?;
    Ok(changed == 1)
}

/// Stops a run only when it is still in the caller's expected state.
pub fn stop_run(
    connection: &Connection,
    run_id: &str,
    expected_state: &str,
    stop_reason: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "UPDATE continuation_runs SET state='stopped',stop_reason=?3,updated_at_ms=?4
         WHERE run_id=?1 AND state=?2",
        params![run_id, expected_state, stop_reason, now_ms],
    )?;
    Ok(changed == 1)
}

/// Changes a run's state using an expected-state compare-and-set.
pub fn transition_run(
    connection: &Connection,
    run_id: &str,
    expected_state: &str,
    next_state: &str,
    stop_reason: Option<&str>,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "UPDATE continuation_runs SET state=?3,stop_reason=?4,updated_at_ms=?5
         WHERE run_id=?1 AND state=?2",
        params![run_id, expected_state, next_state, stop_reason, now_ms],
    )?;
    Ok(changed == 1)
}

/// Lists a run's attempts newest first, with the requested limit clamped to 1–256.
pub fn list_attempts(
    connection: &Connection,
    run_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<AttemptRecord>> {
    let mut statement = connection.prepare(
        "SELECT run_id,attempt_index,gate_id,fingerprint,state,result_json,created_at_ms
         FROM continuation_attempts WHERE run_id=?1 ORDER BY attempt_index DESC LIMIT ?2",
    )?;
    let rows = statement.query_map(params![run_id, limit.clamp(1, 256) as i64], |row| {
        Ok(AttemptRecord {
            run_id: row.get(0)?,
            attempt_index: row.get(1)?,
            gate_id: row.get(2)?,
            fingerprint: row.get(3)?,
            state: row.get(4)?,
            result_json: row.get(5)?,
            created_at_ms: row.get(6)?,
        })
    })?;
    rows.collect()
}

/// Records the first gate result for a run, gate, and attempt tuple.
pub fn record_gate_result(
    connection: &Connection,
    record: &GateResultRecord,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO continuation_gate_results
         (run_id,gate_id,attempt_index,status,evidence_ref,error_code,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(run_id,gate_id,attempt_index) DO NOTHING",
        params![
            record.run_id,
            record.gate_id,
            record.attempt_index,
            record.status,
            record.evidence_ref,
            record.error_code,
            record.created_at_ms
        ],
    )?;
    Ok(())
}

/// Lists the latest recorded gate results for a run.
pub fn list_latest_gate_results(
    connection: &Connection,
    run_id: &str,
) -> rusqlite::Result<Vec<GateResultRecord>> {
    let mut statement = connection.prepare(
        "SELECT run_id,gate_id,attempt_index,status,evidence_ref,error_code,created_at_ms
         FROM continuation_gate_results
         WHERE run_id=?1 AND attempt_index IN
           (SELECT MAX(attempt_index) FROM continuation_gate_results
            WHERE run_id=?1 GROUP BY gate_id)
         ORDER BY gate_id LIMIT 32",
    )?;
    let rows = statement
        .query_map([run_id], |row| {
            Ok(GateResultRecord {
                run_id: row.get(0)?,
                gate_id: row.get(1)?,
                attempt_index: row.get(2)?,
                status: row.get(3)?,
                evidence_ref: row.get(4)?,
                error_code: row.get(5)?,
                created_at_ms: row.get(6)?,
            })
        })?
        .collect();
    rows
}

#[cfg(test)]
#[path = "continuation_store_tests.rs"]
mod tests;
