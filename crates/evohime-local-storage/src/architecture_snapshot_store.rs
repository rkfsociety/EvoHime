use rusqlite::{params, Connection, OptionalExtension, Result};

pub const MAX_RECORD_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRecord {
    pub workspace_identity: String,
    pub source_revision: String,
    pub snapshot_hash: String,
    pub state: String,
    pub record_json: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct PutInput<'a> {
    pub snapshot_id: &'a str,
    pub workspace_identity: &'a str,
    pub source_revision: &'a str,
    pub snapshot_hash: &'a str,
    pub state: &'a str,
    pub record_json: &'a [u8],
    pub updated_at_ms: i64,
}

pub fn install_schema(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS architecture_snapshot_records (snapshot_id TEXT PRIMARY KEY, workspace_identity TEXT NOT NULL, source_revision TEXT NOT NULL, snapshot_hash TEXT NOT NULL, state TEXT NOT NULL, record_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_architecture_snapshot_workspace ON architecture_snapshot_records(workspace_identity, updated_at_ms DESC); CREATE TABLE IF NOT EXISTS architecture_snapshot_refresh (snapshot_id TEXT PRIMARY KEY, state TEXT NOT NULL, last_error TEXT, updated_at_ms INTEGER NOT NULL);")
}

pub fn set_refresh_state(
    c: &Connection,
    snapshot_id: &str,
    state: &str,
    error: Option<&str>,
    now_ms: i64,
) -> Result<()> {
    c.execute("INSERT INTO architecture_snapshot_refresh(snapshot_id,state,last_error,updated_at_ms) VALUES(?1,?2,?3,?4) ON CONFLICT(snapshot_id) DO UPDATE SET state=excluded.state,last_error=excluded.last_error,updated_at_ms=excluded.updated_at_ms", params![snapshot_id, state, error, now_ms])?;
    Ok(())
}

pub fn put(c: &Connection, input: PutInput<'_>) -> Result<bool> {
    if input.record_json.len() > MAX_RECORD_BYTES {
        return Ok(false);
    }
    Ok(c.execute("INSERT INTO architecture_snapshot_records(snapshot_id,workspace_identity,source_revision,snapshot_hash,state,record_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(snapshot_id) DO UPDATE SET workspace_identity=excluded.workspace_identity,source_revision=excluded.source_revision,snapshot_hash=excluded.snapshot_hash,state=excluded.state,record_json=excluded.record_json,updated_at_ms=excluded.updated_at_ms", params![input.snapshot_id, input.workspace_identity, input.source_revision, input.snapshot_hash, input.state, input.record_json, input.updated_at_ms])? == 1)
}

pub fn get(c: &Connection, id: &str) -> Result<Option<SnapshotRecord>> {
    c.query_row("SELECT workspace_identity,source_revision,snapshot_hash,state,record_json FROM architecture_snapshot_records WHERE snapshot_id=?1", [id], |r| Ok(SnapshotRecord { workspace_identity: r.get(0)?, source_revision: r.get(1)?, snapshot_hash: r.get(2)?, state: r.get(3)?, record_json: r.get(4)? })).optional()
}

pub fn list(
    c: &Connection,
    workspace_identity: &str,
    limit: u32,
) -> Result<Vec<(String, String, String, String)>> {
    let mut s=c.prepare("SELECT snapshot_id,source_revision,snapshot_hash,state FROM architecture_snapshot_records WHERE workspace_identity=?1 ORDER BY updated_at_ms DESC LIMIT ?2")?;
    let rows = s.query_map(
        params![workspace_identity, i64::from(limit.min(256))],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_metadata_roundtrip() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(
            &c,
            PutInput {
                snapshot_id: "s",
                workspace_identity: "w",
                source_revision: "r",
                snapshot_hash: "h",
                state: "accepted",
                record_json: b"{}",
                updated_at_ms: 1
            }
        )
        .unwrap());
        assert_eq!(get(&c, "s").unwrap().unwrap().snapshot_hash, "h");
        assert_eq!(list(&c, "w", 10).unwrap().len(), 1);
    }
}
