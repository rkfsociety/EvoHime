use rusqlite::{params, Connection};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS approval_policy_profiles (id TEXT PRIMARY KEY, version INTEGER NOT NULL, enabled INTEGER NOT NULL, profile_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(c: &Connection, id: &str, v: u32, e: bool, j: &[u8], now: i64) -> rusqlite::Result<()> {
    if j.len() > 256 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "policy too large"),
        )));
    }
    c.execute(
        "INSERT OR REPLACE INTO approval_policy_profiles VALUES(?1,?2,?3,?4,?5)",
        params![id, v, e, j, now],
    )?;
    Ok(())
}
pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s = c.prepare("SELECT profile_json FROM approval_policy_profiles ORDER BY id")?;
    let rows = s.query_map([], |r| r.get(0))?.collect();
    rows
}
