use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const STORE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSessionMetadata {
    pub session_id: String,
    pub conversation_id: String,
    pub run_id: Option<String>,
    pub state: String,
    pub revision: u64,
    pub control_generation: u64,
    pub control_owner: String,
    pub profile_policy: String,
    pub network_policy: String,
    pub policy_hash: String,
    pub updated_at_ms: i64,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS browser_session_metadata (session_id TEXT PRIMARY KEY NOT NULL, conversation_id TEXT NOT NULL, run_id TEXT, state TEXT NOT NULL, revision INTEGER NOT NULL, control_generation INTEGER NOT NULL, control_owner TEXT NOT NULL DEFAULT 'agent', profile_policy TEXT NOT NULL, network_policy TEXT NOT NULL, policy_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);")?;
    match connection.execute("ALTER TABLE browser_session_metadata ADD COLUMN control_owner TEXT NOT NULL DEFAULT 'agent'", []) {
        Ok(_) => {}
        Err(error) if error.to_string().contains("duplicate column name") => {}
        Err(error) => return Err(error),
    }
    connection.execute_batch("CREATE INDEX IF NOT EXISTS idx_browser_session_conversation ON browser_session_metadata(conversation_id, updated_at_ms);")
}

pub fn upsert(connection: &Connection, record: &BrowserSessionMetadata) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO browser_session_metadata(session_id,conversation_id,run_id,state,revision,control_generation,control_owner,profile_policy,network_policy,policy_hash,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(session_id) DO UPDATE SET state=excluded.state, revision=excluded.revision, control_generation=excluded.control_generation, control_owner=excluded.control_owner, updated_at_ms=excluded.updated_at_ms", rusqlite::params![record.session_id, record.conversation_id, record.run_id, record.state, record.revision, record.control_generation, record.control_owner, record.profile_policy, record.network_policy, record.policy_hash, record.updated_at_ms])?;
    Ok(())
}

pub fn get(
    connection: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<BrowserSessionMetadata>> {
    connection.query_row("SELECT session_id,conversation_id,run_id,state,revision,control_generation,control_owner,profile_policy,network_policy,policy_hash,updated_at_ms FROM browser_session_metadata WHERE session_id=?1", [session_id], |row| Ok(BrowserSessionMetadata { session_id: row.get(0)?, conversation_id: row.get(1)?, run_id: row.get(2)?, state: row.get(3)?, revision: row.get(4)?, control_generation: row.get(5)?, control_owner: row.get(6)?, profile_policy: row.get(7)?, network_policy: row.get(8)?, policy_hash: row.get(9)?, updated_at_ms: row.get(10)? })).optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_round_trip_is_bounded_and_does_not_store_payloads() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let record = BrowserSessionMetadata {
            session_id: "session-1".into(),
            conversation_id: "conversation-1".into(),
            run_id: Some("run-1".into()),
            state: "starting".into(),
            revision: 0,
            control_generation: 0,
            control_owner: "agent".into(),
            profile_policy: "ephemeral_clean".into(),
            network_policy: "public_internet".into(),
            policy_hash: "hash".into(),
            updated_at_ms: 1,
        };
        upsert(&connection, &record).unwrap();
        assert_eq!(get(&connection, "session-1").unwrap(), Some(record));
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE '%payload%'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
}
