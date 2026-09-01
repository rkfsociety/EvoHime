//! Durable metadata and append-only accounting for Team Resource Budget.
use rusqlite::{params, Connection, OptionalExtension};
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS team_budget_policies (id TEXT PRIMARY KEY, version INTEGER NOT NULL, content_json TEXT NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS team_budget_states (team_session_id TEXT PRIMARY KEY, policy_version INTEGER NOT NULL, state_json TEXT NOT NULL, version INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS team_budget_usage (id TEXT PRIMARY KEY, team_session_id TEXT NOT NULL, event_json TEXT NOT NULL, observed_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS team_budget_requests (id TEXT PRIMARY KEY, team_session_id TEXT NOT NULL, request_json TEXT NOT NULL, status TEXT NOT NULL, created_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_team_budget_usage_session ON team_budget_usage(team_session_id, observed_at_ms);")
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
        "INSERT OR IGNORE INTO team_budget_policies VALUES (?1,?2,?3,?4,?5)",
        params![id, version as i64, json, hash, now],
    )? == 1)
}
pub fn get_policy(c: &Connection, id: &str) -> rusqlite::Result<Option<String>> {
    c.query_row(
        "SELECT content_json FROM team_budget_policies WHERE id=?1",
        params![id],
        |r| r.get(0),
    )
    .optional()
}
pub fn put_state(
    c: &Connection,
    id: &str,
    policy_version: u64,
    json: &str,
    expected: Option<u64>,
    now: i64,
) -> rusqlite::Result<bool> {
    let n=match expected{Some(v)=>c.execute("UPDATE team_budget_states SET policy_version=?1,state_json=?2,version=version+1,updated_at_ms=?3 WHERE team_session_id=?4 AND version=?5",params![policy_version as i64,json,now,id,v as i64])?,None=>c.execute("INSERT OR IGNORE INTO team_budget_states VALUES (?1,?2,?3,1,?4)",params![id,policy_version as i64,json,now])?};
    Ok(n == 1)
}
pub fn append_usage(
    c: &Connection,
    id: &str,
    session: &str,
    json: &str,
    now: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute(
        "INSERT OR IGNORE INTO team_budget_usage VALUES (?1,?2,?3,?4)",
        params![id, session, json, now],
    )? == 1)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idempotent_and_stale_safe() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(put_policy(&c, "p", 1, "{}", "h", 1).unwrap());
        assert!(!put_policy(&c, "p", 1, "{}", "h", 1).unwrap());
        assert!(put_state(&c, "s", 1, "{}", None, 1).unwrap());
        assert!(!put_state(&c, "s", 1, "{}", Some(0), 2).unwrap());
        assert!(put_state(&c, "s", 1, "{\"v\":2}", Some(1), 2).unwrap());
        assert!(append_usage(&c, "u", "s", "{}", 2).unwrap());
        assert!(!append_usage(&c, "u", "s", "{}", 2).unwrap());
    }
}
