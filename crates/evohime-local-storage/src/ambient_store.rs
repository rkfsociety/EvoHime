//! Bounded storage contract for ambient transcripts (план 04.2).
//!
//! Три таблицы схемы v25 — `ambient_episodes`, `ambient_utterances` и
//! `ambient_tombstones` — плюс операции над ними: вставка высказывания с
//! дедупликацией, открытие и закрытие эпизода, чтение под лимитом, удаление
//! эпизода и временного окна с tombstone и retention-purge.
//!
//! Что этот модуль гарантирует по конструкции:
//!
//! - аудио не хранится: в схеме нет BLOB-колонок, а API не принимает байты;
//! - удаление транзакционно и всегда оставляет metadata-only tombstone
//!   (`episode_id`, время, причина, число высказываний) — без текста и без
//!   его хеша;
//! - удаление источника отклоняет производных memory-кандидатов по
//!   существующему индексу `memory_entries(provenance_source_id)`, поэтому
//!   память-сирота не переживает удалённый эпизод;
//! - удаление вычищает ambient-строки из `events`. Это единственное место в
//!   кодовой базе, которое удаляет строки из durable journal: до 04.2 оттуда
//!   не удаляли ничего, даже при очистке истории ревью. Без этого «забыть
//!   последние N минут» оставляло бы вечный список `episode_id` с числом
//!   высказываний и полную хронологию того, когда пользователя слушали.
//!   Для читателей журнала это безопасно: курсор `push_journal_tail`
//!   монотонен по `sequence_id` и дырки переносит.
//!
//! Модуль migration-neutral и не знает про часы: все временные метки
//! приходят от вызывающего в одном формате `%Y-%m-%dT%H:%M:%S%.3fZ` — том же,
//! что SQLite пишет в `events.created_at`, поэтому лексикографическое
//! сравнение совпадает с хронологическим.

use evohime_listener_contract::{ExtractionState, ProposalKind, ProposalState};
use rusqlite::{params, Connection, OptionalExtension};

pub use crate::ambient_contract::*;
use crate::ambient_contract::{validate_non_negative, validate_reason, validate_required};
use crate::ambient_store_cleanup;
use crate::ambient_store_mapping;

/// Префикс ambient-событий в `events`.
pub(crate) const AMBIENT_EVENT_PREFIX: &str = "ambient.%";

/// Идентификатор tombstone выводится из `episode_id` и времени удаления.
///
/// Случайный id здесь ничего не скрыл бы: `episode_id` и так колонка
/// tombstone. Детерминированность важнее — повторный purge того же эпизода
/// не плодит вторую запись, а `UNIQUE(episode_id, removed_at)` остаётся
/// согласован с первичным ключом.
pub(crate) fn tombstone_id(episode_id: &str, removed_at: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(episode_id.as_bytes());
    hasher.update(b"|");
    hasher.update(removed_at.as_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Параметризованный SQL; создание схемы и миграции остаются снаружи.
pub struct AmbientStoreSql;

const EPISODE_COLUMNS: &str = "episode_id, started_at, ended_at, utterance_count, speech_ms,
        engine_version, model_id, extraction_state, expires_at";
const UTTERANCE_COLUMNS: &str = "utterance_id, episode_id, sequence, started_at, duration_ms,
        text, text_hash, language, avg_logprob, speaker, redacted, expires_at";
const TOMBSTONE_COLUMNS: &str =
    "tombstone_id, episode_id, removed_at, reason, utterance_count, expires_at";
const PROPOSAL_COLUMNS: &str = "proposal_id, proposal_key, mute_key, kind, subject_key, subject,
        title, source_episode_id, source_deleted_at, source_deleted_reason, created_at,
        updated_at, expires_at, occurrences, state, accepted_task_id, idempotency_key";

impl AmbientStoreSql {
    /// Открывает эпизод. Счётчики ведёт сам стор, поэтому вызывающий передаёт
    /// нули и не может «дорисовать» эпизоду высказывания.
    pub fn open_episode(
        connection: &Connection,
        record: &AmbientEpisodeRecord,
    ) -> Result<(), AmbientStoreError> {
        record.validate()?;
        if record.utterance_count != 0 || record.speech_ms != 0 {
            return Err(AmbientStoreError::InvalidInitialCounters);
        }
        connection.execute(
            "INSERT INTO ambient_episodes
             (episode_id, started_at, ended_at, utterance_count, speech_ms,
              engine_version, model_id, extraction_state, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.episode_id,
                record.started_at,
                record.ended_at,
                record.utterance_count,
                record.speech_ms,
                record.engine_version,
                record.model_id,
                record.extraction_state.as_str(),
                record.expires_at,
            ],
        )?;
        Ok(())
    }

    /// Sets an episode's end timestamp once; returns `false` if it is already closed or absent.
    pub fn close_episode(
        connection: &Connection,
        episode_id: &str,
        ended_at: &str,
    ) -> Result<bool, AmbientStoreError> {
        validate_required("episode_id", episode_id, MAX_ID_BYTES)?;
        validate_required("ended_at", ended_at, MAX_TIMESTAMP_BYTES)?;
        let changed = connection.execute(
            "UPDATE ambient_episodes SET ended_at = ?2
             WHERE episode_id = ?1 AND ended_at IS NULL",
            params![episode_id, ended_at],
        )?;
        Ok(changed > 0)
    }

    /// Updates an episode's extraction lifecycle state and reports whether it exists.
    pub fn set_extraction_state(
        connection: &Connection,
        episode_id: &str,
        state: ExtractionState,
    ) -> Result<bool, AmbientStoreError> {
        validate_required("episode_id", episode_id, MAX_ID_BYTES)?;
        let changed = connection.execute(
            "UPDATE ambient_episodes SET extraction_state = ?2 WHERE episode_id = ?1",
            params![episode_id, state.as_str()],
        )?;
        Ok(changed > 0)
    }

    /// Вставляет высказывание и поддерживает счётчики эпизода в той же
    /// транзакции.
    ///
    /// Возвращает `false`, если в окне дедупликации уже есть высказывание с
    /// таким же `text_hash`: телевизор и повтор одной фразы не должны
    /// множить строки. Окно задаётся вызывающим как нижняя граница
    /// `started_at`, потому что ширина окна — лимит контракта 04.1, а не
    /// свойство хранилища.
    pub fn insert_utterance(
        connection: &Connection,
        record: &AmbientUtteranceRecord,
        dedup_window_start: &str,
    ) -> Result<bool, AmbientStoreError> {
        record.validate()?;
        validate_required(
            "dedup_window_start",
            dedup_window_start,
            MAX_TIMESTAMP_BYTES,
        )?;
        let transaction = connection.unchecked_transaction()?;
        let duplicate: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM ambient_utterances
                 WHERE text_hash = ?1 AND started_at >= ?2 AND started_at <= ?3 LIMIT 1",
                params![record.text_hash, dedup_window_start, record.started_at],
                |row| row.get(0),
            )
            .optional()?;
        if duplicate.is_some() {
            transaction.commit()?;
            return Ok(false);
        }
        transaction.execute(
            "INSERT INTO ambient_utterances
             (utterance_id, episode_id, sequence, started_at, duration_ms, text, text_hash,
              language, avg_logprob, speaker, redacted, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.utterance_id,
                record.episode_id,
                record.sequence,
                record.started_at,
                record.duration_ms,
                record.text,
                record.text_hash,
                record.language,
                record.avg_logprob,
                record.speaker,
                record.redacted as i64,
                record.expires_at,
            ],
        )?;
        ambient_store_cleanup::recalculate_counters(&transaction, &record.episode_id)?;
        transaction.commit()?;
        Ok(true)
    }

    /// Loads one validated episode by ID, if present.
    pub fn get_episode(
        connection: &Connection,
        episode_id: &str,
    ) -> Result<Option<AmbientEpisodeRecord>, AmbientStoreError> {
        validate_required("episode_id", episode_id, MAX_ID_BYTES)?;
        Ok(connection
            .query_row(
                &format!("SELECT {EPISODE_COLUMNS} FROM ambient_episodes WHERE episode_id = ?1"),
                params![episode_id],
                ambient_store_mapping::map_episode,
            )
            .optional()?)
    }

    /// Свежие эпизоды сначала; `limit` прижимается к [`MAX_ROWS_PER_READ`].
    pub fn list_episodes(
        connection: &Connection,
        limit: usize,
    ) -> Result<Vec<AmbientEpisodeRecord>, AmbientStoreError> {
        let limit = limit.min(MAX_ROWS_PER_READ) as i64;
        let mut statement = connection.prepare(&format!(
            "SELECT {EPISODE_COLUMNS} FROM ambient_episodes
             ORDER BY started_at DESC, episode_id DESC LIMIT ?1"
        ))?;
        let rows = statement.query_map(params![limit], ambient_store_mapping::map_episode)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Lists an episode's utterances up to the bounded read limit.
    pub fn list_utterances(
        connection: &Connection,
        episode_id: &str,
        limit: usize,
    ) -> Result<Vec<AmbientUtteranceRecord>, AmbientStoreError> {
        validate_required("episode_id", episode_id, MAX_ID_BYTES)?;
        let limit = limit.min(MAX_ROWS_PER_READ) as i64;
        let mut statement = connection.prepare(&format!(
            "SELECT {UTTERANCE_COLUMNS} FROM ambient_utterances
             WHERE episode_id = ?1 ORDER BY sequence ASC LIMIT ?2"
        ))?;
        let rows = statement.query_map(
            params![episode_id, limit],
            ambient_store_mapping::map_utterance,
        )?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Lists removal tombstones up to the bounded read limit.
    pub fn list_tombstones(
        connection: &Connection,
        limit: usize,
    ) -> Result<Vec<AmbientTombstoneRecord>, AmbientStoreError> {
        let limit = limit.min(MAX_ROWS_PER_READ) as i64;
        let mut statement = connection.prepare(&format!(
            "SELECT {TOMBSTONE_COLUMNS} FROM ambient_tombstones
             ORDER BY removed_at DESC, tombstone_id DESC LIMIT ?1"
        ))?;
        let rows = statement.query_map(params![limit], ambient_store_mapping::map_tombstone)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Удаляет эпизод целиком: tombstone фиксируется до того, как исчезает
    /// хоть одно высказывание, и всё это — одна транзакция.
    pub fn delete_episode(
        connection: &Connection,
        episode_id: &str,
        reason: &str,
        removed_at: &str,
        tombstone_expires_at: &str,
    ) -> Result<AmbientDeletion, AmbientStoreError> {
        validate_required("episode_id", episode_id, MAX_ID_BYTES)?;
        validate_required("removed_at", removed_at, MAX_TIMESTAMP_BYTES)?;
        validate_required(
            "tombstone_expires_at",
            tombstone_expires_at,
            MAX_TIMESTAMP_BYTES,
        )?;
        validate_reason(reason)?;
        let transaction = connection.unchecked_transaction()?;
        let mut deletion = AmbientDeletion::default();
        ambient_store_cleanup::remove_episode(
            &transaction,
            episode_id,
            reason,
            removed_at,
            tombstone_expires_at,
            &mut deletion,
        )?;
        transaction.commit()?;
        Ok(deletion)
    }

    /// «Забыть последние N минут»: удаляет высказывания с `started_at` в
    /// замкнутом окне `[from, to]`.
    ///
    /// Эпизод, начавшийся до окна, не удаляется целиком только из-за того,
    /// что пересекает границу: у него пересчитываются счётчики. Пустой после
    /// удаления эпизод уходит в той же транзакции вместе с tombstone.
    /// Кандидаты памяти отклоняются у всех задетых эпизодов, а не только у
    /// удалённых: provenance ведёт к эпизоду, а не к высказыванию, поэтому
    /// «этот кандидат пришёл из уцелевшей части» — недоказуемое допущение.
    pub fn forget_window(
        connection: &Connection,
        from: &str,
        to: &str,
        removed_at: &str,
        tombstone_expires_at: &str,
    ) -> Result<AmbientDeletion, AmbientStoreError> {
        validate_required("from", from, MAX_TIMESTAMP_BYTES)?;
        validate_required("to", to, MAX_TIMESTAMP_BYTES)?;
        validate_required("removed_at", removed_at, MAX_TIMESTAMP_BYTES)?;
        validate_required(
            "tombstone_expires_at",
            tombstone_expires_at,
            MAX_TIMESTAMP_BYTES,
        )?;
        let transaction = connection.unchecked_transaction()?;
        let mut deletion = AmbientDeletion::default();
        let affected = ambient_store_cleanup::affected_episodes(
            &transaction,
            "SELECT DISTINCT episode_id FROM ambient_utterances
             WHERE started_at >= ?1 AND started_at <= ?2",
            params![from, to],
        )?;
        deletion.utterances_removed += transaction.execute(
            "DELETE FROM ambient_utterances WHERE started_at >= ?1 AND started_at <= ?2",
            params![from, to],
        )?;
        for episode_id in &affected {
            deletion.candidates_rejected +=
                ambient_store_cleanup::reject_candidates(&transaction, episode_id)?;
            let remaining = ambient_store_cleanup::recalculate_counters(&transaction, episode_id)?;
            if remaining == 0 {
                ambient_store_cleanup::remove_episode(
                    &transaction,
                    episode_id,
                    REASON_FORGET_WINDOW,
                    removed_at,
                    tombstone_expires_at,
                    &mut deletion,
                )?;
            }
        }
        // Хронология «когда слушали» живёт не только в ambient-таблицах:
        // без этой строки список episode_id и число высказываний остались бы
        // в journal навсегда и пережили бы forget.
        deletion.events_removed += transaction.execute(
            "DELETE FROM events
             WHERE event_type LIKE ?1 AND created_at >= ?2 AND created_at <= ?3",
            params![AMBIENT_EVENT_PREFIX, from, to],
        )?;
        // События эпизода могут быть старше окна высказываний (например,
        // `ambient.transcript` был опубликован при его открытии). Если у
        // эпизода удалено хотя бы одно высказывание, его ambient-хронология
        // должна исчезнуть целиком.
        for episode_id in &affected {
            deletion.events_removed += transaction.execute(
                "DELETE FROM events
                 WHERE task_id = ?1 AND event_type LIKE ?2",
                params![episode_id, AMBIENT_EVENT_PREFIX],
            )?;
        }
        transaction.commit()?;
        Ok(deletion)
    }

    // ------------------------------------------------------------------
    // Ограниченная проактивность (план 04.7).
    // ------------------------------------------------------------------

    /// Регистрирует предложение.
    ///
    /// Три исхода вместо одного, и ни один из них не ошибка:
    ///
    /// - `Muted` — тема заглушена навсегда по `mute_key`. Проверка идёт
    ///   первой: заглушённая тема не должна даже поднимать счётчик;
    /// - `Duplicate` — такое уже предлагалось в этой временной корзине.
    ///   Счётчик существующей карточки растёт, второй карточки не появляется;
    /// - `Created` — новая карточка.
    pub fn record_proposal(
        connection: &Connection,
        record: &AmbientProposalRecord,
    ) -> Result<ProposalInsert, AmbientStoreError> {
        record.validate()?;
        let transaction = connection.unchecked_transaction()?;
        let muted: Option<i64> = transaction
            .query_row(
                "SELECT 1 FROM ambient_proposal_mutes WHERE mute_key = ?1",
                params![record.mute_key],
                |row| row.get(0),
            )
            .optional()?;
        if muted.is_some() {
            transaction.commit()?;
            return Ok(ProposalInsert::Muted);
        }
        let existing: Option<(String, i64)> = transaction
            .query_row(
                "SELECT proposal_id, occurrences FROM ambient_proposals WHERE proposal_key = ?1",
                params![record.proposal_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((proposal_id, occurrences)) = existing {
            let occurrences = occurrences.saturating_add(1);
            transaction.execute(
                "UPDATE ambient_proposals SET occurrences = ?2, updated_at = ?3
                 WHERE proposal_id = ?1",
                params![proposal_id, occurrences, record.updated_at],
            )?;
            transaction.commit()?;
            return Ok(ProposalInsert::Duplicate {
                proposal_id,
                occurrences,
            });
        }
        transaction.execute(
            "INSERT INTO ambient_proposals
             (proposal_id, proposal_key, mute_key, kind, subject_key, subject, title,
              source_episode_id, source_deleted_at, source_deleted_reason, created_at,
              updated_at, expires_at, occurrences, state, accepted_task_id, idempotency_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                record.proposal_id,
                record.proposal_key,
                record.mute_key,
                record.kind.as_str(),
                record.subject_key,
                record.subject,
                record.title,
                record.source_episode_id,
                record.source_deleted_at,
                record.source_deleted_reason,
                record.created_at,
                record.updated_at,
                record.expires_at,
                record.occurrences.max(1),
                record.state.as_str(),
                record.accepted_task_id,
                record.idempotency_key,
            ],
        )?;
        transaction.commit()?;
        Ok(ProposalInsert::Created)
    }

    /// Loads one proposal by ID, if present.
    pub fn get_proposal(
        connection: &Connection,
        proposal_id: &str,
    ) -> Result<Option<AmbientProposalRecord>, AmbientStoreError> {
        validate_required("proposal_id", proposal_id, MAX_ID_BYTES)?;
        Ok(connection
            .query_row(
                &format!("SELECT {PROPOSAL_COLUMNS} FROM ambient_proposals WHERE proposal_id = ?1"),
                params![proposal_id],
                ambient_store_mapping::map_proposal,
            )
            .optional()?)
    }

    /// Находит предложение, уже решённое этим ключом идемпотентности.
    ///
    /// Это и есть защита от двойного клика: второй запрос с тем же ключом
    /// находит первую запись и не создаёт вторую задачу.
    pub fn find_proposal_by_idempotency(
        connection: &Connection,
        idempotency_key: &str,
    ) -> Result<Option<AmbientProposalRecord>, AmbientStoreError> {
        validate_required("idempotency_key", idempotency_key, MAX_ID_BYTES)?;
        Ok(connection
            .query_row(
                &format!(
                    "SELECT {PROPOSAL_COLUMNS} FROM ambient_proposals WHERE idempotency_key = ?1"
                ),
                params![idempotency_key],
                ambient_store_mapping::map_proposal,
            )
            .optional()?)
    }

    /// Ожидающие решения предложения, свежие первыми.
    pub fn list_open_proposals(
        connection: &Connection,
        limit: usize,
    ) -> Result<Vec<AmbientProposalRecord>, AmbientStoreError> {
        let limit = limit.min(MAX_ROWS_PER_READ) as i64;
        let mut statement = connection.prepare(&format!(
            "SELECT {PROPOSAL_COLUMNS} FROM ambient_proposals WHERE state = 'proposed'
             ORDER BY created_at DESC, proposal_id DESC LIMIT ?1"
        ))?;
        let rows = statement.query_map(params![limit], ambient_store_mapping::map_proposal)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    /// Переводит предложение в терминальное состояние.
    ///
    /// `Ok(false)` означает «уже решено или такого нет» — вызывающий обязан
    /// ответить именно это, а не «применено». Переход возможен только из
    /// `proposed`, поэтому гонка двух кликов разрешается первым из них.
    pub fn resolve_proposal(
        connection: &Connection,
        proposal_id: &str,
        state: ProposalState,
        updated_at: &str,
        accepted_task_id: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<bool, AmbientStoreError> {
        validate_required("proposal_id", proposal_id, MAX_ID_BYTES)?;
        validate_required("updated_at", updated_at, MAX_TIMESTAMP_BYTES)?;
        if let Some(key) = idempotency_key {
            validate_required("idempotency_key", key, MAX_ID_BYTES)?;
        }
        if !state.is_terminal() {
            return Err(AmbientStoreError::InvalidInitialState);
        }
        let changed = connection.execute(
            "UPDATE ambient_proposals
             SET state = ?2, updated_at = ?3, accepted_task_id = ?4, idempotency_key = ?5
             WHERE proposal_id = ?1 AND state = 'proposed'",
            params![
                proposal_id,
                state.as_str(),
                updated_at,
                accepted_task_id,
                idempotency_key,
            ],
        )?;
        Ok(changed > 0)
    }

    /// Заглушает тему навсегда.
    ///
    /// Ключ здесь — `mute_key` без времени. Со временем внутри ключа mute
    /// заглушил бы ровно одну временную корзину и молча перестал бы
    /// действовать через час.
    pub fn mute_subject(
        connection: &Connection,
        mute_key: &str,
        kind: ProposalKind,
        subject_key: &str,
        muted_at: &str,
    ) -> Result<(), AmbientStoreError> {
        validate_required("mute_key", mute_key, MAX_ID_BYTES)?;
        validate_required("subject_key", subject_key, MAX_ID_BYTES)?;
        validate_required("muted_at", muted_at, MAX_TIMESTAMP_BYTES)?;
        connection.execute(
            "INSERT INTO ambient_proposal_mutes(mute_key, kind, subject_key, muted_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(mute_key) DO UPDATE SET muted_at = excluded.muted_at",
            params![mute_key, kind.as_str(), subject_key, muted_at],
        )?;
        Ok(())
    }

    /// Lists stored subject-mute keys, capped at [`MAX_ROWS_PER_READ`].
    pub fn list_mute_keys(connection: &Connection) -> Result<Vec<String>, AmbientStoreError> {
        let mut statement = connection
            .prepare("SELECT mute_key FROM ambient_proposal_mutes ORDER BY mute_key LIMIT ?1")?;
        let rows =
            statement.query_map([MAX_ROWS_PER_READ as i64], |row| row.get::<_, String>(0))?;
        let mut keys = Vec::new();
        for row in rows {
            keys.push(row?);
        }
        Ok(keys)
    }

    /// Отсутствие реакции переводит предложение в `expired`.
    pub fn expire_stale_proposals(
        connection: &Connection,
        now: &str,
    ) -> Result<usize, AmbientStoreError> {
        validate_required("now", now, MAX_TIMESTAMP_BYTES)?;
        Ok(connection.execute(
            "UPDATE ambient_proposals SET state = 'expired', updated_at = ?1
             WHERE state = 'proposed' AND expires_at <= ?1",
            params![now],
        )?)
    }

    /// Loads the current ambient proactivity counters, if initialized.
    pub fn load_counters(
        connection: &Connection,
    ) -> Result<Option<ProactivityCountersRow>, AmbientStoreError> {
        Ok(connection
            .query_row(
                "SELECT hour_started_at_ms, hour_count, day_started_at_ms, day_count,
                        last_proposed_at_ms
                 FROM ambient_proactivity_counters WHERE profile_id = ?1",
                params![PROACTIVITY_PROFILE_ID],
                |row| {
                    Ok(ProactivityCountersRow {
                        hour_started_at_ms: row.get(0)?,
                        hour_count: row.get(1)?,
                        day_started_at_ms: row.get(2)?,
                        day_count: row.get(3)?,
                        last_proposed_at_ms: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Persists the proactivity counters after validating their non-negative counts.
    pub fn save_counters(
        connection: &Connection,
        row: ProactivityCountersRow,
    ) -> Result<(), AmbientStoreError> {
        validate_non_negative("hour_count", row.hour_count)?;
        validate_non_negative("day_count", row.day_count)?;
        connection.execute(
            "INSERT INTO ambient_proactivity_counters
             (profile_id, hour_started_at_ms, hour_count, day_started_at_ms, day_count,
              last_proposed_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(profile_id) DO UPDATE SET
                hour_started_at_ms = excluded.hour_started_at_ms,
                hour_count = excluded.hour_count,
                day_started_at_ms = excluded.day_started_at_ms,
                day_count = excluded.day_count,
                last_proposed_at_ms = excluded.last_proposed_at_ms",
            params![
                PROACTIVITY_PROFILE_ID,
                row.hour_started_at_ms,
                row.hour_count,
                row.day_started_at_ms,
                row.day_count,
                row.last_proposed_at_ms,
            ],
        )?;
        Ok(())
    }

    /// Retention-прогон.
    ///
    /// - `now` — граница истечения для высказываний, эпизодов и tombstone;
    /// - `tombstone_expires_at` — срок жизни tombstone, который создаётся для
    ///   истёкшего эпизода;
    /// - `event_cutoff` — граница retention ambient-строк в `events`.
    pub fn purge_expired(
        connection: &Connection,
        now: &str,
        tombstone_expires_at: &str,
        event_cutoff: &str,
    ) -> Result<AmbientPurge, AmbientStoreError> {
        validate_required("now", now, MAX_TIMESTAMP_BYTES)?;
        validate_required(
            "tombstone_expires_at",
            tombstone_expires_at,
            MAX_TIMESTAMP_BYTES,
        )?;
        validate_required("event_cutoff", event_cutoff, MAX_TIMESTAMP_BYTES)?;
        let transaction = connection.unchecked_transaction()?;
        let mut purge = AmbientPurge::default();

        // 1. Истёкший текст. Эпизод при этом остаётся: у метаданных свой,
        //    более длинный срок, поэтому счётчики пересчитываются, а не
        //    замораживаются на прежнем значении.
        let partial = ambient_store_cleanup::affected_episodes(
            &transaction,
            "SELECT DISTINCT episode_id FROM ambient_utterances WHERE expires_at <= ?1",
            params![now],
        )?;
        purge.utterances_removed += transaction.execute(
            "DELETE FROM ambient_utterances WHERE expires_at <= ?1",
            params![now],
        )?;
        for episode_id in &partial {
            ambient_store_cleanup::recalculate_counters(&transaction, episode_id)?;
        }

        // 2. Истёкшие метаданные эпизода: удаляются с bounded tombstone.
        let expired = ambient_store_cleanup::affected_episodes(
            &transaction,
            "SELECT episode_id FROM ambient_episodes WHERE expires_at <= ?1",
            params![now],
        )?;
        let mut deletion = AmbientDeletion::default();
        for episode_id in &expired {
            ambient_store_cleanup::remove_episode(
                &transaction,
                episode_id,
                REASON_RETENTION,
                now,
                tombstone_expires_at,
                &mut deletion,
            )?;
        }
        purge.episodes_removed += deletion.episodes_removed;
        purge.utterances_removed += deletion.utterances_removed;
        purge.tombstones_written += deletion.tombstones_written;
        purge.events_removed += deletion.events_removed;
        purge.candidates_rejected += deletion.candidates_rejected;

        // 3. Сам tombstone тоже истекает: «след удаления» не вечен.
        purge.tombstones_removed += transaction.execute(
            "DELETE FROM ambient_tombstones WHERE expires_at <= ?1",
            params![now],
        )?;

        // 4. Ambient-строки durable journal. У `events` нет собственного
        //    retention вообще, поэтому срок вводится здесь.
        purge.events_removed += transaction.execute(
            "DELETE FROM events WHERE event_type LIKE ?1 AND created_at <= ?2",
            params![AMBIENT_EVENT_PREFIX, event_cutoff],
        )?;

        // 5. Предложения. Сначала истечение по 24-часовому окну — карточка,
        //    на которую не ответили, перестаёт ждать ответа, — затем уборка
        //    уже решённых по тому же сроку, что и ambient-строки журнала.
        if ambient_store_cleanup::table_exists(&transaction, "ambient_proposals")? {
            purge.proposals_expired += transaction.execute(
                "UPDATE ambient_proposals SET state = 'expired', updated_at = ?1
                 WHERE state = 'proposed' AND expires_at <= ?1",
                params![now],
            )?;
            purge.proposals_removed += transaction.execute(
                "DELETE FROM ambient_proposals
                 WHERE state <> 'proposed' AND updated_at <= ?1",
                params![event_cutoff],
            )?;
        }

        transaction.commit()?;
        Ok(purge)
    }
}

#[cfg(test)]
#[path = "ambient_store_tests.rs"]
mod tests;
