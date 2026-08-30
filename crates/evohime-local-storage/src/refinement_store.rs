//! Durable, metadata-first storage for Continual Refinement v1.
//!
//! The store deliberately keeps evidence as bounded references and never
//! stores transcripts, credentials, or raw model reasoning.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const STORE_SCHEMA_VERSION: u32 = 1;
pub const MAX_JSON_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRow {
    pub id: String,
    pub revision: i64,
    pub owner_scope: String,
    pub kind: String,
    pub target: String,
    pub status: String,
    pub pattern_key: String,
    pub title: String,
    pub rationale: String,
    pub content_hash: String,
    pub confidence: u32,
    pub evidence_count: u32,
    pub conflict_count: u32,
    pub policy_snapshot_hash: String,
    pub version: i64,
    pub idempotency_key: String,
    pub error_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum RefinementStoreError {
    #[error("sqlite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("refinement value exceeds bounded JSON limit")]
    TooLarge,
    #[error("candidate version conflict: expected {expected}, current {current}")]
    VersionConflict { expected: i64, current: i64 },
    #[error("idempotency key was reused with a different request")]
    IdempotencyConflict,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS refinement_candidates (
            id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            owner_scope TEXT NOT NULL,
            kind TEXT NOT NULL,
            target TEXT NOT NULL,
            status TEXT NOT NULL,
            pattern_key TEXT NOT NULL,
            title TEXT NOT NULL,
            rationale TEXT NOT NULL,
            content_json TEXT NOT NULL,
            source_task_ids_json TEXT NOT NULL,
            evidence_json TEXT NOT NULL,
            conflicts_json TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            confidence INTEGER NOT NULL,
            policy_snapshot_hash TEXT NOT NULL,
            version INTEGER NOT NULL,
            idempotency_key TEXT NOT NULL,
            error_code TEXT,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(id, revision),
            UNIQUE(owner_scope, pattern_key, revision)
        );
        CREATE INDEX IF NOT EXISTS idx_refinement_candidates_queue
            ON refinement_candidates(owner_scope, status, updated_at_ms);
        CREATE TABLE IF NOT EXISTS refinement_events (
            sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS refinement_idempotency (
            owner_scope TEXT NOT NULL,
            idempotency_key TEXT NOT NULL,
            request_hash TEXT NOT NULL,
            candidate_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            PRIMARY KEY(owner_scope, idempotency_key)
        );",
    )
}

pub struct RefinementStore<'a> {
    connection: &'a Connection,
}

impl<'a> RefinementStore<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_candidate(
        &self,
        row: &CandidateRow,
        content_json: &str,
        source_task_ids_json: &str,
        evidence_json: &str,
        conflicts_json: &str,
    ) -> Result<(), RefinementStoreError> {
        for value in [
            content_json,
            source_task_ids_json,
            evidence_json,
            conflicts_json,
        ] {
            if value.len() > MAX_JSON_BYTES {
                return Err(RefinementStoreError::TooLarge);
            }
        }
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO refinement_candidates
             (id, revision, owner_scope, kind, target, status, pattern_key, title,
              rationale, content_json, source_task_ids_json, evidence_json,
              conflicts_json, content_hash, confidence, policy_snapshot_hash,
              version, idempotency_key, error_code, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                row.id,
                row.revision,
                row.owner_scope,
                row.kind,
                row.target,
                row.status,
                row.pattern_key,
                row.title,
                row.rationale,
                content_json,
                source_task_ids_json,
                evidence_json,
                conflicts_json,
                row.content_hash,
                row.confidence,
                row.policy_snapshot_hash,
                row.version,
                row.idempotency_key,
                row.error_code,
                row.created_at_ms,
                row.updated_at_ms
            ],
        )?;
        tx.execute(
            "INSERT INTO refinement_events(candidate_id, revision, event_type, payload_json, created_at_ms)
             VALUES (?, ?, 'candidate.created', ?, ?)",
            params![row.id, row.revision, "{}", row.created_at_ms],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get(
        &self,
        id: &str,
        revision: i64,
    ) -> Result<Option<CandidateRow>, RefinementStoreError> {
        self.connection
            .query_row(
                "SELECT id, revision, owner_scope, kind, target, status, pattern_key,
                    title, rationale, content_hash, confidence,
                    json_array_length(evidence_json), json_array_length(conflicts_json),
                    policy_snapshot_hash, version, idempotency_key, error_code,
                    created_at_ms, updated_at_ms
             FROM refinement_candidates WHERE id = ? AND revision = ?",
                params![id, revision],
                |row| {
                    Ok(CandidateRow {
                        id: row.get(0)?,
                        revision: row.get(1)?,
                        owner_scope: row.get(2)?,
                        kind: row.get(3)?,
                        target: row.get(4)?,
                        status: row.get(5)?,
                        pattern_key: row.get(6)?,
                        title: row.get(7)?,
                        rationale: row.get(8)?,
                        content_hash: row.get(9)?,
                        confidence: row.get(10)?,
                        evidence_count: row.get::<_, u32>(11).unwrap_or(0),
                        conflict_count: row.get::<_, u32>(12).unwrap_or(0),
                        policy_snapshot_hash: row.get(13)?,
                        version: row.get(14)?,
                        idempotency_key: row.get(15)?,
                        error_code: row.get(16)?,
                        created_at_ms: row.get(17)?,
                        updated_at_ms: row.get(18)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn replay_idempotency(
        &self,
        owner_scope: &str,
        key: &str,
        request_hash: &str,
    ) -> Result<Option<CandidateRow>, RefinementStoreError> {
        let Some((stored_hash, candidate_id, revision)) = self
            .connection
            .query_row(
                "SELECT request_hash, candidate_id, revision
                 FROM refinement_idempotency WHERE owner_scope = ? AND idempotency_key = ?",
                params![owner_scope, key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
        else {
            return Ok(None);
        };
        if stored_hash != request_hash {
            return Err(RefinementStoreError::IdempotencyConflict);
        }
        self.get(&candidate_id, revision)
    }

    pub fn list(
        &self,
        owner_scope: &str,
        limit: u32,
    ) -> Result<Vec<CandidateRow>, RefinementStoreError> {
        let limit = i64::from(limit.clamp(1, 128));
        let mut statement = self.connection.prepare(
            "SELECT id, revision, owner_scope, kind, target, status, pattern_key,
                    title, rationale, content_hash, confidence,
                    json_array_length(evidence_json), json_array_length(conflicts_json),
                    policy_snapshot_hash, version, idempotency_key, error_code,
                    created_at_ms, updated_at_ms
             FROM refinement_candidates WHERE owner_scope = ?
             ORDER BY updated_at_ms DESC LIMIT ?",
        )?;
        let rows = statement.query_map(params![owner_scope, limit], |row| {
            Ok(CandidateRow {
                id: row.get(0)?,
                revision: row.get(1)?,
                owner_scope: row.get(2)?,
                kind: row.get(3)?,
                target: row.get(4)?,
                status: row.get(5)?,
                pattern_key: row.get(6)?,
                title: row.get(7)?,
                rationale: row.get(8)?,
                content_hash: row.get(9)?,
                confidence: row.get(10)?,
                evidence_count: row.get::<_, u32>(11).unwrap_or(0),
                conflict_count: row.get::<_, u32>(12).unwrap_or(0),
                policy_snapshot_hash: row.get(13)?,
                version: row.get(14)?,
                idempotency_key: row.get(15)?,
                error_code: row.get(16)?,
                created_at_ms: row.get(17)?,
                updated_at_ms: row.get(18)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn transition(
        &self,
        id: &str,
        revision: i64,
        expected_version: i64,
        status: &str,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<CandidateRow, RefinementStoreError> {
        self.transition_with_idempotency(
            id,
            revision,
            expected_version,
            status,
            error_code,
            now_ms,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn transition_with_idempotency(
        &self,
        id: &str,
        revision: i64,
        expected_version: i64,
        status: &str,
        error_code: Option<&str>,
        now_ms: i64,
        idempotency: Option<(&str, &str)>,
    ) -> Result<CandidateRow, RefinementStoreError> {
        let current = self
            .get(id, revision)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if current.version != expected_version {
            return Err(RefinementStoreError::VersionConflict {
                expected: expected_version,
                current: current.version,
            });
        }
        let tx = self.connection.unchecked_transaction()?;
        if let Some((key, request_hash)) = idempotency {
            let existing = tx
                .query_row(
                    "SELECT request_hash, candidate_id, revision
                     FROM refinement_idempotency WHERE owner_scope = ? AND idempotency_key = ?",
                    params![current.owner_scope, key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((stored_hash, candidate_id, candidate_revision)) = existing {
                if stored_hash != request_hash {
                    return Err(RefinementStoreError::IdempotencyConflict);
                }
                tx.commit()?;
                return self
                    .get(&candidate_id, candidate_revision)?
                    .ok_or(rusqlite::Error::QueryReturnedNoRows.into());
            }
        }
        let changed = tx.execute(
            "UPDATE refinement_candidates SET status = ?, error_code = ?, version = version + 1, updated_at_ms = ?
             WHERE id = ? AND revision = ? AND version = ?",
            params![status, error_code, now_ms, id, revision, expected_version],
        )?;
        if changed != 1 {
            return Err(RefinementStoreError::VersionConflict {
                expected: expected_version,
                current: expected_version,
            });
        }
        if let Some((key, request_hash)) = idempotency {
            tx.execute(
                "INSERT INTO refinement_idempotency
                 (owner_scope, idempotency_key, request_hash, candidate_id, revision)
                 VALUES (?, ?, ?, ?, ?)",
                params![current.owner_scope, key, request_hash, id, revision],
            )?;
        }
        tx.execute(
            "INSERT INTO refinement_events(candidate_id, revision, event_type, payload_json, created_at_ms)
             VALUES (?, ?, ?, ?, ?)",
            params![
                id,
                revision,
                format!("candidate.{status}"),
                serde_json::json!({
                    "version": expected_version + 1,
                    "error_code": error_code,
                })
                .to_string(),
                now_ms
            ],
        )?;
        tx.commit()?;
        self.get(id, revision)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> CandidateRow {
        CandidateRow {
            id: "c1".into(),
            revision: 1,
            owner_scope: "workspace:w1".into(),
            kind: "memory".into(),
            target: "memory".into(),
            status: "proposed".into(),
            pattern_key: "p1".into(),
            title: "bounded".into(),
            rationale: "r".into(),
            content_hash: "h".into(),
            confidence: 80,
            evidence_count: 2,
            conflict_count: 0,
            policy_snapshot_hash: "ph".into(),
            version: 0,
            idempotency_key: "i1".into(),
            error_code: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn round_trip_and_optimistic_transition() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let store = RefinementStore::new(&connection);
        store
            .insert_candidate(&row(), "{}", "[\"t1\"]", "[{} , {}]", "[]")
            .unwrap();
        assert_eq!(store.get("c1", 1).unwrap().unwrap().evidence_count, 2);
        let updated = store
            .transition_with_idempotency(
                "c1",
                1,
                0,
                "approved",
                None,
                2,
                Some(("action-1", "request-hash")),
            )
            .unwrap();
        assert_eq!(updated.version, 1);
        assert_eq!(
            store
                .replay_idempotency("workspace:w1", "action-1", "request-hash")
                .unwrap()
                .unwrap(),
            updated
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM refinement_events WHERE candidate_id = 'c1'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
        assert!(matches!(
            store.transition("c1", 1, 0, "active", None, 3),
            Err(RefinementStoreError::VersionConflict { .. })
        ));
    }

    #[test]
    fn large_payload_is_rejected_before_write() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let store = RefinementStore::new(&connection);
        let value = "x".repeat(MAX_JSON_BYTES + 1);
        assert!(matches!(
            store.insert_candidate(&row(), &value, "[]", "[]", "[]"),
            Err(RefinementStoreError::TooLarge)
        ));
    }
}
