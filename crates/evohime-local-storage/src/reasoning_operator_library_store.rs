use rusqlite::{params, Connection};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS reasoning_operator_definitions (id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_hash TEXT NOT NULL, definition_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(c: &Connection, id: &str, v: u32, h: &str, j: &[u8], now: i64) -> rusqlite::Result<()> {
    if j.len() > 256 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "definition too large"),
        )));
    }
    c.execute(
        "INSERT INTO reasoning_operator_definitions(id,version,content_hash,definition_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET version=excluded.version,content_hash=excluded.content_hash,definition_json=excluded.definition_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.version >= reasoning_operator_definitions.version",
        params![id, v, h, j, now],
    )?;
    Ok(())
}
pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s =
        c.prepare("SELECT definition_json FROM reasoning_operator_definitions ORDER BY id")?;
    let rows = s.query_map([], |r| r.get(0))?.collect();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_version_cannot_replace_operator_definition() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put(&c, "operator", 2, "new", br#"{"version":2}"#, 2).unwrap();
        put(&c, "operator", 1, "old", br#"{"version":1}"#, 3).unwrap();
        assert_eq!(list(&c).unwrap(), vec![br#"{"version":2}"#.to_vec()]);
    }
}
