use rusqlite::{params, Connection};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS approval_policy_profiles (id TEXT PRIMARY KEY, version INTEGER NOT NULL, enabled INTEGER NOT NULL, profile_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    v: u32,
    e: bool,
    j: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    if j.len() > 256 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "policy too large"),
        )));
    }
    Ok(c.execute(
        "INSERT INTO approval_policy_profiles(id,version,enabled,profile_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET version=excluded.version,enabled=excluded.enabled,profile_json=excluded.profile_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.version > approval_policy_profiles.version",
        params![id, v, e, j, now],
    )? == 1)
}

pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s = c.prepare("SELECT profile_json FROM approval_policy_profiles ORDER BY id")?;
    let rows = s.query_map([], |r| r.get(0))?.collect();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_version_cannot_replace_current_policy() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(&c, "policy", 2, true, br#"{"version":2}"#, 2).unwrap());
        assert!(!put(&c, "policy", 1, false, br#"{"version":1}"#, 3).unwrap());
        assert_eq!(list(&c).unwrap(), vec![br#"{"version":2}"#.to_vec()]);
    }
}
