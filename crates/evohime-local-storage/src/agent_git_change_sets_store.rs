use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_JSON_BYTES: usize = 1024 * 1024;

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS agent_git_change_sets (id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_hash TEXT NOT NULL, state_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS agent_git_commit_candidates (id TEXT PRIMARY KEY, change_set_id TEXT NOT NULL, diff_hash TEXT NOT NULL, state_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, FOREIGN KEY(change_set_id) REFERENCES agent_git_change_sets(id)); CREATE INDEX IF NOT EXISTS idx_agent_git_candidates_change_set ON agent_git_commit_candidates(change_set_id, created_at_ms DESC);")
}

pub fn put_change_set(
    connection: &Connection,
    id: &str,
    version: u32,
    content_hash: &str,
    json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<()> {
    if json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "change set too large"),
        )));
    }
    connection.execute("INSERT INTO agent_git_change_sets(id,version,content_hash,state_json,created_at_ms) VALUES(?1,?2,?3,?4,?5)", params![id, version, content_hash, json, created_at_ms])?;
    Ok(())
}
pub fn get_change_set(connection: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT state_json FROM agent_git_change_sets WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
}
pub fn put_candidate(
    connection: &Connection,
    id: &str,
    change_set_id: &str,
    diff_hash: &str,
    json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<()> {
    if json.len() > MAX_JSON_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "candidate too large"),
        )));
    }
    connection.execute("INSERT INTO agent_git_commit_candidates(id,change_set_id,diff_hash,state_json,created_at_ms) VALUES(?1,?2,?3,?4,?5)", params![id, change_set_id, diff_hash, json, created_at_ms])?;
    Ok(())
}
pub fn get_candidate(connection: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT state_json FROM agent_git_commit_candidates WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stores_change_set_and_candidate() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        put_change_set(&c, "s", 1, "h", b"{}", 1).unwrap();
        put_candidate(&c, "c", "s", "d", b"{}", 2).unwrap();
        assert_eq!(get_change_set(&c, "s").unwrap(), Some(b"{}".to_vec()));
        assert_eq!(get_candidate(&c, "c").unwrap(), Some(b"{}".to_vec()));
    }
}
