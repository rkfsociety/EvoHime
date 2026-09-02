use rusqlite::{params, Connection};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS checkpoint_fork_lineages (fork_run_id TEXT PRIMARY KEY, source_checkpoint_id TEXT NOT NULL, parent_run_id TEXT NOT NULL, lineage_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    source: &str,
    parent: &str,
    j: &[u8],
    now: i64,
) -> rusqlite::Result<()> {
    if j.len() > 256 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "lineage too large"),
        )));
    }
    c.execute(
        "INSERT OR REPLACE INTO checkpoint_fork_lineages VALUES(?1,?2,?3,?4,?5)",
        params![id, source, parent, j, now],
    )?;
    Ok(())
}
pub fn list(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s =
        c.prepare("SELECT lineage_json FROM checkpoint_fork_lineages ORDER BY fork_run_id")?;
    let rows = s.query_map([], |r| r.get(0))?.collect();
    rows
}
