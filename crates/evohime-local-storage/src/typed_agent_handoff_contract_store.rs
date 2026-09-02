//! Durable HandoffRecord storage (schema v64), metadata only.
use rusqlite::{params, Connection, OptionalExtension};
pub type HandoffStorageRow = (Vec<u8>, Vec<u8>, String, u64);
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS typed_agent_handoffs (handoff_id TEXT PRIMARY KEY NOT NULL, packet_json BLOB NOT NULL, state_json BLOB NOT NULL, state TEXT NOT NULL, version INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    packet: &[u8],
    state: &[u8],
    status: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO typed_agent_handoffs(handoff_id,packet_json,state_json,state,version,updated_at_ms) VALUES (?1,?2,?3,?4,1,?5)", params![id,packet,state,status,now])? == 1)
}
pub fn transition(
    c: &Connection,
    id: &str,
    state: &[u8],
    status: &str,
    expected: u64,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("UPDATE typed_agent_handoffs SET state_json=?1,state=?2,version=version+1,updated_at_ms=?3 WHERE handoff_id=?4 AND version=?5", params![state,status,now,id,expected as i64])? == 1)
}
pub fn load(c: &Connection, id: &str) -> rusqlite::Result<Option<HandoffStorageRow>> {
    c.query_row(
        "SELECT packet_json,state_json,state,version FROM typed_agent_handoffs WHERE handoff_id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .optional()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_and_stale_are_fenced() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(&c, "h", b"p", b"s", "proposed", 1).unwrap());
        assert!(!put(&c, "h", b"p", b"s", "proposed", 2).unwrap());
        assert!(!transition(&c, "h", b"s2", "accepted", 0, 2).unwrap());
        assert!(transition(&c, "h", b"s2", "accepted", 1, 2).unwrap());
        assert_eq!(load(&c, "h").unwrap().unwrap().3, 2);
    }
}
