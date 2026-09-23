//! Persistence operations for remote conversation channel state.
//!
//! The expected version fences concurrent updates; zero denotes the first
//! insert. Inbound messages and pairing claims are recorded with one-time
//! semantics.
//!
//! ```
//! use evohime_local_storage::remote_conversation_channels_store::{
//!     install_schema, save, ConnectionInput,
//! };
//! let connection = rusqlite::Connection::open_in_memory()?;
//! install_schema(&connection)?;
//! let inserted = save(&connection, ConnectionInput {
//!     id: "channel-1",
//!     owner_scope: "workspace-1",
//!     connection_json: br#"{}"#,
//!     content_hash: "sha256:abc",
//!     expected_version: 0,
//!     idempotency_key: "request-1",
//!     now_ms: 1,
//! })?;
//! assert!(inserted);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use rusqlite::{params, Connection, OptionalExtension};

const MAX_CONNECTION_JSON_BYTES: usize = 64 * 1024;

/// Input for a version-fenced channel connection save.
#[derive(Clone, Copy)]
pub struct ConnectionInput<'a> {
    /// Stable identifier of the remote channel connection.
    pub id: &'a str,
    /// Workspace or account scope that owns the connection.
    pub owner_scope: &'a str,
    /// Serialized connection configuration, limited to 64 KiB.
    pub connection_json: &'a [u8],
    /// Digest of the canonical connection configuration.
    pub content_hash: &'a str,
    /// Version the caller expects to replace; use zero for an insert.
    pub expected_version: u64,
    /// Key used to recognize an identical repeated save.
    pub idempotency_key: &'a str,
    /// Save timestamp in Unix milliseconds.
    pub now_ms: i64,
}

/// Creates the channel, pairing-claim, and inbound-message deduplication tables.
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS remote_conversation_channels (connection_id TEXT PRIMARY KEY, owner_scope TEXT NOT NULL, connection_json BLOB NOT NULL, content_hash TEXT NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS remote_conversation_pairing_claims (connection_id TEXT PRIMARY KEY, code_hash TEXT NOT NULL, expires_at_ms INTEGER NOT NULL, consumed INTEGER NOT NULL DEFAULT 0, external_identity TEXT NOT NULL); CREATE TABLE IF NOT EXISTS remote_conversation_inbound_dedup (connection_id TEXT NOT NULL, message_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(connection_id,message_id));")
}
/// Inserts or updates a channel connection using the supplied expected version.
///
/// Returns `false` for oversized payloads or a version conflict. Repeating the
/// same version, payload, and idempotency key succeeds without incrementing it.
pub fn save(c: &Connection, i: ConnectionInput<'_>) -> rusqlite::Result<bool> {
    if i.connection_json.len() > MAX_CONNECTION_JSON_BYTES {
        return Ok(false);
    }
    let old:Option<(u64,Vec<u8>,String)>=c.query_row("SELECT version,connection_json,idempotency_key FROM remote_conversation_channels WHERE connection_id=?1",[i.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    if let Some((v, j, k)) = old {
        if v == i.expected_version && j == i.connection_json && k == i.idempotency_key {
            return Ok(true);
        };
        if v != i.expected_version {
            return Ok(false);
        };
        return Ok(c.execute("UPDATE remote_conversation_channels SET owner_scope=?1,connection_json=?2,content_hash=?3,version=version+1,idempotency_key=?4,updated_at_ms=?5 WHERE connection_id=?6 AND version=?7",params![i.owner_scope,i.connection_json,i.content_hash,i.idempotency_key,i.now_ms,i.id,i.expected_version as i64])?==1);
    }
    if i.expected_version != 0 {
        return Ok(false);
    };
    c.execute("INSERT INTO remote_conversation_channels(connection_id,owner_scope,connection_json,content_hash,version,idempotency_key,updated_at_ms) VALUES(?1,?2,?3,?4,1,?5,?6)",params![i.id,i.owner_scope,i.connection_json,i.content_hash,i.idempotency_key,i.now_ms])?;
    Ok(true)
}
/// Claims an inbound message identifier once for a connection.
///
/// Returns `false` when the pair has already been recorded.
pub fn claim_message(
    c: &Connection,
    connection_id: &str,
    message_id: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO remote_conversation_inbound_dedup(connection_id,message_id,created_at_ms) VALUES(?1,?2,?3)",params![connection_id,message_id,now_ms])?==1)
}

/// Stored channel connection returned by [`load`].
pub struct ConnectionRecord {
    /// Workspace or account scope that owns the connection.
    pub owner_scope: String,
    /// Serialized connection configuration.
    pub connection_json: Vec<u8>,
    /// Digest of the canonical connection configuration.
    pub content_hash: String,
    /// Current optimistic-concurrency version.
    pub version: u64,
}

/// Loads a channel connection by identifier, if it exists.
pub fn load(c: &Connection, id: &str) -> rusqlite::Result<Option<ConnectionRecord>> {
    c.query_row("SELECT owner_scope,connection_json,content_hash,version FROM remote_conversation_channels WHERE connection_id=?1", [id], |r| Ok(ConnectionRecord { owner_scope: r.get(0)?, connection_json: r.get(1)?, content_hash: r.get(2)?, version: r.get::<_, i64>(3)? as u64 })).optional()
}

/// Creates or replaces an unconsumed pairing claim for a channel.
///
/// The caller stores only a hash of the pairing code; `expires_at_ms` is an
/// absolute Unix timestamp in milliseconds.
pub fn save_pairing(
    c: &Connection,
    id: &str,
    code_hash: &str,
    expires_at_ms: i64,
    external_identity: &str,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR REPLACE INTO remote_conversation_pairing_claims(connection_id,code_hash,expires_at_ms,consumed,external_identity) VALUES(?1,?2,?3,0,?4)", params![id,code_hash,expires_at_ms,external_identity])? == 1)
}
/// Atomically consumes a matching, unexpired pairing claim exactly once.
///
/// Returns `false` if the code hash or identity differs, the claim expired, or
/// another caller already consumed it.
pub fn consume_pairing(
    c: &Connection,
    id: &str,
    code_hash: &str,
    external_identity: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("UPDATE remote_conversation_pairing_claims SET consumed=1 WHERE connection_id=?1 AND code_hash=?2 AND external_identity=?3 AND consumed=0 AND expires_at_ms>?4", params![id,code_hash,external_identity,now_ms])? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_and_dedup_are_fenced() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let i = ConnectionInput {
            id: "c",
            owner_scope: "o",
            connection_json: b"{}",
            content_hash: "h",
            expected_version: 0,
            idempotency_key: "k",
            now_ms: 1,
        };
        assert!(save(&c, i).unwrap());
        assert!(save(
            &c,
            ConnectionInput {
                expected_version: 1,
                ..i
            }
        )
        .unwrap());
        assert!(claim_message(&c, "c", "m", 1).unwrap());
        assert!(!claim_message(&c, "c", "m", 2).unwrap());
    }

    #[test]
    fn oversized_connection_json_is_rejected_before_storage() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(!save(
            &c,
            ConnectionInput {
                id: "channel",
                owner_scope: "scope",
                connection_json: &vec![b'x'; MAX_CONNECTION_JSON_BYTES + 1],
                content_hash: "hash",
                expected_version: 0,
                idempotency_key: "key",
                now_ms: 1,
            }
        )
        .unwrap());
        assert!(load(&c, "channel").unwrap().is_none());
    }
}
