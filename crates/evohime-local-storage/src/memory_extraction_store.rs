//! Durable, metadata-only lifecycle for Core memory extraction candidates.
//!
//! The existing `memory_entries` table remains the only owner of memory
//! bodies. This table stores only bounded source/idempotency hashes and the
//! atomic publication outcome, so a retry cannot create a semantic copy for
//! the same source basis.

use crate::memory_store::{MemoryRecord, MemoryStoreError, MemoryStoreSql};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Maximum byte length accepted for a source basis or idempotency key.
pub const MAX_BASIS_BYTES: usize = 128;
/// Maximum accepted ancestry depth for an extraction source.
pub const MAX_EXTRACTION_DEPTH: u32 = 8;

/// Core-owned classification for a durable memory-extraction source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryExtractionOrigin {
    /// An explicit user dialog turn.
    Dialog,
    /// A retained ambient episode.
    Ambient,
    /// A delegated child execution, suppressed by default.
    Delegated,
    /// A background execution, suppressed by default.
    Background,
    /// A recovery-only source classification.
    Recovery,
    /// A migration-era source without supported execution metadata.
    Legacy,
}

impl MemoryExtractionOrigin {
    /// Returns the stable persisted origin name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dialog => "dialog",
            Self::Ambient => "ambient",
            Self::Delegated => "delegated",
            Self::Background => "background",
            Self::Recovery => "recovery",
            Self::Legacy => "legacy",
        }
    }

    /// Parses a persisted origin, rejecting values unknown to this version.
    pub fn parse(value: &str) -> Result<Self, MemoryStoreError> {
        match value {
            "dialog" => Ok(Self::Dialog),
            "ambient" => Ok(Self::Ambient),
            "delegated" => Ok(Self::Delegated),
            "background" => Ok(Self::Background),
            "recovery" => Ok(Self::Recovery),
            "legacy" => Ok(Self::Legacy),
            _ => Err(MemoryStoreError::InvalidField("origin")),
        }
    }
}

struct ExistingCandidateLifecycle {
    source_basis: String,
    memory_id: Option<String>,
    state: String,
    expected_head_id: Option<String>,
    error_code: Option<String>,
}

struct ExistingCandidateSourceBasis {
    memory_id: Option<String>,
    state: String,
    expected_head_id: Option<String>,
    error_code: Option<String>,
}

struct CandidateLifecycleForFinalize {
    source_basis: String,
    memory_id: Option<String>,
    state: String,
    source_id: Option<String>,
    candidate_slot: Option<String>,
    source_order: i64,
    expected_head_id: Option<String>,
    expected_head_revision: i64,
    error_code: Option<String>,
}

struct ExistingMemoryExtractionSource {
    source_id: String,
    source_ref_id: String,
    source_revision_hash: String,
    scope_id: String,
    origin: String,
    root_execution_id: String,
    depth: u32,
    _source_order: i64,
    primary_request_id: Option<String>,
    primary_response_id: Option<String>,
    state: String,
}

/// Core-provided identity and non-content reference for one extraction source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureSourceInput {
    /// Stable opaque source identifier.
    pub source_id: String,
    /// Opaque digest for the immutable source identity and revision.
    pub source_basis: String,
    /// Core task or ambient episode reference, without source text.
    pub source_ref_id: String,
    /// Opaque digest of the source snapshot.
    pub source_revision_hash: String,
    /// Opaque memory scope key required to resume governance after restart.
    pub scope_id: String,
    /// Core-selected typed origin.
    pub origin: MemoryExtractionOrigin,
    /// Root execution reference established by Core.
    pub root_execution_id: String,
    /// Bounded ancestry depth.
    pub depth: u32,
    /// Monotonic Core source order used to reject stale finalizers.
    pub source_order: i64,
    /// Primary model request that produced the source, when applicable.
    pub primary_request_id: Option<String>,
    /// Primary model response that produced the source, when applicable.
    pub primary_response_id: Option<String>,
    /// Capture timestamp in Unix milliseconds.
    pub now_ms: i64,
}

/// Stable state of one captured source ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryExtractionSourceState {
    /// Source was durably captured and has not been leased for finalization.
    Captured,
    /// One fenced generation currently owns finalization.
    Finalizing,
    /// All candidate effects for this source reached a durable outcome.
    Committed,
    /// Work remains inspectable and may be retried by a later generation.
    Deferred,
    /// The source reached a bounded failure outcome.
    Failed,
    /// The source became stale before publication.
    Stale,
}

impl MemoryExtractionSourceState {
    /// Returns the stable persisted state name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::Finalizing => "finalizing",
            Self::Committed => "committed",
            Self::Deferred => "deferred",
            Self::Failed => "failed",
            Self::Stale => "stale",
        }
    }

    /// Parses a persisted source state, rejecting unknown values.
    pub fn parse(value: &str) -> Result<Self, MemoryStoreError> {
        match value {
            "captured" => Ok(Self::Captured),
            "finalizing" => Ok(Self::Finalizing),
            "committed" => Ok(Self::Committed),
            "deferred" => Ok(Self::Deferred),
            "failed" => Ok(Self::Failed),
            "stale" => Ok(Self::Stale),
            _ => Err(MemoryStoreError::InvalidField("memory_extraction_state")),
        }
    }
}

/// Result of idempotently capturing one source reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSourceOutcome {
    /// A new source reference was committed.
    Captured {
        /// Stable source identifier.
        source_id: String,
    },
    /// The immutable source basis was already recorded.
    AlreadyCaptured {
        /// Identifier retained by the original capture.
        source_id: String,
        /// Current durable state of the original capture.
        state: MemoryExtractionSourceState,
    },
}

/// Metadata-only source row returned to Core recovery and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryExtractionSourceRecord {
    /// Stable source identifier.
    pub source_id: String,
    /// Opaque immutable source basis.
    pub source_basis: String,
    /// Task or episode reference, without source content.
    pub source_ref_id: String,
    /// Opaque source revision digest.
    pub source_revision_hash: String,
    /// Opaque memory scope key used by the source's governance policy.
    pub scope_id: String,
    /// Core-selected typed source origin.
    pub origin: MemoryExtractionOrigin,
    /// Root execution identifier.
    pub root_execution_id: String,
    /// Source ancestry depth.
    pub depth: u32,
    /// Monotonic Core source ordering value.
    pub source_order: i64,
    /// Primary request identifier when this came from a model turn.
    pub primary_request_id: Option<String>,
    /// Primary response identifier when this came from a model turn.
    pub primary_response_id: Option<String>,
    /// Logical extractor request identifier.
    pub extractor_logical_request_id: Option<String>,
    /// Current extractor request identifier.
    pub extractor_request_id: Option<String>,
    /// Extractor response identifier after durable response capture.
    pub extractor_response_id: Option<String>,
    /// Durable lifecycle state.
    pub state: MemoryExtractionSourceState,
    /// Monotonic state revision.
    pub revision: i64,
    /// Current fencing generation.
    pub lease_generation: i64,
    /// Current lease expiry, when leased.
    pub lease_expires_at_ms: Option<i64>,
    /// Bounded failure code, if present.
    pub error_code: Option<String>,
    /// Source capture time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Last state update time in Unix milliseconds.
    pub updated_at_ms: i64,
}

/// Outcome of acquiring a source-scoped finalization lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLeaseOutcome {
    /// A new fencing generation owns the source.
    Acquired {
        /// Monotonic fencing generation.
        generation: i64,
    },
    /// Another unexpired generation already owns the source.
    Busy {
        /// Expiration of the current owner lease.
        expires_at_ms: i64,
    },
    /// The source already has a terminal durable outcome.
    Terminal {
        /// Existing terminal state.
        state: MemoryExtractionSourceState,
    },
}

/// Maximum number of recoverable source rows returned by one query.
pub const MAX_RECOVERABLE_SOURCES: usize = 100;

/// Outcome of publishing a memory extraction candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
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
    /// Candidate source basis became stale before publication.
    StaleBasis,
    /// A newer committed head changed the expected revision.
    RevisionConflict {
        /// Current head revision observed by the transaction.
        current_revision: i64,
    },
    /// A later source revision already published this candidate slot.
    SupersededByNewer {
        /// Memory record belonging to the newer source.
        memory_id: String,
    },
    /// Governance rejected the candidate before publication.
    RejectedByPolicy {
        /// Bounded reason code selected by Core governance.
        reason_code: String,
    },
}

/// Input needed to durably capture one governed, non-active memory candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureCandidateInput<'a> {
    /// Source ingestion that owns this candidate.
    pub source_id: &'a str,
    /// Candidate memory record; its body is stored only in `memory_entries`.
    pub record: &'a MemoryRecord,
    /// Opaque per-candidate source basis.
    pub source_basis: &'a str,
    /// Idempotency key derived from the source basis and stable slot.
    pub idempotency_key: &'a str,
    /// Opaque stable candidate slot used for source freshness comparison.
    pub candidate_slot: &'a str,
    /// Fencing generation currently owning source finalization.
    pub generation: i64,
    /// Capture timestamp in Unix milliseconds.
    pub now_ms: i64,
}

/// Result of durably capturing one candidate memory body and its metadata row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureCandidateOutcome {
    /// Candidate body and lifecycle were committed atomically.
    Captured {
        /// Stable memory entry identifier.
        memory_id: String,
    },
    /// The candidate key already refers to a durable lifecycle row.
    AlreadyCaptured {
        /// Identifier retained by the original capture, if it captured a body.
        /// Stale lifecycle rows intentionally have no memory record.
        memory_id: Option<String>,
        /// Current durable candidate state.
        state: String,
    },
    /// A newer source revision already published this candidate slot.
    SupersededByNewer {
        /// Memory record retained by the newer source.
        memory_id: String,
    },
}

/// Input for committing a captured candidate under its source fencing token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalizeCandidateInput<'a> {
    /// Source whose current generation owns finalization.
    pub source_id: &'a str,
    /// Stable candidate idempotency key.
    pub idempotency_key: &'a str,
    /// Fencing generation returned by [`acquire_source_lease`].
    pub generation: i64,
    /// Core-governed state to apply if no user action changed the candidate.
    pub target_confirmation_state: &'a str,
    /// Finalization timestamp in Unix milliseconds.
    pub now_ms: i64,
}

fn digest_parts(domain: &[u8], parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sha256:{}", hex::encode(digest.finalize()))
}

/// Derives a stable candidate slot from Core's canonical subject and scope.
/// Generated statement text, evidence wording and confidence are not part of
/// the identity, so a retry cannot evade freshness checks by rewriting prose.
pub fn candidate_slot_for(record: &MemoryRecord) -> Result<String, MemoryStoreError> {
    record.validate()?;
    let subject =
        record
            .extraction
            .canonical_subject
            .as_deref()
            .ok_or(MemoryStoreError::Empty {
                field: "canonical_subject",
            })?;
    validate_reference(subject, "canonical_subject", 512)?;
    validate_reference(&record.extraction.kind, "kind", 64)?;
    validate_reference(&record.scope_id, "scope_id", 512)?;
    Ok(digest_parts(
        b"evohime-memory-candidate-slot:v1\0",
        &[
            record.scope.as_str(),
            &record.scope_id,
            &record.extraction.kind,
            subject,
        ],
    ))
}

/// Derives per-slot source and replay identities from the immutable source
/// basis. Both identities remain stable when the model rewrites candidate text.
pub fn candidate_basis_for(
    source_basis: &str,
    candidate_slot: &str,
) -> Result<(String, String), MemoryStoreError> {
    validate_basis(source_basis, "source_basis")?;
    validate_basis(candidate_slot, "candidate_slot")?;
    let candidate_basis = digest_parts(
        b"evohime-memory-candidate-basis:v1\0",
        &[source_basis, candidate_slot],
    );
    let idempotency_key = digest_parts(
        b"evohime-memory-candidate-idempotency:v1\0",
        &[&candidate_basis],
    );
    Ok((candidate_basis, idempotency_key))
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

/// Installs the additive schema-v176 source and freshness indexes.
///
/// The historical v175 installer above intentionally remains unchanged. The
/// v176 migration adds its columns before calling this idempotent installer.
pub(crate) fn install_schema_v176(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_extraction_sources (
           source_id TEXT PRIMARY KEY NOT NULL,
           source_basis TEXT NOT NULL UNIQUE,
           source_ref_id TEXT NOT NULL,
           source_revision_hash TEXT NOT NULL,
           scope_id TEXT NOT NULL,
           origin TEXT NOT NULL CHECK(origin IN ('dialog','ambient','delegated','background','recovery','legacy')),
           root_execution_id TEXT NOT NULL,
           depth INTEGER NOT NULL CHECK(depth BETWEEN 0 AND 8),
           source_order INTEGER NOT NULL CHECK(source_order >= 0),
           primary_request_id TEXT,
           primary_response_id TEXT,
           extractor_logical_request_id TEXT,
           extractor_request_id TEXT,
           extractor_response_id TEXT,
           state TEXT NOT NULL CHECK(state IN ('captured','finalizing','committed','deferred','failed','stale')),
           revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
           lease_generation INTEGER NOT NULL DEFAULT 0 CHECK(lease_generation >= 0),
           lease_owner TEXT,
           lease_expires_at_ms INTEGER,
           error_code TEXT,
           created_at_ms INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL,
           CHECK(length(source_id) BETWEEN 1 AND 128),
           CHECK(length(source_basis) BETWEEN 1 AND 128),
           CHECK(length(source_ref_id) BETWEEN 1 AND 256),
           CHECK(length(source_revision_hash) BETWEEN 1 AND 128),
           CHECK(length(scope_id) BETWEEN 1 AND 512),
           CHECK(length(root_execution_id) BETWEEN 1 AND 128),
           CHECK(primary_request_id IS NULL OR length(primary_request_id) BETWEEN 1 AND 128),
           CHECK(primary_response_id IS NULL OR length(primary_response_id) BETWEEN 1 AND 128),
           CHECK(extractor_logical_request_id IS NULL OR length(extractor_logical_request_id) BETWEEN 1 AND 128),
           CHECK(extractor_request_id IS NULL OR length(extractor_request_id) BETWEEN 1 AND 128),
           CHECK(extractor_response_id IS NULL OR length(extractor_response_id) BETWEEN 1 AND 128),
           CHECK(lease_owner IS NULL OR length(lease_owner) BETWEEN 1 AND 128),
           CHECK(lease_expires_at_ms IS NULL OR lease_expires_at_ms >= 0),
           CHECK(error_code IS NULL OR length(error_code) BETWEEN 1 AND 128),
           CHECK((lease_owner IS NULL AND lease_expires_at_ms IS NULL) OR
                 (lease_owner IS NOT NULL AND lease_expires_at_ms IS NOT NULL))
         );
         CREATE INDEX IF NOT EXISTS idx_memory_extraction_sources_recovery
           ON memory_extraction_sources(state, lease_expires_at_ms, source_order, source_id);
         CREATE TABLE IF NOT EXISTS memory_extraction_heads (
           candidate_slot TEXT PRIMARY KEY NOT NULL,
           memory_id TEXT NOT NULL,
           revision INTEGER NOT NULL CHECK(revision > 0),
           source_order INTEGER NOT NULL CHECK(source_order >= 0),
           source_basis TEXT NOT NULL,
           updated_at_ms INTEGER NOT NULL,
           CHECK(length(candidate_slot) BETWEEN 1 AND 128),
           CHECK(length(memory_id) BETWEEN 1 AND 256),
           CHECK(length(source_basis) BETWEEN 1 AND 128)
         );
         CREATE INDEX IF NOT EXISTS idx_memory_extraction_lifecycle_source
           ON memory_extraction_lifecycle(source_id, state, updated_at_ms, idempotency_key);
         CREATE INDEX IF NOT EXISTS idx_memory_extraction_lifecycle_slot
           ON memory_extraction_lifecycle(candidate_slot, state, source_order, idempotency_key);
         CREATE INDEX IF NOT EXISTS idx_memory_extraction_heads_memory
           ON memory_extraction_heads(memory_id);",
    )
}

/// Installs both the frozen v175 table and the additive v176 lifecycle tables.
pub(crate) fn install_current_schema(connection: &Connection) -> Result<(), rusqlite::Error> {
    install_schema(connection)?;
    install_schema_v176(connection)
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

fn validate_reference(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), MemoryStoreError> {
    if value.trim().is_empty() {
        return Err(MemoryStoreError::Empty { field });
    }
    if value.len() > max_bytes {
        return Err(MemoryStoreError::Limit {
            field,
            max: max_bytes,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(MemoryStoreError::InvalidField(field));
    }
    Ok(())
}

/// Atomically stores a non-active candidate body and its source lifecycle row.
pub fn capture_candidate(
    connection: &Connection,
    input: &CaptureCandidateInput<'_>,
) -> Result<CaptureCandidateOutcome, MemoryStoreError> {
    input.record.validate()?;
    validate_reference(input.source_id, "source_id", MAX_BASIS_BYTES)?;
    validate_basis(input.source_basis, "source_basis")?;
    validate_basis(input.idempotency_key, "idempotency_key")?;
    validate_basis(input.candidate_slot, "candidate_slot")?;
    if input.generation <= 0 || input.now_ms < 0 {
        return Err(MemoryStoreError::InvalidField("lease_generation"));
    }
    if !matches!(
        input.record.extraction.confirmation_state.as_str(),
        "candidate" | "pending_confirmation"
    ) || input.record.extraction.authority != "model_proposed"
    {
        return Err(MemoryStoreError::InvalidField("candidate_authority"));
    }

    let transaction = connection.unchecked_transaction()?;
    let source: Option<(i64, String, i64, Option<i64>, String)> = transaction
        .query_row(
            "SELECT source_order,state,lease_generation,lease_expires_at_ms,source_basis
             FROM memory_extraction_sources WHERE source_id=?1",
            [input.source_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((source_order, source_state, generation, lease_expires_at_ms, parent_basis)) = source
    else {
        return Err(MemoryStoreError::NotFound);
    };
    if source_state != "finalizing"
        || generation != input.generation
        || lease_expires_at_ms.is_none_or(|expires| expires < input.now_ms)
    {
        return Err(MemoryStoreError::InvalidTransition {
            from: source_state,
            to: "captured".to_owned(),
        });
    }
    let expected_slot = candidate_slot_for(input.record)?;
    if input.candidate_slot != expected_slot {
        return Err(MemoryStoreError::InvalidField("candidate_slot"));
    }
    let (expected_basis, expected_idempotency_key) =
        candidate_basis_for(&parent_basis, &expected_slot)?;
    if input.source_basis != expected_basis || input.idempotency_key != expected_idempotency_key {
        return Err(MemoryStoreError::InvalidField("candidate_source_basis"));
    }

    let existing: Option<ExistingCandidateLifecycle> = transaction
        .query_row(
            "SELECT source_basis,memory_id,state,expected_head_id,error_code
             FROM memory_extraction_lifecycle
             WHERE idempotency_key=?1",
            [input.idempotency_key],
            |row| {
                Ok(ExistingCandidateLifecycle {
                    source_basis: row.get(0)?,
                    memory_id: row.get(1)?,
                    state: row.get(2)?,
                    expected_head_id: row.get(3)?,
                    error_code: row.get(4)?,
                })
            },
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing.source_basis != input.source_basis {
            return Err(MemoryStoreError::ExtractionIdempotencyConflict);
        }
        if existing.state == "stale"
            && existing.error_code.as_deref() == Some("superseded_by_newer")
        {
            let Some(memory_id) = existing.expected_head_id else {
                return Err(MemoryStoreError::InvalidTransition {
                    from: "stale_without_head".to_owned(),
                    to: "superseded".to_owned(),
                });
            };
            transaction.commit()?;
            return Ok(CaptureCandidateOutcome::SupersededByNewer { memory_id });
        }
        if let Some(memory_id) = existing.memory_id {
            transaction.commit()?;
            return Ok(CaptureCandidateOutcome::AlreadyCaptured {
                memory_id: Some(memory_id),
                state: existing.state,
            });
        }
        transaction.commit()?;
        return Ok(CaptureCandidateOutcome::AlreadyCaptured {
            memory_id: None,
            state: existing.state,
        });
    }

    let existing_basis: Option<ExistingCandidateSourceBasis> = transaction
        .query_row(
            "SELECT memory_id,state,expected_head_id,error_code
             FROM memory_extraction_lifecycle WHERE source_basis=?1",
            [input.source_basis],
            |row| {
                Ok(ExistingCandidateSourceBasis {
                    memory_id: row.get(0)?,
                    state: row.get(1)?,
                    expected_head_id: row.get(2)?,
                    error_code: row.get(3)?,
                })
            },
        )
        .optional()?;
    if let Some(existing_basis) = existing_basis {
        transaction.commit()?;
        if existing_basis.state == "stale"
            && existing_basis.error_code.as_deref() == Some("superseded_by_newer")
        {
            if let Some(memory_id) = existing_basis.expected_head_id {
                return Ok(CaptureCandidateOutcome::SupersededByNewer { memory_id });
            }
            return Err(MemoryStoreError::InvalidTransition {
                from: "stale_without_head".to_owned(),
                to: "superseded".to_owned(),
            });
        }
        return Ok(CaptureCandidateOutcome::AlreadyCaptured {
            memory_id: existing_basis.memory_id,
            state: existing_basis.state,
        });
    }

    let head: Option<(String, i64, i64)> = transaction
        .query_row(
            "SELECT memory_id,revision,source_order FROM memory_extraction_heads
             WHERE candidate_slot=?1",
            [input.candidate_slot],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((memory_id, _, latest_source_order)) = head.as_ref() {
        if *latest_source_order > source_order {
            transaction.execute(
                "INSERT INTO memory_extraction_lifecycle
                   (idempotency_key,source_basis,state,revision,error_code,created_at_ms,updated_at_ms,
                    source_id,candidate_slot,source_order,expected_head_id,expected_head_revision)
                 VALUES (?1,?2,'stale',1,'superseded_by_newer',?3,?3,?4,?5,?6,?7,?8)",
                params![
                    input.idempotency_key,
                    input.source_basis,
                    input.now_ms,
                    input.source_id,
                    input.candidate_slot,
                    source_order,
                    memory_id,
                    head.as_ref().map(|(_, revision, _)| *revision).unwrap_or(0),
                ],
            )?;
            transaction.commit()?;
            return Ok(CaptureCandidateOutcome::SupersededByNewer {
                memory_id: memory_id.clone(),
            });
        }
    }
    let (expected_head_id, expected_head_revision) = head
        .as_ref()
        .map(|(memory_id, revision, _)| (Some(memory_id.as_str()), *revision))
        .unwrap_or((None, 0));
    transaction.execute(
        "INSERT INTO memory_extraction_lifecycle
           (idempotency_key,source_basis,memory_id,state,revision,created_at_ms,updated_at_ms,
            source_id,candidate_slot,source_order,expected_head_id,expected_head_revision)
         VALUES (?1,?2,?3,'captured',1,?4,?4,?5,?6,?7,?8,?9)",
        params![
            input.idempotency_key,
            input.source_basis,
            input.record.id,
            input.now_ms,
            input.source_id,
            input.candidate_slot,
            source_order,
            expected_head_id,
            expected_head_revision,
        ],
    )?;
    MemoryStoreSql::insert(&transaction, input.record)?;
    transaction.commit()?;
    Ok(CaptureCandidateOutcome::Captured {
        memory_id: input.record.id.clone(),
    })
}

/// Commits one already captured candidate and advances its slot head with CAS.
/// A strictly newer source may refresh an older precondition once in this
/// transaction; an older or equal source cannot overwrite the current head.
pub fn finalize_candidate(
    connection: &Connection,
    input: FinalizeCandidateInput<'_>,
) -> Result<PublishOutcome, MemoryStoreError> {
    validate_reference(input.source_id, "source_id", MAX_BASIS_BYTES)?;
    validate_basis(input.idempotency_key, "idempotency_key")?;
    if !matches!(
        input.target_confirmation_state,
        "candidate" | "pending_confirmation" | "confirmed"
    ) {
        return Err(MemoryStoreError::InvalidField("confirmation_state"));
    }
    if input.generation <= 0 || input.now_ms < 0 {
        return Err(MemoryStoreError::InvalidField("lease_generation"));
    }

    let transaction = connection.unchecked_transaction()?;
    let source: Option<(String, String, i64, i64, Option<i64>)> = transaction
        .query_row(
            "SELECT source_basis,state,source_order,lease_generation,lease_expires_at_ms
             FROM memory_extraction_sources WHERE source_id=?1",
            [input.source_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((parent_basis, source_state, source_order, generation, lease_expires_at_ms)) = source
    else {
        return Err(MemoryStoreError::NotFound);
    };
    let lifecycle: Option<CandidateLifecycleForFinalize> = transaction
        .query_row(
            "SELECT source_basis,memory_id,state,source_id,candidate_slot,source_order,
                    expected_head_id,expected_head_revision,error_code
             FROM memory_extraction_lifecycle WHERE idempotency_key=?1",
            [input.idempotency_key],
            |row| {
                Ok(CandidateLifecycleForFinalize {
                    source_basis: row.get(0)?,
                    memory_id: row.get(1)?,
                    state: row.get(2)?,
                    source_id: row.get(3)?,
                    candidate_slot: row.get(4)?,
                    source_order: row.get(5)?,
                    expected_head_id: row.get(6)?,
                    expected_head_revision: row.get(7)?,
                    error_code: row.get(8)?,
                })
            },
        )
        .optional()?;
    let Some(lifecycle) = lifecycle else {
        return Err(MemoryStoreError::NotFound);
    };
    if lifecycle.source_id.as_deref() != Some(input.source_id)
        || lifecycle.source_order != source_order
    {
        return Err(MemoryStoreError::ExtractionIdempotencyConflict);
    }
    let candidate_slot = lifecycle
        .candidate_slot
        .ok_or(MemoryStoreError::InvalidField("candidate_slot"))?;
    let (expected_basis, expected_key) = candidate_basis_for(&parent_basis, &candidate_slot)?;
    if lifecycle.source_basis != expected_basis || input.idempotency_key != expected_key {
        return Err(MemoryStoreError::ExtractionIdempotencyConflict);
    }
    if lifecycle.state == "committed" {
        let memory_id = lifecycle
            .memory_id
            .ok_or_else(|| MemoryStoreError::InvalidTransition {
                from: lifecycle.state,
                to: "committed".to_owned(),
            })?;
        transaction.commit()?;
        return Ok(PublishOutcome::AlreadyCommitted { memory_id });
    }
    if lifecycle.state == "stale" {
        transaction.commit()?;
        if lifecycle.error_code.as_deref() == Some("superseded_by_newer") {
            if let Some(memory_id) = lifecycle.expected_head_id {
                return Ok(PublishOutcome::SupersededByNewer { memory_id });
            }
        }
        return Ok(PublishOutcome::StaleBasis);
    }
    if lifecycle.state == "failed" {
        transaction.commit()?;
        if lifecycle.error_code.as_deref() == Some("revision_conflict") {
            let current_revision: i64 = connection
                .query_row(
                    "SELECT revision FROM memory_extraction_heads WHERE candidate_slot=?1",
                    [&candidate_slot],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(0);
            return Ok(PublishOutcome::RevisionConflict { current_revision });
        }
        return Ok(PublishOutcome::RejectedByPolicy {
            reason_code: lifecycle
                .error_code
                .unwrap_or_else(|| "candidate_failed".to_owned()),
        });
    }
    if source_state != "finalizing"
        || generation != input.generation
        || lease_expires_at_ms.is_none_or(|expires| expires < input.now_ms)
    {
        transaction.commit()?;
        return Ok(PublishOutcome::StaleBasis);
    }
    if lifecycle.state != "captured" {
        return Err(MemoryStoreError::InvalidTransition {
            from: lifecycle.state,
            to: "committed".to_owned(),
        });
    }
    let memory_id = lifecycle
        .memory_id
        .ok_or_else(|| MemoryStoreError::InvalidTransition {
            from: "captured_without_memory".to_owned(),
            to: "committed".to_owned(),
        })?;

    let current_head: Option<(String, i64, i64)> = transaction
        .query_row(
            "SELECT memory_id,revision,source_order FROM memory_extraction_heads
             WHERE candidate_slot=?1",
            [&candidate_slot],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let expected_matches = match current_head.as_ref() {
        None => lifecycle.expected_head_id.is_none() && lifecycle.expected_head_revision == 0,
        Some((current_id, current_revision, _)) => {
            lifecycle.expected_head_id.as_deref() == Some(current_id.as_str())
                && lifecycle.expected_head_revision == *current_revision
        }
    };
    if !expected_matches {
        if let Some((newer_id, newer_revision, newer_order)) = current_head.as_ref() {
            if *newer_order > source_order {
                transaction.execute(
                    "UPDATE memory_extraction_lifecycle
                     SET state='stale',revision=revision+1,error_code='superseded_by_newer',
                         expected_head_id=?2,expected_head_revision=?3,updated_at_ms=?4
                     WHERE idempotency_key=?1 AND state='captured'",
                    params![
                        input.idempotency_key,
                        newer_id,
                        newer_revision,
                        input.now_ms
                    ],
                )?;
                transaction.commit()?;
                return Ok(PublishOutcome::SupersededByNewer {
                    memory_id: newer_id.clone(),
                });
            }
            if *newer_order == source_order {
                transaction.execute(
                    "UPDATE memory_extraction_lifecycle
                     SET state='failed',error_code='revision_conflict',revision=revision+1,updated_at_ms=?2
                     WHERE idempotency_key=?1 AND state='captured'",
                    params![input.idempotency_key, input.now_ms],
                )?;
                transaction.commit()?;
                return Ok(PublishOutcome::RevisionConflict {
                    current_revision: current_head
                        .as_ref()
                        .map(|(_, revision, _)| *revision)
                        .unwrap_or(0),
                });
            }
            // A strictly newer source may safely refresh its expected head
            // once; the current transaction retains the serialized snapshot.
            transaction.execute(
                "UPDATE memory_extraction_lifecycle
                 SET expected_head_id=?2,expected_head_revision=?3,
                     revision=revision+1,updated_at_ms=?4
                 WHERE idempotency_key=?1 AND state='captured'",
                params![
                    input.idempotency_key,
                    newer_id,
                    current_head
                        .as_ref()
                        .map(|(_, revision, _)| *revision)
                        .unwrap_or(0),
                    input.now_ms
                ],
            )?;
        } else {
            transaction.execute(
                "UPDATE memory_extraction_lifecycle
                 SET state='failed',error_code='revision_conflict',revision=revision+1,updated_at_ms=?2
                 WHERE idempotency_key=?1 AND state='captured'",
                params![input.idempotency_key, input.now_ms],
            )?;
            transaction.commit()?;
            return Ok(PublishOutcome::RevisionConflict {
                current_revision: 0,
            });
        }
    }

    let candidate_state: Option<(String, i64)> = transaction
        .query_row(
            "SELECT confirmation_state,forgotten FROM memory_entries WHERE id=?1",
            [&memory_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((candidate_state, forgotten)) = candidate_state else {
        return Err(MemoryStoreError::NotFound);
    };
    if forgotten != 0
        || matches!(
            candidate_state.as_str(),
            "rejected" | "forgotten" | "expired" | "superseded"
        )
    {
        let reason_code = if candidate_state == "rejected" {
            "candidate_rejected"
        } else if forgotten != 0 || candidate_state == "forgotten" {
            "candidate_forgotten"
        } else if candidate_state == "expired" {
            "candidate_expired"
        } else {
            "candidate_superseded"
        };
        transaction.execute(
            "UPDATE memory_extraction_lifecycle
             SET state='failed',revision=revision+1,error_code=?2,updated_at_ms=?3
             WHERE idempotency_key=?1 AND state='captured'",
            params![input.idempotency_key, reason_code, input.now_ms],
        )?;
        transaction.commit()?;
        return Ok(PublishOutcome::RejectedByPolicy {
            reason_code: reason_code.to_owned(),
        });
    }
    if !matches!(
        candidate_state.as_str(),
        "candidate" | "pending_confirmation" | "confirmed"
    ) {
        return Err(MemoryStoreError::InvalidField("candidate_state"));
    }
    if candidate_state == "candidate" {
        transaction.execute(
            "UPDATE memory_entries SET confirmation_state=?2 WHERE id=?1 AND confirmation_state='candidate'",
            params![memory_id, input.target_confirmation_state],
        )?;
    }

    let next_revision = current_head
        .as_ref()
        .map(|(_, revision, _)| revision.saturating_add(1))
        .unwrap_or(1);
    let head_changed = if let Some((old_id, old_revision, _)) = current_head.as_ref() {
        transaction.execute(
            "UPDATE memory_extraction_heads
             SET memory_id=?2,revision=?3,source_order=?4,source_basis=?5,updated_at_ms=?6
             WHERE candidate_slot=?1 AND memory_id=?7 AND revision=?8",
            params![
                candidate_slot,
                memory_id,
                next_revision,
                source_order,
                lifecycle.source_basis,
                input.now_ms,
                old_id,
                old_revision,
            ],
        )?
    } else {
        transaction.execute(
            "INSERT INTO memory_extraction_heads
             (candidate_slot,memory_id,revision,source_order,source_basis,updated_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                candidate_slot,
                memory_id,
                next_revision,
                source_order,
                lifecycle.source_basis,
                input.now_ms
            ],
        )?
    };
    if head_changed != 1 {
        return Err(MemoryStoreError::InvalidTransition {
            from: "head_changed".to_owned(),
            to: "committed".to_owned(),
        });
    }
    let committed = transaction.execute(
        "UPDATE memory_extraction_lifecycle
         SET state='committed',revision=revision+1,error_code=NULL,updated_at_ms=?2
         WHERE idempotency_key=?1 AND state='captured'",
        params![input.idempotency_key, input.now_ms],
    )?;
    if committed != 1 {
        return Err(MemoryStoreError::InvalidTransition {
            from: "captured".to_owned(),
            to: "committed".to_owned(),
        });
    }
    transaction.commit()?;
    Ok(PublishOutcome::Committed { memory_id })
}

/// Invalidates an extraction head precondition when a user memory action
/// changes the referenced record. Older test schemas without v176 stay valid.
pub(crate) fn bump_candidate_head_revision(
    connection: &Connection,
    memory_id: &str,
) -> Result<(), MemoryStoreError> {
    let has_heads: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='memory_extraction_heads')",
        [],
        |row| row.get(0),
    )?;
    if has_heads {
        connection.execute(
            "UPDATE memory_extraction_heads SET revision=revision+1
             WHERE memory_id=?1 AND revision < 9223372036854775807",
            [memory_id],
        )?;
    }
    Ok(())
}

/// Durably records a non-content reference to a completed primary source.
///
/// Replaying the same basis returns the original source id and state. Reusing
/// one basis for different source metadata fails closed.
pub fn capture_source(
    connection: &Connection,
    input: &CaptureSourceInput,
) -> Result<CaptureSourceOutcome, MemoryStoreError> {
    validate_reference(&input.source_id, "source_id", MAX_BASIS_BYTES)?;
    validate_basis(&input.source_basis, "source_basis")?;
    validate_reference(&input.source_ref_id, "source_ref_id", 256)?;
    validate_basis(&input.source_revision_hash, "source_revision_hash")?;
    validate_reference(&input.scope_id, "scope_id", 512)?;
    validate_reference(
        &input.root_execution_id,
        "root_execution_id",
        MAX_BASIS_BYTES,
    )?;
    if input.origin == MemoryExtractionOrigin::Legacy {
        return Err(MemoryStoreError::InvalidField("origin"));
    }
    if input.depth > MAX_EXTRACTION_DEPTH || input.source_order < 0 || input.now_ms < 0 {
        return Err(MemoryStoreError::InvalidField("source_revision"));
    }
    if input.primary_request_id.is_some() != input.primary_response_id.is_some() {
        return Err(MemoryStoreError::InvalidField("primary_model_reference"));
    }
    for value in [
        input.primary_request_id.as_deref(),
        input.primary_response_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_reference(value, "primary_model_reference", MAX_BASIS_BYTES)?;
    }

    let transaction = connection.unchecked_transaction()?;
    let existing: Option<ExistingMemoryExtractionSource> = transaction
        .query_row(
            "SELECT source_id,source_ref_id,source_revision_hash,scope_id,origin,
                    root_execution_id,depth,source_order,primary_request_id,
                    primary_response_id,state
             FROM memory_extraction_sources WHERE source_basis=?1",
            [&input.source_basis],
            |row| {
                Ok(ExistingMemoryExtractionSource {
                    source_id: row.get(0)?,
                    source_ref_id: row.get(1)?,
                    source_revision_hash: row.get(2)?,
                    scope_id: row.get(3)?,
                    origin: row.get(4)?,
                    root_execution_id: row.get(5)?,
                    depth: row.get(6)?,
                    _source_order: row.get(7)?,
                    primary_request_id: row.get(8)?,
                    primary_response_id: row.get(9)?,
                    state: row.get(10)?,
                })
            },
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing.source_ref_id != input.source_ref_id
            || existing.source_revision_hash != input.source_revision_hash
            || existing.scope_id != input.scope_id
            || existing.origin != input.origin.as_str()
            || existing.root_execution_id != input.root_execution_id
            || existing.depth != input.depth
            || existing.primary_request_id != input.primary_request_id
            || existing.primary_response_id != input.primary_response_id
        {
            return Err(MemoryStoreError::ExtractionIdempotencyConflict);
        }
        let state = MemoryExtractionSourceState::parse(&existing.state)?;
        transaction.commit()?;
        return Ok(CaptureSourceOutcome::AlreadyCaptured {
            source_id: existing.source_id,
            state,
        });
    }

    let last_source_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(source_order), 0) FROM memory_extraction_sources",
        [],
        |row| row.get(0),
    )?;
    let next_source_order = last_source_order
        .checked_add(1)
        .ok_or(MemoryStoreError::InvalidField("source_order"))?;
    let source_order = input.source_order.max(next_source_order);

    transaction.execute(
        "INSERT INTO memory_extraction_sources
           (source_id,source_basis,source_ref_id,source_revision_hash,scope_id,origin,
            root_execution_id,depth,source_order,primary_request_id,
            primary_response_id,state,created_at_ms,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'captured',?12,?12)",
        params![
            input.source_id,
            input.source_basis,
            input.source_ref_id,
            input.source_revision_hash,
            input.scope_id,
            input.origin.as_str(),
            input.root_execution_id,
            input.depth,
            source_order,
            input.primary_request_id,
            input.primary_response_id,
            input.now_ms,
        ],
    )?;
    transaction.commit()?;
    Ok(CaptureSourceOutcome::Captured {
        source_id: input.source_id.clone(),
    })
}

/// Acquires a bounded source-scoped lease with a new fencing generation.
pub fn acquire_source_lease(
    connection: &Connection,
    source_id: &str,
    owner: &str,
    now_ms: i64,
    lease_ms: i64,
) -> Result<SourceLeaseOutcome, MemoryStoreError> {
    validate_reference(source_id, "source_id", MAX_BASIS_BYTES)?;
    validate_reference(owner, "lease_owner", MAX_BASIS_BYTES)?;
    if now_ms < 0 || lease_ms <= 0 || lease_ms > 300_000 {
        return Err(MemoryStoreError::InvalidField("lease_expiry"));
    }
    let transaction = connection.unchecked_transaction()?;
    let existing: Option<(String, i64, Option<i64>)> = transaction
        .query_row(
            "SELECT state,lease_generation,lease_expires_at_ms
             FROM memory_extraction_sources WHERE source_id=?1",
            [source_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((state_text, generation, expires_at_ms)) = existing else {
        return Err(MemoryStoreError::NotFound);
    };
    let state = MemoryExtractionSourceState::parse(&state_text)?;
    if matches!(
        state,
        MemoryExtractionSourceState::Committed
            | MemoryExtractionSourceState::Failed
            | MemoryExtractionSourceState::Stale
    ) {
        transaction.commit()?;
        return Ok(SourceLeaseOutcome::Terminal { state });
    }
    if let Some(expires_at_ms) = expires_at_ms.filter(|expires| *expires > now_ms) {
        transaction.commit()?;
        return Ok(SourceLeaseOutcome::Busy { expires_at_ms });
    }
    if !matches!(
        state,
        MemoryExtractionSourceState::Captured
            | MemoryExtractionSourceState::Deferred
            | MemoryExtractionSourceState::Finalizing
    ) {
        return Err(MemoryStoreError::InvalidTransition {
            from: state.as_str().to_owned(),
            to: MemoryExtractionSourceState::Finalizing.as_str().to_owned(),
        });
    }
    let next_generation = generation
        .checked_add(1)
        .ok_or(MemoryStoreError::InvalidField("lease_generation"))?;
    let expires_at_ms = now_ms.saturating_add(lease_ms);
    let changed = transaction.execute(
        "UPDATE memory_extraction_sources
         SET state='finalizing',revision=revision+1,lease_generation=?2,
             lease_owner=?3,lease_expires_at_ms=?4,error_code=NULL,updated_at_ms=?5
         WHERE source_id=?1 AND lease_generation=?6",
        params![
            source_id,
            next_generation,
            owner,
            expires_at_ms,
            now_ms,
            generation
        ],
    )?;
    if changed != 1 {
        return Err(MemoryStoreError::InvalidTransition {
            from: state.as_str().to_owned(),
            to: MemoryExtractionSourceState::Finalizing.as_str().to_owned(),
        });
    }
    transaction.commit()?;
    Ok(SourceLeaseOutcome::Acquired {
        generation: next_generation,
    })
}

/// Links one extractor request to its source before provider dispatch.
pub fn link_extractor_request(
    connection: &Connection,
    source_id: &str,
    generation: i64,
    logical_request_id: &str,
    request_id: &str,
    now_ms: i64,
) -> Result<bool, MemoryStoreError> {
    validate_reference(source_id, "source_id", MAX_BASIS_BYTES)?;
    validate_reference(logical_request_id, "logical_request_id", MAX_BASIS_BYTES)?;
    validate_reference(request_id, "request_id", MAX_BASIS_BYTES)?;
    if generation <= 0 || now_ms < 0 {
        return Err(MemoryStoreError::InvalidField("lease_generation"));
    }
    let changed = connection.execute(
        "UPDATE memory_extraction_sources
         SET extractor_logical_request_id=?3,extractor_request_id=?4,
             extractor_response_id=NULL,revision=revision+1,updated_at_ms=?5
         WHERE source_id=?1 AND lease_generation=?2 AND state='finalizing'
           AND lease_expires_at_ms>=?5
           AND (extractor_logical_request_id IS NULL OR extractor_logical_request_id=?3)",
        params![
            source_id,
            generation,
            logical_request_id,
            request_id,
            now_ms
        ],
    )?;
    Ok(changed == 1)
}

/// Links the response already committed to provenance for the current request.
pub fn link_extractor_response(
    connection: &Connection,
    source_id: &str,
    generation: i64,
    request_id: &str,
    response_id: &str,
    now_ms: i64,
) -> Result<bool, MemoryStoreError> {
    validate_reference(source_id, "source_id", MAX_BASIS_BYTES)?;
    validate_reference(request_id, "request_id", MAX_BASIS_BYTES)?;
    validate_reference(response_id, "response_id", MAX_BASIS_BYTES)?;
    if generation <= 0 || now_ms < 0 {
        return Err(MemoryStoreError::InvalidField("lease_generation"));
    }
    let changed = connection.execute(
        "UPDATE memory_extraction_sources
         SET extractor_response_id=?4,revision=revision+1,updated_at_ms=?5
         WHERE source_id=?1 AND lease_generation=?2 AND extractor_request_id=?3
           AND state='finalizing' AND lease_expires_at_ms>=?5",
        params![source_id, generation, request_id, response_id, now_ms],
    )?;
    Ok(changed == 1)
}

/// Finishes a source generation only while its lease and fencing token are current.
pub fn finish_source(
    connection: &Connection,
    source_id: &str,
    generation: i64,
    target: MemoryExtractionSourceState,
    error_code: Option<&str>,
    now_ms: i64,
) -> Result<bool, MemoryStoreError> {
    validate_reference(source_id, "source_id", MAX_BASIS_BYTES)?;
    if generation <= 0 || now_ms < 0 {
        return Err(MemoryStoreError::InvalidField("lease_generation"));
    }
    if !matches!(
        target,
        MemoryExtractionSourceState::Committed
            | MemoryExtractionSourceState::Deferred
            | MemoryExtractionSourceState::Failed
            | MemoryExtractionSourceState::Stale
    ) {
        return Err(MemoryStoreError::InvalidField("memory_extraction_state"));
    }
    if let Some(error_code) = error_code {
        validate_reference(error_code, "error_code", MAX_BASIS_BYTES)?;
    }
    let changed = connection.execute(
        "UPDATE memory_extraction_sources
         SET state=?3,revision=revision+1,error_code=?4,lease_owner=NULL,
             lease_expires_at_ms=NULL,updated_at_ms=?5
         WHERE source_id=?1 AND state='finalizing' AND lease_generation=?2
           AND lease_expires_at_ms>=?5",
        params![source_id, generation, target.as_str(), error_code, now_ms],
    )?;
    Ok(changed == 1)
}

fn map_source_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryExtractionSourceRecord> {
    let origin_text: String = row.get(5)?;
    let origin = MemoryExtractionOrigin::parse(&origin_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let state_text: String = row.get(14)?;
    let state = MemoryExtractionSourceState::parse(&state_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(MemoryExtractionSourceRecord {
        source_id: row.get(0)?,
        source_basis: row.get(1)?,
        source_ref_id: row.get(2)?,
        source_revision_hash: row.get(3)?,
        scope_id: row.get(4)?,
        origin,
        root_execution_id: row.get(6)?,
        depth: row.get(7)?,
        source_order: row.get(8)?,
        primary_request_id: row.get(9)?,
        primary_response_id: row.get(10)?,
        extractor_logical_request_id: row.get(11)?,
        extractor_request_id: row.get(12)?,
        extractor_response_id: row.get(13)?,
        state,
        revision: row.get(15)?,
        lease_generation: row.get(16)?,
        lease_expires_at_ms: row.get(17)?,
        error_code: row.get(18)?,
        created_at_ms: row.get(19)?,
        updated_at_ms: row.get(20)?,
    })
}

const SOURCE_RECORD_COLUMNS: &str = "source_id,source_basis,source_ref_id,
    source_revision_hash,scope_id,origin,root_execution_id,depth,source_order,
    primary_request_id,primary_response_id,extractor_logical_request_id,
    extractor_request_id,extractor_response_id,state,revision,lease_generation,
    lease_expires_at_ms,error_code,created_at_ms,updated_at_ms";

/// Reads one bounded source record for finalization or restart recovery.
pub fn get_source(
    connection: &Connection,
    source_id: &str,
) -> Result<Option<MemoryExtractionSourceRecord>, MemoryStoreError> {
    validate_reference(source_id, "source_id", MAX_BASIS_BYTES)?;
    let sql =
        format!("SELECT {SOURCE_RECORD_COLUMNS} FROM memory_extraction_sources WHERE source_id=?1");
    Ok(connection
        .query_row(&sql, [source_id], map_source_record)
        .optional()?)
}

/// Lists a bounded set of captured or deferred sources in source order.
pub fn list_recoverable_sources(
    connection: &Connection,
    now_ms: i64,
    limit: usize,
) -> Result<Vec<MemoryExtractionSourceRecord>, MemoryStoreError> {
    if now_ms < 0 || limit > MAX_RECOVERABLE_SOURCES {
        return Err(MemoryStoreError::InvalidField("recovery_limit"));
    }
    if limit == 0 {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {SOURCE_RECORD_COLUMNS} FROM memory_extraction_sources
         WHERE state IN ('captured','deferred')
            OR (state='finalizing' AND lease_expires_at_ms<=?1)
         ORDER BY source_order ASC,source_id ASC LIMIT ?2"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![now_ms, limit as i64], map_source_record)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Converts an expired source lease to a retryable deferred state.
pub fn defer_expired_source_lease(
    connection: &Connection,
    source_id: &str,
    now_ms: i64,
) -> Result<bool, MemoryStoreError> {
    validate_reference(source_id, "source_id", MAX_BASIS_BYTES)?;
    if now_ms < 0 {
        return Err(MemoryStoreError::InvalidField("updated_at_ms"));
    }
    let changed = connection.execute(
        "UPDATE memory_extraction_sources
         SET state='deferred',revision=revision+1,error_code='lease_expired',
             lease_owner=NULL,lease_expires_at_ms=NULL,updated_at_ms=?2
         WHERE source_id=?1 AND state='finalizing' AND lease_expires_at_ms<=?2",
        params![source_id, now_ms],
    )?;
    Ok(changed == 1)
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
