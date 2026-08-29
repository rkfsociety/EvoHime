//! SQLite persistence for retained children and their durable mailbox.
//! All mutations are parent-scoped and use SQLite uniqueness for idempotency.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;

pub const MAX_PENDING_PER_CHILD: i64 = 32;

#[derive(Debug)]
pub enum RetainedStoreError {
    Sql(rusqlite::Error),
    Json(serde_json::Error),
    LimitExceeded,
    Duplicate,
}
impl fmt::Display for RetainedStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RetainedStoreError {}
impl From<rusqlite::Error> for RetainedStoreError {
    fn from(x: rusqlite::Error) -> Self {
        Self::Sql(x)
    }
}
impl From<serde_json::Error> for RetainedStoreError {
    fn from(x: serde_json::Error) -> Self {
        Self::Json(x)
    }
}

pub fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS retained_children (
        parent_id TEXT NOT NULL, child_id TEXT NOT NULL, family_root_id TEXT NOT NULL,
        registry_version INTEGER NOT NULL, revision INTEGER NOT NULL, lifecycle TEXT NOT NULL,
        record_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, last_active_at_ms INTEGER NOT NULL,
        retained_until_ms INTEGER NOT NULL, PRIMARY KEY(parent_id, child_id));
      CREATE INDEX IF NOT EXISTS idx_retained_children_parent ON retained_children(parent_id, lifecycle, last_active_at_ms);
      CREATE TABLE IF NOT EXISTS child_follow_ups (
        idempotency_key TEXT PRIMARY KEY NOT NULL, parent_id TEXT NOT NULL, child_id TEXT NOT NULL,
        expected_revision INTEGER NOT NULL, parent_sequence INTEGER NOT NULL, request_json BLOB NOT NULL,
        outcome TEXT NOT NULL, created_at_ms INTEGER NOT NULL);
      CREATE INDEX IF NOT EXISTS idx_child_follow_ups_scope ON child_follow_ups(parent_id, child_id, created_at_ms);
      CREATE TABLE IF NOT EXISTS child_mailbox (
        message_id TEXT PRIMARY KEY NOT NULL, parent_id TEXT NOT NULL, receiver_id TEXT NOT NULL,
        idempotency_key TEXT NOT NULL UNIQUE, delivery TEXT NOT NULL, parent_sequence INTEGER NOT NULL,
        entry_json BLOB NOT NULL, created_at_ms INTEGER NOT NULL, delivered_at_ms INTEGER);
      CREATE INDEX IF NOT EXISTS idx_child_mailbox_pending ON child_mailbox(parent_id, receiver_id, delivery, parent_sequence);
      CREATE TABLE IF NOT EXISTS child_retained_sequences (
        parent_id TEXT PRIMARY KEY NOT NULL, next_sequence INTEGER NOT NULL DEFAULT 0);")
}

fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, RetainedStoreError> {
    Ok(serde_json::to_vec(value)?)
}
fn parse<T: DeserializeOwned>(value: &[u8]) -> Result<T, RetainedStoreError> {
    Ok(serde_json::from_slice(value)?)
}

pub struct RetainedChildStore;
impl RetainedChildStore {
    pub fn next_parent_sequence(
        connection: &mut Connection,
        parent_id: &str,
    ) -> Result<u64, RetainedStoreError> {
        let tx = connection.transaction()?;
        tx.execute("INSERT INTO child_retained_sequences(parent_id,next_sequence) VALUES(?1,0) ON CONFLICT(parent_id) DO NOTHING",[parent_id])?;
        tx.execute(
            "UPDATE child_retained_sequences SET next_sequence=next_sequence+1 WHERE parent_id=?1",
            [parent_id],
        )?;
        let n: i64 = tx.query_row(
            "SELECT next_sequence FROM child_retained_sequences WHERE parent_id=?1",
            [parent_id],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(n as u64)
    }
    pub fn upsert_child<T: Serialize>(
        connection: &Connection,
        parent_id: &str,
        child_id: &str,
        revision: u64,
        registry_version: u64,
        lifecycle: &str,
        record: &T,
        created_at_ms: u64,
        last_active_at_ms: u64,
        retained_until_ms: u64,
    ) -> Result<(), RetainedStoreError> {
        connection.execute("INSERT INTO retained_children(parent_id,child_id,family_root_id,registry_version,revision,lifecycle,record_json,created_at_ms,last_active_at_ms,retained_until_ms) VALUES(?1,?2,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(parent_id,child_id) DO UPDATE SET registry_version=excluded.registry_version,revision=excluded.revision,lifecycle=excluded.lifecycle,record_json=excluded.record_json,last_active_at_ms=excluded.last_active_at_ms,retained_until_ms=excluded.retained_until_ms WHERE retained_children.registry_version < excluded.registry_version",params![parent_id,child_id,registry_version as i64,revision as i64,lifecycle,json(record)?,created_at_ms as i64,last_active_at_ms as i64,retained_until_ms as i64])?;
        Ok(())
    }
    pub fn get_child<T: DeserializeOwned>(
        connection: &Connection,
        parent_id: &str,
        child_id: &str,
    ) -> Result<Option<T>, RetainedStoreError> {
        Ok(connection
            .query_row(
                "SELECT record_json FROM retained_children WHERE parent_id=?1 AND child_id=?2",
                params![parent_id, child_id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|x| parse(&x))
            .transpose()?)
    }
    pub fn list_children<T: DeserializeOwned>(
        connection: &Connection,
        parent_id: &str,
        limit: u32,
    ) -> Result<Vec<T>, RetainedStoreError> {
        let mut s=connection.prepare("SELECT record_json FROM retained_children WHERE parent_id=?1 ORDER BY last_active_at_ms DESC LIMIT ?2")?;
        let rows = s.query_map(params![parent_id, i64::from(limit.clamp(1, 100))], |r| {
            r.get::<_, Vec<u8>>(0)
        })?;
        rows.map(|r| r.map_err(RetainedStoreError::from).and_then(|x| parse(&x)))
            .collect()
    }
    pub fn insert_follow_up<T: Serialize>(
        connection: &Connection,
        parent_id: &str,
        child_id: &str,
        key: &str,
        revision: u64,
        sequence: u64,
        request: &T,
        now_ms: u64,
    ) -> Result<bool, RetainedStoreError> {
        let n=connection.execute("INSERT OR IGNORE INTO child_follow_ups(idempotency_key,parent_id,child_id,expected_revision,parent_sequence,request_json,outcome,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'pending',?7)",params![key,parent_id,child_id,revision as i64,sequence as i64,json(request)?,now_ms as i64])?;
        Ok(n == 1)
    }
    pub fn insert_mailbox<T: Serialize>(
        connection: &Connection,
        parent_id: &str,
        child_id: &str,
        key: &str,
        message_id: &str,
        sequence: u64,
        entry: &T,
        now_ms: u64,
    ) -> Result<bool, RetainedStoreError> {
        let pending:i64=connection.query_row("SELECT COUNT(*) FROM child_mailbox WHERE parent_id=?1 AND receiver_id=?2 AND delivery IN ('pending','dispatched')",params![parent_id,child_id],|r|r.get(0))?;
        if pending >= MAX_PENDING_PER_CHILD {
            return Err(RetainedStoreError::LimitExceeded);
        }
        let n=connection.execute("INSERT OR IGNORE INTO child_mailbox(message_id,parent_id,receiver_id,idempotency_key,delivery,parent_sequence,entry_json,created_at_ms) VALUES(?1,?2,?3,?4,'pending',?5,?6,?7)",params![message_id,parent_id,child_id,key,sequence as i64,json(entry)?,now_ms as i64])?;
        Ok(n == 1)
    }
    pub fn get_mailbox<T: DeserializeOwned>(
        connection: &Connection,
        parent_id: &str,
        key: &str,
    ) -> Result<Option<T>, RetainedStoreError> {
        Ok(connection
            .query_row(
                "SELECT entry_json FROM child_mailbox WHERE parent_id=?1 AND idempotency_key=?2",
                params![parent_id, key],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|x| parse(&x))
            .transpose()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, Serialize, serde::Deserialize, PartialEq)]
    struct R {
        v: u8,
    }
    #[test]
    fn scope_and_dedup() {
        let mut c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert_eq!(
            RetainedChildStore::next_parent_sequence(&mut c, "p").unwrap(),
            1
        );
        assert!(
            RetainedChildStore::insert_follow_up(&c, "p", "c", "k", 1, 1, &R { v: 1 }, 1).unwrap()
        );
        assert!(
            !RetainedChildStore::insert_follow_up(&c, "p", "c", "k", 1, 1, &R { v: 1 }, 1).unwrap()
        );
        assert_eq!(
            RetainedChildStore::get_child::<R>(&c, "other", "c").unwrap(),
            None
        );
    }
}
