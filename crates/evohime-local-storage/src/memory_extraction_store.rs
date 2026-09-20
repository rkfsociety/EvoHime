//! Durable, metadata-only lifecycle for Core memory extraction candidates.
//!
//! The existing `memory_entries` table remains the only owner of memory
//! bodies. This table stores only bounded source/idempotency hashes and the
//! atomic publication outcome, so a retry cannot create a semantic copy for
//! the same source basis.

use crate::memory_store::{MemoryRecord, MemoryStoreError, MemoryStoreSql};
use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_BASIS_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    Committed { memory_id: String },
    AlreadyCommitted { memory_id: String },
    SourceBasisAlreadyCaptured { memory_id: Option<String> },
}

/// Installs the additive Plan 175 lifecycle table. No prompt, statement,
/// transcript, provider URL or secret is accepted by this schema.
pub fn install_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_extraction_lifecycle (
           idempotency_key TEXT PRIMARY KEY NOT NULL,
           source_basis TEXT NOT NULL UNIQUE,
           memory_id TEXT,
           state TEXT NOT NULL CHECK(state IN ('captured','finalizing','committed','deferred','failed','stale')),
           revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
           error_code TEXT,
           created_at_ms INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL,
           CHECK(length(idempotency_key) BETWEEN 1 AND 128),
           CHECK(length(source_basis) BETWEEN 1 AND 128),
           CHECK(error_code IS NULL OR length(error_code) BETWEEN 1 AND 128)
         );
         CREATE INDEX IF NOT EXISTS idx_memory_extraction_lifecycle_state
           ON memory_extraction_lifecycle(state, updated_at_ms, idempotency_key);",
    )
}

fn validate_basis(value: &str, field: &'static str) -> Result<(), MemoryStoreError> {
    if value.trim().is_empty() {
        return Err(MemoryStoreError::Empty { field });
    }
    if value.len() > MAX_BASIS_BYTES {
        return Err(MemoryStoreError::Limit {
            field,
            max: MAX_BASIS_BYTES,
        });
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.'))
    {
        return Err(MemoryStoreError::InvalidField(field));
    }
    Ok(())
}

/// Publishes one extracted candidate and its lifecycle in one SQLite
/// transaction. A repeated idempotency key is a read-only replay; a repeated
/// source basis under another key is reported without inserting a duplicate.
pub fn publish_candidate(
    connection: &Connection,
    record: &MemoryRecord,
    source_basis: &str,
    idempotency_key: &str,
    now_ms: i64,
) -> Result<PublishOutcome, MemoryStoreError> {
    record.validate()?;
    validate_basis(source_basis, "source_basis")?;
    validate_basis(idempotency_key, "idempotency_key")?;
    if now_ms < 0 {
        return Err(MemoryStoreError::InvalidField("updated_at_ms"));
    }

    let transaction = connection.unchecked_transaction()?;
    let by_key: Option<(String, Option<String>, String)> = transaction
        .query_row(
            "SELECT source_basis,memory_id,state
             FROM memory_extraction_lifecycle
             WHERE idempotency_key=?1",
            [idempotency_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((existing_basis, memory_id, state)) = by_key {
        if existing_basis != source_basis {
            return Err(MemoryStoreError::ExtractionIdempotencyConflict);
        }
        if state == "committed" {
            if let Some(memory_id) = memory_id {
                transaction.commit()?;
                return Ok(PublishOutcome::AlreadyCommitted { memory_id });
            }
            return Err(MemoryStoreError::InvalidTransition {
                from: state,
                to: "committed".to_owned(),
            });
        }
        return Err(MemoryStoreError::InvalidTransition {
            from: state,
            to: "committed".to_owned(),
        });
    }

    let by_basis: Option<Option<String>> = transaction
        .query_row(
            "SELECT memory_id FROM memory_extraction_lifecycle WHERE source_basis=?1",
            [source_basis],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(memory_id) = by_basis {
        transaction.commit()?;
        return Ok(PublishOutcome::SourceBasisAlreadyCaptured { memory_id });
    }

    transaction.execute(
        "INSERT INTO memory_extraction_lifecycle
           (idempotency_key,source_basis,state,created_at_ms,updated_at_ms)
         VALUES (?1,?2,'finalizing',?3,?3)",
        params![idempotency_key, source_basis, now_ms],
    )?;
    MemoryStoreSql::insert(&transaction, record)?;
    let updated = transaction.execute(
        "UPDATE memory_extraction_lifecycle
         SET memory_id=?2,state='committed',revision=revision+1,updated_at_ms=?3
         WHERE idempotency_key=?1 AND state='finalizing'",
        params![idempotency_key, record.id, now_ms],
    )?;
    if updated != 1 {
        return Err(MemoryStoreError::InvalidTransition {
            from: "finalizing".to_owned(),
            to: "committed".to_owned(),
        });
    }
    transaction.commit()?;
    Ok(PublishOutcome::Committed {
        memory_id: record.id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_store::{MemoryPrivacy, MemoryRecordInput, MemoryScope};

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE memory_entries (
                   id TEXT PRIMARY KEY NOT NULL, scope_kind TEXT NOT NULL,
                   scope_id TEXT NOT NULL, title TEXT NOT NULL, content TEXT NOT NULL,
                   provenance TEXT NOT NULL, privacy TEXT NOT NULL, created_at TEXT NOT NULL,
                   expires_at TEXT, archived INTEGER NOT NULL, forgotten INTEGER NOT NULL,
                   confirmations INTEGER NOT NULL DEFAULT 1, lesson_key TEXT,
                   kind TEXT NOT NULL DEFAULT 'entity', canonical_subject TEXT,
                   confirmation_state TEXT NOT NULL DEFAULT 'confirmed',
                   model_confidence REAL NOT NULL DEFAULT 1.0,
                   verification_confidence REAL NOT NULL DEFAULT 1.0,
                   privacy_class TEXT NOT NULL DEFAULT 'normal', source_trust TEXT NOT NULL DEFAULT 'user',
                   supersedes TEXT, superseded_by TEXT, supersession_reason TEXT,
                   extractor_version TEXT NOT NULL DEFAULT 'v1_legacy',
                   policy_version TEXT NOT NULL DEFAULT 'legacy-v1',
                   validation_status TEXT NOT NULL DEFAULT 'not_required', validated_at TEXT,
                   provenance_source_id TEXT, record_version INTEGER NOT NULL DEFAULT 1,
                   evidence_refs TEXT NOT NULL DEFAULT '[]', execution_event_refs TEXT NOT NULL DEFAULT '[]',
                   authority TEXT NOT NULL DEFAULT 'user_asserted', durability TEXT NOT NULL DEFAULT 'durable',
                   confidence REAL NOT NULL DEFAULT 1.0
                 );",
            )
            .expect("memory schema");
        install_schema(connection).expect("lifecycle schema");
    }

    fn record(id: &str) -> MemoryRecord {
        MemoryRecord::new(MemoryRecordInput {
            id: id.to_owned(),
            scope: MemoryScope::Project,
            scope_id: "scope".to_owned(),
            title: "subject".to_owned(),
            content: "bounded candidate".to_owned(),
            provenance: "{}".to_owned(),
            privacy: MemoryPrivacy::Private,
            created_at: "1".to_owned(),
            expires_at: None,
        })
        .expect("record")
    }

    #[test]
    fn publication_is_atomic_and_idempotent() {
        let connection = Connection::open_in_memory().expect("sqlite");
        schema(&connection);
        let first = publish_candidate(
            &connection,
            &record("memory-1"),
            "sha256:basis",
            "sha256:key",
            10,
        )
        .expect("first publish");
        assert_eq!(
            first,
            PublishOutcome::Committed {
                memory_id: "memory-1".to_owned()
            }
        );
        let replay = publish_candidate(
            &connection,
            &record("memory-2"),
            "sha256:basis",
            "sha256:key",
            11,
        )
        .expect("replay");
        assert_eq!(
            replay,
            PublishOutcome::AlreadyCommitted {
                memory_id: "memory-1".to_owned()
            }
        );
        let rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .expect("row count");
        assert_eq!(rows, 1);
    }

    #[test]
    fn source_basis_prevents_duplicate_under_new_idempotency_key() {
        let connection = Connection::open_in_memory().expect("sqlite");
        schema(&connection);
        publish_candidate(
            &connection,
            &record("memory-1"),
            "sha256:basis",
            "sha256:key-1",
            10,
        )
        .expect("first publish");
        let duplicate = publish_candidate(
            &connection,
            &record("memory-2"),
            "sha256:basis",
            "sha256:key-2",
            11,
        )
        .expect("duplicate source basis");
        assert_eq!(
            duplicate,
            PublishOutcome::SourceBasisAlreadyCaptured {
                memory_id: Some("memory-1".to_owned())
            }
        );
    }
}
