use super::*;

impl IpcBridge {
    // ------------------------------------------------------------------
    // Постоянное слушание (план 04.5).
    //
    // Девять команд ходят прямо через мост, а не через очередь задач: им
    // нужны журнал, разрешения и реестр состояния, и ни одна из них не
    // запускает агента. Ответ уходит JSON-полезной нагрузкой тем же
    // `write_response`, что и у чеков.
    // ------------------------------------------------------------------

    /// Включение, пауза и смена устройства — одна команда с тремя полями.
    ///
    /// Порядок здесь и есть контракт: сперва проверки, потом сохранение
    /// намерения на диск, потом команда листенеру. Намерение переживает
    /// отсутствие листенера — иначе включение при упавшем процессе молча
    /// пропало бы, а пользователь считал бы, что микрофон включён.
    pub(crate) async fn dispatch_set_ambient_listening(
        &self,
        request: generated::SetAmbientListening,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;

        let data_dir = self.ambient_data_dir();
        let snapshot = self.ambient.snapshot().await;

        // Идентификатор устройства проходит bounded-контракт 04.1: через это
        // поле нельзя протащить фразу.
        if !request.device_id.is_empty()
            && evohime_listener_contract::DeviceId::new(request.device_id.clone()).is_err()
        {
            return listening_result(snapshot.state, Some(Code::InvalidArgument));
        }
        if !request.device_id.is_empty()
            && !snapshot
                .devices
                .iter()
                .any(|device| device.device_id == request.device_id)
        {
            return listening_result(snapshot.state, Some(Code::DeviceDisconnected));
        }

        // Микрофон открывается только явным именованным вызовом: общий режим
        // доступа его не трогает (инвариант 04.1), поэтому и здесь он
        // выставляется отдельно и по имени.
        if let Some(tools) = &self.tools {
            tools
                .permissions()
                .set_mode(
                    Permission::MicrophoneListen,
                    if request.enabled {
                        PermissionMode::Allow
                    } else {
                        PermissionMode::Deny
                    },
                )
                .await;
        }

        let mut policy = crate::ambient::load_policy(&data_dir);
        policy.paused = request.paused;
        if crate::ambient::save_policy(&data_dir, &policy).is_err() {
            return listening_result(snapshot.state, Some(Code::StorageFailed));
        }
        let control = crate::ambient::AmbientControl {
            enabled: request.enabled,
            device_id: if request.device_id.is_empty() {
                snapshot.active_device_id.clone()
            } else {
                request.device_id.clone()
            },
        };
        if crate::ambient::save_control(&data_dir, &control).is_err() {
            return listening_result(snapshot.state, Some(Code::StorageFailed));
        }

        let sent = self
            .ambient
            .send(crate::ambient::ListenerControl::Policy(Box::new((
                policy,
                control.clone(),
            ))))
            .await;
        if let Err(code) = sent {
            // Листенера нет. Намерение уже сохранено и применится при его
            // следующем подключении, но утверждать, что микрофон включён,
            // нельзя.
            self.ambient
                .set_state(
                    ListeningState::EngineUnavailable,
                    ListeningReason::EngineUnavailable,
                    None,
                )
                .await;
            self.publish_ambient_state().await;
            return listening_result(ListeningState::EngineUnavailable, Some(code));
        }

        // Устройство занято другим приложением — включать нечего, и
        // оптимистичное «запускаюсь» здесь было бы враньём.
        if request.enabled && snapshot.state == ListeningState::DeviceConflict {
            return listening_result(snapshot.state, Some(Code::DeviceConflict));
        }

        // Оптимистичное состояние: настоящее приедет от листенера отдельным
        // `ambient.state`, и именно оно останется в реестре.
        let (state, reason) = if !request.enabled {
            (ListeningState::Stopped, ListeningReason::UserRequest)
        } else if request.paused {
            (ListeningState::PausedByUser, ListeningReason::UserRequest)
        } else {
            (ListeningState::Starting, ListeningReason::UserRequest)
        };
        let device_id = control.device_id.clone();
        if self.ambient.set_state(state, reason, Some(device_id)).await {
            self.publish_ambient_state().await;
        }
        let engine_ready = self.ambient.engine_ready().await;
        let failure =
            (request.enabled && !request.paused && !engine_ready).then_some(Code::EngineNotReady);
        listening_result(state, failure)
    }

    /// Публикует текущее состояние реестра одним `ambient.state`.
    pub(crate) async fn publish_ambient_state(&self) {
        let snapshot = self.ambient.snapshot().await;
        let _ = self
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::State {
                state: snapshot.state,
                reason: snapshot.reason,
                active_device_id: evohime_listener_contract::DeviceId::new(
                    snapshot.active_device_id,
                )
                .ok(),
            })
            .await;
    }

    pub(crate) async fn dispatch_get_ambient_status(&self) -> serde_json::Value {
        let snapshot = self.ambient.snapshot().await;
        serde_json::json!({
            "state": snapshot.state,
            "reason": snapshot.reason,
            "active_device_id": snapshot.active_device_id,
            "engine_version": snapshot.engine_version,
            "engine_ready": snapshot.engine_ready,
            "devices": snapshot.devices,
            "watching_devices": snapshot.watching_devices,
        })
    }

    /// Список эпизодов. Текста здесь нет: он отдаётся только
    /// `GetAmbientEpisode` и только по явному клику пользователя.
    pub(crate) async fn dispatch_list_ambient_episodes(
        &self,
        request: generated::ListAmbientEpisodes,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            50usize
        } else {
            (request.limit as usize).min(200)
        };
        // Стор отдаёт свежие первыми и не умеет курсора, поэтому окно
        // вырезается здесь: берётся на одну строку больше запрошенного, и
        // лишняя строка и есть ответ на вопрос «есть ли ещё».
        let records = match self.journal.list_ambient_episodes(limit * 4).await {
            Ok(records) => records,
            Err(code) => return serde_json::json!({ "error_code": code.as_str() }),
        };
        let mut rows: Vec<serde_json::Value> = Vec::new();
        let mut skipping = !request.cursor.is_empty();
        let mut next_cursor = String::new();
        for record in records {
            if skipping {
                if record.episode_id == request.cursor {
                    skipping = false;
                }
                continue;
            }
            let started_at_ms = parse_timestamp_ms(&record.started_at);
            if request.since_ms > 0 && started_at_ms < request.since_ms {
                continue;
            }
            if rows.len() == limit {
                next_cursor = record.episode_id;
                break;
            }
            rows.push(serde_json::json!({
                "episode_id": record.episode_id,
                "started_at_ms": started_at_ms,
                "speech_duration_ms": record.speech_ms,
                "utterance_count": record.utterance_count,
                "extraction_state": record.extraction_state.as_str(),
            }));
        }
        serde_json::json!({ "episodes": rows, "next_cursor": next_cursor })
    }

    /// Единственный путь, по которому распознанный текст пересекает границу
    /// IPC. Вызывается только явным раскрытием эпизода в панели.
    pub(crate) async fn dispatch_get_ambient_episode(
        &self,
        request: generated::GetAmbientEpisode,
    ) -> serde_json::Value {
        if request.episode_id.is_empty() {
            return serde_json::json!({
                "error_code": evohime_listener_contract::AmbientErrorCode::InvalidArgument.as_str()
            });
        }
        match self
            .journal
            .list_ambient_utterances(&request.episode_id, 500)
            .await
        {
            Ok(records) => serde_json::json!({
                "episode_id": request.episode_id,
                "utterances": records
                    .into_iter()
                    .map(|record| serde_json::json!({
                        "utterance_id": record.utterance_id,
                        "started_at_ms": parse_timestamp_ms(&record.started_at),
                        "duration_ms": record.duration_ms,
                        "text": record.text,
                        "language": record.language,
                        "redacted": record.redacted,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Err(code) => serde_json::json!({ "error_code": code.as_str() }),
        }
    }

    /// Удаление транскриптов. Без `confirmed` команда отвергается здесь, а не
    /// только модальным окном оболочки: обход UI не должен давать больше прав.
    pub(crate) async fn dispatch_delete_ambient_transcripts(
        &self,
        request: generated::DeleteAmbientTranscripts,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        if !request.confirmed {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::ConfirmationRequired.as_str(),
            });
        }
        let now_ms = crate::task_memory::now_millis();
        let targets: Vec<String> = if request.all {
            match self.journal.list_ambient_episodes(500).await {
                Ok(records) => records.into_iter().map(|r| r.episode_id).collect(),
                Err(code) => {
                    return serde_json::json!({
                        "deleted_count": 0,
                        "error_code": code.as_str(),
                    })
                }
            }
        } else {
            request.episode_ids
        };
        if targets.is_empty() && !request.all {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::InvalidArgument.as_str(),
            });
        }
        let mut deleted = 0u32;
        for episode_id in targets {
            match self
                .journal
                .delete_ambient_episode(&episode_id, now_ms)
                .await
            {
                Ok(deletion) => {
                    deleted = deleted.saturating_add(deletion.utterances_removed as u32)
                }
                Err(code) => {
                    return serde_json::json!({
                        "deleted_count": deleted,
                        "error_code": code.as_str(),
                    })
                }
            }
        }
        let _ = self
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Retention {
                deleted_count: deleted,
                trigger: evohime_listener_contract::RetentionTrigger::Manual,
            })
            .await;
        serde_json::json!({ "deleted_count": deleted, "error_code": "" })
    }

    /// «Забыть последние N минут». Окно приходит в миллисекундах и
    /// округляется вверх: половина минуты — это тоже минута, и оставить её
    /// значило бы не забыть то, что просили забыть.
    pub(crate) async fn dispatch_forget_ambient_window(
        &self,
        request: generated::ForgetAmbientWindow,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        if !request.confirmed {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::ConfirmationRequired.as_str(),
            });
        }
        if request.window_ms <= 0 {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::InvalidArgument.as_str(),
            });
        }
        let minutes = u32::try_from((request.window_ms + 59_999) / 60_000).unwrap_or(u32::MAX);
        let now_ms = crate::task_memory::now_millis();
        match self.journal.forget_ambient_window(minutes, now_ms).await {
            Ok(deletion) => {
                let deleted = deletion.utterances_removed as u32;
                let _ = self
                    .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Retention {
                        deleted_count: deleted,
                        trigger: evohime_listener_contract::RetentionTrigger::ForgetWindow,
                    })
                    .await;
                serde_json::json!({ "deleted_count": deleted, "error_code": "" })
            }
            Err(code) => serde_json::json!({
                "deleted_count": 0,
                "error_code": code.as_str(),
            }),
        }
    }

    pub(crate) fn ambient_policy_json(
        policy: &evohime_listener_contract::AmbientPolicy,
    ) -> serde_json::Value {
        serde_json::json!({
            "quiet_hours": policy
                .quiet_hours
                .iter()
                .map(|window| serde_json::json!({
                    "start_minute": window.start_minute,
                    "end_minute": window.end_minute,
                }))
                .collect::<Vec<_>>(),
            "blocklist_patterns": policy.process_blocklist,
            "window_title_blocklist": policy.window_title_blocklist,
            "retention_days": policy.retention_days,
            "voice_commands": policy.voice_commands,
            "voice_commands_autorun": policy.voice_commands_autorun,
        })
    }

    pub(crate) async fn dispatch_get_ambient_policy(&self) -> serde_json::Value {
        let policy = crate::ambient::load_policy(&self.ambient_data_dir());
        Self::ambient_policy_json(&policy)
    }

    /// Сохранение политики. Невалидная политика не применяется целиком:
    /// частичное применение превратило бы «запретить zoom» в «слушать всё».
    pub(crate) async fn dispatch_save_ambient_policy(
        &self,
        request: generated::SaveAmbientPolicy,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        let Some(incoming) = request.policy else {
            return serde_json::json!({ "applied": false, "error_code": Code::InvalidArgument.as_str() });
        };
        let data_dir = self.ambient_data_dir();
        let previous = crate::ambient::load_policy(&data_dir);
        let mut quiet_hours = Vec::new();
        for window in &incoming.quiet_hours {
            let (Ok(start), Ok(end)) = (
                u32::try_from(window.start_minute),
                u32::try_from(window.end_minute),
            ) else {
                return serde_json::json!({ "applied": false, "error_code": Code::PolicyInvalid.as_str() });
            };
            match evohime_listener_contract::QuietHours::new(start, end) {
                Ok(window) => quiet_hours.push(window),
                Err(error) => {
                    return serde_json::json!({
                        "applied": false,
                        "error_code": error.code().as_str(),
                    })
                }
            }
        }
        let Ok(retention_days) = u32::try_from(incoming.retention_days) else {
            return serde_json::json!({ "applied": false, "error_code": Code::PolicyInvalid.as_str() });
        };
        let policy = evohime_listener_contract::AmbientPolicy {
            // Пауза не редактируется политикой: она принадлежит переключателю
            // и меняется только `SetAmbientListening`.
            paused: previous.paused,
            quiet_hours,
            process_blocklist: incoming.blocklist_patterns,
            window_title_blocklist: incoming.window_title_blocklist,
            retention_days,
            // Поля добавлены позже самого сообщения: клиент, который о них не
            // знает, не шлёт их вовсе, и сохранённое значение остаётся своим.
            // Подстановка `false` вместо этого выключала бы голосовые команды
            // при любом сохранении блок-листа старым клиентом.
            voice_commands: incoming.voice_commands.unwrap_or(previous.voice_commands),
            voice_commands_autorun: incoming
                .voice_commands_autorun
                .unwrap_or(previous.voice_commands_autorun),
        };
        if let Err(error) = policy.validate() {
            return serde_json::json!({
                "applied": false,
                "error_code": error.code().as_str(),
            });
        }
        if crate::ambient::save_policy(&data_dir, &policy).is_err() {
            return serde_json::json!({ "applied": false, "error_code": Code::StorageFailed.as_str() });
        }
        // Сохранённая политика ничего не значит, пока листенер её не получил:
        // недоступный листенер называется своим кодом, а не «применено».
        let control = crate::ambient::load_control(&data_dir);
        match self
            .ambient
            .send(crate::ambient::ListenerControl::Policy(Box::new((
                policy, control,
            ))))
            .await
        {
            Ok(()) => serde_json::json!({ "applied": true, "error_code": "" }),
            Err(code) => serde_json::json!({ "applied": false, "error_code": code.as_str() }),
        }
    }
}
