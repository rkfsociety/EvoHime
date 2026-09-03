use rusqlite::{params, Connection, OptionalExtension};

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS declarative_runtime_components (component_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, config_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, idempotency_key TEXT NOT NULL UNIQUE);")
}
pub struct SaveInput<'a> {
    pub id: &'a str,
    pub expected: u64,
    pub revision: u64,
    pub json: &'a [u8],
    pub hash: &'a str,
    pub idem: &'a str,
    pub now: i64,
}
pub fn save(c: &Connection, input: SaveInput<'_>) -> rusqlite::Result<bool> {
    if input.expected == 0 {
        return Ok(c.execute("INSERT OR IGNORE INTO declarative_runtime_components(component_id,revision,config_json,content_hash,updated_at_ms,idempotency_key) VALUES(?1,?2,?3,?4,?5,?6)", params![input.id,input.revision,input.json,input.hash,input.now,input.idem])? == 1);
    }
    Ok(c.execute("UPDATE declarative_runtime_components SET revision=?3,config_json=?4,content_hash=?5,updated_at_ms=?6,idempotency_key=?7 WHERE component_id=?1 AND revision=?2", params![input.id,input.expected,input.revision,input.json,input.hash,input.now,input.idem])? == 1)
}
pub fn load(c: &Connection, id: &str) -> rusqlite::Result<Option<(u64, Vec<u8>, String)>> {
    c.query_row("SELECT revision,config_json,content_hash FROM declarative_runtime_components WHERE component_id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optimistic_save_is_idempotent_and_stale_is_rejected() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let input = |expected, revision, hash, idem, now| SaveInput {
            id: "c",
            expected,
            revision,
            json: b"{}",
            hash,
            idem,
            now,
        };
        assert!(save(&c, input(0, 1, "h", "i", 1)).unwrap());
        assert!(!save(&c, input(0, 1, "h", "i", 1)).unwrap());
        assert!(!save(&c, input(9, 2, "h2", "j", 2)).unwrap());
        assert_eq!(load(&c, "c").unwrap().unwrap().0, 1);
    }
}
