use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS dependency_task_graphs (graph_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, graph_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put(
    c: &Connection,
    id: &str,
    revision: u64,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO dependency_task_graphs(graph_id,revision,graph_json,content_hash,updated_at_ms) VALUES(?1,?2,?3,?4,?5)",params![id,revision,json,hash,now])?==1)
}
pub fn replace(
    c: &Connection,
    id: &str,
    expected: u64,
    revision: u64,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("UPDATE dependency_task_graphs SET revision=?3,graph_json=?4,content_hash=?5,updated_at_ms=?6 WHERE graph_id=?1 AND revision=?2",params![id,expected,revision,json,hash,now])?==1)
}
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT graph_json FROM dependency_task_graphs WHERE graph_id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}
