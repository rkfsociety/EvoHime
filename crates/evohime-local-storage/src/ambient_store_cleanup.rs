//! Transactional cleanup helpers for ambient retention and source deletion.

use rusqlite::{params, OptionalExtension};

use crate::ambient_store::{
    tombstone_id, AmbientDeletion, AmbientStoreError, AMBIENT_EVENT_PREFIX,
    CANDIDATE_REJECTION_REASON,
};

/// Общий путь удаления эпизода: tombstone → кандидаты → journal → строки.
///
/// Порядок не косметический: tombstone фиксируется до того, как исчезает
/// первое высказывание, поэтому оборванная транзакция не может оставить
/// «удалено без следа».
pub(crate) fn remove_episode(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: &str,
    reason: &str,
    removed_at: &str,
    tombstone_expires_at: &str,
    deletion: &mut AmbientDeletion,
) -> Result<(), AmbientStoreError> {
    let utterance_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM ambient_utterances WHERE episode_id = ?1",
        params![episode_id],
        |row| row.get(0),
    )?;
    deletion.tombstones_written += transaction.execute(
        "INSERT OR REPLACE INTO ambient_tombstones
         (tombstone_id, episode_id, removed_at, reason, utterance_count, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            tombstone_id(episode_id, removed_at),
            episode_id,
            removed_at,
            reason,
            utterance_count,
            tombstone_expires_at,
        ],
    )?;
    deletion.candidates_rejected += reject_candidates(transaction, episode_id)?;
    // Порядок здесь — контракт, а не стиль. `ON DELETE SET NULL` обнулил бы
    // `source_episode_id` первым, и после удаления строки эпизода найти его
    // предложения было бы уже нечем. Поэтому они помечаются истёкшими
    // раньше — в этой же транзакции и тем же моментом, что и tombstone.
    deletion.proposals_expired += expire_proposals_of_episode(transaction, episode_id, removed_at)?;
    deletion.events_removed += transaction.execute(
        "DELETE FROM events WHERE task_id = ?1 AND event_type LIKE ?2",
        params![episode_id, AMBIENT_EVENT_PREFIX],
    )?;
    // Каскад по внешнему ключу сделал бы то же самое, но только при
    // включённом `foreign_keys`; явное удаление не зависит от pragma.
    deletion.utterances_removed += transaction.execute(
        "DELETE FROM ambient_utterances WHERE episode_id = ?1",
        params![episode_id],
    )?;
    deletion.episodes_removed += transaction.execute(
        "DELETE FROM ambient_episodes WHERE episode_id = ?1",
        params![episode_id],
    )?;
    Ok(())
}

/// Переводит предложения удаляемого эпизода в `expired` с причиной
/// `source_deleted`.
pub(crate) fn expire_proposals_of_episode(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: &str,
    removed_at: &str,
) -> Result<usize, AmbientStoreError> {
    if !table_exists(transaction, "ambient_proposals")? {
        return Ok(0);
    }
    Ok(transaction.execute(
        "UPDATE ambient_proposals
         SET state = 'expired', updated_at = ?2,
             source_deleted_at = ?2, source_deleted_reason = ?3
         WHERE source_episode_id = ?1",
        params![episode_id, removed_at, CANDIDATE_REJECTION_REASON],
    )?)
}

/// Отклоняет производных memory-кандидатов удалённого эпизода.
pub(crate) fn reject_candidates(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: &str,
) -> Result<usize, AmbientStoreError> {
    if !table_exists(transaction, "memory_entries")? {
        return Ok(0);
    }
    Ok(transaction.execute(
        "UPDATE memory_entries
         SET confirmation_state = 'rejected', supersession_reason = ?2
         WHERE provenance_source_id = ?1
           AND confirmation_state IN ('candidate', 'pending_confirmation')",
        params![episode_id, CANDIDATE_REJECTION_REASON],
    )?)
}

pub(crate) fn table_exists(
    transaction: &rusqlite::Transaction<'_>,
    name: &str,
) -> Result<bool, AmbientStoreError> {
    let found: Option<i64> = transaction
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

/// Пересчитывает счётчики эпизода из уцелевших строк и возвращает их число.
pub(crate) fn recalculate_counters(
    transaction: &rusqlite::Transaction<'_>,
    episode_id: &str,
) -> Result<i64, AmbientStoreError> {
    transaction.execute(
        "UPDATE ambient_episodes SET
            utterance_count = (SELECT COUNT(*) FROM ambient_utterances WHERE episode_id = ?1),
            speech_ms = (SELECT COALESCE(SUM(duration_ms), 0) FROM ambient_utterances
                         WHERE episode_id = ?1)
         WHERE episode_id = ?1",
        params![episode_id],
    )?;
    Ok(transaction.query_row(
        "SELECT COUNT(*) FROM ambient_utterances WHERE episode_id = ?1",
        params![episode_id],
        |row| row.get(0),
    )?)
}

pub(crate) fn affected_episodes(
    transaction: &rusqlite::Transaction<'_>,
    sql: &str,
    parameters: impl rusqlite::Params,
) -> Result<Vec<String>, AmbientStoreError> {
    let mut statement = transaction.prepare(sql)?;
    let rows = statement.query_map(parameters, |row| row.get::<_, String>(0))?;
    let mut episodes = Vec::new();
    for row in rows {
        episodes.push(row?);
    }
    Ok(episodes)
}
