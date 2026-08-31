//! Durable metadata-only storage for benchmark matrix runs and attempts.

use rusqlite::{params, Connection};

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

#[allow(clippy::too_many_arguments)]
pub fn save_attempt(
    connection: &Connection,
    run_id: &str,
    attempt_id: &str,
    challenge_id: &str,
    model_profile_id: &str,
    agent_profile_id: &str,
    outcome: &str,
    result_json: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
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
        assert!(save_attempt(&connection, "r", "a", "c", "m", "p", "passed", "{}", 1).unwrap());
        assert!(!save_attempt(&connection, "r", "a", "c", "m", "p", "passed", "{}", 1).unwrap());
    }
}
