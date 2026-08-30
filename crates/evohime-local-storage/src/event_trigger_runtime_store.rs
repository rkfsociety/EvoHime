//! Durable metadata store for Event Trigger Runtime; payloads are bounded JSON only.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

pub fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS event_trigger_definitions (trigger_id TEXT NOT NULL, owner_scope TEXT NOT NULL, definition_json BLOB NOT NULL, content_hash TEXT NOT NULL, version INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(trigger_id, version)); CREATE TABLE IF NOT EXISTS event_trigger_events (event_id TEXT PRIMARY KEY, trigger_id TEXT NOT NULL, envelope_json BLOB NOT NULL, outcome TEXT NOT NULL, correlation_id TEXT NOT NULL, accepted_at_ms INTEGER NOT NULL, expires_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_event_trigger_events_trigger ON event_trigger_events(trigger_id, accepted_at_ms); CREATE TABLE IF NOT EXISTS event_trigger_dedup (trigger_id TEXT NOT NULL, dedup_key TEXT NOT NULL, event_id TEXT NOT NULL, expires_at_ms INTEGER NOT NULL, PRIMARY KEY(trigger_id, dedup_key));")
}

pub fn put_definition<T: Serialize>(
    connection: &Connection,
    trigger_id: &str,
    owner_scope: &str,
    definition: &T,
    hash: &str,
    version: u64,
    now_ms: i64,
) -> Result<(), rusqlite::Error> {
    let json = serde_json::to_vec(definition)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    connection.execute("INSERT INTO event_trigger_definitions(trigger_id,owner_scope,definition_json,content_hash,version,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6)", params![trigger_id, owner_scope, json, hash, version as i64, now_ms])?;
    Ok(())
}

pub fn get_definition<T: DeserializeOwned>(
    connection: &Connection,
    trigger_id: &str,
) -> Result<Option<T>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT definition_json FROM event_trigger_definitions WHERE trigger_id=?1 ORDER BY version DESC LIMIT 1",
            params![trigger_id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })
        .transpose()
}

pub struct EventRecordMeta<'a> {
    pub event_id: &'a str,
    pub trigger_id: &'a str,
    pub outcome: &'a str,
    pub correlation_id: &'a str,
    pub accepted_at_ms: i64,
    pub expires_at_ms: i64,
}

pub fn record_event<T: Serialize>(
    connection: &Connection,
    envelope: &T,
    meta: &EventRecordMeta<'_>,
) -> Result<(), rusqlite::Error> {
    let json = serde_json::to_vec(envelope)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    if json.len() > 32 * 1024 {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            "event payload exceeds 32 KiB".into(),
        ));
    }
    connection.execute("INSERT INTO event_trigger_events(event_id,trigger_id,envelope_json,outcome,correlation_id,accepted_at_ms,expires_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![meta.event_id, meta.trigger_id, json, meta.outcome, meta.correlation_id, meta.accepted_at_ms, meta.expires_at_ms])?;
    Ok(())
}

pub fn record_dedup(
    connection: &Connection,
    trigger_id: &str,
    key: &str,
    event_id: &str,
    expires_at_ms: i64,
) -> Result<bool, rusqlite::Error> {
    let changed = connection.execute("INSERT OR IGNORE INTO event_trigger_dedup(trigger_id,dedup_key,event_id,expires_at_ms) VALUES(?1,?2,?3,?4)", params![trigger_id, key, event_id, expires_at_ms])?;
    Ok(changed == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_has_no_secret_payload_columns() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let names: Vec<String> = c
            .prepare("PRAGMA table_info(event_trigger_events)")
            .unwrap()
            .query_map([], |r| r.get(1))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!names
            .iter()
            .any(|n| n.contains("secret") || n.contains("credential")));
        assert!(record_dedup(&c, "t", "k", "e", 10).unwrap());
        assert!(!record_dedup(&c, "t", "k", "e2", 10).unwrap());
        record_event(
            &c,
            &serde_json::json!({"safe": true}),
            &EventRecordMeta {
                event_id: "e",
                trigger_id: "t",
                outcome: "pending",
                correlation_id: "c",
                accepted_at_ms: 1,
                expires_at_ms: 2,
            },
        )
        .unwrap();
        assert_eq!(
            c.query_row(
                "SELECT outcome FROM event_trigger_events WHERE event_id='e'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "pending"
        );
    }
}
