//! Durable versioned Team Coordination policy/state metadata (schema v63).
use rusqlite::{params, Connection, OptionalExtension};

/// Inputs to save a strategy snapshot with optimistic concurrency and idempotency.
pub struct StrategyStateInput<'a> {
    /// Team coordination session identifier.
    pub session_id: &'a str,
    /// Stable strategy identifier.
    pub strategy_id: &'a str,
    /// Immutable strategy revision.
    pub strategy_revision: u64,
    /// Digest of the protocol contract used by the strategy.
    pub protocol_hash: &'a str,
    /// Serialized strategy definition.
    pub strategy_json: &'a [u8],
    /// Serialized mutable strategy state.
    pub state_json: &'a [u8],
    /// Current snapshot version expected by the caller; zero creates it.
    pub expected_version: u64,
    /// Idempotency key for the save operation.
    pub idempotency_key: &'a str,
    /// Save time in Unix milliseconds.
    pub now_ms: i64,
}

/// Loaded strategy definition and state snapshot for one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyStateRecord {
    /// Stable strategy identifier.
    pub strategy_id: String,
    /// Immutable strategy revision.
    pub strategy_revision: u64,
    /// Digest of the associated protocol.
    pub protocol_hash: String,
    /// Serialized strategy definition.
    pub strategy_json: Vec<u8>,
    /// Serialized mutable strategy state.
    pub state_json: Vec<u8>,
    /// Optimistic concurrency version.
    pub version: u64,
}

/// Creates policy, team state, and per-session strategy snapshot tables.
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS team_coordination_policies (team_id TEXT NOT NULL, revision INTEGER NOT NULL, policy_json BLOB NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(team_id, revision)); CREATE TABLE IF NOT EXISTS team_coordination_states (team_id TEXT PRIMARY KEY NOT NULL, policy_revision INTEGER NOT NULL, state_json BLOB NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS team_coordination_strategy_snapshots (session_id TEXT PRIMARY KEY NOT NULL, strategy_id TEXT NOT NULL, strategy_revision INTEGER NOT NULL, protocol_hash TEXT NOT NULL, strategy_json BLOB NOT NULL, state_json BLOB NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);")
}

/// Creates or advances a strategy snapshot when its expected version matches.
///
/// Repeating an identical request with the same idempotency key succeeds without incrementing the
/// version; stale requests return `false`.
pub fn save_strategy_state(
    c: &Connection,
    input: StrategyStateInput<'_>,
) -> rusqlite::Result<bool> {
    let current: Option<(u64, Vec<u8>, Vec<u8>, String)> = c
        .query_row(
            "SELECT version,strategy_json,state_json,idempotency_key FROM team_coordination_strategy_snapshots WHERE session_id=?1",
            [input.session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    if let Some((version, previous_strategy, previous_state, previous_key)) = current {
        if version != input.expected_version {
            return Ok(false);
        }
        if previous_key == input.idempotency_key
            && previous_strategy == input.strategy_json
            && previous_state == input.state_json
        {
            return Ok(true);
        }
        return Ok(c.execute(
            "UPDATE team_coordination_strategy_snapshots SET strategy_id=?1,strategy_revision=?2,protocol_hash=?3,strategy_json=?4,state_json=?5,version=version+1,idempotency_key=?6,updated_at_ms=?7 WHERE session_id=?8 AND version=?9",
            params![input.strategy_id, input.strategy_revision as i64, input.protocol_hash, input.strategy_json, input.state_json, input.idempotency_key, input.now_ms, input.session_id, input.expected_version as i64],
        )? == 1);
    }
    if input.expected_version != 0 {
        return Ok(false);
    }
    Ok(c.execute(
        "INSERT INTO team_coordination_strategy_snapshots(session_id,strategy_id,strategy_revision,protocol_hash,strategy_json,state_json,version,idempotency_key,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,1,?7,?8)",
        params![input.session_id, input.strategy_id, input.strategy_revision as i64, input.protocol_hash, input.strategy_json, input.state_json, input.idempotency_key, input.now_ms],
    )? == 1)
}

/// Loads a session's strategy definition and state snapshot.
pub fn load_strategy_state(
    c: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<StrategyStateRecord>> {
    c.query_row(
        "SELECT strategy_id,strategy_revision,protocol_hash,strategy_json,state_json,version FROM team_coordination_strategy_snapshots WHERE session_id=?1",
        [session_id],
        |r| Ok(StrategyStateRecord { strategy_id: r.get(0)?, strategy_revision: r.get(1)?, protocol_hash: r.get(2)?, strategy_json: r.get(3)?, state_json: r.get(4)?, version: r.get(5)? }),
    )
    .optional()
}

/// Inserts an immutable team policy revision if it is not already present.
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

/// Creates or advances team state using version fencing and an idempotency key.
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
        let updated = c.execute("UPDATE team_coordination_states SET policy_revision=?1,state_json=?2,version=version+1,idempotency_key=?3,updated_at_ms=?4 WHERE team_id=?5 AND version=?6", params![policy_revision as i64, json, idempotency_key, now_ms, team_id, expected_version as i64])?;
        return Ok(updated == 1);
    }
    if expected_version != 0 {
        return Ok(false);
    }
    Ok(c.execute("INSERT INTO team_coordination_states(team_id,policy_revision,state_json,version,idempotency_key,updated_at_ms) VALUES (?1,?2,?3,1,?4,?5)", params![team_id, policy_revision as i64, json, idempotency_key, now_ms])? == 1)
}

/// Loads policy revision, serialized state, and concurrency version for a team.
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

    #[test]
    fn strategy_snapshot_is_durable_and_fenced_to_session() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(save_strategy_state(
            &c,
            StrategyStateInput {
                session_id: "session",
                strategy_id: "strategy",
                strategy_revision: 1,
                protocol_hash: &"a".repeat(64),
                strategy_json: br#"{"strategy":1}"#,
                state_json: br#"{"state":1}"#,
                expected_version: 0,
                idempotency_key: "request",
                now_ms: 1,
            }
        )
        .unwrap());
        assert!(!save_strategy_state(
            &c,
            StrategyStateInput {
                session_id: "session",
                strategy_id: "other",
                strategy_revision: 1,
                protocol_hash: &"a".repeat(64),
                strategy_json: br#"{"strategy":2}"#,
                state_json: br#"{"state":2}"#,
                expected_version: 0,
                idempotency_key: "other-request",
                now_ms: 2,
            }
        )
        .unwrap());
        assert_eq!(
            load_strategy_state(&c, "session")
                .unwrap()
                .unwrap()
                .strategy_id,
            "strategy"
        );
    }
}
