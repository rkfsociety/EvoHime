//! Durable metadata for middleware definitions and run snapshots.

use crate::StorageError;
use rusqlite::{params, Connection};

pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_middleware_definitions (definition_id TEXT NOT NULL, revision INTEGER NOT NULL, contract_hash TEXT NOT NULL, definition_json TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(definition_id, revision)); CREATE TABLE IF NOT EXISTS agent_middleware_runs (run_id TEXT PRIMARY KEY NOT NULL, definition_id TEXT NOT NULL, definition_revision INTEGER NOT NULL, contract_hash TEXT NOT NULL, policy_hash TEXT NOT NULL, capability_snapshot_hash TEXT NOT NULL, state TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_agent_middleware_runs_state ON agent_middleware_runs(state, updated_at_ms);")?;
    Ok(())
}

pub fn save_definition(
    connection: &Connection,
    id: &str,
    revision: u64,
    hash: &str,
    json: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    Ok(connection.execute("INSERT OR IGNORE INTO agent_middleware_definitions(definition_id,revision,contract_hash,definition_json,created_at_ms) VALUES (?1,?2,?3,?4,?5)", params![id, revision as i64, hash, json, now_ms])? == 1)
}

#[allow(clippy::too_many_arguments)]
pub fn save_run(
    connection: &Connection,
    run_id: &str,
    definition_id: &str,
    revision: u64,
    contract_hash: &str,
    policy_hash: &str,
    capability_hash: &str,
    state: &str,
    now_ms: i64,
) -> Result<bool, StorageError> {
    Ok(connection.execute("INSERT OR IGNORE INTO agent_middleware_runs(run_id,definition_id,definition_revision,contract_hash,policy_hash,capability_snapshot_hash,state,created_at_ms,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)", params![run_id, definition_id, revision as i64, contract_hash, policy_hash, capability_hash, state, now_ms])? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definitions_and_runs_are_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(save_definition(&c, "d", 1, "h", "{}", 1).unwrap());
        assert!(!save_definition(&c, "d", 1, "h", "{}", 1).unwrap());
        assert!(save_run(&c, "r", "d", 1, "h", "p", "c", "active", 1).unwrap());
        assert!(!save_run(&c, "r", "d", 1, "h", "p", "c", "active", 1).unwrap());
    }
}
