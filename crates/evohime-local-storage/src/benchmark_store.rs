//! Durable metadata-only storage for benchmark matrix runs and attempts.

use rusqlite::{params, Connection, OptionalExtension};

use crate::StorageError;

pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS benchmark_suites (
           suite_id TEXT NOT NULL, suite_version TEXT NOT NULL,
           content_json TEXT NOT NULL, content_hash TEXT NOT NULL,
           created_at_ms INTEGER NOT NULL,
           PRIMARY KEY(suite_id, suite_version)
         );
         CREATE TABLE IF NOT EXISTS benchmark_runs (
           run_id TEXT PRIMARY KEY NOT NULL, suite_id TEXT NOT NULL,
           suite_version TEXT NOT NULL, policy_json TEXT NOT NULL,
           state TEXT NOT NULL, report_json TEXT, created_at_ms INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS benchmark_attempts (
           run_id TEXT NOT NULL REFERENCES benchmark_runs(run_id) ON DELETE CASCADE,
           attempt_id TEXT NOT NULL, challenge_id TEXT NOT NULL,
           model_profile_id TEXT NOT NULL, agent_profile_id TEXT NOT NULL,
           outcome TEXT NOT NULL, result_json TEXT NOT NULL,
           created_at_ms INTEGER NOT NULL,
           PRIMARY KEY(run_id, attempt_id)
         );
         CREATE TABLE IF NOT EXISTS benchmark_baselines (
           baseline_id TEXT PRIMARY KEY NOT NULL, suite_version TEXT NOT NULL,
           challenge_id TEXT NOT NULL, model_profile_hash TEXT NOT NULL,
           agent_profile_hash TEXT NOT NULL, metrics_json TEXT NOT NULL,
           source_commit TEXT NOT NULL, revision INTEGER NOT NULL,
           created_at_ms INTEGER NOT NULL,
           UNIQUE(suite_version, challenge_id, model_profile_hash, agent_profile_hash, revision)
         );
         CREATE INDEX IF NOT EXISTS idx_benchmark_attempts_run ON benchmark_attempts(run_id);
         CREATE INDEX IF NOT EXISTS idx_benchmark_baselines_lookup ON benchmark_baselines(suite_version, challenge_id);",
    )?;
    Ok(())
}

pub fn save_run(
    connection: &Connection,
    run_id: &str,
    suite_id: &str,
    suite_version: &str,
    policy_json: &str,
    state: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    Ok(connection.execute(
        "INSERT OR IGNORE INTO benchmark_runs(run_id,suite_id,suite_version,policy_json,state,created_at_ms,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?6)",
        params![run_id, suite_id, suite_version, policy_json, state, now_ms],
    )? == 1)
}

#[derive(Clone, Copy)]
pub struct SaveAttemptInput<'a> {
    pub run_id: &'a str,
    pub attempt_id: &'a str,
    pub challenge_id: &'a str,
    pub model_profile_id: &'a str,
    pub agent_profile_id: &'a str,
    pub outcome: &'a str,
    pub result_json: &'a str,
    pub now_ms: i64,
}

pub fn save_attempt(
    connection: &Connection,
    input: SaveAttemptInput<'_>,
) -> Result<bool, StorageError> {
    let SaveAttemptInput {
        run_id,
        attempt_id,
        challenge_id,
        model_profile_id,
        agent_profile_id,
        outcome,
        result_json,
        now_ms,
    } = input;
    Ok(connection.execute(
        "INSERT OR IGNORE INTO benchmark_attempts(run_id,attempt_id,challenge_id,model_profile_id,agent_profile_id,outcome,result_json,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![run_id, attempt_id, challenge_id, model_profile_id, agent_profile_id, outcome, result_json, now_ms],
    )? == 1)
}

pub fn save_report(
    connection: &Connection,
    run_id: &str,
    report_json: &str,
    state: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    Ok(connection.execute(
        "UPDATE benchmark_runs SET report_json=?2,state=?3,updated_at_ms=?4 WHERE run_id=?1",
        params![run_id, report_json, state, now_ms],
    )? == 1)
}

/// Loads the durable suite binding, lifecycle state and report for one run.
pub fn get_run(
    connection: &Connection,
    run_id: &str,
) -> Result<Option<(String, String, String, Option<String>)>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT suite_id,suite_version,state,report_json FROM benchmark_runs WHERE run_id=?1",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?)
}

/// Loads the immutable policy payload associated with one benchmark run.
pub fn get_run_policy_json(
    connection: &Connection,
    run_id: &str,
) -> Result<Option<String>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT policy_json FROM benchmark_runs WHERE run_id=?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// Returns the newest approved baseline revision for an exact compatible key.
pub fn latest_baseline_revision(
    connection: &Connection,
    suite_version: &str,
    challenge_id: &str,
    model_profile_hash: &str,
    agent_profile_hash: &str,
) -> Result<u64, StorageError> {
    Ok(connection.query_row(
        "SELECT COALESCE(MAX(revision),0) FROM benchmark_baselines
         WHERE suite_version=?1 AND challenge_id=?2 AND model_profile_hash=?3 AND agent_profile_hash=?4",
        params![suite_version, challenge_id, model_profile_hash, agent_profile_hash],
        |row| row.get(0),
    )?)
}

/// Inserts one explicitly approved immutable baseline revision.
#[allow(clippy::too_many_arguments)]
pub fn put_baseline(
    connection: &Connection,
    baseline_id: &str,
    suite_version: &str,
    challenge_id: &str,
    model_profile_hash: &str,
    agent_profile_hash: &str,
    metrics_json: &str,
    source_commit: &str,
    revision: u64,
    now_ms: i64,
) -> Result<bool, StorageError> {
    Ok(connection.execute(
        "INSERT OR IGNORE INTO benchmark_baselines
         (baseline_id,suite_version,challenge_id,model_profile_hash,agent_profile_hash,
          metrics_json,source_commit,revision,created_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            baseline_id,
            suite_version,
            challenge_id,
            model_profile_hash,
            agent_profile_hash,
            metrics_json,
            source_commit,
            revision,
            now_ms
        ],
    )? == 1)
}

/// Records the authenticated Core IPC approval that created a baseline.
pub fn put_baseline_approval(
    connection: &Connection,
    baseline_id: &str,
    owner_scope: &str,
    run_id: &str,
    report_sha256: &str,
    idempotency_key: &str,
    approved_at_ms: i64,
) -> Result<bool, StorageError> {
    Ok(connection.execute(
        "INSERT OR IGNORE INTO benchmark_baseline_approvals
         (baseline_id,owner_scope,run_id,report_sha256,idempotency_key,approved_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            baseline_id,
            owner_scope,
            run_id,
            report_sha256,
            idempotency_key,
            approved_at_ms
        ],
    )? == 1)
}

/// Loads the baseline identity recorded for one approval idempotency key.
pub fn get_baseline_approval_by_key(
    connection: &Connection,
    idempotency_key: &str,
) -> Result<Option<(String, String, String, String)>, StorageError> {
    Ok(connection.query_row(
        "SELECT baseline_id,owner_scope,run_id,report_sha256 FROM benchmark_baseline_approvals WHERE idempotency_key=?1",
        [idempotency_key],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional()?)
}

/// Loads the existing approval attribution for an immutable baseline identity.
pub fn get_baseline_approval(
    connection: &Connection,
    baseline_id: &str,
) -> Result<Option<(String, String, String)>, StorageError> {
    Ok(connection.query_row(
        "SELECT owner_scope,run_id,report_sha256 FROM benchmark_baseline_approvals WHERE baseline_id=?1",
        [baseline_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?)
}

/// Loads one immutable baseline row for comparison with a caller's typed snapshot.
pub fn get_baseline(
    connection: &Connection,
    baseline_id: &str,
) -> Result<Option<(String, String, String, String, String, String, u64)>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT suite_version,challenge_id,model_profile_hash,agent_profile_hash,
                metrics_json,source_commit,revision
         FROM benchmark_baselines WHERE baseline_id=?1",
            [baseline_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn run_and_attempt_are_idempotent() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        assert!(save_run(&connection, "r", "s", "1", "{}", "running", 1).unwrap());
        assert!(!save_run(&connection, "r", "s", "1", "{}", "running", 1).unwrap());
        assert_eq!(
            get_run_policy_json(&connection, "r").unwrap().as_deref(),
            Some("{}")
        );
        assert_eq!(get_run(&connection, "r").unwrap().unwrap().2, "running");
        let input = SaveAttemptInput {
            run_id: "r",
            attempt_id: "a",
            challenge_id: "c",
            model_profile_id: "m",
            agent_profile_id: "p",
            outcome: "passed",
            result_json: "{}",
            now_ms: 1,
        };
        assert!(save_attempt(&connection, input).unwrap());
        assert!(!save_attempt(&connection, input).unwrap());
    }

    #[test]
    fn approved_baseline_revision_is_immutable_and_readable() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let insert = |metrics: &str| {
            put_baseline(
                &connection,
                "baseline-1",
                "suite-v1",
                "challenge-1",
                "model-hash",
                "agent-hash",
                metrics,
                "source-commit",
                1,
                10,
            )
            .unwrap()
        };
        assert!(insert("{\"passed\":1}"));
        assert!(!insert("{\"passed\":0}"));
        let stored = get_baseline(&connection, "baseline-1").unwrap().unwrap();
        assert_eq!(stored.4, "{\"passed\":1}");
        assert_eq!(stored.6, 1);
    }
}
