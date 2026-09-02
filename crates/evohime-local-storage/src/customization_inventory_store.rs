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
) -> rusqlite::Result<()> {
    if j.len() > 256 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "item too large"),
        )));
    }
    c.execute(
        "INSERT OR REPLACE INTO customization_inventory VALUES(?1,?2,?3,?4,?5)",
        params![id, kind, v, j, now],
    )?;
    Ok(())
}
pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s = c.prepare("SELECT item_json FROM customization_inventory ORDER BY kind,id")?;
    let rows = s
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<Vec<u8>>>>()?;
    Ok(rows)
}
