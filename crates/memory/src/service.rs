use crate::conflict::{detect_conflict, ConflictHit};
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
