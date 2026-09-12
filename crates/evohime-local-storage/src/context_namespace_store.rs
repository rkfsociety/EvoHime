//! Durable metadata store for Context Namespace.  Authoritative source data
//! and raw model prompts never enter these tables.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_RECORD_BYTES: usize = 128 * 1024;

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS context_namespace_nodes (node_id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, node_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS context_namespace_projections (node_id TEXT NOT NULL, level TEXT NOT NULL, source_revision INTEGER NOT NULL, projection_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(node_id, level)); CREATE TABLE IF NOT EXISTS context_namespace_views (view_id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, view_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS context_namespace_traces (trace_id TEXT PRIMARY KEY NOT NULL, run_id TEXT NOT NULL, trace_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS context_namespace_idempotency (scope TEXT NOT NULL, idempotency_key TEXT NOT NULL, command_hash TEXT NOT NULL, response_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(scope, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_context_namespace_nodes_revision ON context_namespace_nodes(revision); CREATE INDEX IF NOT EXISTS idx_context_namespace_traces_run ON context_namespace_traces(run_id, created_at_ms DESC);")
}

fn bounded(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= MAX_RECORD_BYTES
}

pub fn put_node(
    c: &Connection,
    node_id: &str,
    revision: u64,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    if !bounded(json) || revision == 0 || revision > i64::MAX as u64 {
        return Ok(false);
    }
    Ok(c.execute("INSERT INTO context_namespace_nodes(node_id,revision,node_json,updated_at_ms) VALUES (?1,?2,?3,?4) ON CONFLICT(node_id) DO UPDATE SET revision=excluded.revision,node_json=excluded.node_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision > context_namespace_nodes.revision", params![node_id, revision as i64, json, now])? == 1)
}
pub fn get_node(c: &Connection, node_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT node_json FROM context_namespace_nodes WHERE node_id=?1",
        [node_id],
        |row| row.get(0),
    )
    .optional()
}
pub fn list_nodes(c: &Connection, limit: usize) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s =
        c.prepare("SELECT node_json FROM context_namespace_nodes ORDER BY node_id LIMIT ?1")?;
    let rows = s
        .query_map([limit.min(4096) as i64], |row| row.get(0))?
        .collect();
    rows
}
pub fn list_children(
    c: &Connection,
    parent_ref: &str,
    limit: usize,
) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s=c.prepare("SELECT node_json FROM context_namespace_nodes WHERE json_extract(node_json, '$.logical_parent_ref')=?1 ORDER BY node_id LIMIT ?2")?;
    let rows = s
        .query_map(params![parent_ref, limit.min(4096) as i64], |row| {
            row.get(0)
        })?
        .collect();
    rows
}
pub fn put_projection(
    c: &Connection,
    node_id: &str,
    level: &str,
    revision: u64,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    if !bounded(json) || revision == 0 || revision > i64::MAX as u64 {
        return Ok(false);
    }
    Ok(c.execute("INSERT INTO context_namespace_projections(node_id,level,source_revision,projection_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(node_id,level) DO UPDATE SET source_revision=excluded.source_revision,projection_json=excluded.projection_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.source_revision > context_namespace_projections.source_revision", params![node_id, level, revision as i64, json, now])? == 1)
}
pub fn list_projections(c: &Connection, limit: usize) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s = c.prepare(
        "SELECT projection_json FROM context_namespace_projections ORDER BY node_id,level LIMIT ?1",
    )?;
    let rows = s
        .query_map([limit.min(12288) as i64], |row| row.get(0))?
        .collect();
    rows
}
pub fn put_view(
    c: &Connection,
    id: &str,
    revision: u64,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    if !bounded(json) || revision == 0 || revision > i64::MAX as u64 {
        return Ok(false);
    }
    Ok(c.execute("INSERT INTO context_namespace_views(view_id,revision,view_json,updated_at_ms) VALUES(?1,?2,?3,?4) ON CONFLICT(view_id) DO UPDATE SET revision=excluded.revision,view_json=excluded.view_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision > context_namespace_views.revision", params![id, revision as i64, json, now])? == 1)
}
pub fn get_view(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT view_json FROM context_namespace_views WHERE view_id=?1",
        [id],
        |row| row.get(0),
    )
    .optional()
}
pub fn put_trace(
    c: &Connection,
    id: &str,
    run_id: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    if !bounded(json) {
        return Ok(false);
    }
    Ok(c.execute("INSERT OR IGNORE INTO context_namespace_traces(trace_id,run_id,trace_json,created_at_ms) VALUES(?1,?2,?3,?4)", params![id, run_id, json, now])? == 1)
}
pub fn get_trace(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT trace_json FROM context_namespace_traces WHERE trace_id=?1",
        [id],
        |row| row.get(0),
    )
    .optional()
}
pub fn load_idempotency(
    c: &Connection,
    scope: &str,
    key: &str,
) -> rusqlite::Result<Option<(String, Vec<u8>)>> {
    c.query_row("SELECT command_hash,response_json FROM context_namespace_idempotency WHERE scope=?1 AND idempotency_key=?2", params![scope,key], |row| Ok((row.get(0)?,row.get(1)?))).optional()
}
pub fn save_idempotency(
    c: &Connection,
    scope: &str,
    key: &str,
    command_hash: &str,
    response: &[u8],
    now: i64,
) -> rusqlite::Result<()> {
    if !bounded(response) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    c.execute("INSERT INTO context_namespace_idempotency(scope,idempotency_key,command_hash,response_json,created_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(scope,idempotency_key) DO NOTHING", params![scope,key,command_hash,response,now])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_revision_is_monotonic_and_projection_bounded() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_node(&c, "n", 2, b"two", 1).unwrap());
        assert!(!put_node(&c, "n", 1, b"one", 2).unwrap());
        assert!(
            !put_projection(&c, "n", "abstract", 1, &vec![b'x'; MAX_RECORD_BYTES + 1], 2).unwrap()
        );
        assert!(!put_node(&c, "zero", 0, b"{}", 3).unwrap());
        assert!(!put_projection(&c, "zero", "abstract", 0, b"{}", 3).unwrap());
        assert!(!put_view(&c, "zero", 0, b"{}", 3).unwrap());
    }
    #[test]
    fn idempotency_is_durable_and_conflicts_are_left_to_core() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        save_idempotency(&c, "scope", "key", "hash", b"{}", 1).unwrap();
        assert_eq!(
            load_idempotency(&c, "scope", "key").unwrap(),
            Some(("hash".into(), b"{}".to_vec()))
        );
        save_idempotency(&c, "scope", "key", "different", br#"{"changed":true}"#, 2).unwrap();
        assert_eq!(
            load_idempotency(&c, "scope", "key").unwrap(),
            Some(("hash".into(), b"{}".to_vec()))
        );
    }
    #[test]
    fn traces_are_insert_once() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_trace(&c, "t", "r", b"{}", 1).unwrap());
        assert!(!put_trace(&c, "t", "r", b"changed", 2).unwrap());
        assert_eq!(get_trace(&c, "t").unwrap(), Some(b"{}".to_vec()));
    }
}
