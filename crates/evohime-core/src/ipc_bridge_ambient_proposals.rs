use super::*;

impl IpcBridge {
    /// Список ожидающих карточек (этап 04.7).
    ///
    /// Это единственный путь, по которому человекочитаемый текст предложения
    /// пересекает границу IPC: durable journal его не несёт, потому что
    /// `events` — append-only таблица, из которой ambient-содержимое пришлось
    /// бы вычищать. Тот же принцип, по которому `memory.pending` не несёт
    /// `statement`.
    ///
    /// Просроченные карточки снимаются здесь же: список, показывающий
    /// вчерашнее предложение как ждущее ответа, врал бы пользователю.
    pub(crate) async fn dispatch_list_ambient_proposals(
        &self,
        request: generated::ListAmbientProposals,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            50usize
        } else {
            (request.limit as usize).min(200)
        };
        let now_ms = crate::task_memory::now_millis();
        let _ = self.journal.expire_stale_ambient_proposals(now_ms).await;
        let budget = self.proactivity.budget().await;
        match self.journal.list_open_ambient_proposals(limit).await {
            Ok(records) => serde_json::json!({
                "proposals": records
                    .into_iter()
                    .map(|record| serde_json::json!({
                        "proposal_id": record.proposal_id,
                        "kind": record.kind.as_str(),
                        "subject": record.subject,
                        "title": record.title,
                        "source_episode_id": record.source_episode_id.unwrap_or_default(),
                        "created_at_ms": parse_timestamp_ms(&record.created_at),
                        "expires_at_ms": parse_timestamp_ms(&record.expires_at),
                        "occurrences": record.occurrences,
                        "state": record.state.as_str(),
                    }))
                    .collect::<Vec<_>>(),
                "max_per_hour": budget.max_per_hour,
                "max_per_day": budget.max_per_day,
                "min_interval_ms": budget.min_interval_ms,
                "error_code": "",
            }),
            Err(code) => serde_json::json!({
                "proposals": Vec::<serde_json::Value>::new(),
                "max_per_hour": budget.max_per_hour,
                "max_per_day": budget.max_per_day,
                "min_interval_ms": budget.min_interval_ms,
                "error_code": code.as_str(),
            }),
        }
    }

    /// Решение по ограниченному предложению (этап 04.7).
    ///
    /// Три исхода, а не два: принять, отклонить и «больше не предлагать
    /// такое». Принятие создаёт обычную задачу или неисполняемое напоминание
    /// штатным механизмом Core с сохранением провенанса; ни один другой
    /// эффект здесь недостижим.
    ///
    /// `idempotency_key` обязателен: без него двойной клик по карточке
    /// породил бы две задачи. Повтор с тем же ключом возвращает первое
    /// решение, а не создаёт второе.
    pub(crate) async fn dispatch_resolve_ambient_proposal(
        &self,
        request: generated::ResolveAmbientProposal,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        use evohime_listener_contract::ProposalState;

        let Ok(proposal_id) =
            evohime_listener_contract::ProposalId::new(request.proposal_id.clone())
        else {
            return resolve_failure(Code::InvalidArgument);
        };
        let idempotency_key = request.idempotency_key.trim().to_owned();
        if idempotency_key.is_empty() || idempotency_key.len() > MAX_PROPOSAL_KEY_BYTES {
            return resolve_failure(Code::InvalidArgument);
        }
        // Повтор того же клика: ответ берётся из уже принятого решения, и
        // вторая задача не создаётся.
        match self
            .journal
            .find_ambient_proposal_by_idempotency(&idempotency_key)
            .await
        {
            Ok(Some(existing)) => {
                return serde_json::json!({
                    "applied": true,
                    "state": existing.state.as_str(),
                    "task_id": existing.accepted_task_id.unwrap_or_default(),
                    "error_code": "",
                })
            }
            Ok(None) => {}
            Err(code) => return resolve_failure(code),
        }

        let record = match self
            .journal
            .get_ambient_proposal(proposal_id.as_str())
            .await
        {
            Ok(Some(record)) => record,
            // Нет такого предложения — это честное «не применено», а не
            // вымышленный успех.
            Ok(None) => return resolve_failure(Code::InvalidArgument),
            Err(code) => return resolve_failure(code),
        };
        if record.state.is_terminal() {
            return serde_json::json!({
                "applied": false,
                "state": record.state.as_str(),
                "task_id": record.accepted_task_id.unwrap_or_default(),
                "error_code": Code::InvalidArgument.as_str(),
            });
        }

        let now_ms = crate::task_memory::now_millis();
        let next_state = if request.mute {
            ProposalState::Muted
        } else if request.accepted {
            ProposalState::Accepted
        } else {
            ProposalState::Declined
        };

        // Задача создаётся только при принятии и только до перевода карточки
        // в терминальное состояние: обратный порядок оставил бы «принято» без
        // задачи, если бы создание не удалось.
        let task_id = if next_state == ProposalState::Accepted {
            match self.create_proposal_effect(&record, &idempotency_key).await {
                Ok(task_id) => Some(task_id),
                Err(code) => return resolve_failure(code),
            }
        } else {
            None
        };

        match self
            .journal
            .resolve_ambient_proposal_row(
                proposal_id.as_str(),
                next_state,
                now_ms,
                task_id.as_deref(),
                Some(&idempotency_key),
            )
            .await
        {
            Ok(true) => {}
            // Кто-то решил карточку между чтением и записью: первый клик
            // выигрывает.
            Ok(false) => return resolve_failure(Code::InvalidArgument),
            Err(code) => return resolve_failure(code),
        }

        if next_state == ProposalState::Muted {
            let _ = self
                .proactivity
                .mute(
                    &self.journal,
                    &record.mute_key,
                    record.kind,
                    &record.subject_key,
                    now_ms,
                )
                .await;
        }

        if let Ok(subject_key) =
            evohime_listener_contract::SubjectKey::new(record.subject_key.clone())
        {
            let _ = self
                .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Proposal {
                    proposal_id,
                    episode_id: record
                        .source_episode_id
                        .as_ref()
                        .and_then(|id| evohime_listener_contract::EpisodeId::new(id.clone()).ok()),
                    kind: record.kind,
                    subject_key,
                    proposal_state: next_state,
                })
                .await;
        }

        serde_json::json!({
            "applied": true,
            "state": next_state.as_str(),
            "task_id": task_id.unwrap_or_default(),
            "error_code": "",
        })
    }

    /// Единственный эффект принятого предложения: строка в списке задач.
    ///
    /// Оба вида — обычная запись `work_items` в статусе `backlog`, то есть
    /// ничего не запускающая сама. Напоминание отличается явным `non_goals`:
    /// «не выполняется автоматически» записано в данных, а не подразумевается.
    /// `source_ref` несёт `episode_id` — тот же провенанс, по которому
    /// удаление эпизода находит своих кандидатов памяти.
    pub(crate) async fn create_proposal_effect(
        &self,
        record: &evohime_local_storage::ambient_store::AmbientProposalRecord,
        idempotency_key: &str,
    ) -> Result<String, evohime_listener_contract::AmbientErrorCode> {
        use evohime_listener_contract::AmbientErrorCode as Code;

        // Проектная строка для услышанного заводится один раз и переиспользуется:
        // `work_items.project_id` — внешний ключ, и задача без проекта не
        // сохранится.
        self.journal
            .create_project(
                AMBIENT_PROPOSAL_PROJECT_ID,
                "Услышанное",
                "",
                Some(AMBIENT_PROPOSAL_PROJECT_ID),
            )
            .await
            .map_err(|_| Code::StorageFailed)?;

        let task_id = uuid::Uuid::new_v4().to_string();
        let non_goals = if record.kind == evohime_listener_contract::ProposalKind::Reminder {
            AMBIENT_REMINDER_NON_GOAL.to_owned()
        } else {
            String::new()
        };
        let item = evohime_local_storage::WorkItemRecord {
            id: task_id.clone(),
            project_id: AMBIENT_PROPOSAL_PROJECT_ID.to_owned(),
            parent_id: None,
            title: record.title.clone(),
            description: String::new(),
            source_ref: record.source_episode_id.clone(),
            acceptance_criteria: String::new(),
            non_goals,
            // `backlog`, а не `ready`: подбор следующей задачи берёт только
            // `ready`, поэтому принятое предложение не начинает выполняться
            // само по себе.
            status: "backlog".to_owned(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        };
        // Тот же dedup-путь, что у `CreateTask`: повторный запрос с этим
        // ключом не создаёт второй записи, а возвращает **ту** задачу, что
        // была создана первым кликом. Свежий идентификатор здесь был бы
        // ссылкой в пустоту.
        if let Some(replay) = self
            .journal
            .record_deduplicated(
                AMBIENT_PROPOSAL_CLIENT_ID,
                idempotency_key,
                &record.proposal_id,
                b"",
            )
            .await
            .map_err(|_| Code::StorageFailed)?
        {
            return String::from_utf8(replay).map_err(|_| Code::StorageFailed);
        }
        self.journal
            .create_work_item(&item)
            .await
            .map_err(|_| Code::StorageFailed)?;
        self.journal
            .record_deduplicated(
                AMBIENT_PROPOSAL_CLIENT_ID,
                idempotency_key,
                &record.proposal_id,
                task_id.as_bytes(),
            )
            .await
            .map_err(|_| Code::StorageFailed)?;
        Ok(task_id)
    }

    /// Очередь услышанных команд. Заголовок приложения приходит только здесь:
    /// событие журнала несёт лишь ключ каталога.
    pub(crate) fn dispatch_list_voice_commands(
        &self,
        request: generated::ListVoiceCommands,
    ) -> serde_json::Value {
        let now_ms = crate::task_memory::now_millis();
        let policy = crate::ambient::load_policy(&self.ambient_data_dir());
        let limit = usize::try_from(request.limit)
            .unwrap_or(VOICE_COMMAND_MAX_PENDING)
            .clamp(1, VOICE_COMMAND_MAX_PENDING);
        let commands = self
            .voice_commands
            .list(now_ms)
            .into_iter()
            .take(limit)
            .map(|command| {
                serde_json::json!({
                    "command_id": command.command_id,
                    "kind": command.kind.as_str(),
                    "app_id": command.app_id,
                    "title": command.title,
                    "created_at_ms": command.created_at_ms,
                    "expires_at_ms": command.expires_at_ms(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "commands": commands,
            "requires_confirmation": !policy.voice_commands_autorun,
        })
    }

    /// Решение по услышанной команде.
    ///
    /// Карточка снимается с очереди до запуска, а не после: иначе двойной клик
    /// открыл бы два окна. Второй клик поэтому находит пустоту и получает
    /// `not_found`, а не второй запуск.
    pub(crate) async fn dispatch_resolve_voice_command(
        &self,
        request: generated::ResolveVoiceCommand,
    ) -> serde_json::Value {
        use evohime_listener_contract::VoiceCommandState;

        let now_ms = crate::task_memory::now_millis();
        let Some(command) = self.voice_commands.take(&request.command_id, now_ms) else {
            return serde_json::json!({
                "launched": false,
                "state": VoiceCommandState::Expired.as_str(),
                "app_id": "",
                "error_code": "not_found",
            });
        };
        if !request.accepted {
            self.publish_voice_command(&command, VoiceCommandState::Declined)
                .await;
            return serde_json::json!({
                "launched": false,
                "state": VoiceCommandState::Declined.as_str(),
                "app_id": command.app_id,
                "error_code": "",
            });
        }
        let registry = self.voice_commands.clone();
        let launch_command = command.clone();
        let launched = match tokio::task::spawn_blocking(move || {
            launch_voice_command(&registry, &launch_command, now_ms)
        })
        .await
        {
            Ok(result) => result,
            Err(error) => {
                tracing::error!(%error, "voice command launch task failed");
                Err("voice command launch task failed".to_owned())
            }
        };
        match launched {
            Ok(_) => {
                self.publish_voice_command(&command, VoiceCommandState::Launched)
                    .await;
                serde_json::json!({
                    "launched": true,
                    "state": VoiceCommandState::Launched.as_str(),
                    "app_id": command.app_id,
                    "error_code": "",
                })
            }
            Err(error) => {
                self.publish_voice_command(&command, VoiceCommandState::Failed)
                    .await;
                // Текст ошибки идёт в трассу, а не в ответ: в нём путь к
                // исполняемому файлу, которому в UI делать нечего.
                crate::write_model_trace(
                    "ambient.voice_command.launch_failed",
                    serde_json::json!({ "app_id": command.app_id, "error": error }),
                );
                serde_json::json!({
                    "launched": false,
                    "state": VoiceCommandState::Failed.as_str(),
                    "app_id": command.app_id,
                    "error_code": "launch_failed",
                })
            }
        }
    }
}

pub(crate) const VOICE_COMMAND_TTL_MS: u64 = 5 * 60 * 1000;
pub(crate) const VOICE_COMMAND_MAX_PENDING: usize = 8;
const VOICE_CATALOG_TTL_MS: u64 = 5 * 60 * 1000;

/// Услышанная голосовая команда, ждущая решения в памяти ядра.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingVoiceCommand {
    pub command_id: String,
    pub kind: evohime_listener_contract::VoiceCommandKind,
    pub app_id: String,
    pub title: String,
    pub created_at_ms: u64,
}

impl PendingVoiceCommand {
    pub const fn expires_at_ms(&self) -> u64 {
        self.created_at_ms.saturating_add(VOICE_COMMAND_TTL_MS)
    }

    fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms()
    }
}

/// Решение ядра по голосовой фразе после разбора в сервисе `media-voice`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VoiceCommandDecision {
    Ignore,
    Confirm(PendingVoiceCommand),
    Autorun(PendingVoiceCommand),
}

/// Очередь услышанных команд и кэш каталога приложений на стороне ядра.
#[derive(Debug)]
pub struct VoiceCommandRegistry {
    pending: std::sync::Mutex<std::collections::VecDeque<PendingVoiceCommand>>,
    catalog: std::sync::Mutex<
        Option<(
            u64,
            std::sync::Arc<evohime_tool_runtime::app_catalog::AppCatalog>,
        )>,
    >,
    fixed_catalog: Option<std::sync::Arc<evohime_tool_runtime::app_catalog::AppCatalog>>,
}

impl Default for VoiceCommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceCommandRegistry {
    /// Creates an empty voice command queue.
    pub fn new() -> Self {
        Self {
            pending: std::sync::Mutex::new(std::collections::VecDeque::new()),
            catalog: std::sync::Mutex::new(None),
            fixed_catalog: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_catalog(catalog: evohime_tool_runtime::app_catalog::AppCatalog) -> Self {
        Self {
            fixed_catalog: Some(std::sync::Arc::new(catalog)),
            ..Self::new()
        }
    }

    fn catalog(
        &self,
        now_ms: u64,
    ) -> std::sync::Arc<evohime_tool_runtime::app_catalog::AppCatalog> {
        if let Some(catalog) = &self.fixed_catalog {
            return catalog.clone();
        }
        let mut slot = lock_voice_mutex(&self.catalog);
        if let Some((built_at, catalog)) = slot.as_ref() {
            if now_ms.saturating_sub(*built_at) < VOICE_CATALOG_TTL_MS && now_ms >= *built_at {
                return catalog.clone();
            }
        }
        let catalog = std::sync::Arc::new(evohime_tool_runtime::app_catalog::default_catalog());
        *slot = Some((now_ms, catalog.clone()));
        catalog
    }

    pub(crate) fn list(&self, now_ms: u64) -> Vec<PendingVoiceCommand> {
        let mut pending = lock_voice_mutex(&self.pending);
        pending.retain(|command| !command.is_expired(now_ms));
        pending.iter().cloned().collect()
    }

    pub(crate) fn take(&self, command_id: &str, now_ms: u64) -> Option<PendingVoiceCommand> {
        let mut pending = lock_voice_mutex(&self.pending);
        pending.retain(|command| !command.is_expired(now_ms));
        let index = pending
            .iter()
            .position(|command| command.command_id == command_id)?;
        pending.remove(index)
    }

    pub(crate) fn expire(&self, now_ms: u64) -> Vec<PendingVoiceCommand> {
        let mut pending = lock_voice_mutex(&self.pending);
        let expired: Vec<PendingVoiceCommand> = pending
            .iter()
            .filter(|command| command.is_expired(now_ms))
            .cloned()
            .collect();
        pending.retain(|command| !command.is_expired(now_ms));
        expired
    }

    fn record(&self, command: PendingVoiceCommand) {
        let mut pending = lock_voice_mutex(&self.pending);
        pending.retain(|item| !item.is_expired(command.created_at_ms));
        pending.retain(|item| item.app_id != command.app_id);
        pending.push_back(command);
        while pending.len() > VOICE_COMMAND_MAX_PENDING {
            pending.pop_front();
        }
    }
}

fn lock_voice_mutex<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(error) => {
            tracing::error!("voice command registry mutex poisoned; recovering state");
            error.into_inner()
        }
    }
}

/// Делегирует разбор фразы в сервис `media-voice` и сопоставляет цель с
/// локальным каталогом приложений. При недоступности сервиса работает fail-closed.
pub(crate) async fn decide_voice_command(
    module_registry: &crate::module_registry::ModuleRegistry,
    registry: &VoiceCommandRegistry,
    policy: &evohime_listener_contract::AmbientPolicy,
    text: &str,
    now_ms: u64,
    command_id: String,
) -> VoiceCommandDecision {
    if !policy.voice_commands || policy.paused {
        return VoiceCommandDecision::Ignore;
    }
    let parsed = match module_registry
        .invoke_if_ready(
            format!("voice-parse-{command_id}"),
            "voice.command_parse",
            serde_json::json!({ "text": text }),
            None,
            |_| Ok(()),
        )
        .await
    {
        Some(Ok(value)) => value,
        Some(Err(err)) => {
            tracing::warn!(%err, "media-voice voice.command_parse failed; ignoring utterance");
            return VoiceCommandDecision::Ignore;
        }
        None => return VoiceCommandDecision::Ignore,
    };
    if !parsed
        .get("matched")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return VoiceCommandDecision::Ignore;
    }
    let Some(target) = parsed
        .get("target")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return VoiceCommandDecision::Ignore;
    };
    let catalog = registry.catalog(now_ms);
    let evohime_tool_runtime::app_catalog::Resolution::Found(entry) = catalog.resolve(target)
    else {
        return VoiceCommandDecision::Ignore;
    };
    let command = PendingVoiceCommand {
        command_id,
        kind: evohime_listener_contract::VoiceCommandKind::OpenApp,
        app_id: entry.id,
        title: entry.title,
        created_at_ms: now_ms,
    };
    if policy.voice_commands_autorun {
        VoiceCommandDecision::Autorun(command)
    } else {
        registry.record(command.clone());
        VoiceCommandDecision::Confirm(command)
    }
}

pub(crate) fn launch_voice_command(
    registry: &VoiceCommandRegistry,
    command: &PendingVoiceCommand,
    now_ms: u64,
) -> Result<u32, String> {
    let catalog = registry.catalog(now_ms);
    let Some(entry) = catalog.get(&command.app_id) else {
        return Err("каталог больше не знает это приложение".to_owned());
    };
    evohime_tool_runtime::app_catalog::launch(entry).map_err(|error| error.to_string())
}
