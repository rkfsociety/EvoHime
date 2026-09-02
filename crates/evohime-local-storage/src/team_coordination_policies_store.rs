//! Durable versioned Team Coordination policy/state metadata (schema v63).
use rusqlite::{params, Connection, OptionalExtension};

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS team_coordination_policies (team_id TEXT NOT NULL, revision INTEGER NOT NULL, policy_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(team_id, revision)); CREATE TABLE IF NOT EXISTS team_coordination_states (team_id TEXT PRIMARY KEY NOT NULL, policy_revision INTEGER NOT NULL, state_json BLOB NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT, updated_at_ms INTEGER NOT NULL);")
}

pub fn save_policy(
    c: &Connection,
    team_id: &str,
    revision: u64,
    json: &[u8],
    hash: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO team_coordination_policies(team_id,revision,policy_json,content_hash,updated_at_ms) VALUES (?1,?2,?3,?4,?5)", params![team_id, revision as i64, json, hash, now_ms])? == 1)
}

pub fn save_state(
    c: &Connection,
    team_id: &str,
    policy_revision: u64,
    json: &[u8],
    expected_version: u64,
    idempotency_key: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let current: Option<(u64, Vec<u8>, String)> = c.query_row("SELECT version,state_json,idempotency_key FROM team_coordination_states WHERE team_id=?1", [team_id], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default()))).optional()?;
    if let Some((version, previous, previous_key)) = current {
        if version != expected_version {
            return Ok(false);
        }
        if previous_key == idempotency_key && previous == json {
            return Ok(true);
        }
        c.execute("UPDATE team_coordination_states SET policy_revision=?1,state_json=?2,version=version+1,idempotency_key=?3,updated_at_ms=?4 WHERE team_id=?5 AND version=?6", params![policy_revision as i64, json, idempotency_key, now_ms, team_id, expected_version as i64])?;
        return Ok(true);
    }
    if expected_version != 0 {
        return Ok(false);
    }
    Ok(c.execute("INSERT INTO team_coordination_states(team_id,policy_revision,state_json,version,idempotency_key,updated_at_ms) VALUES (?1,?2,?3,1,?4,?5)", params![team_id, policy_revision as i64, json, idempotency_key, now_ms])? == 1)
}

pub fn load_state(c: &Connection, team_id: &str) -> rusqlite::Result<Option<(u64, Vec<u8>, u64)>> {
    c.query_row(
        "SELECT policy_revision,state_json,version FROM team_coordination_states WHERE team_id=?1",
        [team_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_is_fenced_and_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(save_state(&c, "team", 1, br#"{}"#, 0, "k", 1).unwrap());
        assert!(save_state(&c, "team", 1, br#"{}"#, 1, "k", 2).unwrap());
        assert!(!save_state(&c, "team", 1, br#"{\"x\":1}"#, 0, "other", 3).unwrap());
        assert_eq!(load_state(&c, "team").unwrap().unwrap().2, 1);
    }
}
