//! Durable storage for Continuation Policy v1.
//!
//! The store deliberately keeps canonical contract bytes and bounded metadata;
//! it does not interpret model text or execute a gate. Core owns validation and
//! decisions, while this module provides transactional persistence, dedup and
//! budget reservation primitives.

use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PolicyRecord {
    pub policy_id: String,
    pub revision: i64,
    pub owner_scope: String,
    pub actor: String,
    pub enabled: bool,
    pub canonical_json: Vec<u8>,
    pub content_hash: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RunRecord {
    pub run_id: String,
    pub idempotency_key: String,
    pub task_id: String,
    pub prompt: Option<String>,
    pub workspace_path: Option<String>,
    pub owner_scope: String,
    pub policy_id: String,
    pub policy_revision: i64,
    pub policy_hash: String,
    pub goal_id: Option<String>,
    pub goal_version: Option<i64>,
    pub state: String,
    pub continuation_index: i64,
    pub max_continuations: i64,
    pub max_model_turns: i64,
    pub used_model_turns: i64,
    pub token_budget: Option<i64>,
    pub token_used: i64,
    pub cost_budget_micros: Option<i64>,
    pub cost_used_micros: i64,
    pub stop_reason: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    pub run_id: String,
    pub attempt_index: i64,
    pub gate_id: String,
    pub fingerprint: String,
    pub state: String,
    pub result_json: Vec<u8>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GateResultRecord {
    pub run_id: String,
    pub gate_id: String,
    pub attempt_index: i64,
    pub status: String,
    pub evidence_ref: Option<String>,
    pub error_code: Option<String>,
    pub created_at_ms: i64,
}

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

#[derive(Clone, Copy)]
pub struct TransitionActionInput<'a> {
    pub run_id: &'a str,
    pub idempotency_key: &'a str,
    pub action: &'a str,
    pub expected_state: &'a str,
    pub next_state: &'a str,
    pub stop_reason: &'a str,
    pub now_ms: i64,
}

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

pub fn record_gate_result(
    connection: &Connection,
    record: &GateResultRecord,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR REPLACE INTO continuation_gate_results
         (run_id,gate_id,attempt_index,status,evidence_ref,error_code,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
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
mod tests {
    use super::*;

    fn policy() -> PolicyRecord {
        PolicyRecord {
            policy_id: "p1".into(),
            revision: 1,
            owner_scope: "w1".into(),
            actor: "user".into(),
            enabled: true,
            canonical_json: br#"{}"#.to_vec(),
            content_hash: "h1".into(),
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn run() -> RunRecord {
        RunRecord {
            run_id: "r1".into(),
            idempotency_key: "i1".into(),
            task_id: "t1".into(),
            prompt: None,
            workspace_path: None,
            owner_scope: "w1".into(),
            policy_id: "p1".into(),
            policy_revision: 1,
            policy_hash: "h1".into(),
            goal_id: None,
            goal_version: None,
            state: "running".into(),
            continuation_index: 0,
            max_continuations: 2,
            max_model_turns: 2,
            used_model_turns: 0,
            token_budget: Some(100),
            token_used: 0,
            cost_budget_micros: Some(10),
            cost_used_micros: 0,
            stop_reason: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn schema_and_reservation_are_idempotent() {
        let mut connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        save_policy(&connection, &policy()).unwrap();
        create_run(&connection, &run()).unwrap();
        assert_eq!(
            get_run_by_idempotency(&connection, "w1", "i1")
                .unwrap()
                .unwrap()
                .run_id,
            "r1"
        );
        assert!(attach_task_context(&connection, "t1", "redacted prompt", "workspace", 2).unwrap());
        assert_eq!(
            get_run(&connection, "r1")
                .unwrap()
                .unwrap()
                .prompt
                .as_deref(),
            Some("redacted prompt")
        );
        assert!(!attach_task_context(&connection, "t1", "other", "workspace", 3).unwrap());
        assert!(reserve_attempt(&mut connection, "r1", "g1", "f1", 20, 2, 2).unwrap());
        assert!(!reserve_attempt(&mut connection, "r1", "g1", "f1", 20, 2, 3).unwrap());
        assert_eq!(get_run(&connection, "r1").unwrap().unwrap().token_used, 20);
        assert!(finish_attempt(&connection, "r1", 1, "passed", br#"{}"#, 4).unwrap());
    }

    #[test]
    fn stop_is_compare_and_set() {
        let mut connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        save_policy(&connection, &policy()).unwrap();
        create_run(&connection, &run()).unwrap();
        let input = TransitionActionInput {
            run_id: "r1",
            idempotency_key: "stop-1",
            action: "stop",
            expected_state: "running",
            next_state: "stopped",
            stop_reason: "user_stop",
            now_ms: 2,
        };
        let first = apply_transition_action(&mut connection, input).unwrap();
        let duplicate = apply_transition_action(
            &mut connection,
            TransitionActionInput { now_ms: 3, ..input },
        )
        .unwrap();
        assert_eq!(first, duplicate);
        assert!(!stop_run(&connection, "r1", "running", "again", 4).unwrap());
    }
}
