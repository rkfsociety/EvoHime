use rusqlite::{params, Connection, OptionalExtension, Result};

pub fn install_schema(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS model_purpose_routing (policy_id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_hash TEXT NOT NULL, policy_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}

pub fn put(
    c: &Connection,
    policy_id: &str,
    version: u64,
    hash: &str,
    json: &[u8],
    now_ms: i64,
) -> Result<bool> {
    if version == 1 {
        return Ok(c.execute("INSERT OR IGNORE INTO model_purpose_routing(policy_id,version,content_hash,policy_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5)", params![policy_id, version, hash, json, now_ms])? == 1);
    }
    Ok(c.execute("UPDATE model_purpose_routing SET version=?2,content_hash=?3,policy_json=?4,updated_at_ms=?5 WHERE policy_id=?1 AND version=?2-1", params![policy_id, version, hash, json, now_ms])? == 1)
}

pub fn get(c: &Connection, policy_id: &str) -> Result<Option<(u64, String, Vec<u8>)>> {
    c.query_row(
        "SELECT version,content_hash,policy_json FROM model_purpose_routing WHERE policy_id=?1",
        [policy_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_write_is_fenced() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(&c, "p", 1, "h", b"{}", 1).unwrap());
        assert!(!put(&c, "p", 3, "h", b"{}", 2).unwrap());
        assert_eq!(get(&c, "p").unwrap().unwrap().0, 1);
    }
}
