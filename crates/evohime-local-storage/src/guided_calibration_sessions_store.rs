use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS guided_calibration_sessions (session_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, session_json BLOB NOT NULL, dataset_hash TEXT NOT NULL, idempotency_key TEXT NOT NULL UNIQUE, updated_at_ms INTEGER NOT NULL);")
}
pub struct SaveInput<'a> {
    pub id: &'a str,
    pub expected: u64,
    pub revision: u64,
    pub json: &'a [u8],
    pub dataset_hash: &'a str,
    pub idempotency_key: &'a str,
    pub now: i64,
}
pub fn save(c: &Connection, input: SaveInput<'_>) -> rusqlite::Result<bool> {
    if input.expected == 0 {
        return Ok(c.execute("INSERT OR IGNORE INTO guided_calibration_sessions(session_id,revision,session_json,dataset_hash,idempotency_key,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6)", params![input.id,input.revision,input.json,input.dataset_hash,input.idempotency_key,input.now])? == 1);
    }
    Ok(c.execute("UPDATE guided_calibration_sessions SET revision=?3,session_json=?4,dataset_hash=?5,idempotency_key=?6,updated_at_ms=?7 WHERE session_id=?1 AND revision=?2",params![input.id,input.expected,input.revision,input.json,input.dataset_hash,input.idempotency_key,input.now])?==1)
}
pub fn load(c: &Connection, id: &str) -> rusqlite::Result<Option<(u64, Vec<u8>, String)>> {
    c.query_row("SELECT revision,session_json,dataset_hash FROM guided_calibration_sessions WHERE session_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_is_fenced() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let i = SaveInput {
            id: "s",
            expected: 0,
            revision: 1,
            json: b"{}",
            dataset_hash: "h",
            idempotency_key: "i",
            now: 1,
        };
        assert!(save(&c, i).unwrap());
        let i = SaveInput {
            id: "s",
            expected: 9,
            revision: 2,
            json: b"{}",
            dataset_hash: "h2",
            idempotency_key: "j",
            now: 2,
        };
        assert!(!save(&c, i).unwrap());
        assert_eq!(load(&c, "s").unwrap().unwrap().0, 1);
    }
}
