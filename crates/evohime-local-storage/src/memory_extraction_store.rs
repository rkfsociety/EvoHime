//! Durable, metadata-only lifecycle for Core memory extraction candidates.
//!
//! The existing `memory_entries` table remains the only owner of memory
//! bodies. This table stores only bounded source/idempotency hashes and the
//! atomic publication outcome, so a retry cannot create a semantic copy for
//! the same source basis.

use crate::memory_store::{MemoryRecord, MemoryStoreError, MemoryStoreSql};
use rusqlite::{params, Connection, OptionalExtension};

/// Maximum byte length accepted for a source basis or idempotency key.
pub const MAX_BASIS_BYTES: usize = 128;

/// Outcome of publishing a memory extraction candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    /// A new memory record and lifecycle record were committed atomically.
    Committed {
        /// Identifier of the newly committed memory entry.
        memory_id: String,
    },
    /// The idempotency key already names the committed memory record.
    AlreadyCommitted {
        /// Identifier of the memory entry committed by the prior attempt.
        memory_id: String,
    },
    /// Another idempotency key already captured this source basis.
    SourceBasisAlreadyCaptured {
        /// Identifier of the entry that captured the source basis, if retained.
        memory_id: Option<String>,
    },
}

/// Installs the additive lifecycle table for extraction publication.
/// No prompt, statement, transcript, provider URL, or secret is stored here.
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
#[path = "memory_extraction_store_tests.rs"]
mod tests;
