//! Durable definition/run boundary for automation contract 16.1.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const AUTOMATION_STORE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationDefinitionRecord {
    pub definition_id: String,
    pub revision: u64,
    pub owner_scope: String,
    pub definition_json: String,
    pub definition_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationRunRecord {
    pub run_id: String,
    pub definition_id: String,
    pub revision: u64,
    pub owner_scope: String,
    pub idempotency_key: String,
    pub payload_hash: String,
    pub state: String,
    pub generation: u64,
    pub permission_snapshot: String,
    pub approval_snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmitRunResult {
    Inserted,
    Existing(AutomationRunRecord),
    IdempotencyConflict { existing_payload_hash: String },
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS automation_definitions (definition_id TEXT NOT NULL, revision INTEGER NOT NULL, owner_scope TEXT NOT NULL, definition_json TEXT NOT NULL, definition_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(definition_id, revision, owner_scope)); CREATE TABLE IF NOT EXISTS automation_runs (run_id TEXT PRIMARY KEY, definition_id TEXT NOT NULL, revision INTEGER NOT NULL, owner_scope TEXT NOT NULL, idempotency_key TEXT NOT NULL, payload_hash TEXT NOT NULL, state TEXT NOT NULL, generation INTEGER NOT NULL, permission_snapshot TEXT NOT NULL, approval_snapshot TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, UNIQUE(owner_scope, definition_id, revision, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_automation_runs_state ON automation_runs(state, updated_at_ms);")
}

pub fn insert_definition(
    connection: &Connection,
    record: &AutomationDefinitionRecord,
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO automation_definitions (definition_id, revision, owner_scope, definition_json, definition_hash, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![record.definition_id, record.revision as i64, record.owner_scope, record.definition_json, record.definition_hash, now_ms])?;
    Ok(())
}

pub fn get_definition(
    connection: &Connection,
    definition_id: &str,
    revision: u64,
    owner_scope: &str,
) -> rusqlite::Result<Option<AutomationDefinitionRecord>> {
    connection.query_row("SELECT definition_id, revision, owner_scope, definition_json, definition_hash FROM automation_definitions WHERE definition_id=?1 AND revision=?2 AND owner_scope=?3", params![definition_id, revision as i64, owner_scope], |row| Ok(AutomationDefinitionRecord { definition_id: row.get(0)?, revision: row.get::<_, i64>(1)? as u64, owner_scope: row.get(2)?, definition_json: row.get(3)?, definition_hash: row.get(4)? })).optional()
}

pub fn find_run_by_idempotency(
    connection: &Connection,
    owner_scope: &str,
    definition_id: &str,
    revision: u64,
    idempotency_key: &str,
) -> rusqlite::Result<Option<AutomationRunRecord>> {
    connection.query_row("SELECT run_id, definition_id, revision, owner_scope, idempotency_key, payload_hash, state, generation, permission_snapshot, approval_snapshot FROM automation_runs WHERE owner_scope=?1 AND definition_id=?2 AND revision=?3 AND idempotency_key=?4", params![owner_scope, definition_id, revision as i64, idempotency_key], map_run).optional()
}

pub fn insert_run(
    connection: &Connection,
    record: &AutomationRunRecord,
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO automation_runs (run_id, definition_id, revision, owner_scope, idempotency_key, payload_hash, state, generation, permission_snapshot, approval_snapshot, created_at_ms, updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)", params![record.run_id, record.definition_id, record.revision as i64, record.owner_scope, record.idempotency_key, record.payload_hash, record.state, record.generation as i64, record.permission_snapshot, record.approval_snapshot, now_ms])?;
    Ok(())
}

/// Atomically applies trigger idempotency.  Callers must compare the payload
/// hash before treating a repeated key as the same request.
pub fn admit_run(
    connection: &Connection,
    record: &AutomationRunRecord,
    now_ms: i64,
) -> rusqlite::Result<AdmitRunResult> {
    if let Some(existing) = find_run_by_idempotency(
        connection,
        &record.owner_scope,
        &record.definition_id,
        record.revision,
        &record.idempotency_key,
    )? {
        return if existing.payload_hash == record.payload_hash {
            Ok(AdmitRunResult::Existing(existing))
        } else {
            Ok(AdmitRunResult::IdempotencyConflict {
                existing_payload_hash: existing.payload_hash,
            })
        };
    }
    insert_run(connection, record, now_ms)?;
    Ok(AdmitRunResult::Inserted)
}

fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRunRecord> {
    Ok(AutomationRunRecord {
        run_id: row.get(0)?,
        definition_id: row.get(1)?,
        revision: row.get::<_, i64>(2)? as u64,
        owner_scope: row.get(3)?,
        idempotency_key: row.get(4)?,
        payload_hash: row.get(5)?,
        state: row.get(6)?,
        generation: row.get::<_, i64>(7)? as u64,
        permission_snapshot: row.get(8)?,
        approval_snapshot: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idempotency_is_durable_and_scoped() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let d = AutomationDefinitionRecord {
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            definition_json: "{}".into(),
            definition_hash: "h".into(),
        };
        insert_definition(&c, &d, 1).unwrap();
        let r = AutomationRunRecord {
            run_id: "run".into(),
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            idempotency_key: "k".into(),
            payload_hash: "p".into(),
            state: "admitted".into(),
            generation: 1,
            permission_snapshot: "ps".into(),
            approval_snapshot: "as".into(),
        };
        insert_run(&c, &r, 1).unwrap();
        assert_eq!(
            find_run_by_idempotency(&c, "o", "d", 1, "k")
                .unwrap()
                .unwrap(),
            r
        );
        assert!(find_run_by_idempotency(&c, "other", "d", 1, "k")
            .unwrap()
            .is_none());
        assert!(matches!(
            admit_run(&c, &r, 2).unwrap(),
            AdmitRunResult::Existing(_)
        ));
        let mut conflict = r.clone();
        conflict.payload_hash = "different".into();
        assert!(matches!(
            admit_run(&c, &conflict, 2).unwrap(),
            AdmitRunResult::IdempotencyConflict { .. }
        ));
    }
}
