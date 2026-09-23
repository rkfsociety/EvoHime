use rusqlite::{params, Connection, OptionalExtension};

/// Version of the team coordinator persistence schema.
pub const STORE_SCHEMA_VERSION: u32 = 1;

/// Creates team work-item, assignment, consultation, decision, and idempotency tables.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS team_coordinator_work_items (
           work_item_id TEXT PRIMARY KEY NOT NULL,
           revision INTEGER NOT NULL,
           status TEXT NOT NULL,
           assigned_instance_id TEXT,
           attempt INTEGER NOT NULL,
           item_json BLOB NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS team_coordinator_assignments (
           assignment_id TEXT PRIMARY KEY NOT NULL,
           work_item_id TEXT NOT NULL,
           target_instance_id TEXT NOT NULL,
           proposal_json BLOB NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS team_coordinator_consultations (
           consultation_id TEXT PRIMARY KEY NOT NULL,
           query_json BLOB NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS team_coordinator_decisions (
           decision_id TEXT PRIMARY KEY NOT NULL,
           work_item_id TEXT NOT NULL,
           decision_json BLOB NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS team_coordinator_idempotency (
           idempotency_key TEXT PRIMARY KEY NOT NULL,
           result_json BLOB NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_team_coordinator_work_status
           ON team_coordinator_work_items(status, updated_at_ms);",
    )
}

/// Loads a previously stored result for an idempotency key.
pub fn get_idempotency(connection: &Connection, key: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT result_json FROM team_coordinator_idempotency WHERE idempotency_key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()
}

/// Persists the first result for an idempotency key and leaves any prior result unchanged.
pub fn put_idempotency(
    connection: &Connection,
    key: &str,
    result_json: &[u8],
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO team_coordinator_idempotency(idempotency_key,result_json) VALUES (?1,?2)",
        params![key, result_json],
    )?;
    Ok(())
}

/// Fields required to insert one coordinator work item.
pub struct PutWorkItemInput<'a> {
    /// Stable work-item identifier.
    pub item_id: &'a str,
    /// Revision to persist.
    pub revision: i64,
    /// Current work-item status.
    pub status: &'a str,
    /// Optional agent instance assigned to the item.
    pub assigned_instance_id: Option<&'a str>,
    /// Number of dispatch attempts made for the item.
    pub attempt: i64,
    /// Serialized work-item payload.
    pub item_json: &'a [u8],
    /// Last update time in Unix milliseconds.
    pub now_ms: i64,
}

/// Inserts a work item and its serialized coordinator state.
pub fn put_work_item(connection: &Connection, input: PutWorkItemInput<'_>) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO team_coordinator_work_items(work_item_id,revision,status,assigned_instance_id,attempt,item_json,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![input.item_id, input.revision, input.status, input.assigned_instance_id, input.attempt, input.item_json, input.now_ms])?;
    Ok(())
}

/// Loads a work item's serialized state by identifier.
pub fn get_work_item(connection: &Connection, item_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT item_json FROM team_coordinator_work_items WHERE work_item_id=?1",
            [item_id],
            |row| row.get(0),
        )
        .optional()
}

/// Lists serialized work items newest update first, up to the requested limit.
pub fn list_work_items(connection: &Connection, limit: usize) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = connection.prepare(
        "SELECT item_json FROM team_coordinator_work_items ORDER BY updated_at_ms DESC LIMIT ?1",
    )?;
    let rows = statement.query_map([limit as i64], |row| row.get(0))?;
    rows.collect()
}

/// Inputs for an optimistic work-item replacement.
pub struct ReplaceWorkItemInput<'a> {
    /// Work-item identifier to replace.
    pub item_id: &'a str,
    /// Revision that must currently be stored.
    pub expected_revision: i64,
    /// New revision to store.
    pub revision: i64,
    /// New work-item status.
    pub status: &'a str,
    /// Optional assigned agent instance.
    pub assigned_instance_id: Option<&'a str>,
    /// Updated dispatch attempt count.
    pub attempt: i64,
    /// Serialized replacement payload.
    pub item_json: &'a [u8],
    /// Update time in Unix milliseconds.
    pub now_ms: i64,
}

/// Replaces an item only when its current revision matches `expected_revision`.
///
/// Returns `false` when the item is missing or has advanced since it was read.
pub fn replace_work_item(
    connection: &Connection,
    input: ReplaceWorkItemInput<'_>,
) -> rusqlite::Result<bool> {
    Ok(connection.execute("UPDATE team_coordinator_work_items SET revision=?1,status=?2,assigned_instance_id=?3,attempt=?4,item_json=?5,updated_at_ms=?6 WHERE work_item_id=?7 AND revision=?8", params![input.revision, input.status, input.assigned_instance_id, input.attempt, input.item_json, input.now_ms, input.item_id, input.expected_revision])? == 1)
}

/// Persists a proposed assignment between a work item and a target agent instance.
pub fn put_assignment(
    connection: &Connection,
    assignment_id: &str,
    work_item_id: &str,
    target_instance_id: &str,
    proposal_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO team_coordinator_assignments(assignment_id,work_item_id,target_instance_id,proposal_json,created_at_ms) VALUES (?1,?2,?3,?4,?5)", params![assignment_id, work_item_id, target_instance_id, proposal_json, now_ms])?;
    Ok(())
}

/// Persists a serialized consultation request.
pub fn put_consultation(
    connection: &Connection,
    consultation_id: &str,
    query_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO team_coordinator_consultations(consultation_id,query_json,created_at_ms) VALUES (?1,?2,?3)", params![consultation_id, query_json, now_ms])?;
    Ok(())
}

/// Persists a serialized coordinator decision for a work item.
pub fn put_decision(
    connection: &Connection,
    decision_id: &str,
    work_item_id: &str,
    decision_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO team_coordinator_decisions(decision_id,work_item_id,decision_json,created_at_ms) VALUES (?1,?2,?3,?4)", params![decision_id, work_item_id, decision_json, now_ms])?;
    Ok(())
}

#[cfg(test)]
#[path = "team_coordinator_store_tests.rs"]
mod tests;
