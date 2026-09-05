use rusqlite::{params, Connection, OptionalExtension};

pub const STORE_SCHEMA_VERSION: u32 = 1;

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS project_instruction_rules (
           rule_id TEXT PRIMARY KEY NOT NULL,
           revision INTEGER NOT NULL,
           source_kind TEXT NOT NULL,
           source_ref TEXT NOT NULL,
           content_hash TEXT NOT NULL,
           rule_json BLOB NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS project_instruction_snapshots (
           snapshot_id TEXT PRIMARY KEY NOT NULL,
           workspace_root TEXT NOT NULL,
           content_hash TEXT NOT NULL,
           snapshot_json BLOB NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS project_instruction_idempotency (
           idempotency_key TEXT PRIMARY KEY NOT NULL,
           result_json BLOB NOT NULL
         );",
    )
}

#[derive(Clone, Copy)]
pub struct PutRuleInput<'a> {
    pub rule_id: &'a str,
    pub revision: i64,
    pub source_kind: &'a str,
    pub source_ref: &'a str,
    pub content_hash: &'a str,
    pub rule_json: &'a [u8],
    pub now_ms: i64,
}

pub fn put_rule(connection: &Connection, input: PutRuleInput<'_>) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO project_instruction_rules(rule_id,revision,source_kind,source_ref,content_hash,rule_json,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(rule_id) DO UPDATE SET revision=excluded.revision,source_kind=excluded.source_kind,source_ref=excluded.source_ref,content_hash=excluded.content_hash,rule_json=excluded.rule_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision > project_instruction_rules.revision", params![input.rule_id, input.revision, input.source_kind, input.source_ref, input.content_hash, input.rule_json, input.now_ms])?;
    Ok(())
}

pub fn list_rules(connection: &Connection, limit: usize) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = connection
        .prepare("SELECT rule_json FROM project_instruction_rules ORDER BY rule_id LIMIT ?1")?;
    let rows = statement
        .query_map([limit as i64], |row| row.get(0))?
        .collect();
    rows
}

pub fn put_snapshot(
    connection: &Connection,
    snapshot_id: &str,
    workspace_root: &str,
    content_hash: &str,
    snapshot_json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO project_instruction_snapshots(snapshot_id,workspace_root,content_hash,snapshot_json,created_at_ms) VALUES (?1,?2,?3,?4,?5)", params![snapshot_id, workspace_root, content_hash, snapshot_json, now_ms])?;
    Ok(())
}

pub fn get_snapshot(
    connection: &Connection,
    snapshot_id: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT snapshot_json FROM project_instruction_snapshots WHERE snapshot_id=?1",
            [snapshot_id],
            |row| row.get(0),
        )
        .optional()
}

pub fn get_idempotency(connection: &Connection, key: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT result_json FROM project_instruction_idempotency WHERE idempotency_key=?1",
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
    connection.execute("INSERT OR IGNORE INTO project_instruction_idempotency(idempotency_key,result_json) VALUES (?1,?2)", params![key, result_json])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rule_revision_is_monotonic_and_snapshot_is_durable() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let input = PutRuleInput {
            rule_id: "r",
            revision: 2,
            source_kind: "workspace",
            source_ref: "r",
            content_hash: "h2",
            rule_json: b"{}",
            now_ms: 2,
        };
        put_rule(&connection, input).unwrap();
        put_rule(
            &connection,
            PutRuleInput {
                revision: 1,
                source_kind: "workspace",
                source_ref: "r",
                content_hash: "h1",
                rule_json: b"bad",
                now_ms: 3,
                ..input
            },
        )
        .unwrap();
        assert_eq!(list_rules(&connection, 64).unwrap(), vec![b"{}".to_vec()]);
        put_snapshot(&connection, "s", "root", "hash", b"snapshot", 1).unwrap();
        assert_eq!(
            get_snapshot(&connection, "s").unwrap(),
            Some(b"snapshot".to_vec())
        );
    }
}
