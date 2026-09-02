//! Durable metadata-only ExperienceRecord storage (schema v66).
use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS experience_replay_records (id TEXT PRIMARY KEY NOT NULL, scope TEXT NOT NULL, scope_id TEXT NOT NULL, record_json BLOB NOT NULL, content_hash TEXT NOT NULL, revision INTEGER NOT NULL, created_at_ms INTEGER NOT NULL, pinned INTEGER NOT NULL DEFAULT 0);")
}
pub fn put(
    c: &Connection,
    id: &str,
    scope: &str,
    scope_id: &str,
    json: &[u8],
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO experience_replay_records(id,scope,scope_id,record_json,content_hash,revision,created_at_ms) VALUES(?1,?2,?3,?4,?5,1,?6)",params![id,scope,scope_id,json,hash,now])?==1)
}
pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT record_json FROM experience_replay_records WHERE id=?1",
        [id],
        |r| r.get(0),
    )
    .optional()
}
pub fn list(
    c: &Connection,
    scope: &str,
    scope_id: &str,
    limit: u32,
) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut s=c.prepare("SELECT record_json FROM experience_replay_records WHERE scope=?1 AND scope_id=?2 ORDER BY created_at_ms DESC LIMIT ?3")?;
    let rows = s.query_map(params![scope, scope_id, limit.min(64)], |r| r.get(0))?;
    rows.collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_is_dropped() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put(&c, "e", "Project", "p", b"{}", "h", 1).unwrap());
        assert!(!put(&c, "e", "Project", "p", b"{}", "h", 2).unwrap());
    }
}
