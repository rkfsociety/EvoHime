use rusqlite::{params, Connection, OptionalExtension, Result};
pub fn install_schema(c: &Connection) -> Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS policy_aware_tool_result_cache (cache_key TEXT PRIMARY KEY, version INTEGER NOT NULL, entry_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(c: &Connection, key: &str, version: u64, json: &[u8], now: i64) -> Result<bool> {
    if version == 1 {
        return Ok(c.execute("INSERT OR IGNORE INTO policy_aware_tool_result_cache(cache_key,version,entry_json,updated_at_ms) VALUES(?1,?2,?3,?4)",params![key,version,json,now])? == 1);
    }
    Ok(c.execute("UPDATE policy_aware_tool_result_cache SET version=?2,entry_json=?3,updated_at_ms=?4 WHERE cache_key=?1 AND version=?2-1",params![key,version,json,now])? == 1)
}
pub fn get(c: &Connection, key: &str) -> Result<Option<(u64, Vec<u8>)>> {
    c.query_row(
        "SELECT version,entry_json FROM policy_aware_tool_result_cache WHERE cache_key=?1",
        params![key],
        |r| Ok((r.get(0)?, r.get(1)?)),
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
        assert!(put(&c, "k", 1, b"{}", 1).unwrap());
        assert!(!put(&c, "k", 3, b"{}", 2).unwrap());
        assert_eq!(get(&c, "k").unwrap().unwrap().0, 1);
    }
}
