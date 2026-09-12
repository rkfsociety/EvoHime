use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS event_visualizer_registry (id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_hash TEXT NOT NULL, descriptor_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    version: u32,
    hash: &str,
    json: &[u8],
    now: i64,
) -> rusqlite::Result<()> {
    if json.len() > 256 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "descriptor too large"),
        )));
    }
    c.execute("INSERT INTO event_visualizer_registry(id,version,content_hash,descriptor_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET version=excluded.version,content_hash=excluded.content_hash,descriptor_json=excluded.descriptor_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.version >= event_visualizer_registry.version",params![id,version,hash,json,now])?;
    Ok(())
}
pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s = c.prepare("SELECT descriptor_json FROM event_visualizer_registry ORDER BY id")?;
    let rows = s.query_map([], |r| r.get(0))?;
    rows.collect()
}
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT descriptor_json FROM event_visualizer_registry WHERE id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put(&c, "x", 1, "h", b"{}", 1).unwrap();
        assert_eq!(get(&c, "x").unwrap(), Some(b"{}".to_vec()));
    }

    #[test]
    fn stale_version_cannot_replace_descriptor() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put(&c, "x", 2, "new", br#"{"version":2}"#, 2).unwrap();
        put(&c, "x", 1, "old", br#"{"version":1}"#, 3).unwrap();
        assert_eq!(get(&c, "x").unwrap(), Some(br#"{"version":2}"#.to_vec()));
    }
}
