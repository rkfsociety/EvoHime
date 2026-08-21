//! Core-owned часть ambient-хранилища (план 04.2): retention, персистентная
//! политика и публикация ambient-событий в durable journal.
//!
//! Сам SQL живёт в `evohime_local_storage::ambient_store`; здесь — часы,
//! сроки, файл политики и правила публикации, то есть всё, чего у
//! side-effect-free контракта 04.1 и у migration-neutral стора быть не может.

use std::path::{Path, PathBuf};

use evohime_listener_contract::{
    AmbientErrorCode, AmbientLogEvent, AmbientPolicy, ExtractionState, ListeningReason,
    ListeningState, ProactivityBudget, ProposalKind, ProposalState, SubjectKey,
    DEFAULT_RETENTION_DAYS, MAX_DEDUP_WINDOW_MS, MAX_RETENTION_DAYS,
};
use evohime_local_storage::ambient_store::{
    AmbientDeletion, AmbientEpisodeRecord, AmbientProposalRecord, AmbientPurge, AmbientStoreError,
    AmbientStoreSql, AmbientTombstoneRecord, AmbientUtteranceRecord, ProactivityCountersRow,
    ProposalInsert, REASON_USER_REQUEST, SPEAKER_UNVERIFIED,
};

use crate::ambient_proactivity::{
    decide_proposal, effect_of, AuthorizedEffect, ProposalRejection, RollingCounters,
    PROPOSAL_LIFETIME_MS,
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

/// Имя файла с намерением пользователя по слушанию.
pub const CONTROL_FILE_NAME: &str = "ambient-control.json";

/// Что пользователь выбрал в панели «Слух»: включено ли слушание вообще и
/// каким устройством.
///
/// Живёт отдельно от [`AmbientPolicy`]: политика — это правила (тишина,
/// чёрные списки, срок хранения) из контракта 04.1, а это — переключатель и
/// выбор микрофона. Смешать их значило бы завести в side-effect-free
/// контракте поле, которого там быть не должно.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AmbientControl {
    /// Слушание выключено по умолчанию. Значение по умолчанию здесь — это не
    /// стиль, а требование: неизвестное намерение не должно открывать
    /// микрофон.
    #[serde(default)]
    pub enabled: bool,
    /// Выбранное устройство; пустая строка — «устройство системы».
    #[serde(default)]
    pub device_id: String,
}

pub fn control_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CONTROL_FILE_NAME)
}

/// Читает намерение. Отсутствующий или повреждённый файл означает
/// «выключено»: молчание безопаснее догадки.
pub fn load_control(data_dir: &Path) -> AmbientControl {
    std::fs::read(control_path(data_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<AmbientControl>(&bytes).ok())
        .unwrap_or_default()
}

/// Пишет намерение тем же атомарным путём, что и политику: временный файл,
/// `sync_all`, owner-only ACL, замена.
pub fn save_control(data_dir: &Path, control: &AmbientControl) -> Result<(), String> {
    let path = control_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(control).map_err(|error| error.to_string())?;
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
    harden(&path)
}

/// Каталог данных Core: тот же, из которого поднимается всё остальное
/// ambient-состояние.
pub fn data_dir() -> PathBuf {
    std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("EvoHime")))
        .unwrap_or_else(|| PathBuf::from(".evohime"))
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
        // Предложение адресуется своему эпизоду по той же причине: удаление
        // источника обязано унести и строку `ambient.proposal`, а ищется она
        // по индексу `idx_events_task_sequence`, а не сканом payload.
        AmbientLogEvent::Proposal {
            episode_id: Some(episode_id),
            ..
        } => episode_id.to_string(),
        _ => SESSION_TASK_ID.to_owned(),
    }
}

/// Одно устройство захвата в снимке состояния.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AmbientDeviceInfo {
    pub device_id: String,
    pub display_name: String,
    pub is_default: bool,
    pub is_active: bool,
}

/// Снимок того, что панель «Слух», трей и хоткей показывают пользователю.
///
/// Единственный источник истины живёт в Core: три точки входа отправляют одну
/// и ту же команду и ждут события, а не рисуют себе состояние сами.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AmbientStatusSnapshot {
    pub state: ListeningState,
    pub reason: ListeningReason,
    pub active_device_id: String,
    pub engine_version: String,
    pub engine_ready: bool,
    pub devices: Vec<AmbientDeviceInfo>,
    /// Живёт ли подписка на смену устройств. `false` означает, что список —
    /// снимок, который сам не обновится, и панель обязана это сказать.
    pub watching_devices: bool,
}

impl Default for AmbientStatusSnapshot {
    /// Стартовое состояние — «неизвестно», а не «выключено».
    ///
    /// Пока процесс листенера не подключился, Core не знает, читается ли
    /// микрофон, и обязан сказать именно это: `EngineUnavailable` попадает в
    /// `ListeningState::is_degraded`, и индикатор показывает «проверка
    /// состояния» с предупреждением, а не спокойное «выключено».
    fn default() -> Self {
        Self {
            state: ListeningState::EngineUnavailable,
            reason: ListeningReason::EngineUnavailable,
            active_device_id: String::new(),
            engine_version: String::new(),
            engine_ready: false,
            devices: Vec::new(),
            watching_devices: false,
        }
    }
}

/// Команда процессу листенера. Отправляется только из Core: у трея, хоткея и
/// панели своего канала к листенеру нет.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerControl {
    Enabled(bool),
    Paused(bool),
    SelectDevice(String),
    ResetBuffers,
    /// Полная политика целиком: тихие часы и чёрные списки меняются вместе, а
    /// не по одному полю, поэтому listener получает новый снимок, а не патч.
    Policy(Box<(AmbientPolicy, AmbientControl)>),
}

#[derive(Default)]
struct AmbientRegistryInner {
    status: AmbientStatusSnapshot,
    control: Option<tokio::sync::mpsc::Sender<ListenerControl>>,
}

/// Реестр состояния постоянного слушания.
///
/// По образцу `RoutingApprovalRegistry`: общего `CoreState` в проекте нет,
/// поэтому состояние живёт в собственном разделяемом реестре, а `IpcBridge`
/// держит на него ссылку.
#[derive(Clone, Default)]
pub struct AmbientListeningRegistry {
    inner: std::sync::Arc<tokio::sync::Mutex<AmbientRegistryInner>>,
}

impl AmbientListeningRegistry {
    /// Подключает канал к живому процессу листенера.
    pub async fn attach_control(&self, control: tokio::sync::mpsc::Sender<ListenerControl>) {
        self.inner.lock().await.control = Some(control);
    }

    /// Снимает канал: листенер отвалился. Состояние при этом не «выключено»,
    /// а `EngineUnavailable` — иначе пользователь прочитал бы отказ связи как
    /// собственное решение выключить микрофон.
    pub async fn detach_control(&self) {
        let mut guard = self.inner.lock().await;
        guard.control = None;
        guard.status.engine_ready = false;
        guard.status.devices.clear();
        guard.status.watching_devices = false;
        guard.status.state = ListeningState::EngineUnavailable;
        guard.status.reason = ListeningReason::EngineUnavailable;
    }

    pub async fn snapshot(&self) -> AmbientStatusSnapshot {
        self.inner.lock().await.status.clone()
    }

    /// Отправляет команду листенеру. Отсутствие канала — это
    /// `LISTENER_UNAVAILABLE`, а не тихий успех.
    pub async fn send(&self, control: ListenerControl) -> Result<(), AmbientErrorCode> {
        let sender = {
            let guard = self.inner.lock().await;
            guard.control.clone()
        };
        let sender = sender.ok_or(AmbientErrorCode::ListenerUnavailable)?;
        sender
            .send(control)
            .await
            .map_err(|_| AmbientErrorCode::ListenerUnavailable)
    }

    /// Записывает состояние, о котором сообщил листенер.
    ///
    /// `true` означает «изменилось» — только тогда публикуется
    /// `ambient.state`: повтор состояния не является изменением.
    pub async fn set_state(
        &self,
        state: ListeningState,
        reason: ListeningReason,
        active_device_id: Option<String>,
    ) -> bool {
        let mut guard = self.inner.lock().await;
        if let Some(device_id) = active_device_id {
            guard.status.active_device_id = device_id;
        }
        if guard.status.state == state && guard.status.reason == reason {
            return false;
        }
        guard.status.state = state;
        guard.status.reason = reason;
        true
    }

    pub async fn set_devices(
        &self,
        devices: Vec<AmbientDeviceInfo>,
        active_device_id: String,
        watching: bool,
    ) {
        let mut guard = self.inner.lock().await;
        guard.status.active_device_id = active_device_id;
        guard.status.watching_devices = watching;
        guard.status.devices = devices;
        let active = guard.status.active_device_id.clone();
        for device in &mut guard.status.devices {
            device.is_active = if active.is_empty() {
                device.is_default
            } else {
                device.device_id == active
            };
        }
    }

    pub async fn set_engine(&self, version: String, ready: bool) {
        let mut guard = self.inner.lock().await;
        if !version.is_empty() {
            guard.status.engine_version = version;
        }
        guard.status.engine_ready = ready;
    }

    pub async fn engine_ready(&self) -> bool {
        self.inner.lock().await.status.engine_ready
    }
}

/// Минуты, прошедшие с полуночи по локальным часам.
///
/// Тихие часы пользователь задаёт по своим часам, а не по UTC: окно «с 23:00
/// до 07:00» в UTC означало бы у него совсем другое время суток.
pub fn local_minute_of_day() -> u32 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    now.hour() * 60 + now.minute()
}

/// Реестр ограниченной проактивности (план 04.7).
///
/// По тому же образцу, что [`AmbientListeningRegistry`]: общего `CoreState` в
/// проекте нет, поэтому счётчики окна живут в собственном клонируемом реестре.
/// Потолок [`ProactivityBudget`] неизменяем и приходит из контракта 04.1 — его
/// нельзя поднять в рантайме; меняются только счётчики, и они переживают
/// рестарт строкой таблицы v26.
#[derive(Clone, Default)]
pub struct AmbientProactivityRegistry {
    inner: std::sync::Arc<tokio::sync::Mutex<ProactivityInner>>,
}

struct ProactivityInner {
    budget: ProactivityBudget,
    counters: RollingCounters,
    muted: std::collections::HashSet<String>,
    loaded: bool,
    data_dir: Option<PathBuf>,
    coordinator: Option<crate::TaskCoordinator>,
}

impl Default for ProactivityInner {
    fn default() -> Self {
        Self {
            budget: ProactivityBudget::DEFAULT,
            counters: RollingCounters::default(),
            muted: std::collections::HashSet::new(),
            loaded: false,
            data_dir: None,
            coordinator: None,
        }
    }
}

impl AmbientProactivityRegistry {
    /// Каталог политики. Как и у моста, `None` означает продовый путь из
    /// окружения: поле существует ради тестов, чтобы соседние тесты не
    /// зависели друг от друга через переменную процесса.
    pub async fn set_data_dir(&self, directory: PathBuf) {
        self.inner.lock().await.data_dir = Some(directory);
    }

    /// Подключает координатор, чтобы записанное предложение будило открытое
    /// окно. Без этого сигнала `push_journal_tail` узнал бы о карточке только
    /// со следующим событием задачи.
    pub async fn attach_coordinator(&self, coordinator: crate::TaskCoordinator) {
        self.inner.lock().await.coordinator = Some(coordinator);
    }

    pub async fn budget(&self) -> ProactivityBudget {
        self.inner.lock().await.budget
    }

    async fn data_dir(&self) -> PathBuf {
        self.inner
            .lock()
            .await
            .data_dir
            .clone()
            .unwrap_or_else(data_dir)
    }

    /// Поднимает счётчики и mute-ключи из базы ровно один раз.
    ///
    /// Сбой чтения не открывает проактивность: реестр остаётся незагруженным
    /// и попробует снова, а не считает, что бюджет пуст и mute-ключей нет.
    pub async fn ensure_loaded(&self, journal: &crate::EventJournal) {
        if self.inner.lock().await.loaded {
            return;
        }
        let counters = journal.load_ambient_counters().await;
        let mutes = journal.list_ambient_mute_keys().await;
        let (Ok(counters), Ok(mutes)) = (counters, mutes) else {
            return;
        };
        let mut guard = self.inner.lock().await;
        if let Some(row) = counters {
            guard.counters = RollingCounters {
                hour_started_at_ms: row.hour_started_at_ms.max(0) as u64,
                hour_count: row.hour_count.max(0) as u32,
                day_started_at_ms: row.day_started_at_ms.max(0) as u64,
                day_count: row.day_count.max(0) as u32,
                last_proposed_at_ms: row.last_proposed_at_ms.map(|value| value.max(0) as u64),
            };
        }
        guard.muted = mutes.into_iter().collect();
        guard.loaded = true;
    }

    /// Полное решение «предлагать ли сейчас».
    ///
    /// Счётчик **не** увеличивается здесь: он растёт только после того, как
    /// карточка действительно появилась ([`Self::commit`]). Иначе отброшенное
    /// хранилищем предложение съедало бы часовой потолок.
    pub async fn decide(
        &self,
        journal: &crate::EventJournal,
        kind: ProposalKind,
        mute_key: &str,
        now_ms: u64,
    ) -> Result<AuthorizedEffect, ProposalRejection> {
        self.ensure_loaded(journal).await;
        let policy = load_policy(&self.data_dir().await);
        let guard = self.inner.lock().await;
        decide_proposal(
            effect_of(kind),
            guard.muted.contains(mute_key),
            &policy,
            &guard.budget,
            guard.counters,
            now_ms,
            local_minute_of_day(),
        )
    }

    /// Учитывает показанную карточку и сохраняет окно.
    pub async fn commit(&self, journal: &crate::EventJournal, now_ms: u64) {
        let counters = {
            let mut guard = self.inner.lock().await;
            guard.counters = guard.counters.record(now_ms);
            guard.counters
        };
        let _ = journal
            .save_ambient_counters(ProactivityCountersRow {
                hour_started_at_ms: i64::try_from(counters.hour_started_at_ms).unwrap_or(i64::MAX),
                hour_count: i64::from(counters.hour_count),
                day_started_at_ms: i64::try_from(counters.day_started_at_ms).unwrap_or(i64::MAX),
                day_count: i64::from(counters.day_count),
                last_proposed_at_ms: counters
                    .last_proposed_at_ms
                    .map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
            })
            .await;
    }

    /// «Больше не предлагать такое». Ключ без времени, поэтому mute переживает
    /// смену временной корзины, а строка таблицы — рестарт Core.
    pub async fn mute(
        &self,
        journal: &crate::EventJournal,
        mute_key: &str,
        kind: ProposalKind,
        subject_key: &str,
        now_ms: u64,
    ) -> Result<(), AmbientErrorCode> {
        journal
            .mute_ambient_subject(mute_key, kind, subject_key, now_ms)
            .await?;
        self.inner.lock().await.muted.insert(mute_key.to_owned());
        Ok(())
    }

    pub async fn is_muted(&self, journal: &crate::EventJournal, mute_key: &str) -> bool {
        self.ensure_loaded(journal).await;
        self.inner.lock().await.muted.contains(mute_key)
    }

    /// Публикует ambient-событие и будит push журнала.
    pub async fn publish(
        &self,
        journal: &crate::EventJournal,
        event: &AmbientLogEvent,
    ) -> Result<i64, AmbientErrorCode> {
        let sequence = journal.append_ambient_event(event).await?;
        if let Some(coordinator) = self.inner.lock().await.coordinator.as_ref() {
            coordinator.notify_journalled(sequence.max(0) as u64);
        }
        Ok(sequence)
    }
}

/// Собирает строку предложения для хранилища.
///
/// Срок жизни считает Core: 24 часа молчания — это ответ «нет».
#[allow(clippy::too_many_arguments)]
pub fn proposal_record(
    proposal_id: &str,
    proposal_key: &str,
    mute_key: &str,
    kind: ProposalKind,
    subject_key: &SubjectKey,
    subject: &str,
    title: &str,
    source_episode_id: Option<&str>,
    now_ms: u64,
) -> AmbientProposalRecord {
    AmbientProposalRecord {
        proposal_id: proposal_id.to_owned(),
        proposal_key: proposal_key.to_owned(),
        mute_key: mute_key.to_owned(),
        kind,
        subject_key: subject_key.as_str().to_owned(),
        subject: subject.to_owned(),
        title: title.to_owned(),
        source_episode_id: source_episode_id.map(str::to_owned),
        source_deleted_at: None,
        source_deleted_reason: None,
        created_at: timestamp_ms(now_ms),
        updated_at: timestamp_ms(now_ms),
        expires_at: timestamp_ms(now_ms.saturating_add(PROPOSAL_LIFETIME_MS)),
        occurrences: 1,
        state: ProposalState::Proposed,
        accepted_task_id: None,
        idempotency_key: None,
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

    pub async fn get_ambient_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<AmbientProposalRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::get_proposal(database.connection(), proposal_id)
            .map_err(|error| store_error_code(&error))
    }

    pub async fn find_ambient_proposal_by_idempotency(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<AmbientProposalRecord>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::find_proposal_by_idempotency(database.connection(), idempotency_key)
            .map_err(|error| store_error_code(&error))
    }

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

    pub async fn list_ambient_mute_keys(&self) -> Result<Vec<String>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::list_mute_keys(database.connection())
            .map_err(|error| store_error_code(&error))
    }

    pub async fn expire_stale_ambient_proposals(
        &self,
        now_ms: u64,
    ) -> Result<usize, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::expire_stale_proposals(database.connection(), &timestamp_ms(now_ms))
            .map_err(|error| store_error_code(&error))
    }

    pub async fn load_ambient_counters(
        &self,
    ) -> Result<Option<ProactivityCountersRow>, AmbientErrorCode> {
        let database = self.database.lock().await;
        AmbientStoreSql::load_counters(database.connection())
            .map_err(|error| store_error_code(&error))
    }

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

    // ------------------------------------------------------------------
    // Ограниченная проактивность (план 04.7).
    // ------------------------------------------------------------------

    async fn seeded_registry(
        journal: &crate::EventJournal,
        directory: &Path,
    ) -> AmbientProactivityRegistry {
        // Явная политика без пауз и тихих часов: тест про потолок не должен
        // зависеть от того, который сейчас час у машины.
        save_policy(
            directory,
            &AmbientPolicy {
                paused: false,
                quiet_hours: Vec::new(),
                process_blocklist: Vec::new(),
                window_title_blocklist: Vec::new(),
                retention_days: DEFAULT_RETENTION_DAYS,
            },
        )
        .expect("policy saves");
        let registry = AmbientProactivityRegistry::default();
        registry.set_data_dir(directory.to_path_buf()).await;
        registry.ensure_loaded(journal).await;
        registry
    }

    fn seed_proposal_record(
        proposal_id: &str,
        subject: &str,
        episode_id: Option<&str>,
        now_ms: u64,
    ) -> AmbientProposalRecord {
        let kind = ProposalKind::Reminder;
        let subject_key = crate::ambient_proactivity::subject_key(subject);
        proposal_record(
            proposal_id,
            &crate::ambient_proactivity::proposal_key(kind, &subject_key, now_ms),
            &crate::ambient_proactivity::mute_key(kind, &subject_key),
            kind,
            &subject_key,
            subject,
            "Напомнить купить хлеб",
            episode_id,
            now_ms,
        )
    }

    /// Часовой потолок держится и после рестарта: счётчик живёт строкой
    /// таблицы, а не полем реестра в памяти процесса. Иначе перезапуск Core
    /// обнулял бы потолок и делал его необязательным.
    #[tokio::test]
    async fn the_hourly_cap_survives_a_restart_of_the_registry() {
        let (journal, directory) = temporary_journal("ambient-proactivity-cap");
        let registry = seeded_registry(&journal, directory.path()).await;
        let mut now = NOW_MS;
        for index in 0..3 {
            let subject = format!("тема {index}");
            let key = crate::ambient_proactivity::mute_key(
                ProposalKind::Reminder,
                &crate::ambient_proactivity::subject_key(&subject),
            );
            registry
                .decide(&journal, ProposalKind::Reminder, &key, now)
                .await
                .expect("три предложения за час укладываются в потолок");
            registry.commit(&journal, now).await;
            now += 11 * 60 * 1000;
        }
        // Свежий реестр — тот же, что после перезапуска процесса.
        let restarted = AmbientProactivityRegistry::default();
        restarted.set_data_dir(directory.path().to_path_buf()).await;
        restarted.ensure_loaded(&journal).await;
        let key = crate::ambient_proactivity::mute_key(
            ProposalKind::Reminder,
            &crate::ambient_proactivity::subject_key("четвёртая тема"),
        );
        assert_eq!(
            restarted
                .decide(&journal, ProposalKind::Reminder, &key, now)
                .await,
            Err(crate::ambient_proactivity::ProposalRejection::Denied(
                evohime_listener_contract::ProactivityDenial::HourlyCapReached
            )),
            "перезапуск не обнуляет часовой потолок"
        );
        // И отброшенное не копится: через час доступен ровно один слот, а не
        // отложенная очередь.
        assert!(restarted
            .decide(
                &journal,
                ProposalKind::Reminder,
                &key,
                NOW_MS + 2 * 60 * 60 * 1000
            )
            .await
            .is_ok());
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
            NOW_MS - 2 * crate::ambient_proactivity::PROPOSAL_LIFETIME_MS,
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

    /// Повтор той же темы поднимает счётчик и **не** тратит часовой потолок:
    /// иначе разговор об одном и том же за десять минут выел бы весь бюджет.
    #[tokio::test]
    async fn a_duplicate_proposal_neither_creates_a_card_nor_spends_the_budget() {
        let (journal, directory) = temporary_journal("ambient-proposal-duplicate");
        let registry = seeded_registry(&journal, directory.path()).await;
        let first = seed_proposal_record("prop-1", "хлеб", None, NOW_MS);
        assert_eq!(
            journal.record_ambient_proposal(&first).await,
            Ok(ProposalInsert::Created)
        );
        registry.commit(&journal, NOW_MS).await;

        let again = seed_proposal_record("prop-2", "хлеб", None, NOW_MS + 60_000);
        assert_eq!(
            journal.record_ambient_proposal(&again).await,
            Ok(ProposalInsert::Duplicate {
                proposal_id: "prop-1".to_owned(),
                occurrences: 2,
            })
        );
        let open = journal
            .list_open_ambient_proposals(10)
            .await
            .expect("чтение");
        assert_eq!(open.len(), 1, "второй карточки не появилось");
        assert_eq!(open[0].occurrences, 2);
        let counters = journal
            .load_ambient_counters()
            .await
            .expect("чтение")
            .expect("строка счётчиков");
        assert_eq!(counters.hour_count, 1, "дубликат бюджета не потратил");
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
