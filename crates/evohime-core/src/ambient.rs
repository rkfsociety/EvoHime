//! Core-owned часть ambient-хранилища (план 04.2): retention, персистентная
//! политика и публикация ambient-событий в durable journal.
//!
//! Сам SQL живёт в `evohime_local_storage::ambient_store`; здесь — часы,
//! сроки, файл политики и правила публикации, то есть всё, чего у
//! side-effect-free контракта 04.1 и у migration-neutral стора быть не может.

use std::path::{Path, PathBuf};

use evohime_listener_contract::{
    AmbientErrorCode, AmbientLogEvent, AmbientPolicy, ExtractionState, ListeningState,
    DEFAULT_RETENTION_DAYS, MAX_RETENTION_DAYS,
};
use evohime_local_storage::ambient_store::{
    AmbientDeletion, AmbientEpisodeRecord, AmbientPurge, AmbientStoreError, AmbientStoreSql,
    AmbientTombstoneRecord, AmbientUtteranceRecord, REASON_USER_REQUEST, SPEAKER_UNVERIFIED,
};

/// Имя файла политики в data dir.
pub const POLICY_FILE_NAME: &str = "ambient-policy.json";

/// Retention текста транскриптов, в сутках. Потолок берётся из контракта
/// 04.1, чтобы значение не разъехалось с валидацией политики.
pub const RETENTION_DAYS_ENV: &str = "EVOHIME_AMBIENT_RETENTION_DAYS";

/// Метаданные эпизода живут дольше текста: список «когда слушали» нужен
/// пользователю, чтобы было что удалять, но и он не вечен.
pub const EPISODE_METADATA_RETENTION_DAYS: u32 = 30;
/// Tombstone — след удаления, а не вечная запись.
pub const TOMBSTONE_RETENTION_DAYS: u32 = 30;
/// Собственный срок ambient-строк в `events`. У самой таблицы retention нет
/// вообще, поэтому без этого срока хронология слушания жила бы вечно.
pub const EVENT_RETENTION_DAYS: u32 = 30;

/// Интервал фонового purge.
pub const PURGE_INTERVAL_SECONDS: u64 = 60 * 60;

/// `task_id` ambient-событий, у которых нет эпизода.
///
/// Соглашение «`task_id` ambient-события — это `episode_id`» существует ради
/// удаления: иначе вычищать журнал пришлось бы сканом BLOB-payload, а так
/// оно идёт по существующему индексу `idx_events_task_sequence`.
pub const SESSION_TASK_ID: &str = "ambient-session";

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// Retention текста из окружения: по умолчанию 7 суток, потолок 90.
/// Мусор в переменной — это не «слушать вечно», а дефолт.
pub fn retention_days_from_env() -> u32 {
    parse_retention_days(std::env::var(RETENTION_DAYS_ENV).ok().as_deref())
}

pub(crate) fn parse_retention_days(raw: Option<&str>) -> u32 {
    raw.and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|days| *days > 0)
        .map(|days| days.min(MAX_RETENTION_DAYS))
        .unwrap_or(DEFAULT_RETENTION_DAYS)
}

/// Единый формат времени ambient-строк: тот же, что SQLite пишет в
/// `events.created_at`, поэтому лексикографическое сравнение совпадает с
/// хронологическим и окно forget можно применять к обеим таблицам сразу.
pub fn timestamp_ms(unix_ms: u64) -> String {
    let clamped = i64::try_from(unix_ms).unwrap_or(i64::MAX);
    chrono::DateTime::from_timestamp_millis(clamped)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Момент истечения: `now + days`.
pub fn expires_at(now_ms: u64, days: u32) -> String {
    timestamp_ms(now_ms.saturating_add(u64::from(days) * DAY_MS))
}

/// Граница retention: `now - days`.
pub fn cutoff_at(now_ms: u64, days: u32) -> String {
    timestamp_ms(now_ms.saturating_sub(u64::from(days) * DAY_MS))
}

pub fn policy_path(data_dir: &Path) -> PathBuf {
    data_dir.join(POLICY_FILE_NAME)
}

/// Политика по умолчанию: слушать не запрещено политикой, retention — из
/// окружения. Микрофон при этом всё равно закрыт, пока `microphone_listen`
/// не переведён в `allow`.
pub fn default_policy() -> AmbientPolicy {
    AmbientPolicy {
        retention_days: retention_days_from_env(),
        ..AmbientPolicy::default()
    }
}

/// Fail-safe в пользу тишины: политику, которую не удалось прочитать,
/// нельзя молча заменить на «слушать всё».
pub fn paused_default_policy() -> AmbientPolicy {
    AmbientPolicy {
        paused: true,
        ..default_policy()
    }
}

/// Читает политику из data dir.
///
/// Отсутствующий файл означает «ещё не настраивали» — дефолт. Повреждённый
/// или невалидный файл означает «неизвестно, чего хотел пользователь» — тот
/// же дефолт, но с включённой паузой.
pub fn load_policy(data_dir: &Path) -> AmbientPolicy {
    let path = policy_path(data_dir);
    let Ok(bytes) = std::fs::read(&path) else {
        return default_policy();
    };
    let Ok(policy) = serde_json::from_slice::<AmbientPolicy>(&bytes) else {
        return paused_default_policy();
    };
    if policy.validate().is_err() {
        return paused_default_policy();
    }
    policy
}

/// Пишет политику атомарно: временный файл, `sync_all`, owner-only ACL,
/// `rename`, ACL ещё раз (rename не переносит дескриптор, если целевой файл
/// уже существовал).
///
/// Невалидная политика не сохраняется: иначе следующий старт прочитал бы её
/// как повреждённую и молча встал на паузу.
pub fn save_policy(data_dir: &Path, policy: &AmbientPolicy) -> Result<(), String> {
    policy.validate().map_err(|error| error.to_string())?;
    let path = policy_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(policy).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    if let Err(error) = harden(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = replace_file(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    harden(&path)?;
    if let Some(parent) = path.parent() {
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
    Ok(())
}

#[cfg(windows)]
fn harden(path: &Path) -> Result<(), String> {
    evohime_desktop_ipc::windows_security::harden_file_owner_only(path)
        .map_err(|error| error.to_string())
}

/// Atomically replaces the destination on every supported platform.
///
/// `std::fs::rename` replaces an existing destination on Unix, but Windows
/// rejects that case. `MoveFileExW(REPLACE_EXISTING)` preserves the atomic
/// replacement contract needed for a policy update.
#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::rename(from, to).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn harden(_: &Path) -> Result<(), String> {
    Ok(())
}

/// `task_id`, под которым ambient-событие попадает в `events`.
pub fn event_task_id(event: &AmbientLogEvent) -> String {
    match event {
        AmbientLogEvent::Transcript { episode_id, .. } => episode_id.to_string(),
        _ => SESSION_TASK_ID.to_owned(),
    }
}

/// Одно распознанное высказывание в том виде, в каком его отдаёт листенер.
///
/// `text_hash` в структуре нет: его считает Core при записи. Хеш живёт
/// только в таблице и никогда не покидает хранилище — короткую фразу
/// перебирают по хешу за секунды, поэтому хеш приравнивается к содержимому.
#[derive(Debug, Clone, PartialEq)]
pub struct AmbientUtteranceInput {
    pub utterance_id: String,
    pub episode_id: String,
    pub sequence: i64,
    pub started_at_ms: u64,
    pub duration_ms: i64,
    pub text: String,
    pub language: String,
    pub avg_logprob: f64,
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

    pub async fn close_ambient_episode(
        &self,
        episode_id: &str,
        now_ms: u64,
    ) -> Result<bool, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::close_episode(database.connection(), episode_id, &timestamp_ms(now_ms))
            .map_err(|error| store_error_code(&error))
    }

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

    pub async fn list_ambient_episodes(
        &self,
        limit: usize,
    ) -> Result<Vec<AmbientEpisodeRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::list_episodes(database.connection(), limit)
            .map_err(|error| store_error_code(&error))
    }

    pub async fn list_ambient_utterances(
        &self,
        episode_id: &str,
        limit: usize,
    ) -> Result<Vec<AmbientUtteranceRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::list_utterances(database.connection(), episode_id, limit)
            .map_err(|error| store_error_code(&error))
    }

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
    use evohime_listener_contract::{EpisodeId, ListeningReason, QuietHours};

    fn temporary_journal(name: &str) -> (crate::EventJournal, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = crate::EventJournal::open(directory.path().join(format!("{name}.db")))
            .expect("journal opens");
        (journal, directory)
    }

    const NOW_MS: u64 = 1_770_000_000_000;
    const DAY: u64 = 24 * 60 * 60 * 1000;

    async fn seed_episode(
        journal: &crate::EventJournal,
        episode_id: &str,
        started_at_ms: u64,
        text: &str,
    ) {
        journal
            .open_ambient_episode(
                episode_id,
                "whisper-base-q5_1",
                "base-q5_1",
                ExtractionState::Pending,
                started_at_ms,
            )
            .await
            .expect("episode opens");
        assert!(journal
            .insert_ambient_utterance(
                &AmbientUtteranceInput {
                    utterance_id: format!("{episode_id}-u0"),
                    episode_id: episode_id.to_owned(),
                    sequence: 0,
                    started_at_ms,
                    duration_ms: 1_200,
                    text: text.to_owned(),
                    language: "ru".to_owned(),
                    avg_logprob: -0.3,
                    redacted: false,
                },
                DEFAULT_RETENTION_DAYS,
                60_000,
            )
            .await
            .expect("utterance stores"));
    }

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

    #[test]
    fn retention_falls_back_to_the_default_instead_of_listening_forever() {
        assert_eq!(parse_retention_days(Some("14")), 14);
        assert_eq!(parse_retention_days(Some(" 3 ")), 3);
        assert_eq!(parse_retention_days(Some("365")), MAX_RETENTION_DAYS);
        for broken in ["", "0", "-1", "семь", "7.5"] {
            assert_eq!(
                parse_retention_days(Some(broken)),
                DEFAULT_RETENTION_DAYS,
                "«{broken}» must not change retention"
            );
        }
        assert_eq!(parse_retention_days(None), DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn timestamps_sort_the_same_way_the_clock_moves() {
        assert_eq!(timestamp_ms(0), "1970-01-01T00:00:00.000Z");
        let earlier = timestamp_ms(1_770_000_000_000);
        let later = timestamp_ms(1_770_000_001_000);
        assert!(earlier < later);
        assert!(cutoff_at(1_770_000_000_000, 7) < timestamp_ms(1_770_000_000_000));
        assert!(expires_at(1_770_000_000_000, 7) > timestamp_ms(1_770_000_000_000));
        assert_eq!(earlier.len(), later.len(), "ширина метки постоянна");
    }

    #[test]
    fn policy_round_trips_through_the_data_directory() {
        let directory = tempfile::tempdir().expect("temp dir");
        assert_eq!(load_policy(directory.path()), default_policy());
        let policy = AmbientPolicy {
            paused: false,
            quiet_hours: vec![QuietHours::new(23 * 60, 7 * 60).unwrap()],
            process_blocklist: vec!["bank*.exe".to_owned()],
            window_title_blocklist: Vec::new(),
            retention_days: 14,
        };
        save_policy(directory.path(), &policy).expect("policy saves");
        assert_eq!(load_policy(directory.path()), policy);

        // Обновление существующего файла должно сохранять атомарность и на
        // Windows, где обычный std::fs::rename не заменяет destination.
        let updated = AmbientPolicy {
            paused: true,
            ..policy
        };
        save_policy(directory.path(), &updated).expect("policy updates");
        assert_eq!(load_policy(directory.path()), updated);
    }

    #[test]
    fn a_corrupted_policy_reads_back_as_a_paused_default() {
        let directory = tempfile::tempdir().expect("temp dir");
        std::fs::write(policy_path(directory.path()), b"{ not json").expect("corrupt file");
        let loaded = load_policy(directory.path());
        assert!(
            loaded.paused,
            "повреждённая политика обязана вставать на паузу"
        );
        assert_eq!(loaded, paused_default_policy());

        // Синтаксически валидный, но запрещённый контрактом файл — тот же
        // случай: неизвестно, чего хотел пользователь.
        std::fs::write(
            policy_path(directory.path()),
            br#"{"paused":false,"retention_days":4000}"#,
        )
        .expect("invalid policy writes");
        assert!(load_policy(directory.path()).paused);
    }

    #[test]
    fn an_invalid_policy_is_never_persisted() {
        let directory = tempfile::tempdir().expect("temp dir");
        let policy = AmbientPolicy {
            retention_days: MAX_RETENTION_DAYS + 1,
            ..AmbientPolicy::default()
        };
        assert!(save_policy(directory.path(), &policy).is_err());
        assert!(!policy_path(directory.path()).exists());
    }

    #[cfg(windows)]
    #[test]
    fn the_written_policy_file_is_owner_only() {
        let directory = tempfile::tempdir().expect("temp dir");
        save_policy(directory.path(), &default_policy()).expect("policy saves");
        let sid = evohime_desktop_ipc::windows_security::current_user_sid().expect("sid");
        let dacl =
            evohime_desktop_ipc::windows_security::file_dacl_sddl(&policy_path(directory.path()))
                .expect("DACL reads");
        assert_eq!(dacl, format!("D:P(A;;FA;;;{sid})(A;;FA;;;SY)"));
    }

    #[test]
    fn only_transcript_events_are_addressed_to_an_episode() {
        assert_eq!(
            event_task_id(&AmbientLogEvent::Transcript {
                episode_id: EpisodeId::new("ep-1").unwrap(),
                started_at_ms: 1_770_000_000_000,
                utterance_count: 2,
                extraction_state: ExtractionState::Pending,
            }),
            "ep-1"
        );
        assert_eq!(
            event_task_id(&AmbientLogEvent::State {
                state: ListeningState::Listening,
                reason: ListeningReason::UserRequest,
                active_device_id: None,
            }),
            SESSION_TASK_ID
        );
    }
}
