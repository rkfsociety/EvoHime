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
pub struct UpsertChildInput<'a, T> {
    pub parent_id: &'a str,
    pub child_id: &'a str,
    pub family_root_id: &'a str,
    pub revision: u64,
    pub registry_version: u64,
    pub lifecycle: &'a str,
    pub record: &'a T,
    pub created_at_ms: u64,
    pub last_active_at_ms: u64,
    pub retained_until_ms: u64,
}

pub struct InsertFollowUpInput<'a, T> {
    pub parent_id: &'a str,
    pub child_id: &'a str,
    pub idempotency_key: &'a str,
    pub expected_revision: u64,
    pub parent_sequence: u64,
    pub request: &'a T,
    pub now_ms: u64,
}

impl<'a, T> Copy for InsertFollowUpInput<'a, T> {}
impl<'a, T> Clone for InsertFollowUpInput<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Clone, Copy)]
pub struct InsertMailboxInput<'a, T> {
    pub parent_id: &'a str,
    pub child_id: &'a str,
    pub idempotency_key: &'a str,
    pub message_id: &'a str,
    pub parent_sequence: u64,
    pub entry: &'a T,
    pub now_ms: u64,
}

pub struct EnqueueFollowUpInput<'a, R, F> {
    pub parent_id: &'a str,
    pub child_id: &'a str,
    pub idempotency_key: &'a str,
    pub expected_revision: u64,
    pub request: &'a R,
    pub message_id: &'a str,
    pub build_entry: F,
    pub now_ms: u64,
}

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
        input: UpsertChildInput<'_, T>,
    ) -> Result<(), RetainedStoreError> {
        connection.execute("INSERT INTO retained_children(parent_id,child_id,family_root_id,registry_version,revision,lifecycle,record_json,created_at_ms,last_active_at_ms,retained_until_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(parent_id,child_id) DO UPDATE SET family_root_id=excluded.family_root_id,registry_version=excluded.registry_version,revision=excluded.revision,lifecycle=excluded.lifecycle,record_json=excluded.record_json,last_active_at_ms=excluded.last_active_at_ms,retained_until_ms=excluded.retained_until_ms WHERE retained_children.registry_version < excluded.registry_version",params![input.parent_id,input.child_id,input.family_root_id,input.registry_version as i64,input.revision as i64,input.lifecycle,json(input.record)?,input.created_at_ms as i64,input.last_active_at_ms as i64,input.retained_until_ms as i64])?;
        Ok(())
    }
    pub fn get_child<T: DeserializeOwned>(
        connection: &Connection,
        parent_id: &str,
        child_id: &str,
    ) -> Result<Option<T>, RetainedStoreError> {
        connection
            .query_row(
                "SELECT record_json FROM retained_children WHERE parent_id=?1 AND child_id=?2",
                params![parent_id, child_id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|x| parse(&x))
            .transpose()
    }
    pub fn list_children<T: DeserializeOwned>(
        connection: &Connection,
        parent_id: &str,
        now_ms: u64,
        limit: u32,
    ) -> Result<Vec<T>, RetainedStoreError> {
        let mut s=connection.prepare("SELECT record_json FROM retained_children WHERE parent_id=?1 AND retained_until_ms>=?2 AND lifecycle <> 'deleted' ORDER BY last_active_at_ms DESC LIMIT ?3")?;
        let rows = s.query_map(
            params![parent_id, now_ms as i64, i64::from(limit.clamp(1, 100))],
            |r| r.get::<_, Vec<u8>>(0),
        )?;
        rows.map(|r| r.map_err(RetainedStoreError::from).and_then(|x| parse(&x)))
            .collect()
    }
    pub fn insert_follow_up<T: Serialize>(
        connection: &Connection,
        input: InsertFollowUpInput<'_, T>,
    ) -> Result<bool, RetainedStoreError> {
        let n=connection.execute("INSERT OR IGNORE INTO child_follow_ups(idempotency_key,parent_id,child_id,expected_revision,parent_sequence,request_json,outcome,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'pending',?7)",params![input.idempotency_key,input.parent_id,input.child_id,input.expected_revision as i64,input.parent_sequence as i64,json(input.request)?,input.now_ms as i64])?;
        Ok(n == 1)
    }
    pub fn has_follow_up(connection: &Connection, key: &str) -> Result<bool, RetainedStoreError> {
        Ok(connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM child_follow_ups WHERE idempotency_key=?1)",
            [key],
            |r| r.get(0),
        )?)
    }
    pub fn insert_mailbox<T: Serialize>(
        connection: &Connection,
        input: InsertMailboxInput<'_, T>,
    ) -> Result<bool, RetainedStoreError> {
        let pending:i64=connection.query_row("SELECT COUNT(*) FROM child_mailbox WHERE parent_id=?1 AND receiver_id=?2 AND delivery IN ('pending','dispatched')",params![input.parent_id,input.child_id],|r|r.get(0))?;
        if pending >= MAX_PENDING_PER_CHILD {
            return Err(RetainedStoreError::LimitExceeded);
        }
        let n=connection.execute("INSERT OR IGNORE INTO child_mailbox(message_id,parent_id,receiver_id,idempotency_key,delivery,parent_sequence,entry_json,created_at_ms) VALUES(?1,?2,?3,?4,'pending',?5,?6,?7)",params![input.message_id,input.parent_id,input.child_id,input.idempotency_key,input.parent_sequence as i64,json(input.entry)?,input.now_ms as i64])?;
        Ok(n == 1)
    }
    pub fn enqueue_follow_up<R: Serialize, E: Serialize, F: FnOnce(u64) -> E>(
        connection: &mut Connection,
        input: EnqueueFollowUpInput<'_, R, F>,
    ) -> Result<bool, RetainedStoreError> {
        let tx = connection.transaction()?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM child_follow_ups WHERE idempotency_key=?1)",
            [input.idempotency_key],
            |r| r.get::<_, bool>(0),
        )? {
            return Ok(false);
        }
        let pending: i64 = tx.query_row(
            "SELECT COUNT(*) FROM child_mailbox WHERE parent_id=?1 AND receiver_id=?2 AND delivery IN ('pending','dispatched')",
            params![input.parent_id, input.child_id],
            |r| r.get(0),
        )?;
        if pending >= MAX_PENDING_PER_CHILD {
            return Err(RetainedStoreError::LimitExceeded);
        }
        tx.execute("INSERT INTO child_retained_sequences(parent_id,next_sequence) VALUES(?1,0) ON CONFLICT(parent_id) DO NOTHING", [input.parent_id])?;
        tx.execute(
            "UPDATE child_retained_sequences SET next_sequence=next_sequence+1 WHERE parent_id=?1",
            [input.parent_id],
        )?;
        let sequence: i64 = tx.query_row(
            "SELECT next_sequence FROM child_retained_sequences WHERE parent_id=?1",
            [input.parent_id],
            |r| r.get(0),
        )?;
        tx.execute("INSERT INTO child_follow_ups(idempotency_key,parent_id,child_id,expected_revision,parent_sequence,request_json,outcome,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'pending',?7)", params![input.idempotency_key,input.parent_id,input.child_id,input.expected_revision as i64,sequence,json(input.request)?,input.now_ms as i64])?;
        let sequence = u64::try_from(sequence).map_err(|_| RetainedStoreError::LimitExceeded)?;
        let entry = (input.build_entry)(sequence);
        tx.execute("INSERT INTO child_mailbox(message_id,parent_id,receiver_id,idempotency_key,delivery,parent_sequence,entry_json,created_at_ms) VALUES(?1,?2,?3,?4,'pending',?5,?6,?7)", params![input.message_id,input.parent_id,input.child_id,input.idempotency_key,sequence,json(&entry)?,input.now_ms as i64])?;
        tx.commit()?;
        Ok(true)
    }
    pub fn get_mailbox<T: DeserializeOwned>(
        connection: &Connection,
        parent_id: &str,
        key: &str,
    ) -> Result<Option<T>, RetainedStoreError> {
        connection
            .query_row(
                "SELECT entry_json FROM child_mailbox WHERE parent_id=?1 AND idempotency_key=?2",
                params![parent_id, key],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|x| parse(&x))
            .transpose()
    }
    pub fn mark_lifecycle(
        connection: &Connection,
        parent_id: &str,
        child_id: &str,
        lifecycle: &str,
        registry_version: u64,
    ) -> Result<bool, RetainedStoreError> {
        Ok(connection.execute("UPDATE retained_children SET lifecycle=?3,registry_version=?4 WHERE parent_id=?1 AND child_id=?2 AND registry_version < ?4", params![parent_id, child_id, lifecycle, registry_version as i64])? == 1)
    }
    pub fn pending_count(
        connection: &Connection,
        parent_id: &str,
        child_id: &str,
    ) -> Result<u32, RetainedStoreError> {
        Ok(connection.query_row("SELECT COUNT(*) FROM child_mailbox WHERE parent_id=?1 AND receiver_id=?2 AND delivery IN ('pending','dispatched')", params![parent_id, child_id], |r| r.get::<_, i64>(0))? as u32)
    }
    pub fn delete_child(
        connection: &Connection,
        parent_id: &str,
        child_id: &str,
        expected_registry_version: u64,
    ) -> Result<bool, RetainedStoreError> {
        Ok(connection.execute("UPDATE retained_children SET lifecycle='deleted', registry_version=registry_version+1 WHERE parent_id=?1 AND child_id=?2 AND registry_version=?3 AND lifecycle <> 'deleted'", params![parent_id, child_id, expected_registry_version as i64])? == 1)
    }
    pub fn transition_mailbox(
        connection: &Connection,
        parent_id: &str,
        message_id: &str,
        from: &str,
        to: &str,
        delivered_at_ms: Option<u64>,
    ) -> Result<bool, RetainedStoreError> {
        if to == "delivered" && from == "unknown" {
            return Ok(false);
        }
        Ok(connection.execute("UPDATE child_mailbox SET delivery=?4, delivered_at_ms=?5 WHERE parent_id=?1 AND message_id=?2 AND delivery=?3", params![parent_id, message_id, from, to, delivered_at_ms.map(|x| x as i64)])? == 1)
    }
    pub fn reconcile_unknown(
        connection: &Connection,
        parent_id: &str,
    ) -> Result<u32, RetainedStoreError> {
        Ok(connection.execute("UPDATE child_mailbox SET delivery='unknown' WHERE parent_id=?1 AND delivery='dispatched'", [parent_id])? as u32)
    }
    pub fn reconcile_all_unknown(connection: &Connection) -> Result<u32, RetainedStoreError> {
        Ok(connection.execute(
            "UPDATE child_mailbox SET delivery='unknown' WHERE delivery='dispatched'",
            [],
        )? as u32)
    }
    pub fn expire_due(connection: &Connection, now_ms: u64) -> Result<u32, RetainedStoreError> {
        Ok(connection.execute("UPDATE child_mailbox SET delivery='expired' WHERE delivery IN ('pending','dispatched') AND created_at_ms < ?1", [now_ms.saturating_sub(24 * 60 * 60 * 1000) as i64])? as u32)
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
        let input = InsertFollowUpInput {
            parent_id: "p",
            child_id: "c",
            idempotency_key: "k",
            expected_revision: 1,
            parent_sequence: 1,
            request: &R { v: 1 },
            now_ms: 1,
        };
        assert!(RetainedChildStore::insert_follow_up(&c, input).unwrap());
        assert!(!RetainedChildStore::insert_follow_up(&c, input).unwrap());
        assert_eq!(
            RetainedChildStore::get_child::<R>(&c, "other", "c").unwrap(),
            None
        );
    }
    #[test]
    fn unknown_delivery_is_terminal_and_not_success() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(RetainedChildStore::insert_mailbox(
            &c,
            InsertMailboxInput {
                parent_id: "p",
                child_id: "c",
                idempotency_key: "k",
                message_id: "m",
                parent_sequence: 1,
                entry: &R { v: 1 },
                now_ms: 1,
            },
        )
        .unwrap());
        assert!(
            RetainedChildStore::transition_mailbox(&c, "p", "m", "pending", "unknown", None)
                .unwrap()
        );
        assert!(!RetainedChildStore::transition_mailbox(
            &c,
            "p",
            "m",
            "unknown",
            "delivered",
            Some(2)
        )
        .unwrap());
    }
}
