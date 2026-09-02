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
    let bridge: serde_json::Value = serde_json::from_slice(json).unwrap_or_default();
    c.execute(
        "INSERT OR REPLACE INTO conversation_bridges VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            id,
            bridge["provider"].as_str().unwrap_or("generic"),
            bridge["conversation_id"].as_str().unwrap_or(""),
            bridge["principal_id"].as_str().unwrap_or(""),
            bridge["pairing_hash"].as_str().unwrap_or(""),
            bridge["state"].as_str().unwrap_or("paired"),
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
    let binding: serde_json::Value = serde_json::from_slice(json).unwrap_or_default();
    Ok(c.execute(
        "INSERT OR IGNORE INTO conversation_thread_bindings VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            binding_id,
            bridge_id,
            thread_id,
            binding["conversation_id"].as_str().unwrap_or(""),
            binding["principal_id"].as_str().unwrap_or(""),
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
    c.execute("DELETE FROM conversation_bridge_idempotency", [])?;
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
        clear_bridge(&c, "b").unwrap();
        assert!(list_inbound(&c).unwrap().is_empty());
    }
}
