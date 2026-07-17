//! Apply feedback signals to persisted memory items (roadmap 6.23).

use crate::feedback::{apply_feedback_signal, FeedbackAdjustment, FeedbackSignal};
use crate::MemoryError;
use evohime_storage::{
    apply_memory_item_feedback, get_memory_item, list_idle_memory_for_decay, MemoryItemRow,
    MemoryStatus,
};
use sqlx::PgPool;
use uuid::Uuid;

pub const DEFAULT_IDLE_DAYS: i32 = 14;
pub const DEFAULT_IDLE_BATCH: i64 = 25;

#[derive(Debug, Clone)]
pub struct FeedbackApplyResult {
    pub memory_id: Uuid,
    pub signal: FeedbackSignal,
    pub adjustment: FeedbackAdjustment,
    pub row: MemoryItemRow,
}

async fn apply_one(
    pool: &PgPool,
    id: Uuid,
    signal: FeedbackSignal,
    task_id: Option<Uuid>,
) -> Result<Option<FeedbackApplyResult>, MemoryError> {
    let Some(existing) = get_memory_item(pool, id).await? else {
        return Ok(None);
    };
    let status = MemoryStatus::parse(&existing.status).unwrap_or(MemoryStatus::Candidate);
    let adjustment = apply_feedback_signal(
        existing.confidence,
        existing.importance,
        existing.pinned,
        status,
        signal,
    );

    let mark_used = matches!(
        signal,
        FeedbackSignal::Used | FeedbackSignal::Helpful | FeedbackSignal::Harmful
    );
    let bump_helpful = matches!(signal, FeedbackSignal::Helpful);
    let bump_harmful = matches!(signal, FeedbackSignal::Harmful | FeedbackSignal::Rejected);

    let Some(row) = apply_memory_item_feedback(
        pool,
        id,
        signal.as_str(),
        adjustment.confidence,
        adjustment.importance,
        adjustment.next_status,
        task_id,
        adjustment.delta_confidence,
        mark_used,
        bump_helpful,
        bump_harmful,
    )
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(FeedbackApplyResult {
        memory_id: id,
        signal,
        adjustment,
        row,
    }))
}

/// Record that memory ids were injected into a prompt.
pub async fn record_memory_used(
    pool: &PgPool,
    memory_ids: &[Uuid],
    task_id: Option<Uuid>,
) -> Result<Vec<FeedbackApplyResult>, MemoryError> {
    let mut out = Vec::new();
    for id in memory_ids {
        if let Some(result) = apply_one(pool, *id, FeedbackSignal::Used, task_id).await? {
            out.push(result);
        }
    }
    Ok(out)
}

/// Soft helpful bump after a successful task that used these memories.
pub async fn record_memory_helpful(
    pool: &PgPool,
    memory_ids: &[Uuid],
    task_id: Option<Uuid>,
) -> Result<Vec<FeedbackApplyResult>, MemoryError> {
    let mut out = Vec::new();
    for id in memory_ids {
        if let Some(result) = apply_one(pool, *id, FeedbackSignal::Helpful, task_id).await? {
            out.push(result);
        }
    }
    Ok(out)
}

/// Decay after a failed task that used these memories.
pub async fn record_memory_harmful(
    pool: &PgPool,
    memory_ids: &[Uuid],
    task_id: Option<Uuid>,
) -> Result<Vec<FeedbackApplyResult>, MemoryError> {
    let mut out = Vec::new();
    for id in memory_ids {
        if let Some(result) = apply_one(pool, *id, FeedbackSignal::Harmful, task_id).await? {
            out.push(result);
        }
    }
    Ok(out)
}

pub async fn record_memory_rejected(
    pool: &PgPool,
    memory_id: Uuid,
    task_id: Option<Uuid>,
) -> Result<Option<FeedbackApplyResult>, MemoryError> {
    apply_one(pool, memory_id, FeedbackSignal::Rejected, task_id).await
}

pub async fn record_memory_corrected(
    pool: &PgPool,
    memory_id: Uuid,
    task_id: Option<Uuid>,
) -> Result<Option<FeedbackApplyResult>, MemoryError> {
    apply_one(pool, memory_id, FeedbackSignal::Corrected, task_id).await
}

/// Decay a batch of prolonged unused low-confidence items toward archive.
pub async fn decay_unused_memory(
    pool: &PgPool,
    idle_days: i32,
    limit: i64,
) -> Result<Vec<FeedbackApplyResult>, MemoryError> {
    let candidates = list_idle_memory_for_decay(pool, idle_days, limit).await?;
    let mut out = Vec::new();
    for item in candidates {
        if let Some(result) = apply_one(pool, item.id, FeedbackSignal::IdleDecay, None).await? {
            out.push(result);
        }
    }
    Ok(out)
}
