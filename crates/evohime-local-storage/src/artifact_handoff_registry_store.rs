//! Durable metadata for the Core-owned Artifact Handoff Registry.
//! Bytes remain exclusively in `ArtifactStore`; this module stores only
//! immutable revisions, graph edges, handoffs and command outcomes.

use rusqlite::{params, Connection, OptionalExtension, Transaction};

pub const STORE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryRow {
    pub artifact_id: String,
    pub project_id: String,
    pub revision: u64,
    pub state: String,
    pub content_locator: String,
    pub content_hash: String,
    pub metadata_json: Vec<u8>,
    pub created_at_ms: i64,
}

pub fn install_schema(connection: &Transaction<'_>) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_artifact_revisions (
            artifact_id TEXT NOT NULL, project_id TEXT NOT NULL,
            revision INTEGER NOT NULL, state TEXT NOT NULL,
            content_locator TEXT NOT NULL, content_hash TEXT NOT NULL,
            metadata_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL,
            PRIMARY KEY (artifact_id, revision),
            UNIQUE (project_id, artifact_id, revision)
        );
        CREATE INDEX IF NOT EXISTS idx_project_artifact_project
            ON project_artifact_revisions(project_id, created_at_ms);
        CREATE TABLE IF NOT EXISTS artifact_lineage_edges (
            artifact_id TEXT NOT NULL, artifact_revision INTEGER NOT NULL,
            parent_artifact_id TEXT NOT NULL, parent_revision INTEGER NOT NULL,
            PRIMARY KEY (artifact_id, artifact_revision, parent_artifact_id, parent_revision),
            FOREIGN KEY (artifact_id, artifact_revision)
                REFERENCES project_artifact_revisions(artifact_id, revision)
        );
        CREATE TABLE IF NOT EXISTS artifact_handoffs (
            handoff_id TEXT PRIMARY KEY NOT NULL, artifact_id TEXT NOT NULL,
            artifact_revision INTEGER NOT NULL, producer_identity TEXT NOT NULL,
            consumer_identity TEXT NOT NULL, state TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS artifact_acceptances (
            handoff_id TEXT PRIMARY KEY NOT NULL, decision TEXT NOT NULL,
            reason TEXT NOT NULL, decided_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS artifact_registry_commands (
            idempotency_key TEXT PRIMARY KEY NOT NULL, correlation_id TEXT NOT NULL,
            operation TEXT NOT NULL, payload_hash TEXT NOT NULL,
            outcome_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL
        );
        PRAGMA user_version = 55;",
    )
}

pub fn insert_revision(
    tx: &Transaction<'_>,
    row: &RegistryRow,
    parents: &[(String, u64)],
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO project_artifact_revisions
         (artifact_id,project_id,revision,state,content_locator,content_hash,metadata_json,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![row.artifact_id, row.project_id, row.revision as i64, row.state,
            row.content_locator, row.content_hash, row.metadata_json, row.created_at_ms],
    )?;
    for (parent_id, parent_revision) in parents {
        tx.execute(
            "INSERT INTO artifact_lineage_edges
             (artifact_id,artifact_revision,parent_artifact_id,parent_revision)
             VALUES (?1,?2,?3,?4)",
            params![
                row.artifact_id,
                row.revision as i64,
                parent_id,
                *parent_revision as i64
            ],
        )?;
    }
    Ok(())
}

pub fn insert_revision_atomic(
    connection: &Connection,
    row: &RegistryRow,
    parents: &[(String, u64)],
) -> rusqlite::Result<()> {
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| {
        connection.execute("INSERT INTO project_artifact_revisions (artifact_id,project_id,revision,state,content_locator,content_hash,metadata_json,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![row.artifact_id,row.project_id,row.revision as i64,row.state,row.content_locator,row.content_hash,row.metadata_json,row.created_at_ms])?;
        for (parent_id, parent_revision) in parents {
            connection.execute("INSERT INTO artifact_lineage_edges (artifact_id,artifact_revision,parent_artifact_id,parent_revision) VALUES (?1,?2,?3,?4)", params![row.artifact_id,row.revision as i64,parent_id,*parent_revision as i64])?;
        }
        Ok::<(), rusqlite::Error>(())
    })();
    match result {
        Ok(()) => connection.execute_batch("COMMIT"),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

pub fn list(
    connection: &Connection,
    project_id: &str,
    limit: u32,
) -> rusqlite::Result<Vec<RegistryRow>> {
    let mut statement = connection.prepare(
        "SELECT artifact_id,project_id,revision,state,content_locator,content_hash,metadata_json,created_at_ms
         FROM project_artifact_revisions WHERE project_id=?1 ORDER BY created_at_ms DESC LIMIT ?2",
    )?;
    let rows = statement.query_map(params![project_id, limit as i64], |row| {
        Ok(RegistryRow {
            artifact_id: row.get(0)?,
            project_id: row.get(1)?,
            revision: row.get::<_, i64>(2)? as u64,
            state: row.get(3)?,
            content_locator: row.get(4)?,
            content_hash: row.get(5)?,
            metadata_json: row.get(6)?,
            created_at_ms: row.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn get(
    connection: &Connection,
    artifact_id: &str,
    revision: u64,
) -> rusqlite::Result<Option<RegistryRow>> {
    connection.query_row(
        "SELECT artifact_id,project_id,revision,state,content_locator,content_hash,metadata_json,created_at_ms
         FROM project_artifact_revisions WHERE artifact_id=?1 AND revision=?2",
        params![artifact_id, revision as i64],
        |row| Ok(RegistryRow {
            artifact_id: row.get(0)?, project_id: row.get(1)?, revision: row.get::<_, i64>(2)? as u64,
            state: row.get(3)?, content_locator: row.get(4)?, content_hash: row.get(5)?,
            metadata_json: row.get(6)?, created_at_ms: row.get(7)?,
        }),
    ).optional()
}

pub fn record_command(
    connection: &Connection,
    key: &str,
    correlation: &str,
    operation: &str,
    payload_hash: &str,
    outcome: &[u8],
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "INSERT OR IGNORE INTO artifact_registry_commands
         (idempotency_key,correlation_id,operation,payload_hash,outcome_json,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![key, correlation, operation, payload_hash, outcome, now_ms],
    )?;
    Ok(changed == 1)
}

pub fn command_outcome(connection: &Connection, key: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT outcome_json FROM artifact_registry_commands WHERE idempotency_key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()
}

pub fn transition(
    connection: &Connection,
    artifact_id: &str,
    revision: u64,
    state: &str,
) -> rusqlite::Result<bool> {
    Ok(connection.execute(
        "UPDATE project_artifact_revisions SET state=?3 WHERE artifact_id=?1 AND revision=?2",
        params![artifact_id, revision as i64, state],
    )? == 1)
}

pub fn insert_handoff(
    connection: &Connection,
    id: &str,
    artifact_id: &str,
    revision: u64,
    producer: &str,
    consumer: &str,
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO artifact_handoffs (handoff_id,artifact_id,artifact_revision,producer_identity,consumer_identity,state,created_at_ms) VALUES (?1,?2,?3,?4,?5,'pending',?6)", params![id, artifact_id, revision as i64, producer, consumer, now_ms])?;
    Ok(())
}

pub fn accept_handoff(
    connection: &Connection,
    id: &str,
    decision: &str,
    reason: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = connection.execute(
        "UPDATE artifact_handoffs SET state=?2 WHERE handoff_id=?1 AND state='pending'",
        params![id, decision],
    )?;
    if changed == 1 {
        connection.execute("INSERT OR REPLACE INTO artifact_acceptances (handoff_id,decision,reason,decided_at_ms) VALUES (?1,?2,?3,?4)", params![id, decision, reason, now_ms])?;
    }
    Ok(changed == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_is_additive_and_idempotent() {
        let mut connection = Connection::open_in_memory().unwrap();
        let tx = connection.transaction().unwrap();
        install_schema(&tx).unwrap();
        tx.commit().unwrap();
        let tx = connection.transaction().unwrap();
        install_schema(&tx).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            55
        );
    }

    #[test]
    fn command_idempotency_is_unique() {
        let mut connection = Connection::open_in_memory().unwrap();
        let tx = connection.transaction().unwrap();
        install_schema(&tx).unwrap();
        tx.commit().unwrap();
        assert!(record_command(&connection, "k", "c", "publish", "h", b"{}", 1).unwrap());
        assert!(!record_command(&connection, "k", "c", "publish", "h", b"{}", 2).unwrap());
    }
}
