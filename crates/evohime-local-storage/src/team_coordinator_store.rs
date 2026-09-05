use rusqlite::{params, Connection, OptionalExtension};

pub const STORE_SCHEMA_VERSION: u32 = 1;

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

pub fn get_idempotency(connection: &Connection, key: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT result_json FROM team_coordinator_idempotency WHERE idempotency_key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()
}

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

pub struct PutWorkItemInput<'a> {
    pub item_id: &'a str,
    pub revision: i64,
    pub status: &'a str,
    pub assigned_instance_id: Option<&'a str>,
    pub attempt: i64,
    pub item_json: &'a [u8],
    pub now_ms: i64,
}

pub fn put_work_item(connection: &Connection, input: PutWorkItemInput<'_>) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO team_coordinator_work_items(work_item_id,revision,status,assigned_instance_id,attempt,item_json,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![input.item_id, input.revision, input.status, input.assigned_instance_id, input.attempt, input.item_json, input.now_ms])?;
    Ok(())
}

pub fn get_work_item(connection: &Connection, item_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT item_json FROM team_coordinator_work_items WHERE work_item_id=?1",
            [item_id],
            |row| row.get(0),
        )
        .optional()
}

pub fn list_work_items(connection: &Connection, limit: usize) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = connection.prepare(
        "SELECT item_json FROM team_coordinator_work_items ORDER BY updated_at_ms DESC LIMIT ?1",
    )?;
    let rows = statement.query_map([limit as i64], |row| row.get(0))?;
    rows.collect()
}

pub struct ReplaceWorkItemInput<'a> {
    pub item_id: &'a str,
    pub expected_revision: i64,
    pub revision: i64,
    pub status: &'a str,
    pub assigned_instance_id: Option<&'a str>,
    pub attempt: i64,
    pub item_json: &'a [u8],
    pub now_ms: i64,
}

pub fn replace_work_item(
    connection: &Connection,
    input: ReplaceWorkItemInput<'_>,
) -> rusqlite::Result<bool> {
    Ok(connection.execute("UPDATE team_coordinator_work_items SET revision=?1,status=?2,assigned_instance_id=?3,attempt=?4,item_json=?5,updated_at_ms=?6 WHERE work_item_id=?7 AND revision=?8", params![input.revision, input.status, input.assigned_instance_id, input.attempt, input.item_json, input.now_ms, input.item_id, input.expected_revision])? == 1)
}

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

pub fn put_consultation(
    connection: &Connection,
    consultation_id: &str,
    query_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO team_coordinator_consultations(consultation_id,query_json,created_at_ms) VALUES (?1,?2,?3)", params![consultation_id, query_json, now_ms])?;
    Ok(())
}

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
mod tests {
    use super::*;

    #[test]
    fn work_item_update_is_revision_fenced_and_tables_are_additive() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        put_work_item(
            &connection,
            PutWorkItemInput {
                item_id: "w",
                revision: 1,
                status: "unassigned",
                assigned_instance_id: None,
                attempt: 0,
                item_json: b"{}",
                now_ms: 1,
            },
        )
        .unwrap();
        assert!(!replace_work_item(
            &connection,
            ReplaceWorkItemInput {
                item_id: "w",
                expected_revision: 0,
                revision: 2,
                status: "assigned",
                assigned_instance_id: Some("a"),
                attempt: 1,
                item_json: b"{}",
                now_ms: 2
            }
        )
        .unwrap());
        assert!(replace_work_item(
            &connection,
            ReplaceWorkItemInput {
                item_id: "w",
                expected_revision: 1,
                revision: 2,
                status: "assigned",
                assigned_instance_id: Some("a"),
                attempt: 1,
                item_json: b"{}",
                now_ms: 2
            }
        )
        .unwrap());
        assert_eq!(
            get_work_item(&connection, "w").unwrap(),
            Some(b"{}".to_vec())
        );
        put_idempotency(&connection, "k", b"result").unwrap();
        assert_eq!(
            get_idempotency(&connection, "k").unwrap(),
            Some(b"result".to_vec())
        );
    }
}
