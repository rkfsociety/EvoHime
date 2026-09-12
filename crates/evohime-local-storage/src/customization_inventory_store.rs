use rusqlite::{params, Connection};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS customization_inventory (id TEXT PRIMARY KEY, kind TEXT NOT NULL, version INTEGER NOT NULL, item_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    kind: &str,
    v: u32,
    j: &[u8],
    now: i64,
) -> rusqlite::Result<bool> {
    if j.len() > 256 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "item too large"),
        )));
    }
    Ok(c.execute(
        "INSERT INTO customization_inventory(id,kind,version,item_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,version=excluded.version,item_json=excluded.item_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.version > customization_inventory.version",
        params![id, kind, v, j, now],
    )? == 1)
}
pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s = c.prepare("SELECT item_json FROM customization_inventory ORDER BY kind,id")?;
    let rows = s
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<Vec<u8>>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_version_cannot_replace_inventory_item() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(&c, "item", "model", 2, br#"{"version":2}"#, 2).unwrap());
        assert!(!put(&c, "item", "old", 1, br#"{"version":1}"#, 3).unwrap());
        assert_eq!(list(&c).unwrap(), vec![br#"{"version":2}"#.to_vec()]);
    }
}
