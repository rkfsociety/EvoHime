//! Durable, metadata-only storage for the persistent agent organization registry.
//!
//! The Core owns the contract and validation.  This module deliberately stores
//! opaque bounded JSON plus indexed metadata so SQLite cannot become a second
//! authority for runtime, grants, prompts, credentials, or transcripts.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_RECORD_BYTES: usize = 64 * 1024;

pub type ReportingHistoryRow = (u64, Option<String>, String, String, i64);

pub fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS persistent_agents (
            id TEXT PRIMARY KEY NOT NULL,
            revision INTEGER NOT NULL,
            status TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            agent_json BLOB NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS persistent_agent_revisions (
            agent_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            agent_json BLOB NOT NULL,
            actor TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(agent_id, revision)
        );
        CREATE TABLE IF NOT EXISTS persistent_agent_reporting_history (
            agent_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            parent_agent_id TEXT,
            event_type TEXT NOT NULL,
            actor TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(agent_id, revision)
        );
        CREATE TABLE IF NOT EXISTS persistent_agent_goal_bindings (
            agent_id TEXT NOT NULL,
            goal_id TEXT NOT NULL,
            goal_revision INTEGER NOT NULL,
            responsibility TEXT NOT NULL,
            scope_json BLOB,
            binding_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(agent_id, goal_id, goal_revision, responsibility)
        );
        CREATE TABLE IF NOT EXISTS persistent_agent_assignments (
            id TEXT PRIMARY KEY NOT NULL,
            revision INTEGER NOT NULL,
            agent_id TEXT NOT NULL,
            status TEXT NOT NULL,
            source_kind TEXT NOT NULL,
            source_ref TEXT NOT NULL,
            assignment_json BLOB NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS persistent_agent_commands (
            idempotency_key TEXT PRIMARY KEY NOT NULL,
            command_hash TEXT NOT NULL,
            outcome_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_persistent_agent_revisions_agent
            ON persistent_agent_revisions(agent_id, revision DESC);
        CREATE INDEX IF NOT EXISTS idx_persistent_agent_assignments_agent
            ON persistent_agent_assignments(agent_id, updated_at_ms DESC);
        CREATE INDEX IF NOT EXISTS idx_persistent_agent_assignments_source
            ON persistent_agent_assignments(source_kind, source_ref);
        CREATE INDEX IF NOT EXISTS idx_persistent_agent_goal_bindings_agent
            ON persistent_agent_goal_bindings(agent_id, created_at_ms DESC);",
    )
}

#[derive(Clone, Copy)]
pub struct SaveAgentRevisionInput<'a> {
    pub id: &'a str,
    pub revision: u64,
    pub status: &'a str,
    pub content_hash: &'a str,
    pub agent_json: &'a [u8],
    pub actor: &'a str,
    pub now_ms: i64,
}

pub fn save_agent_revision(
    connection: &Connection,
    input: SaveAgentRevisionInput<'_>,
) -> Result<bool, rusqlite::Error> {
    if input.agent_json.len() > MAX_RECORD_BYTES {
        return Ok(false);
    }
    let tx = connection.unchecked_transaction()?;
    let current: Option<u64> = tx
        .query_row(
            "SELECT revision FROM persistent_agents WHERE id = ?1",
            [input.id],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some_and(|value| value >= input.revision) {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO persistent_agent_revisions(agent_id, revision, content_hash, agent_json, actor, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![input.id, input.revision as i64, input.content_hash, input.agent_json, input.actor, input.now_ms],
    )?;
    tx.execute(
        "INSERT INTO persistent_agents(id, revision, status, content_hash, agent_json, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET revision=excluded.revision, status=excluded.status,
           content_hash=excluded.content_hash, agent_json=excluded.agent_json,
           updated_at_ms=excluded.updated_at_ms",
        params![input.id, input.revision as i64, input.status, input.content_hash, input.agent_json, input.now_ms],
    )?;
    tx.commit()?;
    Ok(true)
}

pub fn load_agent(connection: &Connection, id: &str) -> Result<Option<Vec<u8>>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT agent_json FROM persistent_agents WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()
}

pub fn load_agents(connection: &Connection, limit: usize) -> Result<Vec<Vec<u8>>, rusqlite::Error> {
    let mut statement =
        connection.prepare("SELECT agent_json FROM persistent_agents ORDER BY id LIMIT ?1")?;
    let rows = statement
        .query_map(params![limit.min(256) as i64], |row| row.get(0))?
        .collect();
    rows
}

pub fn load_agent_revision(
    connection: &Connection,
    id: &str,
    revision: u64,
) -> Result<Option<Vec<u8>>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT agent_json FROM persistent_agent_revisions WHERE agent_id = ?1 AND revision = ?2",
            params![id, revision as i64],
            |row| row.get(0),
        )
        .optional()
}

pub fn save_reporting_history(
    connection: &Connection,
    agent_id: &str,
    revision: u64,
    parent_agent_id: Option<&str>,
    event_type: &str,
    actor: &str,
    now_ms: i64,
) -> Result<(), rusqlite::Error> {
    connection.execute(
        "INSERT OR REPLACE INTO persistent_agent_reporting_history
         (agent_id, revision, parent_agent_id, event_type, actor, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            agent_id,
            revision as i64,
            parent_agent_id,
            event_type,
            actor,
            now_ms
        ],
    )?;
    Ok(())
}

pub fn load_reporting_history(
    connection: &Connection,
    agent_id: &str,
    limit: usize,
) -> Result<Vec<ReportingHistoryRow>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT revision, parent_agent_id, event_type, actor, created_at_ms
         FROM persistent_agent_reporting_history WHERE agent_id = ?1
         ORDER BY revision DESC LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![agent_id, limit.min(256) as i64], |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect();
    rows
}

#[derive(Clone, Copy)]
pub struct SaveGoalBindingInput<'a> {
    pub agent_id: &'a str,
    pub goal_id: &'a str,
    pub goal_revision: u64,
    pub responsibility: &'a str,
    pub scope_json: Option<&'a [u8]>,
    pub binding_json: &'a [u8],
    pub now_ms: i64,
}

pub fn save_goal_binding(
    connection: &Connection,
    input: SaveGoalBindingInput<'_>,
) -> Result<bool, rusqlite::Error> {
    if input.binding_json.len() > MAX_RECORD_BYTES {
        return Ok(false);
    }
    Ok(connection.execute(
        "INSERT INTO persistent_agent_goal_bindings
         (agent_id, goal_id, goal_revision, responsibility, scope_json, binding_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(agent_id, goal_id, goal_revision, responsibility) DO UPDATE SET
           scope_json=excluded.scope_json, binding_json=excluded.binding_json,
           created_at_ms=excluded.created_at_ms",
        params![
            input.agent_id,
            input.goal_id,
            input.goal_revision as i64,
            input.responsibility,
            input.scope_json,
            input.binding_json,
            input.now_ms
        ],
    )? == 1)
}

pub fn remove_goal_binding(
    connection: &Connection,
    agent_id: &str,
    goal_id: &str,
    goal_revision: u64,
    responsibility: &str,
) -> Result<bool, rusqlite::Error> {
    Ok(connection.execute(
        "DELETE FROM persistent_agent_goal_bindings
         WHERE agent_id=?1 AND goal_id=?2 AND goal_revision=?3 AND responsibility=?4",
        params![agent_id, goal_id, goal_revision as i64, responsibility],
    )? == 1)
}

pub fn load_goal_bindings(
    connection: &Connection,
    agent_id: &str,
    limit: usize,
) -> Result<Vec<Vec<u8>>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT binding_json FROM persistent_agent_goal_bindings WHERE agent_id=?1
         ORDER BY goal_id, goal_revision, responsibility LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![agent_id, limit.min(256) as i64], |row| row.get(0))?
        .collect();
    rows
}

#[derive(Clone, Copy)]
pub struct SaveAssignmentInput<'a> {
    pub id: &'a str,
    pub revision: u64,
    pub agent_id: &'a str,
    pub status: &'a str,
    pub source_kind: &'a str,
    pub source_ref: &'a str,
    pub assignment_json: &'a [u8],
    pub now_ms: i64,
}

pub fn save_assignment(
    connection: &Connection,
    input: SaveAssignmentInput<'_>,
) -> Result<bool, rusqlite::Error> {
    if input.assignment_json.len() > MAX_RECORD_BYTES {
        return Ok(false);
    }
    Ok(connection.execute(
        "INSERT INTO persistent_agent_assignments
         (id, revision, agent_id, status, source_kind, source_ref, assignment_json, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET revision=excluded.revision, agent_id=excluded.agent_id,
           status=excluded.status, source_kind=excluded.source_kind, source_ref=excluded.source_ref,
           assignment_json=excluded.assignment_json, updated_at_ms=excluded.updated_at_ms",
        params![
            input.id,
            input.revision as i64,
            input.agent_id,
            input.status,
            input.source_kind,
            input.source_ref,
            input.assignment_json,
            input.now_ms
        ],
    )? == 1)
}

pub fn load_assignment(
    connection: &Connection,
    id: &str,
) -> Result<Option<Vec<u8>>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT assignment_json FROM persistent_agent_assignments WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
}

pub fn load_assignments_for_agent(
    connection: &Connection,
    agent_id: &str,
    limit: usize,
) -> Result<Vec<Vec<u8>>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT assignment_json FROM persistent_agent_assignments WHERE agent_id=?1
         ORDER BY updated_at_ms DESC, id LIMIT ?2",
    )?;
    let rows = statement
        .query_map(params![agent_id, limit.min(256) as i64], |row| row.get(0))?
        .collect();
    rows
}

pub fn load_assignments(
    connection: &Connection,
    limit: usize,
) -> Result<Vec<Vec<u8>>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT assignment_json FROM persistent_agent_assignments ORDER BY updated_at_ms DESC, id LIMIT ?1",
    )?;
    let rows = statement
        .query_map(params![limit.min(512) as i64], |row| row.get(0))?
        .collect();
    rows
}

pub fn load_command_outcome(
    connection: &Connection,
    idempotency_key: &str,
) -> Result<Option<(String, Vec<u8>)>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT command_hash, outcome_json FROM persistent_agent_commands WHERE idempotency_key=?1",
            [idempotency_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
}

pub fn record_command_outcome(
    connection: &Connection,
    idempotency_key: &str,
    command_hash: &str,
    outcome_json: &[u8],
    now_ms: i64,
) -> Result<Option<(String, Vec<u8>)>, rusqlite::Error> {
    if outcome_json.len() > MAX_RECORD_BYTES {
        return Ok(None);
    }
    let existing: Option<(String, Vec<u8>)> = connection
        .query_row(
            "SELECT command_hash, outcome_json FROM persistent_agent_commands WHERE idempotency_key=?1",
            [idempotency_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(existing);
    }
    connection.execute(
        "INSERT INTO persistent_agent_commands(idempotency_key, command_hash, outcome_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![idempotency_key, command_hash, outcome_json, now_ms],
    )?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_storage_roundtrip_and_idempotency() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let input = SaveAgentRevisionInput {
            id: "a",
            revision: 1,
            status: "draft",
            content_hash: "h",
            agent_json: br#"{}"#,
            actor: "user",
            now_ms: 1,
        };
        assert!(save_agent_revision(&connection, input).unwrap());
        assert!(
            !save_agent_revision(&connection, SaveAgentRevisionInput { now_ms: 2, ..input })
                .unwrap()
        );
        assert_eq!(
            load_agent(&connection, "a").unwrap().unwrap(),
            b"{}".to_vec()
        );
        assert!(
            record_command_outcome(&connection, "k", "h", br#"{"ok":true}"#, 1)
                .unwrap()
                .is_none()
        );
        let previous = record_command_outcome(&connection, "k", "h", br#"{"ok":false}"#, 2)
            .unwrap()
            .unwrap();
        assert_eq!(previous.0, "h");
        assert_eq!(previous.1, br#"{"ok":true}"#.to_vec());
    }
}
