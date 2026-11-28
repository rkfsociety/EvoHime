//! Реестры состояния слушания и ограниченной проактивности (план 04.7).
//!
//! По образцу `RoutingApprovalRegistry`: общего `CoreState` в проекте нет,
//! поэтому runtime-состояние живёт в собственных клонируемых реестрах, а
//! `IpcBridge` держит на них ссылки. Само решение «предлагать ли сейчас»
//! принимает внешний `ambient`-сервис через `ModuleRegistry` (план 203.1);
//! ядру принадлежат только счётчики, mute-ключи и публикация в журнал.

use std::path::PathBuf;

use evohime_listener_contract::{
    authorize_proactive, effect_of, local_minute_of_day, AmbientControl, AmbientDeviceInfo,
    AmbientErrorCode, AmbientLogEvent, AmbientPolicy, AmbientStatusSnapshot, AuthorizedEffect,
    ListenerControl, ListeningReason, ListeningState, ProactiveEffect, ProactivityBudget,
    ProactivityDecideResponse, ProactivityRejectionPayload, ProposalKind, ProposalRejection,
    RollingCounters,
};
use evohime_local_storage::ambient_store::ProactivityCountersRow;

pub use crate::ambient_policy_store::{data_dir, load_policy};

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

    /// Returns the latest Core-owned listening status snapshot.
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

    /// Replaces the device list and updates each entry's active marker.
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

    /// Updates the reported engine version and readiness state.
    pub async fn set_engine(&self, version: String, ready: bool) {
        let mut guard = self.inner.lock().await;
        if !version.is_empty() {
            guard.status.engine_version = version;
        }
        guard.status.engine_ready = ready;
    }

    /// Returns whether the listener currently reports a ready engine.
    pub async fn engine_ready(&self) -> bool {
        self.inner.lock().await.status.engine_ready
    }
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
    module_registry: Option<crate::module_registry::ModuleRegistry>,
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
            module_registry: None,
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

    /// Подключает реестр модулей-сервисов: решение о предложении принимает
    /// внешний `ambient`-сервис (план 203.1), ядро владеет только журналом.
    pub async fn attach_module_registry(
        &self,
        module_registry: crate::module_registry::ModuleRegistry,
    ) {
        self.inner.lock().await.module_registry = Some(module_registry);
    }

    /// Returns the immutable proactivity budget enforced by the contract.
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
    ///
    /// Само решение принимает внешний `ambient`-сервис через
    /// `ModuleRegistry` (план 203.1): ядро читает политику и счётчики и
    /// передаёт их вместе с фактом mute. Недоступность сервиса — тоже отказ
    /// ([`ProposalRejection::ServiceUnavailable`]): отсутствие решения
    /// не открывает проактивность.
    pub async fn decide(
        &self,
        journal: &crate::EventJournal,
        kind: ProposalKind,
        mute_key: &str,
        now_ms: u64,
    ) -> Result<AuthorizedEffect, ProposalRejection> {
        self.ensure_loaded(journal).await;
        let policy = load_policy(&self.data_dir().await);
        let (budget, muted, counters, module_registry) = {
            let guard = self.inner.lock().await;
            (
                guard.budget,
                guard.muted.contains(mute_key),
                guard.counters,
                guard.module_registry.clone(),
            )
        };
        let Some(module_registry) = module_registry else {
            return Err(ProposalRejection::ServiceUnavailable);
        };
        let payload = serde_json::json!({
            "effect": effect_of(kind).as_str(),
            "muted": muted,
            "policy": policy,
            "budget": budget,
            "counters": counters,
            "now_ms": now_ms,
            "minute_of_day": local_minute_of_day(),
        });
        let request_id = format!("ambient-proactivity-{now_ms}");
        match module_registry
            .invoke_if_ready(
                request_id,
                "ambient.proactivity_decide",
                payload,
                None,
                |_| Ok(()),
            )
            .await
        {
            Some(Ok(response)) => parse_proactivity_decide_response(response),
            // Сервис не зарегистрирован, не готов или ответил ошибкой —
            // решение недоступно, предложение не появляется.
            Some(Err(_)) | None => Err(ProposalRejection::ServiceUnavailable),
        }
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

    /// Checks a mute key, loading persisted mute state before answering.
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

/// Разбирает ответ `ambient.proactivity_decide` внешнего сервиса.
///
/// Любое отклонение от контракта ответа трактуется как недоступность решения:
/// разрешение, пришедшее в неожиданной форме, не открывает проактивность.
/// Закрытый список эффектов перепроверяется ядром — сервис не может выдать
/// запрещённый эффект за разрешённый.
fn parse_proactivity_decide_response(
    response: serde_json::Value,
) -> Result<AuthorizedEffect, ProposalRejection> {
    use evohime_listener_contract::ProactivityRejectionPayload;
    let parsed: evohime_listener_contract::ProactivityDecideResponse =
        serde_json::from_value(response).map_err(|_| ProposalRejection::ServiceUnavailable)?;
    if !parsed.authorized {
        let rejection = match parsed.rejection {
            Some(ProactivityRejectionPayload::Forbidden { effect }) => {
                let effect =
                    ProactiveEffect::parse(&effect).ok_or(ProposalRejection::ServiceUnavailable)?;
                ProposalRejection::Forbidden(effect)
            }
            Some(ProactivityRejectionPayload::Muted) => ProposalRejection::Muted,
            Some(ProactivityRejectionPayload::Denied { denial }) => {
                ProposalRejection::Denied(denial)
            }
            None => return Err(ProposalRejection::ServiceUnavailable),
        };
        return Err(rejection);
    }
    let effect = parsed
        .effect
        .as_deref()
        .and_then(ProactiveEffect::parse)
        .ok_or(ProposalRejection::ServiceUnavailable)?;
    evohime_listener_contract::authorize_proactive(effect)
        .map_err(|_| ProposalRejection::ServiceUnavailable)
}
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::Path;

    use evohime_listener_contract::{
        authorize_proactive, EpisodeId, ExtractionState, ProactiveEffect, ProactivityDecideRequest,
        ProactivityDecideResponse, ProactivityRejectionPayload, ProposalKind, ProposalRejection,
        DEFAULT_RETENTION_DAYS,
    };
    use evohime_local_storage::ambient_store::{
        proposal_record, AmbientProposalRecord, ProposalRecordInput,
    };
    use evohime_service_contract::{
        ServiceContractError, ServiceErrorCode, ServiceHandler, ServiceRequest, ServiceResponse,
        SERVICE_CONTRACT_VERSION,
    };

    /// Внутрипроцессный транспорт к настоящему обработчику
    /// `evohime-ambient`: тесты реестра гоняют реальное сервисное решение
    /// без запуска дочернего процесса.
    pub(crate) struct MockAmbientServiceTransport;

    impl crate::module_registry::ServiceTransport for MockAmbientServiceTransport {
        fn send(
            &self,
            request: evohime_service_contract::ServiceRequest,
        ) -> crate::module_registry::ServiceFuture<
            '_,
            Result<
                evohime_service_contract::ServiceResponse,
                evohime_service_contract::ServiceContractError,
            >,
        > {
            Box::pin(async move {
                match request {
                    evohime_service_contract::ServiceRequest::Invoke {
                        request_id,
                        capability,
                        payload,
                        ..
                    } if capability == "ambient.proactivity_decide" => {
                        let req: evohime_listener_contract::ProactivityDecideRequest =
                            serde_json::from_value(payload).map_err(|error| {
                                evohime_service_contract::ServiceContractError::invalid_input(
                                    error.to_string(),
                                )
                            })?;
                        let effect = evohime_listener_contract::ProactiveEffect::parse(&req.effect)
                            .ok_or_else(|| {
                                evohime_service_contract::ServiceContractError::invalid_input(
                                    format!("unknown proactive effect: {}", req.effect),
                                )
                            })?;
                        let decision = evohime_listener_contract::authorize_proactive(effect)
                            .and_then(|authorized| {
                                if req.muted {
                                    Err(evohime_listener_contract::ProposalRejection::Muted)
                                } else {
                                    req.budget
                                        .decide(
                                            &req.policy,
                                            req.counters.rolled(req.now_ms).snapshot(),
                                            req.now_ms,
                                            req.minute_of_day,
                                        )
                                        .map(|()| authorized)
                                        .map_err(
                                            evohime_listener_contract::ProposalRejection::Denied,
                                        )
                                }
                            });
                        let response = match decision {
                            Ok(authorized) => evohime_listener_contract::ProactivityDecideResponse {
                                authorized: true,
                                effect: Some(authorized.effect().as_str().to_owned()),
                                rejection: None,
                            },
                            Err(rejection) => evohime_listener_contract::ProactivityDecideResponse {
                                authorized: false,
                                effect: None,
                                rejection:
                                    evohime_listener_contract::ProactivityRejectionPayload::from_rejection(
                                        rejection,
                                    ),
                            },
                        };
                        Ok(evohime_service_contract::ServiceResponse::InvokeResult {
                            request_id,
                            payload: serde_json::to_value(response).map_err(|error| {
                                evohime_service_contract::ServiceContractError::new(
                                    evohime_service_contract::ServiceErrorCode::Internal,
                                    error.to_string(),
                                )
                            })?,
                        })
                    }
                    evohime_service_contract::ServiceRequest::Invoke {
                        request_id,
                        capability,
                        ..
                    } => Ok(evohime_service_contract::ServiceResponse::Error {
                        request_id: Some(request_id),
                        error: evohime_service_contract::ServiceContractError::not_found(format!(
                            "unknown capability: {capability}"
                        )),
                    }),
                    _ => Ok(evohime_service_contract::ServiceResponse::Pong {
                        contract_version: evohime_service_contract::SERVICE_CONTRACT_VERSION,
                    }),
                }
            })
        }
    }

    /// Тестовый реестр модулей с mock ambient-сервисом.
    pub(crate) fn ambient_module_registry() -> crate::module_registry::ModuleRegistry {
        let module_registry = crate::module_registry::ModuleRegistry::new();
        let descriptor = evohime_service_contract::ServiceDescriptor {
            contract_version: evohime_service_contract::SERVICE_CONTRACT_VERSION,
            service_id: "ambient".to_owned(),
            version: "0.1.0".to_owned(),
            capabilities: vec![evohime_service_contract::ServiceCapabilityDescriptor {
                name: "ambient.proactivity_decide".to_owned(),
                description: "Pure proactivity decision".to_owned(),
                input_schema: serde_json::json!({}),
                required_permissions: Vec::new(),
                supports_streaming: false,
            }],
        };
        module_registry
            .register(descriptor, std::sync::Arc::new(MockAmbientServiceTransport))
            .expect("ambient service registers");
        module_registry
    }

    pub(crate) fn temporary_journal(name: &str) -> (crate::EventJournal, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = crate::EventJournal::open(directory.path().join(format!("{name}.db")))
            .expect("journal opens");
        (journal, directory)
    }

    pub(crate) const NOW_MS: u64 = 1_770_000_000_000;
    pub(crate) const DAY: u64 = 24 * 60 * 60 * 1000;

    pub(crate) async fn seed_episode(
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
                &crate::core_journal_ambient::AmbientUtteranceInput {
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

    pub(crate) fn seed_proposal_record(
        proposal_id: &str,
        subject: &str,
        episode_id: Option<&str>,
        now_ms: u64,
    ) -> AmbientProposalRecord {
        let kind = ProposalKind::Reminder;
        let subject_key = evohime_listener_contract::subject_key(subject);
        let proposal_key = evohime_listener_contract::proposal_key(kind, &subject_key, now_ms);
        let mute_key = evohime_listener_contract::mute_key(kind, &subject_key);
        proposal_record(ProposalRecordInput {
            proposal_id,
            proposal_key: &proposal_key,
            mute_key: &mute_key,
            kind,
            subject_key: &subject_key,
            subject,
            title: "Напомнить купить хлеб",
            source_episode_id: episode_id,
            now_ms,
        })
    }
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::ambient_policy_store::save_policy;
    use crate::ambient_registry::test_support::*;
    use evohime_listener_contract::{
        ProactivityDenial, ProposalKind, ProposalState, DEFAULT_RETENTION_DAYS,
    };
    use evohime_local_storage::ambient_store::ProposalInsert;
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
                voice_commands: true,
                voice_commands_autorun: false,
            },
        )
        .expect("policy saves");
        let registry = AmbientProactivityRegistry::default();
        registry.set_data_dir(directory.to_path_buf()).await;
        registry
            .attach_module_registry(ambient_module_registry())
            .await;
        registry.ensure_loaded(journal).await;
        registry
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
            let key = evohime_listener_contract::mute_key(
                ProposalKind::Reminder,
                &evohime_listener_contract::subject_key(&subject),
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
        restarted
            .attach_module_registry(ambient_module_registry())
            .await;
        restarted.ensure_loaded(&journal).await;
        let key = evohime_listener_contract::mute_key(
            ProposalKind::Reminder,
            &evohime_listener_contract::subject_key("четвёртая тема"),
        );
        assert_eq!(
            restarted
                .decide(&journal, ProposalKind::Reminder, &key, now)
                .await,
            Err(evohime_listener_contract::ProposalRejection::Denied(
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
}
