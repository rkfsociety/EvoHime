//! Durable message substrate for the Core collaboration bus.
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};
pub const MAX_INBOX_PER_SESSION: i64 = 128;
pub fn install_schema(c: &Connection) -> Result<(), rusqlite::Error> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS collaboration_messages(message_id TEXT PRIMARY KEY NOT NULL,session_id TEXT NOT NULL, sender_json BLOB NOT NULL,receiver_json BLOB NOT NULL,envelope_json BLOB NOT NULL,idempotency_key TEXT NOT NULL,delivery TEXT NOT NULL,revision INTEGER NOT NULL DEFAULT 1,sequence INTEGER NOT NULL,created_at_ms INTEGER NOT NULL,delivered_at_ms INTEGER); CREATE UNIQUE INDEX IF NOT EXISTS idx_collaboration_idempotency ON collaboration_messages(session_id,idempotency_key); CREATE INDEX IF NOT EXISTS idx_collaboration_inbox ON collaboration_messages(session_id,delivery,sequence);")
}
fn json<T: Serialize>(v: &T) -> Result<Vec<u8>, rusqlite::Error> {
    serde_json::to_vec(v).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}
fn parse<T: DeserializeOwned>(v: Vec<u8>) -> Result<T, rusqlite::Error> {
    serde_json::from_slice(&v).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(v.len(), rusqlite::types::Type::Blob, Box::new(e))
    })
}
pub struct EnqueueInput<'a, T, U, V> {
    pub session: &'a str,
    pub key: &'a str,
    pub message_id: &'a str,
    pub sender: &'a T,
    pub receiver: &'a U,
    pub envelope: &'a V,
    pub sequence: u64,
    pub now: i64,
}

pub fn enqueue<T: Serialize, U: Serialize, V: Serialize>(
    c: &mut Connection,
    input: EnqueueInput<'_, T, U, V>,
) -> Result<bool, rusqlite::Error> {
    let EnqueueInput {
        session,
        key,
        message_id,
        sender,
        receiver,
        envelope,
        sequence,
        now,
    } = input;
    let tx = c.transaction()?;
    let n:i64=tx.query_row("SELECT COUNT(*) FROM collaboration_messages WHERE session_id=?1 AND delivery IN ('accepted','queued','delivered')",[session],|r|r.get(0))?;
    if n >= MAX_INBOX_PER_SESSION {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::other("inbox_full"),
        )));
    }
    let changed=tx.execute("INSERT OR IGNORE INTO collaboration_messages(message_id,session_id,sender_json,receiver_json,envelope_json,idempotency_key,delivery,sequence,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'queued',?7,?8)",params![message_id,session,json(sender)?,json(receiver)?,json(envelope)?,key,sequence as i64,now])?;
    tx.commit()?;
    Ok(changed == 1)
}
pub fn transition(
    c: &Connection,
    message_id: &str,
    from: &str,
    to: &str,
    expected: u64,
) -> Result<bool, rusqlite::Error> {
    if from == "unknown" {
        return Ok(false);
    }
    Ok(c.execute("UPDATE collaboration_messages SET delivery=?1,revision=revision+1,delivered_at_ms=CASE WHEN ?1='delivered' THEN COALESCE(delivered_at_ms, strftime('%s','now')*1000) ELSE delivered_at_ms END WHERE message_id=?2 AND delivery=?3 AND revision=?4",params![to,message_id,from,expected as i64])?==1)
}
pub fn list<T: DeserializeOwned>(
    c: &Connection,
    session: &str,
    limit: u32,
) -> Result<Vec<T>, rusqlite::Error> {
    let mut s=c.prepare("SELECT envelope_json FROM collaboration_messages WHERE session_id=?1 ORDER BY sequence LIMIT ?2")?;
    let rows = s.query_map(params![session, i64::from(limit.min(128))], |r| {
        parse(r.get(0)?)
    })?;
    rows.collect()
}
pub fn reconcile(c: &Connection) -> Result<u32, rusqlite::Error> {
    Ok(c.execute("UPDATE collaboration_messages SET delivery='unknown',revision=revision+1 WHERE delivery='delivered'",[])? as u32)
}
pub fn exists(c: &Connection, key: &str) -> Result<bool, rusqlite::Error> {
    c.query_row(
        "SELECT EXISTS(SELECT 1 FROM collaboration_messages WHERE idempotency_key=?1)",
        [key],
        |r| r.get(0),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_is_bounded_and_deduped() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(!exists(&c, "x").unwrap());
    }
    #[test]
    fn transition_is_compare_and_set_and_unknown_is_terminal() {
        let mut c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(enqueue(
            &mut c,
            EnqueueInput {
                session: "s",
                key: "k",
                message_id: "m",
                sender: &"parent",
                receiver: &"slot",
                envelope: &"envelope",
                sequence: 1,
                now: 1,
            },
        )
        .unwrap());
        assert!(transition(&c, "m", "queued", "delivered", 1).unwrap());
        assert!(!transition(&c, "m", "queued", "consumed", 1).unwrap());
        assert!(transition(&c, "m", "delivered", "unknown", 2).unwrap());
        assert!(!transition(&c, "m", "unknown", "delivered", 3).unwrap());
    }
}
