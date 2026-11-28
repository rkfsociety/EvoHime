//! Ambient-часть durable-журнала ядра (план 04.2): эпизоды, высказывания,
//! предложения, tombstone и retention-прогоны.
//!
//! Сам SQL живёт в `evohime_local_storage::ambient_store`; этот модуль —
//! persistence-порт ядра: типы записей, ошибки в терминах закрытого набора
//! кодов и потоковые обёртки над `EventJournal`. Бизнес-решения (решение о
//! предложении, политика) — в контракте и `evohime-ambient`-сервисе.

use evohime_listener_contract::{
    cutoff_at, event_task_id, expires_at, timestamp_ms, AmbientErrorCode, AmbientLogEvent,
    ExtractionState, ListeningState, ProposalKind, ProposalState, EPISODE_METADATA_RETENTION_DAYS,
    EVENT_RETENTION_DAYS, MAX_DEDUP_WINDOW_MS, MAX_RETENTION_DAYS, SESSION_TASK_ID,
    TOMBSTONE_RETENTION_DAYS,
};
use evohime_local_storage::ambient_store::{
    AmbientDeletion, AmbientEpisodeRecord, AmbientProposalRecord, AmbientPurge, AmbientStoreError,
    AmbientStoreSql, AmbientTombstoneRecord, AmbientUtteranceRecord, ProactivityCountersRow,
    ProposalInsert, REASON_USER_REQUEST, SPEAKER_UNVERIFIED,
};

/// Одно распознанное высказывание в том виде, в каком его отдаёт листенер.
///
/// `text_hash` в структуре нет: его считает Core при записи. Хеш живёт
/// только в таблице и никогда не покидает хранилище — короткую фразу
/// перебирают по хешу за секунды, поэтому хеш приравнивается к содержимому.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientUtteranceInput {
    /// Stable identifier assigned to this recognition result.
    pub utterance_id: String,
    /// Parent listening episode identifier.
    pub episode_id: String,
    /// Monotonic order of this utterance within the episode.
    pub sequence: i64,
    /// Recognition start time in Unix milliseconds.
    pub started_at_ms: u64,
    /// Audio duration represented by the utterance.
    pub duration_ms: i64,
    /// Recognized text after any listener-side redaction.
    pub text: String,
    /// Language tag reported by the recognition engine.
    pub language: String,
    /// Average model log probability for the recognition result.
    pub avg_logprob: f64,
    /// Whether the text has passed through redaction.
    pub redacted: bool,
}

fn text_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Ошибка стора в терминах закрытого набора кодов 04.1: отказ SQLite —
/// `STORAGE_FAILED`, всё остальное — нарушение контракта вызывающим.
fn store_error_code(error: &AmbientStoreError) -> AmbientErrorCode {
    match error {
        AmbientStoreError::Sqlite(_) => AmbientErrorCode::StorageFailed,
        _ => AmbientErrorCode::InvalidArgument,
    }
}

impl crate::EventJournal {
    /// Открывает эпизод. Срок жизни метаданных фиксируется здесь, а не
    /// листенером: политика хранения — забота Core.
    pub async fn open_ambient_episode(
        &self,
        episode_id: &str,
        engine_version: &str,
        model_id: &str,
        extraction_state: ExtractionState,
        now_ms: u64,
    ) -> Result<(), AmbientErrorCode> {
        let record = AmbientEpisodeRecord {
            episode_id: episode_id.to_owned(),
            started_at: timestamp_ms(now_ms),
            ended_at: None,
            utterance_count: 0,
            speech_ms: 0,
            engine_version: engine_version.to_owned(),
            model_id: model_id.to_owned(),
            extraction_state,
            expires_at: expires_at(now_ms, EPISODE_METADATA_RETENTION_DAYS),
        };
        let database = self.database.lock().await;
        AmbientStoreSql::open_episode(database.connection(), &record)
            .map_err(|error| store_error_code(&error))
    }

    /// Closes an episode and records its end time; returns false if it was absent.
    pub async fn close_ambient_episode(
        &self,
        episode_id: &str,
        now_ms: u64,
    ) -> Result<bool, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::close_episode(database.connection(), episode_id, &timestamp_ms(now_ms))
            .map_err(|error| store_error_code(&error))
    }

    /// Updates the persisted extraction workflow state for an episode.
    pub async fn set_ambient_extraction_state(
        &self,
        episode_id: &str,
        state: ExtractionState,
    ) -> Result<bool, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::set_extraction_state(database.connection(), episode_id, state)
            .map_err(|error| store_error_code(&error))
    }

    /// Записывает высказывание. `Ok(false)` — дубликат в окне дедупликации,
    /// а не ошибка.
    ///
    /// Отказ SQLite не ретраится: Core возвращает `STORAGE_FAILED`, не
    /// создаёт ложную запись и best-effort публикует `ambient.error`, чтобы
    /// пользователь увидел, что запись потеряна. Публикация тоже идёт в
    /// базу, поэтому при полностью недоступной базе она не сработает — и это
    /// честнее, чем притворяться, будто уведомление доставлено.
    pub async fn insert_ambient_utterance(
        &self,
        input: &AmbientUtteranceInput,
        retention_days: u32,
        dedup_window_ms: u32,
    ) -> Result<bool, AmbientErrorCode> {
        if retention_days == 0 || retention_days > MAX_RETENTION_DAYS {
            return Err(AmbientErrorCode::InvalidArgument);
        }
        if dedup_window_ms == 0 || dedup_window_ms > MAX_DEDUP_WINDOW_MS {
            return Err(AmbientErrorCode::InvalidArgument);
        }
        let record = AmbientUtteranceRecord {
            utterance_id: input.utterance_id.clone(),
            episode_id: input.episode_id.clone(),
            sequence: input.sequence,
            started_at: timestamp_ms(input.started_at_ms),
            duration_ms: input.duration_ms,
            text_hash: text_hash(&input.text),
            text: input.text.clone(),
            language: input.language.clone(),
            avg_logprob: input.avg_logprob,
            speaker: SPEAKER_UNVERIFIED.to_owned(),
            redacted: input.redacted,
            expires_at: expires_at(input.started_at_ms, retention_days),
        };
        let window_start = timestamp_ms(
            input
                .started_at_ms
                .saturating_sub(u64::from(dedup_window_ms)),
        );
        let outcome = {
            let database = self.database.lock().await;
            AmbientStoreSql::insert_utterance(database.connection(), &record, &window_start)
                .map_err(|error| store_error_code(&error))
        };
        if outcome == Err(AmbientErrorCode::StorageFailed) {
            let _ = self
                .append_ambient_event(&AmbientLogEvent::Error {
                    code: AmbientErrorCode::StorageFailed,
                    state: ListeningState::Listening,
                })
                .await;
        }
        outcome
    }

    /// Lists recent episode metadata, bounded by `limit`.
    pub async fn list_ambient_episodes(
        &self,
        limit: usize,
    ) -> Result<Vec<AmbientEpisodeRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::list_episodes(database.connection(), limit)
            .map_err(|error| store_error_code(&error))
    }

    /// Loads one retained ambient episode by its stable identifier.
    pub async fn get_ambient_episode(
        &self,
        episode_id: &str,
    ) -> Result<Option<AmbientEpisodeRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::get_episode(database.connection(), episode_id)
            .map_err(|error| store_error_code(&error))
    }

    /// Lists utterances for one episode, bounded by `limit`.
    pub async fn list_ambient_utterances(
        &self,
        episode_id: &str,
        limit: usize,
    ) -> Result<Vec<AmbientUtteranceRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::list_utterances(database.connection(), episode_id, limit)
            .map_err(|error| store_error_code(&error))
    }

    /// Lists retained deletion records, bounded by `limit`.
    pub async fn list_ambient_tombstones(
        &self,
        limit: usize,
    ) -> Result<Vec<AmbientTombstoneRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::list_tombstones(database.connection(), limit)
            .map_err(|error| store_error_code(&error))
    }

    /// Удаляет эпизод целиком и вращает состарившиеся backup-контейнеры.
    pub async fn delete_ambient_episode(
        &self,
        episode_id: &str,
        now_ms: u64,
    ) -> Result<AmbientDeletion, AmbientErrorCode> {
        let deletion = {
            let database = self.database.lock().await;
            AmbientStoreSql::delete_episode(
                database.connection(),
                episode_id,
                REASON_USER_REQUEST,
                &timestamp_ms(now_ms),
                &expires_at(now_ms, TOMBSTONE_RETENTION_DAYS),
            )
            .map_err(|error| store_error_code(&error))?
        };
        self.rotate_ambient_backups(now_ms);
        Ok(deletion)
    }

    /// «Забыть последние N минут». Окно замкнутое: `[now - minutes, now]`.
    pub async fn forget_ambient_window(
        &self,
        minutes: u32,
        now_ms: u64,
    ) -> Result<AmbientDeletion, AmbientErrorCode> {
        let from = now_ms.saturating_sub(u64::from(minutes) * 60 * 1000);
        let deletion = {
            let database = self.database.lock().await;
            AmbientStoreSql::forget_window(
                database.connection(),
                &timestamp_ms(from),
                &timestamp_ms(now_ms),
                &timestamp_ms(now_ms),
                &expires_at(now_ms, TOMBSTONE_RETENTION_DAYS),
            )
            .map_err(|error| store_error_code(&error))?
        };
        self.rotate_ambient_backups(now_ms);
        Ok(deletion)
    }

    /// Retention-прогон: истёкший текст, истёкшие метаданные, истёкшие
    /// tombstone и состарившиеся ambient-строки журнала.
    pub async fn purge_ambient(&self, now_ms: u64) -> Result<AmbientPurge, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::purge_expired(
            database.connection(),
            &timestamp_ms(now_ms),
            &expires_at(now_ms, TOMBSTONE_RETENTION_DAYS),
            &cutoff_at(now_ms, EVENT_RETENTION_DAYS),
        )
        .map_err(|error| store_error_code(&error))
    }

    // ------------------------------------------------------------------
    // Ограниченная проактивность (план 04.7).
    // ------------------------------------------------------------------

    /// Записывает предложение. `expires_at` считается здесь, а не вызывающим:
    /// срок жизни карточки — политика Core, ровно как срок метаданных эпизода.
    pub async fn record_ambient_proposal(
        &self,
        record: &AmbientProposalRecord,
    ) -> Result<ProposalInsert, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::record_proposal(database.connection(), record)
            .map_err(|error| store_error_code(&error))
    }

    /// Loads a proposal by its stable identifier.
    pub async fn get_ambient_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<AmbientProposalRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::get_proposal(database.connection(), proposal_id)
            .map_err(|error| store_error_code(&error))
    }

    /// Finds the proposal associated with an idempotency key, if one exists.
    pub async fn find_ambient_proposal_by_idempotency(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<AmbientProposalRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::find_proposal_by_idempotency(database.connection(), idempotency_key)
            .map_err(|error| store_error_code(&error))
    }

    /// Lists unresolved proposals, bounded by `limit`.
    pub async fn list_open_ambient_proposals(
        &self,
        limit: usize,
    ) -> Result<Vec<AmbientProposalRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::list_open_proposals(database.connection(), limit)
            .map_err(|error| store_error_code(&error))
    }

    /// Переводит предложение в терминальное состояние. `Ok(false)` — «уже
    /// решено или такого нет».
    pub async fn resolve_ambient_proposal_row(
        &self,
        proposal_id: &str,
        state: ProposalState,
        now_ms: u64,
        accepted_task_id: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<bool, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::resolve_proposal(
            database.connection(),
            proposal_id,
            state,
            &timestamp_ms(now_ms),
            accepted_task_id,
            idempotency_key,
        )
        .map_err(|error| store_error_code(&error))
    }

    /// Mutes a proposal subject from producing future ambient suggestions.
    pub async fn mute_ambient_subject(
        &self,
        mute_key: &str,
        kind: ProposalKind,
        subject_key: &str,
        now_ms: u64,
    ) -> Result<(), AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::mute_subject(
            database.connection(),
            mute_key,
            kind,
            subject_key,
            &timestamp_ms(now_ms),
        )
        .map_err(|error| store_error_code(&error))
    }

    /// Returns persisted mute keys used to suppress proposals.
    pub async fn list_ambient_mute_keys(&self) -> Result<Vec<String>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::list_mute_keys(database.connection())
            .map_err(|error| store_error_code(&error))
    }

    /// Expires unresolved proposals whose Core-owned deadline has passed.
    pub async fn expire_stale_ambient_proposals(
        &self,
        now_ms: u64,
    ) -> Result<usize, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::expire_stale_proposals(database.connection(), &timestamp_ms(now_ms))
            .map_err(|error| store_error_code(&error))
    }

    /// Loads persisted rolling proactivity counters, if initialized.
    pub async fn load_ambient_counters(
        &self,
    ) -> Result<Option<ProactivityCountersRow>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::load_counters(database.connection())
            .map_err(|error| store_error_code(&error))
    }

    /// Persists rolling proactivity counters for restart recovery.
    pub async fn save_ambient_counters(
        &self,
        row: ProactivityCountersRow,
    ) -> Result<(), AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::save_counters(database.connection(), row)
            .map_err(|error| store_error_code(&error))
    }

    /// Публикует ambient-событие в durable journal.
    ///
    /// Payload собирает типизированный фасад 04.1, поэтому ни текста, ни его
    /// хеша в нём нет по типам, а не по дисциплине.
    pub async fn append_ambient_event(
        &self,
        event: &AmbientLogEvent,
    ) -> Result<i64, AmbientErrorCode> {
        let payload = serde_json::to_vec(event).map_err(|_| AmbientErrorCode::InvalidArgument)?;
        let database = self.database.lock().await;
        database
            .append_event(&event_task_id(event), event.event_name(), &payload)
            .map_err(|_| AmbientErrorCode::StorageFailed)
    }

    /// Удалённый транскрипт остаётся внутри каждого backup-контейнера,
    /// снятого до удаления. Ротация вычищает **только состарившиеся**
    /// контейнеры: в снимке моложе семи суток текст физически остаётся на
    /// диске. Это окно называется пользователю прямо в тексте про удаление и
    /// в `docs/architecture.md`, а не заметается под «удалено безвозвратно».
    /// Ровно то же и той же продовой константой делает forget памяти.
    ///
    /// Каталог берётся от самой базы, а не из `local_data_dir()`: в
    /// продакшене это один и тот же путь, но у ротации не появляется скрытой
    /// зависимости от окружения, способной выйти за пределы своей базы.
    fn rotate_ambient_backups(&self, now_ms: u64) {
        let Some(directory) = self.database_path.parent() else {
            return;
        };
        let _ = evohime_local_storage::LocalDatabase::purge_expired_backups(
            directory,
            crate::memory_extraction::FORGET_BACKUP_RETENTION_MS,
            now_ms,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ambient_registry::test_support::*;
    use evohime_listener_contract::{EpisodeId, ListeningReason, SubjectKey};

    #[tokio::test]
    async fn deleting_an_episode_clears_its_transcript_and_its_journal_trail() {
        let (journal, _directory) = temporary_journal("ambient-delete");
        seed_episode(&journal, "ep-1", NOW_MS, "надо купить хлеб").await;
        journal
            .append_ambient_event(&AmbientLogEvent::Transcript {
                episode_id: EpisodeId::new("ep-1").unwrap(),
                started_at_ms: NOW_MS,
                utterance_count: 1,
                extraction_state: ExtractionState::Pending,
            })
            .await
            .expect("event publishes");

        let deletion = journal
            .delete_ambient_episode("ep-1", NOW_MS + 1_000)
            .await
            .expect("episode deletes");
        assert_eq!(deletion.episodes_removed, 1);
        assert_eq!(deletion.utterances_removed, 1);
        assert_eq!(deletion.events_removed, 1);
        assert_eq!(deletion.tombstones_written, 1);

        assert!(journal
            .list_ambient_episodes(10)
            .await
            .expect("read")
            .is_empty());
        let tombstones = journal.list_ambient_tombstones(10).await.expect("read");
        assert_eq!(tombstones.len(), 1);
        assert_eq!(tombstones[0].episode_id, "ep-1");
        let database = journal.database().lock().await;
        let remaining = database.read_events_after(0, 100).expect("journal reads");
        assert!(
            remaining
                .iter()
                .all(|event| !event.event_type.starts_with("ambient.")),
            "удаление обязано не оставлять ambient-строк в журнале"
        );
    }

    #[tokio::test]
    async fn forgetting_a_window_spares_what_happened_before_it() {
        let (journal, _directory) = temporary_journal("ambient-forget");
        seed_episode(&journal, "ep-old", NOW_MS - 3_600_000, "старое").await;
        seed_episode(&journal, "ep-new", NOW_MS - 60_000, "свежее").await;

        let deletion = journal
            .forget_ambient_window(10, NOW_MS)
            .await
            .expect("window forgets");
        assert_eq!(deletion.utterances_removed, 1);
        assert_eq!(deletion.episodes_removed, 1);

        let episodes = journal.list_ambient_episodes(10).await.expect("read");
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].episode_id, "ep-old");
    }

    /// Стартовый purge обязан отработать до первого `sleep`: база,
    /// открытая с просроченными строками, чиста сразу после запуска Core, а
    /// не через час.
    #[tokio::test]
    async fn the_startup_purge_cleans_an_expired_database_before_the_first_sleep() {
        let (journal, _directory) = temporary_journal("ambient-startup-purge");
        // Эпизод и его текст истекли ещё месяц назад.
        seed_episode(&journal, "ep-stale", NOW_MS - 60 * DAY, "давнее").await;
        assert_eq!(journal.list_ambient_episodes(10).await.unwrap().len(), 1);

        let task = crate::spawn_ambient_retention(journal.clone());
        let mut cleaned = false;
        for _ in 0..200 {
            if journal
                .list_ambient_episodes(10)
                .await
                .expect("read")
                .is_empty()
            {
                cleaned = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        task.abort();
        assert!(
            cleaned,
            "purge при старте обязан отработать до первого часа ожидания"
        );
        let tombstones = journal.list_ambient_tombstones(10).await.expect("read");
        assert_eq!(tombstones.len(), 1);
        assert_eq!(tombstones[0].reason, "retention");
    }

    /// Ни при каких входных данных ambient-событие не несёт ни текста, ни
    /// его хеша: фасад 04.1 запрещает это по типам, а этот тест проверяет,
    /// что публикация в journal ничего не добавляет от себя.
    #[tokio::test]
    async fn published_ambient_events_carry_no_transcript() {
        let (journal, _directory) = temporary_journal("ambient-events");
        let secret = "секретная фраза про пароль";
        seed_episode(&journal, "ep-1", NOW_MS, secret).await;
        for event in [
            AmbientLogEvent::Transcript {
                episode_id: EpisodeId::new("ep-1").unwrap(),
                started_at_ms: NOW_MS,
                utterance_count: 1,
                extraction_state: ExtractionState::Pending,
            },
            AmbientLogEvent::State {
                state: ListeningState::Listening,
                reason: ListeningReason::UserRequest,
                active_device_id: None,
            },
            AmbientLogEvent::Error {
                code: AmbientErrorCode::StorageFailed,
                state: ListeningState::Listening,
            },
        ] {
            journal
                .append_ambient_event(&event)
                .await
                .expect("event publishes");
        }
        let hash = super::text_hash(secret);
        let database = journal.database().lock().await;
        let events = database.read_events_after(0, 100).expect("journal reads");
        assert_eq!(events.len(), 3);
        for event in events {
            let payload = String::from_utf8(event.payload).expect("payload is JSON");
            assert!(!payload.contains(secret), "{payload} carries speech");
            assert!(!payload.contains(&hash), "{payload} carries a text hash");
            let value: serde_json::Value = serde_json::from_str(&payload).expect("payload parses");
            for key in value.as_object().expect("object").keys() {
                assert!(
                    !matches!(key.as_str(), "text" | "text_hash"),
                    "ambient event exposes {key}"
                );
            }
            assert!(
                event.task_id == "ep-1" || event.task_id == SESSION_TASK_ID,
                "ambient event addressed to {}",
                event.task_id
            );
        }
    }

    #[tokio::test]
    async fn utterance_storage_rejects_limits_outside_the_contract() {
        let (journal, _directory) = temporary_journal("ambient-input-limits");
        let input = AmbientUtteranceInput {
            utterance_id: "u-1".to_owned(),
            episode_id: "ep-1".to_owned(),
            sequence: 0,
            started_at_ms: NOW_MS,
            duration_ms: 1_000,
            text: "текст".to_owned(),
            language: "ru".to_owned(),
            avg_logprob: -0.2,
            redacted: false,
        };
        assert_eq!(
            journal
                .insert_ambient_utterance(&input, MAX_RETENTION_DAYS + 1, 60_000)
                .await,
            Err(AmbientErrorCode::InvalidArgument)
        );
        assert_eq!(
            journal
                .insert_ambient_utterance(&input, 7, MAX_DEDUP_WINDOW_MS + 1)
                .await,
            Err(AmbientErrorCode::InvalidArgument)
        );
    }

    /// Удаление эпизода-источника переводит его предложения в `expired` с
    /// причиной `source_deleted` и уносит строку `ambient.proposal` из
    /// журнала — тем же механизмом, что и остальные ambient-строки.
    #[tokio::test]
    async fn deleting_the_source_episode_expires_its_proposal_and_its_journal_row() {
        let (journal, _directory) = temporary_journal("ambient-proposal-source");
        seed_episode(&journal, "ep-1", NOW_MS, "надо не забыть про хлеб").await;
        let record = seed_proposal_record("prop-1", "хлеб", Some("ep-1"), NOW_MS);
        assert_eq!(
            journal.record_ambient_proposal(&record).await,
            Ok(ProposalInsert::Created)
        );
        journal
            .append_ambient_event(&AmbientLogEvent::Proposal {
                proposal_id: evohime_listener_contract::ProposalId::new("prop-1").unwrap(),
                episode_id: Some(evohime_listener_contract::EpisodeId::new("ep-1").unwrap()),
                kind: ProposalKind::Reminder,
                subject_key: SubjectKey::new(record.subject_key.clone()).unwrap(),
                proposal_state: ProposalState::Proposed,
            })
            .await
            .expect("событие публикуется");

        let deletion = journal
            .delete_ambient_episode("ep-1", NOW_MS + 1_000)
            .await
            .expect("эпизод удаляется");
        assert_eq!(deletion.proposals_expired, 1);

        let stored = journal
            .get_ambient_proposal("prop-1")
            .await
            .expect("чтение")
            .expect("след предложения остаётся");
        assert_eq!(stored.state, ProposalState::Expired);
        assert_eq!(
            stored.source_deleted_reason.as_deref(),
            Some("source_deleted")
        );
        assert!(
            stored.source_episode_id.is_none(),
            "внешний ключ обнуляется, но уже после пометки"
        );
        assert!(journal
            .list_open_ambient_proposals(10)
            .await
            .expect("чтение")
            .is_empty());

        let database = journal.database().lock().await;
        let remaining = database.read_events_after(0, 100).expect("журнал читается");
        assert!(
            remaining
                .iter()
                .all(|event| event.event_type != "ambient.proposal"),
            "строка ambient.proposal обязана уйти вместе с эпизодом"
        );
    }

    /// Молчание сутки снимает карточку с ожидания на обычном retention-прогоне.
    #[tokio::test]
    async fn an_unanswered_proposal_expires_on_the_retention_pass() {
        let (journal, _directory) = temporary_journal("ambient-proposal-expiry");
        let stale = seed_proposal_record(
            "prop-stale",
            "хлеб",
            None,
            NOW_MS - 2 * evohime_listener_contract::PROPOSAL_LIFETIME_MS,
        );
        let fresh = seed_proposal_record("prop-fresh", "отчёт", None, NOW_MS);
        journal
            .record_ambient_proposal(&stale)
            .await
            .expect("запись");
        journal
            .record_ambient_proposal(&fresh)
            .await
            .expect("запись");

        let purge = journal.purge_ambient(NOW_MS).await.expect("retention");
        assert_eq!(purge.proposals_expired, 1);
        let open = journal
            .list_open_ambient_proposals(10)
            .await
            .expect("чтение");
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].proposal_id, "prop-fresh");
    }
}
