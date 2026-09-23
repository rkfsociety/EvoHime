use rusqlite::{params, Connection, OptionalExtension};

/// Creates the durable architect/editor pipeline state table.
///
/// # Errors
///
/// Returns the underlying SQLite schema error.
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS architect_editor_pipelines (id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_hash TEXT NOT NULL, state_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
/// Inserts a serialized pipeline state, limited to 256 KiB.
///
/// IDs are unique; this operation does not replace an existing pipeline row.
///
/// # Errors
///
/// Returns a SQLite error for oversized state, duplicate IDs, or failed writes.
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
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "pipeline too large"),
        )));
    }
    c.execute("INSERT INTO architect_editor_pipelines(id,version,content_hash,state_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5)",params![id,version,hash,json,now])?;
    Ok(())
}
/// Loads a pipeline's serialized state by ID, if present.
///
/// # Errors
///
/// Returns the underlying SQLite query error.
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT state_json FROM architect_editor_pipelines WHERE id=?1",
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
        put(&c, "p", 1, "h", b"{}", 1).unwrap();
        assert_eq!(get(&c, "p").unwrap(), Some(b"{}".to_vec()));
    }
}
