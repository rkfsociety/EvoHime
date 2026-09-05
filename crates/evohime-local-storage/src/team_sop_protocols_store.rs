//! Durable metadata store for Team SOP definitions and immutable sessions.
use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> Result<(), rusqlite::Error> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS team_sop_protocols (id TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL, content_hash TEXT NOT NULL, protocol_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS team_sop_protocol_revisions (protocol_id TEXT NOT NULL, version INTEGER NOT NULL, content_hash TEXT NOT NULL, protocol_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(protocol_id, version)); CREATE TABLE IF NOT EXISTS team_sop_sessions (id TEXT PRIMARY KEY NOT NULL, protocol_id TEXT NOT NULL, protocol_version INTEGER NOT NULL, content_hash TEXT NOT NULL, snapshot_json BLOB NOT NULL, status TEXT NOT NULL, current_phase TEXT NOT NULL, version INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS team_sop_transitions (session_id TEXT NOT NULL, version INTEGER NOT NULL, event_type TEXT NOT NULL, metadata_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(session_id, version));")
}
pub fn save_protocol(
    c: &Connection,
    id: &str,
    version: u64,
    hash: &str,
    json: &[u8],
    now: i64,
) -> Result<bool, rusqlite::Error> {
    let tx = c.unchecked_transaction()?;
    let cur: Option<u64> = tx
        .query_row(
            "SELECT version FROM team_sop_protocols WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    if cur.is_some_and(|v| v >= version) {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO team_sop_protocol_revisions VALUES(?1,?2,?3,?4,?5)",
        [
            id,
            &(version as i64).to_string(),
            hash,
            std::str::from_utf8(json).unwrap_or(""),
            &now.to_string(),
        ],
    )?;
    tx.execute("INSERT INTO team_sop_protocols VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET version=excluded.version,content_hash=excluded.content_hash,protocol_json=excluded.protocol_json,updated_at_ms=excluded.updated_at_ms",params![id,version as i64,hash,json,now])?;
    tx.commit()?;
    Ok(true)
}
#[derive(Clone, Copy)]
pub struct SaveSessionInput<'a> {
    pub id: &'a str,
    pub protocol_id: &'a str,
    pub protocol_version: u64,
    pub hash: &'a str,
    pub snapshot: &'a [u8],
    pub status: &'a str,
    pub phase: &'a str,
    pub version: u64,
    pub now_ms: i64,
}

pub fn save_session(c: &Connection, input: SaveSessionInput<'_>) -> Result<(), rusqlite::Error> {
    c.execute("INSERT INTO team_sop_sessions VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET status=excluded.status,current_phase=excluded.current_phase,version=excluded.version,updated_at_ms=excluded.updated_at_ms",params![input.id,input.protocol_id,input.protocol_version as i64,input.hash,input.snapshot,input.status,input.phase,input.version as i64,input.now_ms])?;
    Ok(())
}
pub fn load_all_json(c: &Connection) -> Result<Vec<Vec<u8>>, rusqlite::Error> {
    let mut s = c.prepare("SELECT protocol_json FROM team_sop_protocols ORDER BY id")?;
    let rows = s.query_map([], |r| r.get(0))?.collect();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_revision_is_immutable_and_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(save_protocol(&c, "coding", 1, "h", br#"{}"#, 1).unwrap());
        assert!(!save_protocol(&c, "coding", 1, "h", br#"{}"#, 2).unwrap());
        assert_eq!(load_all_json(&c).unwrap().len(), 1);
    }
}
