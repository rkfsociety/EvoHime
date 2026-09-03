use rusqlite::{params, Connection, OptionalExtension};

#[derive(Clone, Copy)]
pub struct ConnectionInput<'a> {
    pub id: &'a str,
    pub owner_scope: &'a str,
    pub connection_json: &'a [u8],
    pub content_hash: &'a str,
    pub expected_version: u64,
    pub idempotency_key: &'a str,
    pub now_ms: i64,
}
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS remote_conversation_channels (connection_id TEXT PRIMARY KEY, owner_scope TEXT NOT NULL, connection_json BLOB NOT NULL, content_hash TEXT NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS remote_conversation_pairing_claims (connection_id TEXT PRIMARY KEY, code_hash TEXT NOT NULL, expires_at_ms INTEGER NOT NULL, consumed INTEGER NOT NULL DEFAULT 0, external_identity TEXT NOT NULL); CREATE TABLE IF NOT EXISTS remote_conversation_inbound_dedup (connection_id TEXT NOT NULL, message_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(connection_id,message_id));")
}
pub fn save(c: &Connection, i: ConnectionInput<'_>) -> rusqlite::Result<bool> {
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
pub fn claim_message(
    c: &Connection,
    connection_id: &str,
    message_id: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR IGNORE INTO remote_conversation_inbound_dedup(connection_id,message_id,created_at_ms) VALUES(?1,?2,?3)",params![connection_id,message_id,now_ms])?==1)
}

pub struct ConnectionRecord {
    pub owner_scope: String,
    pub connection_json: Vec<u8>,
    pub content_hash: String,
    pub version: u64,
}
pub fn load(c: &Connection, id: &str) -> rusqlite::Result<Option<ConnectionRecord>> {
    c.query_row("SELECT owner_scope,connection_json,content_hash,version FROM remote_conversation_channels WHERE connection_id=?1", [id], |r| Ok(ConnectionRecord { owner_scope: r.get(0)?, connection_json: r.get(1)?, content_hash: r.get(2)?, version: r.get::<_, i64>(3)? as u64 })).optional()
}

pub fn save_pairing(
    c: &Connection,
    id: &str,
    code_hash: &str,
    expires_at_ms: i64,
    external_identity: &str,
) -> rusqlite::Result<bool> {
    Ok(c.execute("INSERT OR REPLACE INTO remote_conversation_pairing_claims(connection_id,code_hash,expires_at_ms,consumed,external_identity) VALUES(?1,?2,?3,0,?4)", params![id,code_hash,expires_at_ms,external_identity])? == 1)
}
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
}
