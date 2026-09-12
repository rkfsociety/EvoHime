use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_QUEUE: i64 = 256;

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS conversation_bridges (bridge_id TEXT PRIMARY KEY, provider TEXT NOT NULL, conversation_id TEXT NOT NULL, principal_id TEXT NOT NULL, pairing_hash TEXT NOT NULL, state TEXT NOT NULL, revision INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS conversation_thread_bindings (binding_id TEXT PRIMARY KEY, bridge_id TEXT NOT NULL, external_thread_id TEXT NOT NULL, conversation_id TEXT NOT NULL, principal_id TEXT NOT NULL, revision INTEGER NOT NULL, UNIQUE(bridge_id, external_thread_id)); CREATE TABLE IF NOT EXISTS conversation_bridge_inbound (message_id TEXT PRIMARY KEY, binding_id TEXT NOT NULL, message_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS conversation_bridge_idempotency (idempotency_key TEXT PRIMARY KEY, operation TEXT NOT NULL);")
}

pub fn claim_idempotency(c: &Connection, key: &str, operation: &str) -> rusqlite::Result<bool> {
    Ok(c.execute(
        "INSERT OR IGNORE INTO conversation_bridge_idempotency VALUES(?1,?2)",
        params![key, operation],
    )? == 1)
}

pub fn put_bridge(c: &Connection, id: &str, json: &[u8], revision: u64) -> rusqlite::Result<()> {
    let revision = revision_i64(revision)?;
    let bridge: serde_json::Value = serde_json::from_slice(json)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let provider = required_string(&bridge, "provider")?;
    let conversation_id = required_string(&bridge, "conversation_id")?;
    let principal_id = required_string(&bridge, "principal_id")?;
    let pairing_hash = required_string(&bridge, "pairing_hash")?;
    let state = required_string(&bridge, "state")?;
    let current_revision: Option<i64> = c
        .query_row(
            "SELECT revision FROM conversation_bridges WHERE bridge_id=?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    if current_revision.is_some_and(|current| current >= revision) {
        return Err(rusqlite::Error::InvalidParameterName(
            "bridge revision is stale".into(),
        ));
    }
    c.execute(
        "INSERT OR REPLACE INTO conversation_bridges VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            id,
            provider,
            conversation_id,
            principal_id,
            pairing_hash,
            state,
            revision as i64
        ],
    )?;
    Ok(())
}

pub fn get_bridge(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT provider,conversation_id,principal_id,pairing_hash,state,revision FROM conversation_bridges WHERE bridge_id=?1",
        params![id],
        |row| {
            let state: String = row.get(4)?;
            Ok(serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "bridge_id": id,
                "provider": row.get::<_, String>(0)?,
                "conversation_id": row.get::<_, String>(1)?,
                "principal_id": row.get::<_, String>(2)?,
                "pairing_hash": row.get::<_, String>(3)?,
                "state": state,
                "revision": row.get::<_, i64>(5)? as u64
            }))
            .expect("bridge metadata serializes"))
        },
    )
    .optional()
}

pub fn get_binding(c: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT bridge_id,external_thread_id,conversation_id,principal_id,revision FROM conversation_thread_bindings WHERE binding_id=?1",
        params![id],
        |row| Ok(serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "binding_id": id,
            "bridge_id": row.get::<_, String>(0)?,
            "external_thread_id": row.get::<_, String>(1)?,
            "conversation_id": row.get::<_, String>(2)?,
            "principal_id": row.get::<_, String>(3)?,
            "revision": row.get::<_, i64>(4)? as u64
        })).expect("binding metadata serializes")),
    )
    .optional()
}

pub fn put_binding(
    c: &Connection,
    json: &[u8],
    binding_id: &str,
    bridge_id: &str,
    thread_id: &str,
    revision: u64,
) -> rusqlite::Result<bool> {
    let revision = revision_i64(revision)?;
    let binding: serde_json::Value = serde_json::from_slice(json)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let conversation_id = required_string(&binding, "conversation_id")?;
    let principal_id = required_string(&binding, "principal_id")?;
    let bridge_identity: Option<(String, String)> = c
        .query_row(
            "SELECT conversation_id,principal_id FROM conversation_bridges WHERE bridge_id=?1",
            params![bridge_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((bridge_conversation_id, bridge_principal_id)) = bridge_identity else {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    };
    if conversation_id != bridge_conversation_id || principal_id != bridge_principal_id {
        return Err(rusqlite::Error::InvalidParameterName(
            "binding identity does not match bridge".into(),
        ));
    }
    Ok(c.execute(
        "INSERT OR IGNORE INTO conversation_thread_bindings VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            binding_id,
            bridge_id,
            thread_id,
            conversation_id,
            principal_id,
            revision as i64
        ],
    )? == 1)
}

pub fn put_inbound(
    c: &Connection,
    id: &str,
    binding_id: &str,
    json: &[u8],
    created_at_ms: i64,
) -> rusqlite::Result<bool> {
    let count: i64 = c.query_row(
        "SELECT COUNT(*) FROM conversation_bridge_inbound",
        [],
        |r| r.get(0),
    )?;
    if count >= MAX_QUEUE {
        return Ok(false);
    }
    Ok(c.execute(
        "INSERT OR IGNORE INTO conversation_bridge_inbound VALUES(?1,?2,?3,?4)",
        params![id, binding_id, json, created_at_ms],
    )? == 1)
}

pub fn list_inbound(c: &Connection) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = c.prepare("SELECT message_json FROM conversation_bridge_inbound ORDER BY created_at_ms,message_id LIMIT 256")?;
    let rows = statement.query_map([], |row| row.get(0))?.collect();
    rows
}

pub fn clear_bridge(c: &Connection, bridge_id: &str) -> rusqlite::Result<()> {
    c.execute("DELETE FROM conversation_bridge_inbound WHERE binding_id IN (SELECT binding_id FROM conversation_thread_bindings WHERE bridge_id=?1)", params![bridge_id])?;
    c.execute(
        "DELETE FROM conversation_thread_bindings WHERE bridge_id=?1",
        params![bridge_id],
    )?;
    c.execute(
        "DELETE FROM conversation_bridges WHERE bridge_id=?1",
        params![bridge_id],
    )?;
    Ok(())
}

pub fn bridge_revision(c: &Connection, bridge_id: &str) -> rusqlite::Result<Option<u64>> {
    c.query_row(
        "SELECT revision FROM conversation_bridges WHERE bridge_id=?1",
        params![bridge_id],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map(|v| v.map(|x| x as u64))
}

fn required_string<'a>(value: &'a serde_json::Value, field: &str) -> rusqlite::Result<&'a str> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            rusqlite::Error::InvalidParameterName(format!("missing bridge field: {field}"))
        })
}

fn revision_i64(revision: u64) -> rusqlite::Result<i64> {
    i64::try_from(revision)
        .map_err(|_| rusqlite::Error::InvalidParameterName("revision exceeds SQLite range".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inbound_is_deduplicated_and_clear_is_cascading() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let bridge = br#"{"provider":"telegram","conversation_id":"c","principal_id":"p","pairing_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","state":"paired"}"#;
        put_bridge(&c, "b", bridge, 1).unwrap();
        assert!(get_bridge(&c, "b").unwrap().is_some());
        let binding = br#"{"conversation_id":"c","principal_id":"p"}"#;
        assert!(put_binding(&c, binding, "bind", "b", "thread", 1).unwrap());
        assert!(get_binding(&c, "bind").unwrap().is_some());
        assert!(put_inbound(&c, "m", "bind", b"{}", 1).unwrap());
        assert!(!put_inbound(&c, "m", "bind", b"{}", 1).unwrap());
        assert!(claim_idempotency(&c, "other-bridge-key", "create").unwrap());
        clear_bridge(&c, "b").unwrap();
        assert!(list_inbound(&c).unwrap().is_empty());
        assert!(!claim_idempotency(&c, "other-bridge-key", "create").unwrap());
    }

    #[test]
    fn rejects_invalid_bridge_and_binding_json() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();

        assert!(put_bridge(&c, "b", b"{invalid", 1).is_err());
        assert!(put_binding(&c, b"{invalid", "bind", "b", "thread", 1).is_err());
        assert!(get_bridge(&c, "b").unwrap().is_none());
        assert!(get_binding(&c, "bind").unwrap().is_none());
    }

    #[test]
    fn rejects_structurally_empty_bridge_and_binding_json() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();

        assert!(put_bridge(&c, "b", br#"{}"#, 1).is_err());
        assert!(put_binding(&c, br#"{}"#, "bind", "b", "thread", 1).is_err());
    }

    #[test]
    fn rejects_orphan_and_cross_identity_bindings() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let bridge = br#"{"provider":"telegram","conversation_id":"c","principal_id":"p","pairing_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","state":"paired"}"#;
        put_bridge(&c, "b", bridge, 1).unwrap();

        let mismatch = br#"{"conversation_id":"other","principal_id":"p"}"#;
        assert!(put_binding(&c, mismatch, "bind-mismatch", "b", "thread", 1).is_err());
        let valid = br#"{"conversation_id":"c","principal_id":"p"}"#;
        assert!(put_binding(&c, valid, "bind-orphan", "missing", "thread", 1).is_err());
    }

    #[test]
    fn rejects_stale_and_unrepresentable_bridge_revisions() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let bridge = br#"{"provider":"telegram","conversation_id":"c","principal_id":"p","pairing_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","state":"paired"}"#;
        put_bridge(&c, "b", bridge, 2).unwrap();

        assert!(put_bridge(&c, "b", bridge, 2).is_err());
        assert!(put_bridge(&c, "b", bridge, 1).is_err());
        assert!(put_bridge(&c, "other", bridge, u64::MAX).is_err());
        let stored: serde_json::Value =
            serde_json::from_slice(&get_bridge(&c, "b").unwrap().unwrap()).unwrap();
        assert_eq!(stored["revision"], 2);
    }
}
