//! Durable termination policy and first-trigger snapshots.
use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS termination_policies (id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_json TEXT NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS termination_states (run_id TEXT PRIMARY KEY, policy_id TEXT NOT NULL, state_json TEXT NOT NULL, version INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
pub fn put_policy(
    c: &Connection,
    id: &str,
    version: u64,
    json: &str,
    hash: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute(
        "INSERT OR IGNORE INTO termination_policies VALUES (?1,?2,?3,?4,?5)",
        params![id, version as i64, json, hash, now],
    )? == 1)
}
pub fn put_state(
    c: &Connection,
    run_id: &str,
    policy_id: &str,
    json: &str,
    expected: u64,
    now: i64,
) -> rusqlite::Result<bool> {
    let n = if expected == 0 {
        c.execute(
            "INSERT OR IGNORE INTO termination_states VALUES (?1,?2,?3,1,?4)",
            params![run_id, policy_id, json, now],
        )?
    } else {
        c.execute("UPDATE termination_states SET state_json=?1,version=version+1,updated_at_ms=?2 WHERE run_id=?3 AND version=?4",params![json,now,run_id,expected as i64])?
    };
    Ok(n == 1)
}
pub fn get_state(c: &Connection, run_id: &str) -> rusqlite::Result<Option<String>> {
    c.query_row(
        "SELECT state_json FROM termination_states WHERE run_id=?1",
        params![run_id],
        |r| r.get(0),
    )
    .optional()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durable_state_is_idempotent_and_stale_safe() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_policy(&c, "p", 1, "{}", "h", 1).unwrap());
        assert!(put_state(&c, "r", "p", "{}", 0, 1).unwrap());
        assert!(!put_state(&c, "r", "p", "{}", 0, 2).unwrap());
        assert!(put_state(&c, "r", "p", "{\"cursor\":\"e1\"}", 1, 2).unwrap());
        assert_eq!(get_state(&c, "r").unwrap().unwrap(), "{\"cursor\":\"e1\"}");
    }
}
