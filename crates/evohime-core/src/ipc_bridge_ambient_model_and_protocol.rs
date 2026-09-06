use super::*;

impl IpcBridge {
    pub(super) async fn dispatch_model_and_protocol<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        _request_id: String,
        _client_id: String,
        _command_hash: String,
        command: Option<generated::command_envelope::Command>,
        stale_generation: bool,
    ) -> Result<(), IpcBridgeError> {
        match command {
            Some(generated::command_envelope::Command::ResyncRequest(request)) => {
                evohime_desktop_ipc::validate_resync_request(&request)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                if stale_generation {
                    let latest = self.latest_sequence().await;
                    let gap = self.replay_gap_envelope(
                        request.after_sequence,
                        None,
                        latest,
                        REPLAY_GAP_REASON_STALE_GENERATION,
                    );
                    transport::write_frame(writer, &gap.encode_to_vec()).await?;
                }
                let limit = if request.max_events == 0 {
                    evohime_desktop_ipc::DEFAULT_RESYNC_MAX_EVENTS
                } else {
                    request.max_events
                } as usize;
                let batch = self
                    .journal
                    .replay_bounded(request.after_sequence as i64, limit)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let last_sequence = batch
                    .events
                    .last()
                    .map(|record| record.sequence_id as u64)
                    .unwrap_or(request.after_sequence);
                if batch.gap_detected {
                    let latest = self.latest_sequence().await;
                    let gap = self.replay_gap_envelope(
                        request.after_sequence,
                        batch.first_available_sequence.map(|value| value as u64),
                        latest,
                        REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED,
                    );
                    transport::write_frame(writer, &gap.encode_to_vec()).await?;
                }
                // Снапшот, не влезающий в кадр IPC, раньше обрывал соединение с
                // оболочкой: она навсегда оставалась без состояния и рисовала
                // «нет связи». Теперь превышение лимита деградирует до
                // поштучной отправки тех же событий.
                let snapshot = if request.include_full_snapshot {
                    let snapshot_json = serde_json::to_vec(&serde_json::json!({
                        "schema_version": 1,
                        "core_instance_id": self.core_instance_id,
                        "session_epoch": self.session_epoch,
                        "snapshot_sequence_id": last_sequence,
                        "after_sequence": request.after_sequence,
                        "actions": typed_snapshot_actions(&batch.events),
                        "events": batch.events.iter().map(|record| serde_json::json!({
                            "sequence_id": record.sequence_id,
                            "task_id": record.task_id,
                            "event_type": record.event_type,
                            "payload": record.payload,
                            "created_at": record.created_at,
                        })).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let candidate = generated::FullSnapshot {
                        sequence_id: last_sequence,
                        snapshot_json,
                    };
                    match evohime_desktop_ipc::validate_full_snapshot(&candidate) {
                        Ok(()) => Some(candidate),
                        Err(error) => {
                            tracing::warn!(
                                event = "ipc.snapshot_oversized",
                                error = %error,
                                events = batch.events.len(),
                                snapshot_bytes = candidate.snapshot_json.len(),
                                "снапшот не влез в кадр, переходим на поштучную отправку"
                            );
                            let payload = serde_json::to_vec(&serde_json::json!({
                                "after_sequence": request.after_sequence,
                                "last_sequence": last_sequence,
                                "events": batch.events.len(),
                                "snapshot_bytes": candidate.snapshot_json.len(),
                                "reason": "snapshot_too_large",
                            }))
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                            self.write_response(writer, "replay.snapshot_skipped", payload)
                                .await?;
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some(snapshot) = snapshot {
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "replay.full_snapshot".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: Some(generated::event_envelope::Event::FullSnapshot(snapshot)),
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                } else {
                    for record in batch.events {
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: record.sequence_id as u64,
                            task_id: record.task_id,
                            event_type: record.event_type,
                            payload: record.payload,
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: None,
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    }
                }
                // Каждый resync отдаёт не больше `limit` событий за раз. Без
                // этого флага оболочка узнавала об оставшемся хвосте истории
                // только по случайному разрыву sequence в живом потоке — и
                // гонялась за ним кругами, так и не догоняя (план про «нет
                // связи», возникавшую после больших сессий).
                let latest_after_batch = self.latest_sequence().await;
                let end_payload = serde_json::to_vec(&serde_json::json!({
                    "more_available": last_sequence < latest_after_batch,
                    "latest_sequence": latest_after_batch,
                }))
                .map_err(|error| FrameError::Io(error.to_string()))?;
                let end = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: last_sequence,
                    task_id: String::new(),
                    event_type: "resync.end".into(),
                    payload: end_payload,
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &end.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ReplayEvents(replay)) => {
                if stale_generation {
                    let latest = self.latest_sequence().await;
                    let gap = self.replay_gap_envelope(
                        replay.after_sequence,
                        None,
                        latest,
                        REPLAY_GAP_REASON_STALE_GENERATION,
                    );
                    transport::write_frame(writer, &gap.encode_to_vec()).await?;
                }
                let batch = self
                    .journal
                    .replay_bounded(replay.after_sequence as i64, 1_000)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let mut last_sequence = batch.last_sequence as u64;
                if batch.gap_detected {
                    let latest = self.latest_sequence().await;
                    let gap = self.replay_gap_envelope(
                        replay.after_sequence,
                        batch.first_available_sequence.map(|value| value as u64),
                        latest,
                        REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED,
                    );
                    transport::write_frame(writer, &gap.encode_to_vec()).await?;
                }
                for record in batch.events {
                    last_sequence = record.sequence_id as u64;
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: record.sequence_id as u64,
                        task_id: record.task_id,
                        event_type: record.event_type,
                        payload: record.payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                let end = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: last_sequence,
                    task_id: String::new(),
                    event_type: "replay.end".into(),
                    payload: Vec::new(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &end.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::SelectModel(request)) => {
                // Bounded: a model identifier is a short single-line token.
                let model = request.model.trim();
                if model.len() > 128 || model.contains(char::is_whitespace) {
                    self.write_response(
                        writer,
                        "model.select.rejected",
                        serde_json::to_vec(&serde_json::json!({ "reason": "invalid_model" }))
                            .unwrap_or_default(),
                    )
                    .await?;
                    return Ok(());
                }
                let Some(route) = self
                    .gateway_config
                    .as_ref()
                    .and_then(|config| config.routes.get(&config.default_route))
                else {
                    self.write_response(
                        writer,
                        "model.select.rejected",
                        serde_json::to_vec(
                            &serde_json::json!({ "reason": "provider_not_configured" }),
                        )
                        .unwrap_or_default(),
                    )
                    .await?;
                    return Ok(());
                };
                let available = evohime_model_gateway::fetch_model_catalog(route)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                if !available.iter().any(|entry| entry.id == model) {
                    self.write_response(
                        writer,
                        "model.select.rejected",
                        serde_json::to_vec(
                            &serde_json::json!({ "reason": "model_not_returned_by_provider" }),
                        )
                        .unwrap_or_default(),
                    )
                    .await?;
                    return Ok(());
                }
                self.selected_model.set(model);
                let payload = match serde_json::to_vec(&self.current_model_config()) {
                    Ok(payload) => payload,
                    Err(error) => {
                        tracing::warn!(%error, "model config serialization failed");
                        b"null".to_vec()
                    }
                };
                self.write_response(writer, "model.config", payload).await?;
            }
            Some(generated::command_envelope::Command::ModelConfig(_)) => {
                let payload = match serde_json::to_vec(&self.current_model_config()) {
                    Ok(payload) => payload,
                    Err(error) => {
                        tracing::warn!(%error, "model config serialization failed");
                        b"null".to_vec()
                    }
                };
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "model.config".into(),
                    payload,
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ModelCatalog(request)) => {
                let mode = if request.mode == "paid" {
                    "paid"
                } else {
                    "free"
                };
                let provider = self
                    .gateway_config
                    .as_ref()
                    .and_then(|config| config.routes.get(&config.default_route))
                    .map(|route| route.provider.as_str().to_string())
                    .unwrap_or_else(|| "unknown".into());
                let result = self
                    .gateway_config
                    .as_ref()
                    .and_then(|config| config.routes.get(&config.default_route))
                    .map(|route| async move {
                        evohime_model_gateway::fetch_model_catalog(route)
                            .await
                            .map(|entries| {
                                entries
                                    .into_iter()
                                    .filter(|entry| {
                                        if mode == "free" {
                                            entry.id.ends_with(":free")
                                        } else {
                                            !entry.id.ends_with(":free")
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            })
                    });
                let (entries, error) = match result {
                    Some(request) => request.await,
                    None => Err(evohime_model_gateway::providers::ProviderError::Config(
                        "provider is not configured".into(),
                    )),
                }
                .map_or_else(
                    |error| (Vec::new(), Some(error.to_string())),
                    |entries| (entries, None),
                );
                // Лимиты переживают сессию: планировщик контекста и ревью
                // должны знать окно модели ещё до первого обновления каталога,
                // а неудачный запрос не должен стирать то, что уже известно.
                self.remember_model_limits(&provider, &entries).await;
                let models = entries
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect::<Vec<_>>();
                let limits = entries
                    .iter()
                    .map(|entry| {
                        (
                            entry.id.clone(),
                            serde_json::json!({
                                "context": entry.context_tokens,
                                "maxOutput": entry.max_output_tokens,
                            }),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>();
                let payload = serde_json::json!({
                    "mode": mode,
                    "models": models,
                    "limits": limits,
                    "error": error,
                });
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "model.catalog".into(),
                    payload: serde_json::to_vec(&payload).unwrap_or_default(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }

            _ => unreachable!("command routed to the wrong domain"),
        }
        Ok(())
    }
}
