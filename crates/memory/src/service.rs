use crate::conflict::{detect_conflict, ConflictHit};
use crate::decision::{decide_gate, GateDecision, GateInput};
use crate::dedupe::detect_duplicate;
use crate::normalize::normalize_content;
use crate::redact::redact_secrets;
use evohime_storage::{
    insert_memory_item, list_memory_items, update_memory_item_status, MemoryItemRow, MemoryKind,
    MemoryScope, MemoryStatus, NewMemoryItem, StorageError,
};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ExistingMemory {
    pub id: Uuid,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub content: String,
    pub pinned: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedMemoryItem {
    pub item: NewMemoryItem,
    pub content: String,
    pub redacted: bool,
}

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("rejected memory: {reason}")]
    Rejected { reason: String },
    #[error(transparent)]
    Storage(#[from] StorageError),
}

#[derive(Debug, Clone)]
pub enum AdmitOutcome {
    Inserted(MemoryItemRow),
    Duplicate {
        existing_id: Uuid,
    },
    Conflict {
        existing_id: Uuid,
        item: MemoryItemRow,
        reason: String,
    },
    Rejected {
        reason: String,
    },
}

pub struct MemoryService;

impl MemoryService {
    pub fn prepare(item: &NewMemoryItem) -> Result<PreparedMemoryItem, MemoryError> {
        let redaction = redact_secrets(&item.content);
        let content = normalize_content(&redaction.text);
        if content.is_empty() || content == "[REDACTED]" {
            return Err(MemoryError::Rejected {
                reason: "content empty or only secrets after redaction".into(),
            });
        }

        let mut prepared = item.clone();
        prepared.content = content.clone();
        let embedding = crate::embed::embed_text(&content);
        prepared.embedding = Some(embedding);
        prepared.embedding_version = crate::embed::EMBEDDING_VERSION;
        prepared.validate().map_err(MemoryError::Storage)?;
        Ok(PreparedMemoryItem {
            item: prepared,
            content,
            redacted: redaction.redacted,
        })
    }

    pub fn evaluate(
        prepared: &PreparedMemoryItem,
        existing: &[ExistingMemory],
    ) -> Result<Evaluation, MemoryError> {
        if let Some(dup) = detect_duplicate(&prepared.content, existing) {
            return Ok(Evaluation::Duplicate {
                existing_id: dup.existing_id,
            });
        }
        if let Some(ConflictHit {
            existing_id,
            reason,
        }) = detect_conflict(prepared.item.kind, &prepared.content, existing)
        {
            return Ok(Evaluation::Conflict {
                existing_id,
                reason,
            });
        }
        Ok(Evaluation::Accept)
    }
}

#[derive(Debug, Clone)]
pub enum Evaluation {
    Accept,
    Duplicate { existing_id: Uuid },
    Conflict { existing_id: Uuid, reason: String },
}

fn row_to_existing(row: &MemoryItemRow) -> Option<ExistingMemory> {
    Some(ExistingMemory {
        id: row.id,
        kind: MemoryKind::parse(&row.kind)?,
        status: MemoryStatus::parse(&row.status)?,
        content: row.content.clone(),
        pinned: row.pinned,
    })
}

async fn load_existing(
    pool: &PgPool,
    scope: MemoryScope,
    scope_key: &str,
) -> Result<Vec<ExistingMemory>, MemoryError> {
    let rows = list_memory_items(
        pool,
        scope,
        scope_key,
        &[
            MemoryStatus::Candidate,
            MemoryStatus::Active,
            MemoryStatus::Conflict,
        ],
        500,
    )
    .await?;
    Ok(rows.iter().filter_map(row_to_existing).collect())
}

/// Redact → normalize → dedupe/conflict → insert (or skip).
pub async fn admit_memory_item(pool: &PgPool, item: NewMemoryItem) -> Result<AdmitOutcome, MemoryError> {
    let prepared = match MemoryService::prepare(&item) {
        Ok(prepared) => prepared,
        Err(MemoryError::Rejected { reason }) => return Ok(AdmitOutcome::Rejected { reason }),
        Err(other) => return Err(other),
    };

    let existing = load_existing(pool, prepared.item.scope, &prepared.item.scope_key).await?;
    match MemoryService::evaluate(&prepared, &existing)? {
        Evaluation::Duplicate { existing_id } => Ok(AdmitOutcome::Duplicate { existing_id }),
        Evaluation::Conflict {
            existing_id,
            reason,
        } => {
            let mut conflicted = prepared.item.clone();
            conflicted.status = MemoryStatus::Conflict;
            conflicted.supersedes = Some(existing_id);
            let inserted = insert_memory_item(pool, &conflicted).await?;
            let _ = update_memory_item_status(pool, existing_id, MemoryStatus::Conflict).await?;
            Ok(AdmitOutcome::Conflict {
                existing_id,
                item: inserted,
                reason,
            })
        }
        Evaluation::Accept => {
            let inserted = insert_memory_item(pool, &prepared.item).await?;
            Ok(AdmitOutcome::Inserted(inserted))
        }
    }
}

/// Map an admit outcome into a gate decision (and optional row for promote/ask).
pub fn gate_after_admit(outcome: &AdmitOutcome) -> (GateDecision, Option<&MemoryItemRow>) {
    match outcome {
        AdmitOutcome::Rejected { .. } => (
            decide_gate(&GateInput {
                scope: MemoryScope::Session,
                kind: MemoryKind::Fact,
                status: MemoryStatus::Rejected,
                content: String::new(),
                confidence: 0.0,
                pinned: false,
                had_conflict: false,
                was_duplicate: false,
                was_rejected: true,
            }),
            None,
        ),
        AdmitOutcome::Duplicate { .. } => (
            decide_gate(&GateInput {
                scope: MemoryScope::Session,
                kind: MemoryKind::Fact,
                status: MemoryStatus::Candidate,
                content: String::new(),
                confidence: 0.0,
                pinned: false,
                had_conflict: false,
                was_duplicate: true,
                was_rejected: false,
            }),
            None,
        ),
        AdmitOutcome::Conflict { item, .. } => {
            let scope = MemoryScope::parse(&item.scope).unwrap_or(MemoryScope::Workspace);
            let kind = MemoryKind::parse(&item.kind).unwrap_or(MemoryKind::Fact);
            (
                decide_gate(&GateInput {
                    scope,
                    kind,
                    status: MemoryStatus::Conflict,
                    content: item.content.clone(),
                    confidence: item.confidence,
                    pinned: item.pinned,
                    had_conflict: true,
                    was_duplicate: false,
                    was_rejected: false,
                }),
                Some(item),
            )
        }
        AdmitOutcome::Inserted(item) => {
            let scope = MemoryScope::parse(&item.scope).unwrap_or(MemoryScope::Workspace);
            let kind = MemoryKind::parse(&item.kind).unwrap_or(MemoryKind::Fact);
            let status = MemoryStatus::parse(&item.status).unwrap_or(MemoryStatus::Candidate);
            (
                decide_gate(&GateInput {
                    scope,
                    kind,
                    status,
                    content: item.content.clone(),
                    confidence: item.confidence,
                    pinned: item.pinned,
                    had_conflict: false,
                    was_duplicate: false,
                    was_rejected: false,
                }),
                Some(item),
            )
        }
    }
}

/// Promote a candidate (or conflict awaiting resolution) to `active`.
pub async fn accept_memory_item(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<MemoryItemRow>, MemoryError> {
    Ok(update_memory_item_status(pool, id, MemoryStatus::Active).await?)
}

/// Reject a memory item so it is excluded from retrieval.
pub async fn reject_memory_item(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<MemoryItemRow>, MemoryError> {
    Ok(update_memory_item_status(pool, id, MemoryStatus::Rejected).await?)
}

/// Apply an auto-promote decision: set status to `active`.
pub async fn promote_memory_item(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<MemoryItemRow>, MemoryError> {
    accept_memory_item(pool, id).await
}
