//! Durable metadata store for Event Trigger Runtime; payloads are bounded JSON only.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

const MAX_DEFINITION_BYTES: usize = 64 * 1024;

/// A latest immutable definition revision returned by a scoped query.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredDefinition<T> {
    /// The typed event-trigger definition.
    pub definition: T,
    /// The immutable revision number.
    pub version: u64,
    /// Time the revision was recorded in Unix milliseconds.
    pub updated_at_ms: i64,
}

/// Installs the additive event-trigger metadata schema.
pub fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS event_trigger_definitions (trigger_id TEXT NOT NULL, owner_scope TEXT NOT NULL, definition_json BLOB NOT NULL, content_hash TEXT NOT NULL, version INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, PRIMARY KEY(trigger_id, version)); CREATE TABLE IF NOT EXISTS event_trigger_events (event_id TEXT PRIMARY KEY, trigger_id TEXT NOT NULL, envelope_json BLOB NOT NULL, outcome TEXT NOT NULL, correlation_id TEXT NOT NULL, accepted_at_ms INTEGER NOT NULL, expires_at_ms INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_event_trigger_events_trigger ON event_trigger_events(trigger_id, accepted_at_ms); CREATE TABLE IF NOT EXISTS event_trigger_dedup (trigger_id TEXT NOT NULL, dedup_key TEXT NOT NULL, event_id TEXT NOT NULL, expires_at_ms INTEGER NOT NULL, PRIMARY KEY(trigger_id, dedup_key));")
}

/// Stores one immutable definition revision if it has not already been written.
pub fn put_definition<T: Serialize>(
    connection: &Connection,
    trigger_id: &str,
    owner_scope: &str,
    definition: &T,
    hash: &str,
    version: u64,
    now_ms: i64,
) -> Result<(), rusqlite::Error> {
    if version > i64::MAX as u64 {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            "trigger revision exceeds SQLite integer range".into(),
        ));
    }
    let json = serde_json::to_vec(definition)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    if json.len() > MAX_DEFINITION_BYTES {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            "trigger definition exceeds 64 KiB".into(),
        ));
    }
    connection.execute("INSERT INTO event_trigger_definitions(trigger_id,owner_scope,definition_json,content_hash,version,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(trigger_id,version) DO NOTHING", params![trigger_id, owner_scope, json, hash, version as i64, now_ms])?;
    Ok(())
}

/// Reads the latest definition revision for one trigger and owner scope.
pub fn get_definition<T: DeserializeOwned>(
    connection: &Connection,
    trigger_id: &str,
    owner_scope: &str,
) -> Result<Option<T>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT definition_json FROM event_trigger_definitions WHERE trigger_id=?1 AND owner_scope=?2 ORDER BY version DESC LIMIT 1",
            params![trigger_id, owner_scope],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })
        .transpose()
}

/// Returns the latest immutable revision number for a scoped trigger.
pub fn definition_version(
    connection: &Connection,
    trigger_id: &str,
    owner_scope: &str,
) -> Result<Option<u64>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT MAX(version) FROM event_trigger_definitions WHERE trigger_id=?1 AND owner_scope=?2",
            params![trigger_id, owner_scope],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map(|value| value.and_then(|version| u64::try_from(version).ok()))
}

/// Returns the single owner assigned to a trigger identity, if it exists.
pub fn definition_owner_scope(
    connection: &Connection,
    trigger_id: &str,
) -> Result<Option<String>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT owner_scope FROM event_trigger_definitions WHERE trigger_id=?1 ORDER BY version DESC LIMIT 1",
            params![trigger_id],
            |row| row.get(0),
        )
        .optional()
}

/// Lists each scoped trigger's latest immutable definition revision.
pub fn list_definitions<T: DeserializeOwned>(
    connection: &Connection,
    owner_scope: &str,
) -> Result<Vec<StoredDefinition<T>>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT d.definition_json, d.version, d.updated_at_ms FROM event_trigger_definitions d \
         WHERE d.owner_scope=?1 AND d.version=( \
           SELECT MAX(latest.version) FROM event_trigger_definitions latest \
           WHERE latest.owner_scope=d.owner_scope AND latest.trigger_id=d.trigger_id \
         ) ORDER BY d.updated_at_ms DESC, d.trigger_id ASC",
    )?;
    let rows = statement.query_map(params![owner_scope], |row| {
        Ok((
            row.get::<_, Vec<u8>>(0)?,
            row.get::<_, u64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    rows.map(|row| {
        let (bytes, version, updated_at_ms) = row?;
        let definition = serde_json::from_slice(&bytes)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        Ok(StoredDefinition {
            definition,
            version,
            updated_at_ms,
        })
    })
    .collect()
}

/// Metadata associated with a persisted event; raw payload is excluded.
pub struct EventRecordMeta<'a> {
    /// Stable event identity.
    pub event_id: &'a str,
    /// Owning trigger identity.
    pub trigger_id: &'a str,
    /// Bounded outcome name.
    pub outcome: &'a str,
    /// Correlation identity for the admitted event.
    pub correlation_id: &'a str,
    /// Acceptance time in Unix milliseconds.
    pub accepted_at_ms: i64,
    /// Retention deadline in Unix milliseconds.
    pub expires_at_ms: i64,
}

/// A redacted event-history entry containing metadata only.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EventSummary {
    /// Stable event identity.
    pub event_id: String,
    /// Owning trigger identity.
    pub trigger_id: String,
    /// Bounded admission outcome.
    pub outcome: String,
    /// Correlation identity.
    pub correlation_id: String,
    /// Acceptance time in Unix milliseconds.
    pub accepted_at_ms: i64,
}

/// Lists unexpired, payload-free event summaries for an owned trigger.
pub fn list_events(
    connection: &Connection,
    trigger_id: &str,
    owner_scope: &str,
    now_ms: i64,
    limit: u32,
) -> Result<Vec<EventSummary>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT e.event_id, e.trigger_id, e.outcome, e.correlation_id, e.accepted_at_ms \
         FROM event_trigger_events e WHERE e.trigger_id=?1 AND e.expires_at_ms>?2 \
           AND EXISTS (SELECT 1 FROM event_trigger_definitions d \
             WHERE d.trigger_id=e.trigger_id AND d.owner_scope=?3) \
         ORDER BY e.accepted_at_ms DESC, e.event_id DESC LIMIT ?4",
    )?;
    let rows = statement.query_map(
        params![trigger_id, now_ms, owner_scope, limit.min(100)],
        |row| {
            Ok(EventSummary {
                event_id: row.get(0)?,
                trigger_id: row.get(1)?,
                outcome: row.get(2)?,
                correlation_id: row.get(3)?,
                accepted_at_ms: row.get(4)?,
            })
        },
    )?;
    rows.collect()
}

/// Persists a bounded event envelope without replacing an existing event ID.
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
    connection.execute("INSERT OR IGNORE INTO event_trigger_events(event_id,trigger_id,envelope_json,outcome,correlation_id,accepted_at_ms,expires_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![meta.event_id, meta.trigger_id, json, meta.outcome, meta.correlation_id, meta.accepted_at_ms, meta.expires_at_ms])?;
    Ok(())
}

/// Reserves a deduplication key once for the specified trigger.
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
#[path = "event_trigger_runtime_store_tests.rs"]
mod tests;
