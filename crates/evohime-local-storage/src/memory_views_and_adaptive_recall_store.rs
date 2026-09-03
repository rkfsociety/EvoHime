//! Durable, metadata-only storage for Core-owned memory views and read barriers.

use rusqlite::{params, Connection, OptionalExtension};

pub struct ViewInput<'a> {
    pub view_id: &'a str,
    pub owner_scope: &'a str,
    pub revision: u64,
    pub view_json: &'a [u8],
    pub content_hash: &'a str,
    pub expected_version: u64,
    pub idempotency_key: &'a str,
    pub now_ms: i64,
}

pub struct RecallInput<'a> {
    pub view_id: &'a str,
    pub view_revision: u64,
    pub barrier_generation: u64,
    pub decision_json: &'a [u8],
    pub expected_version: u64,
    pub idempotency_key: &'a str,
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewRecord {
    pub owner_scope: String,
    pub revision: u64,
    pub view_json: Vec<u8>,
    pub content_hash: String,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallRecord {
    pub view_revision: u64,
    pub barrier_generation: u64,
    pub decision_json: Vec<u8>,
    pub version: u64,
}

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_views_and_adaptive_recall_views (view_id TEXT PRIMARY KEY NOT NULL, owner_scope TEXT NOT NULL, revision INTEGER NOT NULL, view_json BLOB NOT NULL, content_hash TEXT NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS memory_views_and_adaptive_recall_barriers (view_id TEXT PRIMARY KEY NOT NULL, view_revision INTEGER NOT NULL, barrier_generation INTEGER NOT NULL, decision_json BLOB NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);",
    )
}

pub fn save_view(c: &Connection, input: ViewInput<'_>) -> rusqlite::Result<bool> {
    let current: Option<(u64, Vec<u8>, String)> = c
        .query_row(
            "SELECT version,view_json,idempotency_key FROM memory_views_and_adaptive_recall_views WHERE view_id=?1",
            [input.view_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((version, previous, previous_key)) = current {
        if version != input.expected_version {
            return Ok(false);
        }
        if previous == input.view_json && previous_key == input.idempotency_key {
            return Ok(true);
        }
        return Ok(c.execute(
            "UPDATE memory_views_and_adaptive_recall_views SET owner_scope=?1,revision=?2,view_json=?3,content_hash=?4,version=version+1,idempotency_key=?5,updated_at_ms=?6 WHERE view_id=?7 AND version=?8",
            params![input.owner_scope, input.revision as i64, input.view_json, input.content_hash, input.idempotency_key, input.now_ms, input.view_id, input.expected_version as i64],
        )? == 1);
    }
    if input.expected_version != 0 {
        return Ok(false);
    }
    Ok(c.execute(
        "INSERT INTO memory_views_and_adaptive_recall_views(view_id,owner_scope,revision,view_json,content_hash,version,idempotency_key,updated_at_ms) VALUES (?1,?2,?3,?4,?5,1,?6,?7)",
        params![input.view_id, input.owner_scope, input.revision as i64, input.view_json, input.content_hash, input.idempotency_key, input.now_ms],
    )? == 1)
}

pub fn load_view(c: &Connection, view_id: &str) -> rusqlite::Result<Option<ViewRecord>> {
    c.query_row(
        "SELECT owner_scope,revision,view_json,content_hash,version FROM memory_views_and_adaptive_recall_views WHERE view_id=?1",
        [view_id],
        |r| Ok(ViewRecord { owner_scope: r.get(0)?, revision: r.get(1)?, view_json: r.get(2)?, content_hash: r.get(3)?, version: r.get(4)? }),
    )
    .optional()
}

pub fn save_recall(c: &Connection, input: RecallInput<'_>) -> rusqlite::Result<bool> {
    let current: Option<(u64, u64, String, Vec<u8>)> = c
        .query_row(
            "SELECT version,barrier_generation,idempotency_key,decision_json FROM memory_views_and_adaptive_recall_barriers WHERE view_id=?1",
            [input.view_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    if let Some((version, previous_generation, previous_key, previous_decision)) = current {
        if version != input.expected_version {
            return Ok(false);
        }
        if previous_key == input.idempotency_key && previous_decision == input.decision_json {
            return Ok(true);
        }
        if input.barrier_generation < previous_generation {
            return Ok(false);
        }
        return Ok(c.execute(
            "UPDATE memory_views_and_adaptive_recall_barriers SET view_revision=?1,barrier_generation=?2,decision_json=?3,version=version+1,idempotency_key=?4,updated_at_ms=?5 WHERE view_id=?6 AND version=?7",
            params![input.view_revision as i64, input.barrier_generation as i64, input.decision_json, input.idempotency_key, input.now_ms, input.view_id, input.expected_version as i64],
        )? == 1);
    }
    if input.expected_version != 0 {
        return Ok(false);
    }
    Ok(c.execute(
        "INSERT INTO memory_views_and_adaptive_recall_barriers(view_id,view_revision,barrier_generation,decision_json,version,idempotency_key,updated_at_ms) VALUES (?1,?2,?3,?4,1,?5,?6)",
        params![input.view_id, input.view_revision as i64, input.barrier_generation as i64, input.decision_json, input.idempotency_key, input.now_ms],
    )? == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_and_barrier_are_durable_and_version_fenced() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        assert!(save_view(
            &c,
            ViewInput {
                view_id: "view",
                owner_scope: "agent",
                revision: 1,
                view_json: br#"{}"#,
                content_hash: "hash",
                expected_version: 0,
                idempotency_key: "create",
                now_ms: 1,
            }
        )
        .unwrap());
        assert!(save_recall(
            &c,
            RecallInput {
                view_id: "view",
                view_revision: 1,
                barrier_generation: 4,
                decision_json: br#"{}"#,
                expected_version: 0,
                idempotency_key: "recall",
                now_ms: 2,
            }
        )
        .unwrap());
        assert!(!save_recall(
            &c,
            RecallInput {
                view_id: "view",
                view_revision: 1,
                barrier_generation: 3,
                decision_json: br#"{"older":true}"#,
                expected_version: 1,
                idempotency_key: "older",
                now_ms: 3,
            }
        )
        .unwrap());
        assert_eq!(load_view(&c, "view").unwrap().unwrap().version, 1);
        assert!(!save_view(
            &c,
            ViewInput {
                view_id: "view",
                owner_scope: "agent",
                revision: 2,
                view_json: br#"{"changed":true}"#,
                content_hash: "hash2",
                expected_version: 0,
                idempotency_key: "stale",
                now_ms: 3,
            }
        )
        .unwrap());
    }
}
