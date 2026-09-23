//! Durable, metadata-first storage for Continual Refinement v1.
//!
//! The store deliberately keeps evidence as bounded references and never
//! stores transcripts, credentials, or raw model reasoning.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Version of the continual-refinement persistence schema.
pub const STORE_SCHEMA_VERSION: u32 = 1;
/// Maximum size of each serialized candidate, evidence, conflicts, or source-task JSON value.
pub const MAX_JSON_BYTES: usize = 64 * 1024;

/// Bounded summary of one immutable refinement candidate revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRow {
    /// Stable candidate identifier.
    pub id: String,
    /// Immutable candidate revision.
    pub revision: i64,
    /// Scope that owns the candidate.
    pub owner_scope: String,
    /// Candidate category.
    pub kind: String,
    /// Target subsystem or behavior the candidate concerns.
    pub target: String,
    /// Candidate lifecycle status.
    pub status: String,
    /// Stable key used to detect duplicate refinement patterns.
    pub pattern_key: String,
    /// Short human-readable title.
    pub title: String,
    /// Bounded rationale for the candidate.
    pub rationale: String,
    /// Digest of canonical candidate content.
    pub content_hash: String,
    /// Confidence score recorded by the producer.
    pub confidence: u32,
    /// Number of evidence references stored with the candidate.
    pub evidence_count: u32,
    /// Number of conflicting candidate references.
    pub conflict_count: u32,
    /// Digest of the policy snapshot used for evaluation.
    pub policy_snapshot_hash: String,
    /// Optimistic concurrency version for status transitions.
    pub version: i64,
    /// Idempotency key used to admit the candidate.
    pub idempotency_key: String,
    /// Optional error classification associated with its current status.
    pub error_code: Option<String>,
    /// Candidate creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Last candidate update time in Unix milliseconds.
    pub updated_at_ms: i64,
}

/// Serialization, size, concurrency, or idempotency errors from refinement persistence.
#[derive(Debug, thiserror::Error)]
pub enum RefinementStoreError {
    /// A SQLite query or row conversion failed.
    #[error("sqlite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A serialized value could not be parsed or encoded.
    #[error("json operation failed: {0}")]
    Json(#[from] serde_json::Error),
    /// One of the bounded JSON values exceeded [`MAX_JSON_BYTES`].
    #[error("refinement value exceeds bounded JSON limit")]
    TooLarge,
    /// Candidate revision did not match the caller's expected version.
    #[error("candidate version conflict: expected {expected}, current {current}")]
    VersionConflict {
        /// Version expected by the caller.
        expected: i64,
        /// Version currently stored.
        current: i64,
    },
    /// An idempotency key was reused with a different request hash.
    #[error("idempotency key was reused with a different request")]
    IdempotencyConflict,
}

/// Creates the candidate, event, and idempotency tables and indexes.
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

/// SQLite store for candidate revisions, refinement events, and idempotency records.
pub struct RefinementStore<'a> {
    connection: &'a Connection,
}

/// Inputs needed to insert a candidate and its bounded JSON components.
pub struct InsertCandidateInput<'a> {
    /// Summary fields for the candidate revision.
    pub row: &'a CandidateRow,
    /// Serialized candidate content, bounded by [`MAX_JSON_BYTES`].
    pub content_json: &'a str,
    /// Serialized source task identifiers, bounded by [`MAX_JSON_BYTES`].
    pub source_task_ids_json: &'a str,
    /// Serialized evidence references, bounded by [`MAX_JSON_BYTES`].
    pub evidence_json: &'a str,
    /// Serialized conflict references, bounded by [`MAX_JSON_BYTES`].
    pub conflicts_json: &'a str,
}

/// Inputs for an optimistic status transition with optional idempotency.
pub struct TransitionWithIdempotencyInput<'a> {
    /// Candidate identifier.
    pub id: &'a str,
    /// Candidate revision to transition.
    pub revision: i64,
    /// Current version required for compare-and-swap.
    pub expected_version: i64,
    /// New candidate status.
    pub status: &'a str,
    /// Optional error classification to persist with the transition.
    pub error_code: Option<&'a str>,
    /// Transition time in Unix milliseconds.
    pub now_ms: i64,
    /// Optional owner-scoped idempotency key and request digest.
    pub idempotency: Option<(&'a str, &'a str)>,
}

impl<'a> RefinementStore<'a> {
    /// Creates a store borrowing the caller-owned SQLite connection.
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// Inserts a candidate revision and its creation event in one transaction.
    ///
    /// Each serialized JSON component is limited to [`MAX_JSON_BYTES`].
    pub fn insert_candidate(
        &self,
        input: InsertCandidateInput<'_>,
    ) -> Result<(), RefinementStoreError> {
        let row = input.row;
        let content_json = input.content_json;
        let source_task_ids_json = input.source_task_ids_json;
        let evidence_json = input.evidence_json;
        let conflicts_json = input.conflicts_json;
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

    /// Loads a candidate summary by identifier and revision.
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

    /// Replays a previous admission for the same owner scope and request hash.
    ///
    /// Returns an idempotency conflict if the key exists with a different hash.
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

    /// Lists candidate summaries for an owner scope, newest update first, capped at 128 rows.
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

    /// Changes candidate status only if its current optimistic version matches.
    pub fn transition(
        &self,
        id: &str,
        revision: i64,
        expected_version: i64,
        status: &str,
        error_code: Option<&str>,
        now_ms: i64,
    ) -> Result<CandidateRow, RefinementStoreError> {
        self.transition_with_idempotency(TransitionWithIdempotencyInput {
            id,
            revision,
            expected_version,
            status,
            error_code,
            now_ms,
            idempotency: None,
        })
    }

    /// Applies a version-checked status transition and optionally records its idempotency key.
    pub fn transition_with_idempotency(
        &self,
        input: TransitionWithIdempotencyInput<'_>,
    ) -> Result<CandidateRow, RefinementStoreError> {
        let TransitionWithIdempotencyInput {
            id,
            revision,
            expected_version,
            status,
            error_code,
            now_ms,
            idempotency,
        } = input;
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
#[path = "refinement_store_tests.rs"]
mod tests;
