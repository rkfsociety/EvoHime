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

pub const MAX_ID_BYTES: usize = 256;
pub const MAX_TIMESTAMP_BYTES: usize = 64;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_HASH_BYTES: usize = 128;
pub const MAX_LANGUAGE_BYTES: usize = 32;
/// Потолок одной выборки: одна операция чтения не может вытащить весь
/// транскрипт целиком.
pub const MAX_ROWS_PER_READ: usize = 500;

/// v1 не распознаёт говорящего и не хранит голосовые профили, поэтому
/// единственное допустимое значение — «не проверен».
pub const SPEAKER_UNVERIFIED: &str = "unverified";

/// Закрытый набор причин удаления. Свободный текст сюда не попадает: причина
/// уходит в tombstone, который переживает сам эпизод.
pub const REASON_USER_REQUEST: &str = "user_request";
pub const REASON_RETENTION: &str = "retention";
pub const REASON_FORGET_WINDOW: &str = "forget_window";
const REASONS: [&str; 3] = [REASON_USER_REQUEST, REASON_RETENTION, REASON_FORGET_WINDOW];

/// Причина, по которой отклоняется производный memory-кандидат удалённого
/// эпизода. Ей же помечается предложение, чей эпизод-источник исчез.
pub const CANDIDATE_REJECTION_REASON: &str = "source_deleted";

/// Единственный ambient-профиль, под которым живут счётчики бюджета.
pub const PROACTIVITY_PROFILE_ID: &str = "ambient";

/// Сколько живёт решённое предложение, прежде чем его уберёт purge. Совпадает
/// со сроком ambient-строк журнала: карточка, по которой уже кликнули, — тот
/// же след «когда слушали», и вечной ей быть незачем.
pub const MAX_TITLE_BYTES: usize = 2 * 1024;

/// Префикс ambient-событий в `events`. Совпадает с именами записей
/// типизированного фасада (`ambient.state`, `ambient.transcript`, …).
const AMBIENT_EVENT_PREFIX: &str = "ambient.%";

#[derive(Debug, thiserror::Error)]
pub enum AmbientStoreError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("ambient v1 stores only unverified speakers")]
    InvalidSpeaker,
    #[error("ambient episodes must start with zero counters")]
    InvalidInitialCounters,
    #[error("unknown removal reason")]
    InvalidReason,
    #[error("{field} must not be negative")]
    Negative { field: &'static str },
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("a proposal may only carry both source_deleted fields or neither")]
    InvalidSourceDeletion,
    #[error("a new proposal must start in the proposed state")]
    InvalidInitialState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmbientEpisodeRecord {
    pub episode_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub utterance_count: i64,
    pub speech_ms: i64,
    pub engine_version: String,
    pub model_id: String,
    pub extraction_state: ExtractionState,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmbientUtteranceRecord {
    pub utterance_id: String,
    pub episode_id: String,
    pub sequence: i64,
    pub started_at: String,
    pub duration_ms: i64,
    pub text: String,
    pub text_hash: String,
    pub language: String,
    pub avg_logprob: f64,
    pub speaker: String,
    pub redacted: bool,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbientTombstoneRecord {
    pub tombstone_id: String,
    pub episode_id: String,
    pub removed_at: String,
    pub reason: String,
    pub utterance_count: i64,
    pub expires_at: String,
}

/// Одно ограниченное проактивное предложение (план 04.7).
///
/// Два ключа, а не один: [`Self::proposal_key`] несёт округлённое время и
/// стоит под `UNIQUE` — это дедупликация; [`Self::mute_key`] времени не несёт
/// и живёт в отдельной таблице — это «больше не предлагать такое».
///
/// `title` — человекочитаемый текст карточки. Он живёт здесь, а не в
/// `events`: журнал append-only, а эта таблица уходит вместе с эпизодом и
/// подчиняется ambient-retention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbientProposalRecord {
    pub proposal_id: String,
    pub proposal_key: String,
    pub mute_key: String,
    pub kind: ProposalKind,
    pub subject_key: String,
    pub subject: String,
    pub title: String,
    /// Эпизод-источник. `None` означает, что источник уже удалён: внешний
    /// ключ объявлен `ON DELETE SET NULL`, чтобы связь не блокировала
    /// удаление транскрипта.
    pub source_episode_id: Option<String>,
    pub source_deleted_at: Option<String>,
    pub source_deleted_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    pub occurrences: i64,
    pub state: ProposalState,
    pub accepted_task_id: Option<String>,
    pub idempotency_key: Option<String>,
}

/// Что произошло с попыткой предложить.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalInsert {
    Created,
    /// Такое уже предлагалось: у существующей карточки поднят счётчик, второй
    /// карточки не появилось.
    Duplicate {
        proposal_id: String,
        occurrences: i64,
    },
    /// Пользователь заглушил тему навсегда.
    Muted,
}

/// Персистентные счётчики окна проактивности.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProactivityCountersRow {
    pub hour_started_at_ms: i64,
    pub hour_count: i64,
    pub day_started_at_ms: i64,
    pub day_count: i64,
    pub last_proposed_at_ms: Option<i64>,
}

impl AmbientProposalRecord {
    fn validate(&self) -> Result<(), AmbientStoreError> {
        validate_required("proposal_id", &self.proposal_id, MAX_ID_BYTES)?;
        validate_required("proposal_key", &self.proposal_key, MAX_ID_BYTES)?;
        validate_required("mute_key", &self.mute_key, MAX_ID_BYTES)?;
        validate_required("subject_key", &self.subject_key, MAX_ID_BYTES)?;
        validate_required("subject", &self.subject, MAX_TITLE_BYTES)?;
        validate_required("title", &self.title, MAX_TITLE_BYTES)?;
        validate_required("created_at", &self.created_at, MAX_TIMESTAMP_BYTES)?;
        validate_required("updated_at", &self.updated_at, MAX_TIMESTAMP_BYTES)?;
        validate_required("expires_at", &self.expires_at, MAX_TIMESTAMP_BYTES)?;
        if let Some(episode_id) = &self.source_episode_id {
            validate_required("source_episode_id", episode_id, MAX_ID_BYTES)?;
        }
        if self.source_deleted_at.is_some() != self.source_deleted_reason.is_some() {
            return Err(AmbientStoreError::InvalidSourceDeletion);
        }
        validate_non_negative("occurrences", self.occurrences)?;
        if self.state != ProposalState::Proposed {
            return Err(AmbientStoreError::InvalidInitialState);
        }
        Ok(())
    }
}

/// Итог явного удаления: эпизода или временного окна.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AmbientDeletion {
    pub episodes_removed: usize,
    pub utterances_removed: usize,
    pub tombstones_written: usize,
    pub events_removed: usize,
    pub candidates_rejected: usize,
    /// Предложения, переведённые в `expired` причиной `source_deleted`.
    pub proposals_expired: usize,
}

/// Итог retention-прогона.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AmbientPurge {
    pub episodes_removed: usize,
    pub utterances_removed: usize,
    pub tombstones_written: usize,
    pub tombstones_removed: usize,
    pub events_removed: usize,
    pub candidates_rejected: usize,
    pub proposals_expired: usize,
    pub proposals_removed: usize,
}

impl AmbientEpisodeRecord {
    fn validate(&self) -> Result<(), AmbientStoreError> {
        validate_required("episode_id", &self.episode_id, MAX_ID_BYTES)?;
        validate_required("started_at", &self.started_at, MAX_TIMESTAMP_BYTES)?;
        if let Some(ended_at) = &self.ended_at {
            validate_required("ended_at", ended_at, MAX_TIMESTAMP_BYTES)?;
        }
        validate_required("engine_version", &self.engine_version, MAX_ID_BYTES)?;
        validate_required("model_id", &self.model_id, MAX_ID_BYTES)?;
        validate_required("expires_at", &self.expires_at, MAX_TIMESTAMP_BYTES)?;
        validate_non_negative("utterance_count", self.utterance_count)?;
        validate_non_negative("speech_ms", self.speech_ms)?;
        Ok(())
    }
}

impl AmbientUtteranceRecord {
    fn validate(&self) -> Result<(), AmbientStoreError> {
        validate_required("utterance_id", &self.utterance_id, MAX_ID_BYTES)?;
        validate_required("episode_id", &self.episode_id, MAX_ID_BYTES)?;
        validate_required("started_at", &self.started_at, MAX_TIMESTAMP_BYTES)?;
        validate_required("text", &self.text, MAX_TEXT_BYTES)?;
        validate_required("text_hash", &self.text_hash, MAX_HASH_BYTES)?;
        validate_required("language", &self.language, MAX_LANGUAGE_BYTES)?;
        validate_required("expires_at", &self.expires_at, MAX_TIMESTAMP_BYTES)?;
        validate_non_negative("sequence", self.sequence)?;
        validate_non_negative("duration_ms", self.duration_ms)?;
        if self.speaker != SPEAKER_UNVERIFIED {
            return Err(AmbientStoreError::InvalidSpeaker);
        }
        Ok(())
    }
}

fn validate_required(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), AmbientStoreError> {
    if value.trim().is_empty() {
        return Err(AmbientStoreError::Empty { field });
    }
    if value.len() > max {
        return Err(AmbientStoreError::Limit { field, max });
    }
    Ok(())
}

fn validate_non_negative(field: &'static str, value: i64) -> Result<(), AmbientStoreError> {
    if value < 0 {
        return Err(AmbientStoreError::Negative { field });
    }
    Ok(())
}

fn validate_reason(reason: &str) -> Result<(), AmbientStoreError> {
    if REASONS.contains(&reason) {
        Ok(())
    } else {
        Err(AmbientStoreError::InvalidReason)
    }
}

/// Идентификатор tombstone выводится из `episode_id` и времени удаления.
///
/// Случайный id здесь ничего не скрыл бы: `episode_id` и так колонка
/// tombstone. Детерминированность важнее — повторный purge того же эпизода
/// не плодит вторую запись, а `UNIQUE(episode_id, removed_at)` остаётся
/// согласован с первичным ключом.
fn tombstone_id(episode_id: &str, removed_at: &str) -> String {
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
        recalculate_counters(&transaction, &record.episode_id)?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn get_episode(
        connection: &Connection,
        episode_id: &str,
    ) -> Result<Option<AmbientEpisodeRecord>, AmbientStoreError> {
        validate_required("episode_id", episode_id, MAX_ID_BYTES)?;
        Ok(connection
            .query_row(
                &format!("SELECT {EPISODE_COLUMNS} FROM ambient_episodes WHERE episode_id = ?1"),
                params![episode_id],
                map_episode,
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
        let rows = statement.query_map(params![limit], map_episode)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

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
        let rows = statement.query_map(params![episode_id, limit], map_utterance)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn list_tombstones(
        connection: &Connection,
        limit: usize,
    ) -> Result<Vec<AmbientTombstoneRecord>, AmbientStoreError> {
        let limit = limit.min(MAX_ROWS_PER_READ) as i64;
        let mut statement = connection.prepare(&format!(
            "SELECT {TOMBSTONE_COLUMNS} FROM ambient_tombstones
             ORDER BY removed_at DESC, tombstone_id DESC LIMIT ?1"
        ))?;
        let rows = statement.query_map(params![limit], map_tombstone)?;
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
        remove_episode(
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
        let affected = affected_episodes(
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
            deletion.candidates_rejected += reject_candidates(&transaction, episode_id)?;
            let remaining = recalculate_counters(&transaction, episode_id)?;
            if remaining == 0 {
                remove_episode(
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

    pub fn get_proposal(
        connection: &Connection,
        proposal_id: &str,
    ) -> Result<Option<AmbientProposalRecord>, AmbientStoreError> {
        validate_required("proposal_id", proposal_id, MAX_ID_BYTES)?;
        Ok(connection
            .query_row(
                &format!("SELECT {PROPOSAL_COLUMNS} FROM ambient_proposals WHERE proposal_id = ?1"),
                params![proposal_id],
                map_proposal,
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
                map_proposal,
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
        let rows = statement.query_map(params![limit], map_proposal)?;
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

    pub fn list_mute_keys(connection: &Connection) -> Result<Vec<String>, AmbientStoreError> {
        let mut statement =
            connection.prepare("SELECT mute_key FROM ambient_proposal_mutes ORDER BY mute_key")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
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
        let partial = affected_episodes(
            &transaction,
            "SELECT DISTINCT episode_id FROM ambient_utterances WHERE expires_at <= ?1",
            params![now],
        )?;
        purge.utterances_removed += transaction.execute(
            "DELETE FROM ambient_utterances WHERE expires_at <= ?1",
            params![now],
        )?;
        for episode_id in &partial {
            recalculate_counters(&transaction, episode_id)?;
        }

        // 2. Истёкшие метаданные эпизода: удаляются с bounded tombstone.
        let expired = affected_episodes(
            &transaction,
            "SELECT episode_id FROM ambient_episodes WHERE expires_at <= ?1",
            params![now],
        )?;
        let mut deletion = AmbientDeletion::default();
        for episode_id in &expired {
            remove_episode(
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
        if table_exists(&transaction, "ambient_proposals")? {
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

/// Общий путь удаления эпизода: tombstone → кандидаты → journal → строки.
///
/// Порядок не косметический: tombstone фиксируется до того, как исчезает
/// первое высказывание, поэтому оборванная транзакция не может оставить
/// «удалено без следа».
fn remove_episode(
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
///
/// Обнулённая ссылка вместо этого оставила бы карточку висеть в очереди без
/// источника: пользователь видел бы предложение по речи, которой в базе уже
/// нет.
fn expire_proposals_of_episode(
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
///
/// `supersession_reason` — единственная колонка причины у `memory_entries`;
/// заводить ради ambient ещё одну означало бы менять схему памяти из этапа
/// про хранение транскриптов. Подтверждённая пользователем запись не
/// трогается: её содержимое больше не принадлежит источнику.
fn reject_candidates(
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

fn table_exists(
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
fn recalculate_counters(
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

fn affected_episodes(
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

fn map_episode(row: &rusqlite::Row<'_>) -> rusqlite::Result<AmbientEpisodeRecord> {
    let stored: String = row.get(7)?;
    let extraction_state = ExtractionState::parse(&stored).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(AmbientStoreError::Empty {
                field: "extraction_state",
            }),
        )
    })?;
    Ok(AmbientEpisodeRecord {
        episode_id: row.get(0)?,
        started_at: row.get(1)?,
        ended_at: row.get(2)?,
        utterance_count: row.get(3)?,
        speech_ms: row.get(4)?,
        engine_version: row.get(5)?,
        model_id: row.get(6)?,
        extraction_state,
        expires_at: row.get(8)?,
    })
}

fn map_utterance(row: &rusqlite::Row<'_>) -> rusqlite::Result<AmbientUtteranceRecord> {
    Ok(AmbientUtteranceRecord {
        utterance_id: row.get(0)?,
        episode_id: row.get(1)?,
        sequence: row.get(2)?,
        started_at: row.get(3)?,
        duration_ms: row.get(4)?,
        text: row.get(5)?,
        text_hash: row.get(6)?,
        language: row.get(7)?,
        avg_logprob: row.get(8)?,
        speaker: row.get(9)?,
        redacted: row.get::<_, i64>(10)? != 0,
        expires_at: row.get(11)?,
    })
}

fn map_proposal(row: &rusqlite::Row<'_>) -> rusqlite::Result<AmbientProposalRecord> {
    let stored_kind: String = row.get(3)?;
    let kind = ProposalKind::parse(&stored_kind).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(AmbientStoreError::Empty { field: "kind" }),
        )
    })?;
    let stored_state: String = row.get(14)?;
    let state = ProposalState::parse(&stored_state).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            14,
            rusqlite::types::Type::Text,
            Box::new(AmbientStoreError::Empty { field: "state" }),
        )
    })?;
    Ok(AmbientProposalRecord {
        proposal_id: row.get(0)?,
        proposal_key: row.get(1)?,
        mute_key: row.get(2)?,
        kind,
        subject_key: row.get(4)?,
        subject: row.get(5)?,
        title: row.get(6)?,
        source_episode_id: row.get(7)?,
        source_deleted_at: row.get(8)?,
        source_deleted_reason: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        expires_at: row.get(12)?,
        occurrences: row.get(13)?,
        state,
        accepted_task_id: row.get(15)?,
        idempotency_key: row.get(16)?,
    })
}

fn map_tombstone(row: &rusqlite::Row<'_>) -> rusqlite::Result<AmbientTombstoneRecord> {
    Ok(AmbientTombstoneRecord {
        tombstone_id: row.get(0)?,
        episode_id: row.get(1)?,
        removed_at: row.get(2)?,
        reason: row.get(3)?,
        utterance_count: row.get(4)?,
        expires_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Схема, эквивалентная миграции v25, плюс те таблицы Core, к которым
    /// удаление обязано прикоснуться: durable journal и память.
    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE ambient_episodes (
                    episode_id TEXT PRIMARY KEY NOT NULL,
                    started_at TEXT NOT NULL,
                    ended_at TEXT,
                    utterance_count INTEGER NOT NULL,
                    speech_ms INTEGER NOT NULL,
                    engine_version TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    extraction_state TEXT NOT NULL CHECK(extraction_state IN
                        ('disabled','pending','done','failed')),
                    expires_at TEXT NOT NULL
                 );
                 CREATE TABLE ambient_utterances (
                    utterance_id TEXT PRIMARY KEY NOT NULL,
                    episode_id TEXT NOT NULL
                        REFERENCES ambient_episodes(episode_id) ON DELETE CASCADE,
                    sequence INTEGER NOT NULL,
                    started_at TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    text TEXT NOT NULL,
                    text_hash TEXT NOT NULL,
                    language TEXT NOT NULL,
                    avg_logprob REAL NOT NULL,
                    speaker TEXT NOT NULL,
                    redacted INTEGER NOT NULL DEFAULT 0,
                    expires_at TEXT NOT NULL,
                    UNIQUE(episode_id, sequence)
                 );
                 CREATE TABLE ambient_tombstones (
                    tombstone_id TEXT PRIMARY KEY NOT NULL,
                    episode_id TEXT NOT NULL,
                    removed_at TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    utterance_count INTEGER NOT NULL,
                    expires_at TEXT NOT NULL,
                    UNIQUE(episode_id, removed_at)
                 );
                 CREATE TABLE ambient_proposals (
                    proposal_id TEXT PRIMARY KEY NOT NULL,
                    proposal_key TEXT NOT NULL UNIQUE,
                    mute_key TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK(kind IN ('suggestion','reminder')),
                    subject_key TEXT NOT NULL,
                    subject TEXT NOT NULL,
                    title TEXT NOT NULL,
                    source_episode_id TEXT
                        REFERENCES ambient_episodes(episode_id) ON DELETE SET NULL,
                    source_deleted_at TEXT,
                    source_deleted_reason TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL,
                    occurrences INTEGER NOT NULL DEFAULT 1,
                    state TEXT NOT NULL CHECK(state IN
                        ('proposed','accepted','declined','muted','expired')),
                    accepted_task_id TEXT,
                    idempotency_key TEXT,
                    CHECK((source_deleted_at IS NULL AND source_deleted_reason IS NULL)
                       OR (source_deleted_at IS NOT NULL AND source_deleted_reason IS NOT NULL))
                 );
                 CREATE UNIQUE INDEX idx_ambient_proposal_idempotency
                    ON ambient_proposals(idempotency_key) WHERE idempotency_key IS NOT NULL;
                 CREATE TABLE ambient_proposal_mutes (
                    mute_key TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL CHECK(kind IN ('suggestion','reminder')),
                    subject_key TEXT NOT NULL,
                    muted_at TEXT NOT NULL
                 );
                 CREATE TABLE ambient_proactivity_counters (
                    profile_id TEXT PRIMARY KEY NOT NULL,
                    hour_started_at_ms INTEGER NOT NULL,
                    hour_count INTEGER NOT NULL,
                    day_started_at_ms INTEGER NOT NULL,
                    day_count INTEGER NOT NULL,
                    last_proposed_at_ms INTEGER
                 );
                 CREATE TABLE events (
                    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE TABLE memory_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    confirmation_state TEXT NOT NULL,
                    supersession_reason TEXT,
                    provenance_source_id TEXT
                 );",
            )
            .expect("ambient schema installs");
    }

    fn open() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database");
        schema(&connection);
        connection
    }

    fn episode(id: &str, started_at: &str, expires_at: &str) -> AmbientEpisodeRecord {
        AmbientEpisodeRecord {
            episode_id: id.to_owned(),
            started_at: started_at.to_owned(),
            ended_at: None,
            utterance_count: 0,
            speech_ms: 0,
            engine_version: "whisper-base-q5_1".to_owned(),
            model_id: "base-q5_1".to_owned(),
            extraction_state: ExtractionState::Pending,
            expires_at: expires_at.to_owned(),
        }
    }

    fn utterance(
        id: &str,
        episode_id: &str,
        sequence: i64,
        started_at: &str,
        text: &str,
        expires_at: &str,
    ) -> AmbientUtteranceRecord {
        AmbientUtteranceRecord {
            utterance_id: id.to_owned(),
            episode_id: episode_id.to_owned(),
            sequence,
            started_at: started_at.to_owned(),
            duration_ms: 1_000,
            text: text.to_owned(),
            text_hash: format!("hash-{text}"),
            language: "ru".to_owned(),
            avg_logprob: -0.25,
            speaker: SPEAKER_UNVERIFIED.to_owned(),
            redacted: false,
            expires_at: expires_at.to_owned(),
        }
    }

    fn append_event(connection: &Connection, task_id: &str, event_type: &str, created_at: &str) {
        connection
            .execute(
                "INSERT INTO events(task_id, event_type, payload, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![task_id, event_type, Vec::<u8>::new(), created_at],
            )
            .expect("event appends");
    }

    fn candidate(connection: &Connection, id: &str, source: &str, state: &str) {
        connection
            .execute(
                "INSERT INTO memory_entries(id, confirmation_state, provenance_source_id)
                 VALUES (?1, ?2, ?3)",
                params![id, state, source],
            )
            .expect("candidate inserts");
    }

    fn candidate_state(connection: &Connection, id: &str) -> (String, Option<String>) {
        connection
            .query_row(
                "SELECT confirmation_state, supersession_reason FROM memory_entries WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("candidate exists")
    }

    fn counters(connection: &Connection, episode_id: &str) -> (i64, i64) {
        connection
            .query_row(
                "SELECT utterance_count, speech_ms FROM ambient_episodes WHERE episode_id = ?1",
                params![episode_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("episode exists")
    }

    fn event_types(connection: &Connection) -> Vec<String> {
        let mut statement = connection
            .prepare("SELECT event_type FROM events ORDER BY sequence_id")
            .expect("statement");
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query");
        rows.map(|row| row.expect("row")).collect()
    }

    #[test]
    fn utterances_round_trip_and_keep_episode_counters_in_step() {
        let connection = open();
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-20T10:00:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        for (index, text) in ["первое", "второе"].into_iter().enumerate() {
            assert!(AmbientStoreSql::insert_utterance(
                &connection,
                &utterance(
                    &format!("u-{index}"),
                    "ep-1",
                    index as i64,
                    &format!("2026-08-20T10:0{index}:00.000Z"),
                    text,
                    "2026-08-27T10:00:00.000Z",
                ),
                "2026-08-20T09:59:00.000Z",
            )
            .expect("utterance inserts"));
        }
        assert_eq!(counters(&connection, "ep-1"), (2, 2_000));
        assert!(
            AmbientStoreSql::close_episode(&connection, "ep-1", "2026-08-20T10:05:00.000Z")
                .expect("episode closes")
        );
        let stored = AmbientStoreSql::get_episode(&connection, "ep-1")
            .expect("read")
            .expect("episode exists");
        assert_eq!(stored.ended_at.as_deref(), Some("2026-08-20T10:05:00.000Z"));
        assert_eq!(stored.extraction_state, ExtractionState::Pending);
        assert!(
            AmbientStoreSql::set_extraction_state(&connection, "ep-1", ExtractionState::Done)
                .expect("state updates")
        );
        let texts: Vec<String> = AmbientStoreSql::list_utterances(&connection, "ep-1", 100)
            .expect("read")
            .into_iter()
            .map(|record| record.text)
            .collect();
        assert_eq!(texts, vec!["первое".to_owned(), "второе".to_owned()]);
        assert_eq!(
            AmbientStoreSql::list_episodes(&connection, 100)
                .expect("read")
                .len(),
            1
        );
    }

    #[test]
    fn duplicate_text_inside_the_window_is_dropped_but_accepted_after_it() {
        let connection = open();
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-20T10:00:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        assert!(AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                "u-0",
                "ep-1",
                0,
                "2026-08-20T10:00:00.000Z",
                "повтор",
                "2026-08-27T10:00:00.000Z",
            ),
            "2026-08-20T09:59:00.000Z",
        )
        .expect("insert"));
        assert!(!AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                "u-1",
                "ep-1",
                1,
                "2026-08-20T10:00:30.000Z",
                "повтор",
                "2026-08-27T10:00:00.000Z",
            ),
            "2026-08-20T09:59:30.000Z",
        )
        .expect("insert"));
        assert!(AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                "u-2",
                "ep-1",
                2,
                "2026-08-20T11:00:00.000Z",
                "повтор",
                "2026-08-27T10:00:00.000Z",
            ),
            "2026-08-20T10:59:00.000Z",
        )
        .expect("insert"));
        assert_eq!(counters(&connection, "ep-1"), (2, 2_000));
    }

    #[test]
    fn a_future_duplicate_does_not_hide_an_older_out_of_order_utterance() {
        let connection = open();
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-20T10:00:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        let future = utterance(
            "u-future",
            "ep-1",
            1,
            "2026-08-20T10:05:00.000Z",
            "повтор",
            "2026-08-27T10:00:00.000Z",
        );
        assert!(AmbientStoreSql::insert_utterance(
            &connection,
            &future,
            "2026-08-20T09:00:00.000Z"
        )
        .expect("future insert"));
        let older = utterance(
            "u-older",
            "ep-1",
            0,
            "2026-08-20T10:04:00.000Z",
            "повтор",
            "2026-08-27T10:00:00.000Z",
        );
        assert!(
            AmbientStoreSql::insert_utterance(&connection, &older, "2026-08-20T10:00:00.000Z")
                .expect("older insert")
        );
    }

    #[test]
    fn v1_stores_no_speaker_identity() {
        let connection = open();
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-20T10:00:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        let mut record = utterance(
            "u-0",
            "ep-1",
            0,
            "2026-08-20T10:00:00.000Z",
            "фраза",
            "2026-08-27T10:00:00.000Z",
        );
        record.speaker = "роман".to_owned();
        assert!(matches!(
            AmbientStoreSql::insert_utterance(&connection, &record, "2026-08-20T09:00:00.000Z"),
            Err(AmbientStoreError::InvalidSpeaker)
        ));
    }

    #[test]
    fn store_owns_episode_counters_and_rejects_prefilled_values() {
        let connection = open();
        let mut record = episode(
            "ep-1",
            "2026-08-20T10:00:00.000Z",
            "2026-09-19T10:00:00.000Z",
        );
        record.utterance_count = 1;
        assert!(matches!(
            AmbientStoreSql::open_episode(&connection, &record),
            Err(AmbientStoreError::InvalidInitialCounters)
        ));
    }

    #[test]
    fn deleting_an_episode_leaves_a_tombstone_and_no_orphans() {
        let connection = open();
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-20T10:00:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                "u-0",
                "ep-1",
                0,
                "2026-08-20T10:00:00.000Z",
                "фраза",
                "2026-08-27T10:00:00.000Z",
            ),
            "2026-08-20T09:00:00.000Z",
        )
        .expect("insert");
        append_event(
            &connection,
            "ep-1",
            "ambient.transcript",
            "2026-08-20T10:00:01.000Z",
        );
        append_event(
            &connection,
            "task-7",
            "task.started",
            "2026-08-20T10:00:02.000Z",
        );
        candidate(&connection, "mem-1", "ep-1", "pending_confirmation");
        candidate(&connection, "mem-2", "ep-1", "confirmed");

        let deletion = AmbientStoreSql::delete_episode(
            &connection,
            "ep-1",
            REASON_USER_REQUEST,
            "2026-08-20T12:00:00.000Z",
            "2026-09-19T12:00:00.000Z",
        )
        .expect("episode deletes");
        assert_eq!(deletion.episodes_removed, 1);
        assert_eq!(deletion.utterances_removed, 1);
        assert_eq!(deletion.tombstones_written, 1);
        assert_eq!(deletion.events_removed, 1);
        assert_eq!(deletion.candidates_rejected, 1);

        let tombstones = AmbientStoreSql::list_tombstones(&connection, 10).expect("read");
        assert_eq!(tombstones.len(), 1);
        assert_eq!(tombstones[0].episode_id, "ep-1");
        assert_eq!(tombstones[0].utterance_count, 1);
        assert_eq!(tombstones[0].reason, REASON_USER_REQUEST);

        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM ambient_utterances", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(remaining, 0);
        assert_eq!(
            event_types(&connection),
            vec!["task.started".to_owned()],
            "ambient-строки уходят, чужие события остаются"
        );
        assert_eq!(
            candidate_state(&connection, "mem-1"),
            (
                "rejected".to_owned(),
                Some(CANDIDATE_REJECTION_REASON.to_owned())
            )
        );
        assert_eq!(candidate_state(&connection, "mem-2").0, "confirmed");
    }

    #[test]
    fn unknown_removal_reason_never_reaches_a_tombstone() {
        let connection = open();
        assert!(matches!(
            AmbientStoreSql::delete_episode(
                &connection,
                "ep-1",
                "потому что",
                "2026-08-20T12:00:00.000Z",
                "2026-09-19T12:00:00.000Z",
            ),
            Err(AmbientStoreError::InvalidReason)
        ));
    }

    #[test]
    fn forget_window_spares_the_episode_that_only_crosses_its_border() {
        let connection = open();
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-20T09:50:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-2",
                "2026-08-20T10:30:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        // ep-1 наполовину внутри окна, ep-2 — целиком.
        for (id, episode_id, sequence, started_at, text) in [
            ("u-0", "ep-1", 0, "2026-08-20T09:51:00.000Z", "до окна"),
            ("u-1", "ep-1", 1, "2026-08-20T10:31:00.000Z", "в окне"),
            ("u-2", "ep-2", 0, "2026-08-20T10:32:00.000Z", "тоже в окне"),
        ] {
            AmbientStoreSql::insert_utterance(
                &connection,
                &utterance(
                    id,
                    episode_id,
                    sequence,
                    started_at,
                    text,
                    "2026-08-27T10:00:00.000Z",
                ),
                "2026-08-20T09:00:00.000Z",
            )
            .expect("insert");
        }
        candidate(&connection, "mem-1", "ep-1", "candidate");
        candidate(&connection, "mem-2", "ep-2", "candidate");
        append_event(
            &connection,
            "ep-2",
            "ambient.transcript",
            "2026-08-20T10:32:01.000Z",
        );
        append_event(
            &connection,
            "ambient-session",
            "ambient.state",
            "2026-08-20T10:31:00.000Z",
        );
        append_event(
            &connection,
            "ep-1",
            "ambient.transcript",
            "2026-08-20T09:51:01.000Z",
        );

        let deletion = AmbientStoreSql::forget_window(
            &connection,
            "2026-08-20T10:30:00.000Z",
            "2026-08-20T11:00:00.000Z",
            "2026-08-20T11:00:00.000Z",
            "2026-09-19T11:00:00.000Z",
        )
        .expect("window forgets");
        assert_eq!(deletion.utterances_removed, 2);
        assert_eq!(deletion.episodes_removed, 1, "пустой эпизод уходит целиком");
        assert_eq!(deletion.candidates_rejected, 2);
        assert_eq!(deletion.events_removed, 3);

        assert!(AmbientStoreSql::get_episode(&connection, "ep-1")
            .expect("read")
            .is_some());
        assert!(AmbientStoreSql::get_episode(&connection, "ep-2")
            .expect("read")
            .is_none());
        assert_eq!(counters(&connection, "ep-1"), (1, 1_000));
        assert_eq!(candidate_state(&connection, "mem-1").0, "rejected");
        assert_eq!(candidate_state(&connection, "mem-2").0, "rejected");
        assert_eq!(
            event_types(&connection),
            Vec::<String>::new(),
            "ambient-строки затронутых эпизодов не переживают forget"
        );
    }

    #[test]
    fn a_journal_reader_walks_over_the_gap_left_by_forget() {
        let connection = open();
        for index in 0..5 {
            let event_type = if index % 2 == 0 {
                "ambient.state"
            } else {
                "task.progress"
            };
            append_event(
                &connection,
                "ambient-session",
                event_type,
                &format!("2026-08-20T10:0{index}:00.000Z"),
            );
        }
        AmbientStoreSql::forget_window(
            &connection,
            "2026-08-20T10:00:00.000Z",
            "2026-08-20T10:04:00.000Z",
            "2026-08-20T10:05:00.000Z",
            "2026-09-19T10:05:00.000Z",
        )
        .expect("window forgets");
        let mut cursor = 0_i64;
        let mut seen = Vec::new();
        loop {
            let next: Option<(i64, String)> = connection
                .query_row(
                    "SELECT sequence_id, event_type FROM events
                     WHERE sequence_id > ?1 ORDER BY sequence_id LIMIT 1",
                    params![cursor],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .expect("cursor read");
            let Some((sequence_id, event_type)) = next else {
                break;
            };
            assert!(sequence_id > cursor, "курсор монотонен");
            cursor = sequence_id;
            seen.push(event_type);
        }
        assert_eq!(seen, vec!["task.progress".to_owned(); 2]);
        assert_eq!(cursor, 4, "дырка в нумерации не останавливает чтение");
    }

    #[test]
    fn retention_removes_exactly_what_expired_and_lets_tombstones_expire_too() {
        let connection = open();
        // ep-1: текст истёк, метаданные ещё живут.
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-01T10:00:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                "u-0",
                "ep-1",
                0,
                "2026-08-01T10:00:00.000Z",
                "истёкшее",
                "2026-08-08T10:00:00.000Z",
            ),
            "2026-08-01T09:00:00.000Z",
        )
        .expect("insert");
        AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                "u-1",
                "ep-1",
                1,
                "2026-08-20T10:00:00.000Z",
                "свежее",
                "2026-08-27T10:00:00.000Z",
            ),
            "2026-08-20T09:00:00.000Z",
        )
        .expect("insert");
        // ep-2: истекли и метаданные.
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-2",
                "2026-07-01T10:00:00.000Z",
                "2026-07-31T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        candidate(&connection, "mem-2", "ep-2", "candidate");
        append_event(
            &connection,
            "ep-2",
            "ambient.transcript",
            "2026-07-01T10:00:01.000Z",
        );
        append_event(
            &connection,
            "ambient-session",
            "ambient.state",
            "2026-08-20T09:00:00.000Z",
        );
        append_event(
            &connection,
            "task-1",
            "task.started",
            "2026-07-01T09:00:00.000Z",
        );
        // Просроченный tombstone от прошлого удаления.
        connection
            .execute(
                "INSERT INTO ambient_tombstones
                 (tombstone_id, episode_id, removed_at, reason, utterance_count, expires_at)
                 VALUES ('old', 'ep-0', '2026-06-01T10:00:00.000Z', 'retention', 3,
                         '2026-07-01T10:00:00.000Z')",
                [],
            )
            .expect("stale tombstone");

        let purge = AmbientStoreSql::purge_expired(
            &connection,
            "2026-08-20T12:00:00.000Z",
            "2026-09-19T12:00:00.000Z",
            "2026-07-21T12:00:00.000Z",
        )
        .expect("purge runs");
        assert_eq!(purge.utterances_removed, 1);
        assert_eq!(purge.episodes_removed, 1);
        assert_eq!(purge.tombstones_written, 1);
        assert_eq!(purge.tombstones_removed, 1);
        assert_eq!(purge.candidates_rejected, 1);
        assert_eq!(purge.events_removed, 1);

        assert_eq!(counters(&connection, "ep-1"), (1, 1_000));
        assert!(AmbientStoreSql::get_episode(&connection, "ep-2")
            .expect("read")
            .is_none());
        let tombstones = AmbientStoreSql::list_tombstones(&connection, 10).expect("read");
        assert_eq!(tombstones.len(), 1);
        assert_eq!(tombstones[0].episode_id, "ep-2");
        assert_eq!(candidate_state(&connection, "mem-2").0, "rejected");
        assert_eq!(
            event_types(&connection),
            vec!["ambient.state".to_owned(), "task.started".to_owned()],
            "свежая ambient-строка и чужие события остаются"
        );

        // Повторный прогон на тех же данных ничего не меняет.
        let repeat = AmbientStoreSql::purge_expired(
            &connection,
            "2026-08-20T12:00:00.000Z",
            "2026-09-19T12:00:00.000Z",
            "2026-07-21T12:00:00.000Z",
        )
        .expect("purge repeats");
        assert_eq!(repeat, AmbientPurge::default());
    }

    #[test]
    fn reads_stay_bounded_even_when_the_caller_asks_for_everything() {
        let connection = open();
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-20T10:00:00.000Z",
                "2026-09-19T10:00:00.000Z",
            ),
        )
        .expect("episode opens");
        for index in 0..(MAX_ROWS_PER_READ + 10) {
            AmbientStoreSql::insert_utterance(
                &connection,
                &utterance(
                    &format!("u-{index}"),
                    "ep-1",
                    index as i64,
                    "2026-08-20T10:00:00.000Z",
                    &format!("фраза {index}"),
                    "2026-08-27T10:00:00.000Z",
                ),
                "2026-08-20T09:00:00.000Z",
            )
            .expect("insert");
        }
        assert_eq!(
            AmbientStoreSql::list_utterances(&connection, "ep-1", usize::MAX)
                .expect("read")
                .len(),
            MAX_ROWS_PER_READ
        );
    }

    fn proposal(
        id: &str,
        key: &str,
        mute_key: &str,
        episode_id: Option<&str>,
        created_at: &str,
        expires_at: &str,
    ) -> AmbientProposalRecord {
        AmbientProposalRecord {
            proposal_id: id.to_owned(),
            proposal_key: key.to_owned(),
            mute_key: mute_key.to_owned(),
            kind: ProposalKind::Reminder,
            subject_key: "hleb".to_owned(),
            subject: "хлеб".to_owned(),
            title: "Напомнить купить хлеб".to_owned(),
            source_episode_id: episode_id.map(str::to_owned),
            source_deleted_at: None,
            source_deleted_reason: None,
            created_at: created_at.to_owned(),
            updated_at: created_at.to_owned(),
            expires_at: expires_at.to_owned(),
            occurrences: 1,
            state: ProposalState::Proposed,
            accepted_task_id: None,
            idempotency_key: None,
        }
    }

    /// Повтор той же темы в той же временной корзине поднимает счётчик
    /// существующей карточки. Второй карточки не появляется: иначе за час
    /// разговора об одном и том же очередь заполнилась бы копиями.
    #[test]
    fn a_duplicate_proposal_raises_a_counter_instead_of_creating_a_second_card() {
        let connection = open();
        let record = proposal(
            "p-1",
            "reminder:hleb:470000",
            "reminder:hleb",
            None,
            "2026-08-21T10:00:00.000Z",
            "2026-08-22T10:00:00.000Z",
        );
        assert_eq!(
            AmbientStoreSql::record_proposal(&connection, &record).unwrap(),
            ProposalInsert::Created
        );
        let again = AmbientProposalRecord {
            proposal_id: "p-2".to_owned(),
            updated_at: "2026-08-21T10:20:00.000Z".to_owned(),
            ..record.clone()
        };
        assert_eq!(
            AmbientStoreSql::record_proposal(&connection, &again).unwrap(),
            ProposalInsert::Duplicate {
                proposal_id: "p-1".to_owned(),
                occurrences: 2,
            }
        );
        let open_cards = AmbientStoreSql::list_open_proposals(&connection, 10).unwrap();
        assert_eq!(open_cards.len(), 1);
        assert_eq!(open_cards[0].occurrences, 2);
        assert!(AmbientStoreSql::get_proposal(&connection, "p-2")
            .unwrap()
            .is_none());
    }

    /// Mute идёт по ключу без времени, поэтому он продолжает действовать и
    /// после смены временной корзины — то есть глушит предложение, чей
    /// `proposal_key` уже другой.
    #[test]
    fn a_mute_outlives_the_time_bucket_it_was_set_in() {
        let connection = open();
        AmbientStoreSql::mute_subject(
            &connection,
            "reminder:hleb",
            ProposalKind::Reminder,
            "hleb",
            "2026-08-21T10:00:00.000Z",
        )
        .unwrap();
        let later = proposal(
            "p-9",
            "reminder:hleb:999999",
            "reminder:hleb",
            None,
            "2026-09-01T10:00:00.000Z",
            "2026-09-02T10:00:00.000Z",
        );
        assert_eq!(
            AmbientStoreSql::record_proposal(&connection, &later).unwrap(),
            ProposalInsert::Muted
        );
        assert!(AmbientStoreSql::list_open_proposals(&connection, 10)
            .unwrap()
            .is_empty());
        assert_eq!(
            AmbientStoreSql::list_mute_keys(&connection).unwrap(),
            vec!["reminder:hleb".to_owned()]
        );
    }

    /// Истёкшее предложение освобождает тему: новая корзина времени даёт
    /// новый `proposal_key`, и `UNIQUE` ему не мешает.
    #[test]
    fn a_new_proposal_after_expiry_does_not_hit_the_unique_key() {
        let connection = open();
        let first = proposal(
            "p-1",
            "reminder:hleb:470000",
            "reminder:hleb",
            None,
            "2026-08-21T10:00:00.000Z",
            "2026-08-22T10:00:00.000Z",
        );
        AmbientStoreSql::record_proposal(&connection, &first).unwrap();
        assert_eq!(
            AmbientStoreSql::expire_stale_proposals(&connection, "2026-08-22T10:00:00.000Z")
                .unwrap(),
            1
        );
        assert!(AmbientStoreSql::list_open_proposals(&connection, 10)
            .unwrap()
            .is_empty());
        let second = proposal(
            "p-2",
            "reminder:hleb:470024",
            "reminder:hleb",
            None,
            "2026-08-22T11:00:00.000Z",
            "2026-08-23T11:00:00.000Z",
        );
        assert_eq!(
            AmbientStoreSql::record_proposal(&connection, &second).unwrap(),
            ProposalInsert::Created
        );
    }

    /// Удаление эпизода-источника обязано пометить его предложения истёкшими
    /// с причиной `source_deleted`, а не оставить их с обнулённой ссылкой.
    #[test]
    fn deleting_the_source_episode_expires_its_proposals_instead_of_orphaning_them() {
        let connection = open();
        AmbientStoreSql::open_episode(
            &connection,
            &episode(
                "ep-1",
                "2026-08-21T10:00:00.000Z",
                "2026-09-20T10:00:00.000Z",
            ),
        )
        .unwrap();
        AmbientStoreSql::record_proposal(
            &connection,
            &proposal(
                "p-1",
                "reminder:hleb:470000",
                "reminder:hleb",
                Some("ep-1"),
                "2026-08-21T10:00:00.000Z",
                "2026-08-22T10:00:00.000Z",
            ),
        )
        .unwrap();
        let deletion = AmbientStoreSql::delete_episode(
            &connection,
            "ep-1",
            REASON_USER_REQUEST,
            "2026-08-21T11:00:00.000Z",
            "2026-09-20T11:00:00.000Z",
        )
        .unwrap();
        assert_eq!(deletion.proposals_expired, 1);
        let stored = AmbientStoreSql::get_proposal(&connection, "p-1")
            .unwrap()
            .expect("предложение переживает удаление источника как след, а не как карточка");
        assert_eq!(stored.state, ProposalState::Expired);
        assert_eq!(
            stored.source_deleted_reason.as_deref(),
            Some(CANDIDATE_REJECTION_REASON)
        );
        assert_eq!(
            stored.source_deleted_at.as_deref(),
            Some("2026-08-21T11:00:00.000Z")
        );
        assert!(AmbientStoreSql::list_open_proposals(&connection, 10)
            .unwrap()
            .is_empty());
    }

    /// Второй `resolve` того же предложения не проходит: терминальное
    /// состояние не переигрывается, и повторный клик не создаёт вторую задачу.
    #[test]
    fn a_resolved_proposal_never_moves_again() {
        let connection = open();
        AmbientStoreSql::record_proposal(
            &connection,
            &proposal(
                "p-1",
                "reminder:hleb:470000",
                "reminder:hleb",
                None,
                "2026-08-21T10:00:00.000Z",
                "2026-08-22T10:00:00.000Z",
            ),
        )
        .unwrap();
        assert!(AmbientStoreSql::resolve_proposal(
            &connection,
            "p-1",
            ProposalState::Accepted,
            "2026-08-21T10:05:00.000Z",
            Some("task-1"),
            Some("idem-1"),
        )
        .unwrap());
        assert!(!AmbientStoreSql::resolve_proposal(
            &connection,
            "p-1",
            ProposalState::Declined,
            "2026-08-21T10:06:00.000Z",
            None,
            Some("idem-2"),
        )
        .unwrap());
        let replay = AmbientStoreSql::find_proposal_by_idempotency(&connection, "idem-1")
            .unwrap()
            .expect("повтор с тем же ключом находит первое решение");
        assert_eq!(replay.state, ProposalState::Accepted);
        assert_eq!(replay.accepted_task_id.as_deref(), Some("task-1"));
    }

    /// Счётчики окна переживают рестарт: без строки в таблице перезапуск Core
    /// обнулял бы часовой потолок.
    #[test]
    fn proactivity_counters_round_trip_through_their_row() {
        let connection = open();
        assert_eq!(AmbientStoreSql::load_counters(&connection).unwrap(), None);
        let row = ProactivityCountersRow {
            hour_started_at_ms: 1_770_000_000_000,
            hour_count: 2,
            day_started_at_ms: 1_769_990_000_000,
            day_count: 5,
            last_proposed_at_ms: Some(1_770_000_500_000),
        };
        AmbientStoreSql::save_counters(&connection, row).unwrap();
        assert_eq!(
            AmbientStoreSql::load_counters(&connection).unwrap(),
            Some(row)
        );
        let updated = ProactivityCountersRow {
            hour_count: 3,
            ..row
        };
        AmbientStoreSql::save_counters(&connection, updated).unwrap();
        assert_eq!(
            AmbientStoreSql::load_counters(&connection).unwrap(),
            Some(updated)
        );
    }

    /// Половинчатая пара `source_deleted_*` отвергается до SQL: «источник
    /// удалён неизвестно когда» — не состояние, а порча данных.
    #[test]
    fn a_half_filled_source_deletion_is_rejected() {
        let connection = open();
        let broken = AmbientProposalRecord {
            source_deleted_reason: Some(CANDIDATE_REJECTION_REASON.to_owned()),
            ..proposal(
                "p-1",
                "reminder:hleb:470000",
                "reminder:hleb",
                None,
                "2026-08-21T10:00:00.000Z",
                "2026-08-22T10:00:00.000Z",
            )
        };
        assert!(matches!(
            AmbientStoreSql::record_proposal(&connection, &broken),
            Err(AmbientStoreError::InvalidSourceDeletion)
        ));
    }
}
