use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS batch_invocations (id TEXT PRIMARY KEY, version INTEGER NOT NULL, status TEXT NOT NULL, content_hash TEXT NOT NULL, batch_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    version: u64,
    status: &str,
    hash: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    if version == 1 {
        return Ok(c.execute(
            "INSERT OR IGNORE INTO batch_invocations VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, version as i64, status, hash, json, now],
        )? == 1);
    }
    Ok(c.execute("UPDATE batch_invocations SET version=?1,status=?2,content_hash=?3,batch_json=?4,updated_at_ms=?5 WHERE id=?6 AND version=?7", params![version as i64,status,hash,json,now,id,(version-1) as i64])? == 1)
}
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<(u64, Vec<u8>)>> {
    c.query_row(
        "SELECT version,batch_json FROM batch_invocations WHERE id=?1",
        params![id],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get(1)?)),
    )
    .optional()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optimistic_fence_is_stale_safe() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(&c, "b", 1, "pending", "h", b"{}", 1).unwrap());
        assert!(!put(&c, "b", 3, "running", "h", b"{}", 2).unwrap());
        assert!(put(&c, "b", 2, "running", "h", b"{}", 2).unwrap());
        assert_eq!(get(&c, "b").unwrap().unwrap().0, 2);
    }
}
