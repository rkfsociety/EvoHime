use super::*;
use evohime_desktop_ipc::MAX_FRAME_BYTES;

/// Filters catalog entries by free/paid tier only for LiteRouter, which uses
/// the `:free` model ID suffix convention. Other providers return all models.
fn filter_catalog_entries_by_tier(
    provider: evohime_model_gateway::providers::ProviderKind,
    mode: &str,
    entries: Vec<evohime_model_gateway::ModelCatalogEntry>,
) -> Vec<evohime_model_gateway::ModelCatalogEntry> {
    if provider != evohime_model_gateway::providers::ProviderKind::LiteRouter {
        return entries;
    }
    entries
        .into_iter()
        .filter(|entry| {
            if mode == "free" {
                entry.id.ends_with(":free")
            } else {
                !entry.id.ends_with(":free")
            }
        })
        .collect()
}

/// Parses an optional `profile[:tier]` selector for Hive model catalog requests.
///
/// When `tier` is omitted, the tier is inferred from the currently configured
/// Hive model (if it does not end with `:free`, `"paid"`, otherwise `"free"`).
fn parse_hive_catalog_profile_and_tier<'a>(
    raw_requested: &'a str,
    stored_profile: Option<&'a str>,
    stored_model: Option<&str>,
) -> (Option<&'a str>, &'static str) {
    let trimmed = raw_requested.trim();
    let (requested_profile_part, requested_tier_part) = match trimmed.split_once(':') {
        Some((prof, tier)) => (prof.trim(), Some(tier.trim())),
        None => (trimmed, None),
    };
    let profile = if !requested_profile_part.is_empty() {
        Some(requested_profile_part)
    } else {
        stored_profile
    }
    .map(str::trim)
    .filter(|value| !value.is_empty());
    let tier_mode = match requested_tier_part {
        Some(tier) if tier.eq_ignore_ascii_case("paid") => "paid",
        Some(tier) if tier.eq_ignore_ascii_case("free") => "free",
        _ => {
            let configured_model = stored_model.map(str::trim).filter(|m| !m.is_empty());
            if configured_model.is_some_and(|m| !m.ends_with(":free")) {
                "paid"
            } else {
                "free"
            }
        }
    };
    (profile, tier_mode)
}

fn safe_model_catalog_error_code(
    error: &evohime_model_gateway::providers::ProviderError,
) -> &'static str {
    match error {
        evohime_model_gateway::providers::ProviderError::Config(_) => {
            "provider_configuration_error"
        }
        evohime_model_gateway::providers::ProviderError::Http(message) => {
            let message = message.to_ascii_lowercase();
            if message.contains("timeout") || message.contains("timed out") {
                "provider_timeout"
            } else {
                "provider_transport_error"
            }
        }
        evohime_model_gateway::providers::ProviderError::Api(message) => {
            let message = message.to_ascii_lowercase();
            if message.contains("404")
                || message.contains("model not found")
                || message.contains("model_not_found")
            {
                "catalog_model_not_found"
            } else if message.contains("too many") {
                "catalog_too_many_entries"
            } else if message.contains("size limit") || message.contains("exceeds size") {
                "catalog_response_too_large"
            } else {
                "catalog_response_invalid"
            }
        }
        evohime_model_gateway::providers::ProviderError::ApiWithDiagnostics { .. } => {
            match error.failure_code() {
                "provider_model_not_found" => "catalog_model_not_found",
                "provider_timeout" => "provider_timeout",
                _ => "catalog_response_invalid",
            }
        }
        evohime_model_gateway::providers::ProviderError::Stream(_) => "catalog_stream_error",
        evohime_model_gateway::providers::ProviderError::ImagePreflightRejected => {
            "catalog_preflight_rejected"
        }
        evohime_model_gateway::providers::ProviderError::ImageCapabilityStale => {
            "catalog_capability_stale"
        }
        evohime_model_gateway::providers::ProviderError::AllModelsExhausted => {
            "provider_all_models_exhausted"
        }
    }
}

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
                let available = match evohime_model_gateway::fetch_model_catalog(route).await {
                    Ok(entries) => entries,
                    Err(error) => {
                        // A provider failure while validating a selection is a
                        // command outcome, not a transport failure: closing the
                        // connection here made the shell reconnect, retry the
                        // same selection and loop forever.
                        let reason = safe_model_catalog_error_code(&error);
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(&serde_json::json!({ "reason": reason }))
                                .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    }
                };
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
                if request.mode == "verify_free_access" {
                    let payload = self
                        .run_manual_free_access_probe(
                            &request.model_id,
                            request.confirm_possible_cost,
                        )
                        .await;
                    self.write_response(
                        writer,
                        "free_access.probe",
                        serde_json::to_vec(&payload)
                            .map_err(|error| FrameError::Io(error.to_string()))?,
                    )
                    .await?;
                    return Ok(());
                }
                if request.mode == "hive_catalog" || request.mode == "hive_fallback_catalog" {
                    let is_fallback = request.mode == "hive_fallback_catalog";
                    let route_key = if is_fallback { "hive_fallback" } else { "hive" };
                    let event_type = if is_fallback {
                        "hive.fallback_model.catalog"
                    } else {
                        "hive.model.catalog"
                    };
                    let settings = self.journal.hive_settings().await.unwrap_or_default();
                    let (requested_profile, tier_mode) = if is_fallback {
                        parse_hive_catalog_profile_and_tier(
                            &request.model_id,
                            settings.hive_fallback_provider_profile.as_deref(),
                            settings.hive_fallback_model.as_deref(),
                        )
                    } else {
                        parse_hive_catalog_profile_and_tier(
                            &request.model_id,
                            settings.hive_provider_profile.as_deref(),
                            settings.hive_model.as_deref(),
                        )
                    };
                    let mut adjusted_config = self.gateway_config.clone();
                    if let Some(config) = adjusted_config.as_mut() {
                        if is_fallback {
                            config.apply_hive_fallback_route_overrides(
                                requested_profile,
                                settings.hive_fallback_model.as_deref(),
                            );
                        } else {
                            config.apply_hive_route_overrides(
                                requested_profile,
                                settings.hive_model.as_deref(),
                            );
                        }
                    }
                    let route = adjusted_config
                        .as_ref()
                        .and_then(|config| config.routes.get(route_key));
                    let profile_label = route
                        .and_then(|r| r.provider_profile_id.map(|id| id.as_str()))
                        .or_else(|| route.map(|r| r.provider.as_str()))
                        .or(requested_profile)
                        .unwrap_or(if is_fallback { "" } else { "rout" });
                    let (models, error): (Vec<String>, Option<String>) = match route {
                        Some(route)
                            if route.provider
                                == evohime_model_gateway::providers::ProviderKind::Ollama =>
                        {
                            match evohime_model_gateway::fetch_model_catalog(route).await {
                                Ok(entries) => (entries.into_iter().map(|e| e.id).collect(), None),
                                Err(err) => {
                                    (Vec::new(), Some(safe_model_catalog_error_code(&err).into()))
                                }
                            }
                        }
                        Some(route) if route.configured() => {
                            match evohime_model_gateway::fetch_model_catalog(route).await {
                                Ok(entries) => (
                                    filter_catalog_entries_by_tier(
                                        route.provider,
                                        tier_mode,
                                        entries,
                                    )
                                    .into_iter()
                                    .map(|e| e.id)
                                    .collect(),
                                    None,
                                ),
                                Err(err) => {
                                    let fallback_models = if route.provider
                                        == evohime_model_gateway::providers::ProviderKind::LiteRouter
                                    {
                                        if tier_mode == "free" {
                                            evohime_model_gateway::model_ranker::get_ranked_models()
                                                .into_iter()
                                                .filter(|m| m.ends_with(":free"))
                                                .collect()
                                        } else {
                                            Vec::new()
                                        }
                                    } else if route.provider_profile_id
                                        == Some(evohime_model_gateway::ProviderProfileId::Rout)
                                    {
                                        evohime_model_gateway::model_ranker::get_rout_ranked_models()
                                    } else {
                                        Vec::new()
                                    };
                                    if !fallback_models.is_empty() {
                                        (fallback_models, None)
                                    } else {
                                        (
                                            Vec::new(),
                                            Some(safe_model_catalog_error_code(&err).into()),
                                        )
                                    }
                                }
                            }
                        }
                        _ => {
                            let static_models = if profile_label.eq_ignore_ascii_case("literouter")
                            {
                                if tier_mode == "free" {
                                    evohime_model_gateway::model_ranker::get_ranked_models()
                                        .into_iter()
                                        .filter(|m| m.ends_with(":free"))
                                        .collect()
                                } else {
                                    Vec::new()
                                }
                            } else if profile_label.eq_ignore_ascii_case("rout") {
                                evohime_model_gateway::model_ranker::get_rout_ranked_models()
                            } else {
                                Vec::new()
                            };
                            let err_code = if static_models.is_empty() {
                                Some("provider_not_configured".into())
                            } else {
                                None
                            };
                            (static_models, err_code)
                        }
                    };
                    let payload = serde_json::json!({
                        "profile": profile_label,
                        "tier": tier_mode,
                        "models": models,
                        "error": error,
                    });
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: event_type.into(),
                        payload: serde_json::to_vec(&payload).unwrap_or_default(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                    return Ok(());
                }
                let payload = self
                    .fetch_model_catalog_payload(
                        &request.mode,
                        (!request.provider.is_empty()).then_some(request.provider.as_str()),
                        (!request.profile_id.is_empty()).then_some(request.profile_id.as_str()),
                    )
                    .await;
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

    pub(crate) async fn fetch_model_catalog_payload(
        &self,
        raw_mode: &str,
        requested_provider: Option<&str>,
        requested_profile_id: Option<&str>,
    ) -> serde_json::Value {
        let mode = if raw_mode == "paid" { "paid" } else { "free" };
        let route = self.gateway_config.as_ref().and_then(|config| {
            resolve_catalog_route(config, requested_provider, requested_profile_id)
        });
        let provider = route
            .map(|route| route.provider.as_str().to_string())
            .unwrap_or_else(|| "unknown".into());
                let (mut entries, error, ollama, catalog_failure): (
                    Vec<evohime_model_gateway::ModelCatalogEntry>,
                    Option<String>,
                    Option<serde_json::Value>,
                    Option<crate::free_provider_reliability_routing::CatalogFailureCode>,
                ) = match route {
                    Some(route)
                        if route.provider
                            == evohime_model_gateway::providers::ProviderKind::Ollama =>
                    {
                        let hardware = crate::local_model_runtime_manager::discover_hardware()
                            .map_err(|error| error.to_string());
                        match hardware {
                            Ok(hardware) => {
                                let device =
                                    evohime_model_gateway::providers::ollama::OllamaDeviceProfile {
                                        cpu_threads: hardware.cpu_threads,
                                        ram_bytes: hardware.ram_bytes,
                                        disk_free_bytes: hardware.disk_free_bytes,
                                        accelerator_bytes: hardware.accelerator_bytes,
                                    };
                                match evohime_model_gateway::providers::ollama::fetch_catalog(
                                    &route.literouter,
                                    device.clone(),
                                )
                                .await
                                {
                                    Ok(catalog) => {
                                        // Collect entries and enforce IPC frame size limits
let mut entries = catalog.installed.clone();
// Serialize to check size against MAX_FRAME_BYTES
if let Ok(serialized) = serde_json::to_vec(&entries) {
                tracing::info!("Model catalog serialized size: {} bytes, entries count: {}", serialized.len(), entries.len());
    if serialized.len() > evohime_desktop_ipc::MAX_FRAME_BYTES {
        // Simple hard cap to avoid oversized frames
        const HARD_CAP: usize = 200;
        entries.truncate(HARD_CAP);
        // If still too big, iteratively shrink
        while let Ok(s) = serde_json::to_vec(&entries) {
            if s.len() <= evohime_desktop_ipc::MAX_FRAME_BYTES { break; }
            if entries.is_empty() { break; }
            entries.truncate(entries.len().saturating_sub(10));
        }
    }
}
                                        let projection = serde_json::json!({
                                            "device": catalog.device,
                                            "recommendations": catalog.recommendations,
                                            "installed": entries.iter().map(|entry| entry.id.clone()).collect::<Vec<_>>(),
                                            "error": null,
                                        });
                                        (entries, None, Some(projection), None)
                                    }
                                    Err(error) => {
                                        let recommendations =
                                            evohime_model_gateway::providers::ollama::recommend_models(
                                                &device,
                                                &[],
                                            );
                                        let error_code = safe_model_catalog_error_code(&error);
                                        (
                                            Vec::new(),
                                            Some(error_code.into()),
                                            Some(serde_json::json!({
                                                "device": device,
                                                "recommendations": recommendations,
                                                "installed": [],
                                                "error": error_code,
                                            })),
                                            Some(crate::free_provider_reliability_routing::classify_catalog_error(&error)),
                                        )
                                    }
                                }
                            }
                            Err(_) => (
                                Vec::new(),
                                Some("hardware_discovery_failed".into()),
                                Some(serde_json::json!({
                                    "recommendations": [],
                                    "installed": [],
                                    "error": "hardware_discovery_failed",
                                })),
                                Some(crate::free_provider_reliability_routing::CatalogFailureCode::Unknown),
                            ),
                        }
                    }
                    Some(route) => match evohime_model_gateway::fetch_model_catalog(route).await {
                        Ok(entries) => (
                            filter_catalog_entries_by_tier(route.provider, mode, entries),
                            None,
                            None,
                            None,
                        ),
                        Err(error) => (
                            Vec::new(),
                            Some(safe_model_catalog_error_code(&error).into()),
                            None,
                            Some(
                                crate::free_provider_reliability_routing::classify_catalog_error(
                                    &error,
                                ),
                            ),
                        ),
                    },
                    None => (
                        Vec::new(),
                        Some("provider_not_configured".into()),
                        None,
                        None,
                    ),
                };
                if let Some(route) = route {
                    let cached_entries = self
                        .remember_provider_catalog_snapshot(route, &entries, catalog_failure)
                        .await;
                    if entries.is_empty() {
                        if let Some(cached_entries) = cached_entries {
                            entries = filter_catalog_entries_by_tier(
                                route.provider,
                                mode,
                                cached_entries,
                            );
                        }
                    }
                }
                let selected_model = self.selected_model.get();
                let provider_catalog = if let Some(route) = route {
                    self.provider_catalog_projection(route, selected_model.as_deref())
                        .await
                } else {
                    serde_json::json!({
                        "schema_version": 1,
                        "provider": null,
                        "catalog": { "state": "unobserved", "model_count": 0 },
                        "models": [],
                        "redacted": true,
                    })
                };
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
                serde_json::json!({
                    "mode": mode,
                    "models": models,
                    "limits": limits,
                    "error": if provider == "ollama" { None } else { error },
                    "ollama": ollama,
                    "provider_catalog": provider_catalog,
                })
    }
}

fn resolve_catalog_route<'a>(
    config: &'a evohime_model_gateway::ModelGatewayConfig,
    provider: Option<&str>,
    profile_id: Option<&str>,
) -> Option<&'a evohime_model_gateway::ModelRouteConfig> {
    let requested_provider = provider.map(str::trim).filter(|value| !value.is_empty());
    let requested_profile = profile_id.map(str::trim).filter(|value| !value.is_empty());
    if requested_provider.is_none() && requested_profile.is_none() {
        return config.routes.get(&config.default_route);
    }

    let requested_kind =
        requested_provider.and_then(evohime_model_gateway::providers::ProviderKind::parse);
    let matches_provider =
        |route_id: &str, route: &evohime_model_gateway::ModelRouteConfig| -> bool {
            let Some(req_provider) = requested_provider else {
                return true;
            };
            route_id.eq_ignore_ascii_case(req_provider)
                || route.provider.as_str().eq_ignore_ascii_case(req_provider)
                || requested_kind.is_some_and(|kind| route.provider == kind)
        };

    if let Some(req_profile) = requested_profile {
        let mut matching: Vec<(&String, &evohime_model_gateway::ModelRouteConfig)> = config
            .routes
            .iter()
            .filter(|(route_id, route)| {
                let profile_matches = route
                    .provider_profile_id
                    .is_some_and(|id| id.as_str().eq_ignore_ascii_case(req_profile))
                    || route_id.eq_ignore_ascii_case(req_profile);
                profile_matches && matches_provider(route_id, route)
            })
            .collect();
        matching.sort_by(|(left, _), (right, _)| left.cmp(right));
        if let Some((_, route)) = matching
            .iter()
            .find(|(route_id, _)| *route_id == &config.default_route)
        {
            return Some(route);
        }
        return matching.first().map(|(_, route)| *route);
    }

    if let Some(default_route) = config.routes.get(&config.default_route) {
        if matches_provider(&config.default_route, default_route) {
            return Some(default_route);
        }
    }
    let mut matching: Vec<(&String, &evohime_model_gateway::ModelRouteConfig)> = config
        .routes
        .iter()
        .filter(|(route_id, route)| matches_provider(route_id, route))
        .collect();
    matching.sort_by(|(left, _), (right, _)| left.cmp(right));
    matching.first().map(|(_, route)| *route)
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_desktop_ipc::MAX_FRAME_BYTES;

    #[test]
    fn model_catalog_error_projection_is_bounded_and_does_not_include_details() {
        let cases = [
            (
                evohime_model_gateway::providers::ProviderError::Config(
                    "provider API key is not configured".into(),
                ),
                "provider_configuration_error",
            ),
            (
                evohime_model_gateway::providers::ProviderError::Http(
                    "error sending request to https://provider.test/models: operation timed out"
                        .into(),
                ),
                "provider_timeout",
            ),
            (
                evohime_model_gateway::providers::ProviderError::Api(
                    "provider model catalog response is invalid: secret-token".into(),
                ),
                "catalog_response_invalid",
            ),
            (
                evohime_model_gateway::providers::ProviderError::Stream(
                    "stream ended at https://provider.test".into(),
                ),
                "catalog_stream_error",
            ),
        ];

        for (error, expected) in cases {
            let code = safe_model_catalog_error_code(&error);
            assert_eq!(code, expected);
            assert!(!code.contains("provider.test"));
            assert!(!code.contains("secret"));
        }
    }

    #[test]
    fn model_catalog_limits_have_stable_safe_codes() {
        assert_eq!(
            safe_model_catalog_error_code(&evohime_model_gateway::providers::ProviderError::Api(
                "provider model catalog contains too many entries".into(),
            )),
            "catalog_too_many_entries"
        );
        assert_eq!(
            safe_model_catalog_error_code(&evohime_model_gateway::providers::ProviderError::Api(
                "provider model catalog response exceeds size limit".into(),
            )),
            "catalog_response_too_large"
        );
        assert_eq!(
            safe_model_catalog_error_code(&evohime_model_gateway::providers::ProviderError::Api(
                "provider model catalog request failed with HTTP 404".into(),
            )),
            "catalog_model_not_found"
        );
    }

    #[test]
    fn filter_catalog_entries_by_tier_applies_only_to_literouter() {
        let entry = |id: &str| evohime_model_gateway::ModelCatalogEntry {
            id: id.into(),
            context_tokens: None,
            max_output_tokens: None,
        };
        let literouter_entries = vec![entry("deepseek:free"), entry("gpt-4o")];
        let free_only = filter_catalog_entries_by_tier(
            evohime_model_gateway::providers::ProviderKind::LiteRouter,
            "free",
            literouter_entries.clone(),
        );
        assert_eq!(
            free_only.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["deepseek:free"]
        );
        let paid_only = filter_catalog_entries_by_tier(
            evohime_model_gateway::providers::ProviderKind::LiteRouter,
            "paid",
            literouter_entries,
        );
        assert_eq!(
            paid_only.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["gpt-4o"]
        );

        let deepseek_entries = vec![entry("deepseek-flash(1)"), entry("deepseek-v4-pro")];
        let filtered_deepseek = filter_catalog_entries_by_tier(
            evohime_model_gateway::providers::ProviderKind::OpenAICompatible,
            "free",
            deepseek_entries,
        );
        assert_eq!(
            filtered_deepseek
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            vec!["deepseek-flash(1)", "deepseek-v4-pro"]
        );

        let responses_entries = vec![entry("gpt-4.1"), entry("o3-mini")];
        let filtered_responses = filter_catalog_entries_by_tier(
            evohime_model_gateway::providers::ProviderKind::OpenAIResponses,
            "free",
            responses_entries,
        );
        assert_eq!(
            filtered_responses
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-4.1", "o3-mini"]
        );
    }

    #[test]
    fn parse_hive_catalog_profile_and_tier_resolves_explicit_and_inferred_tiers() {
        assert_eq!(
            parse_hive_catalog_profile_and_tier("literouter:free", Some("rout"), Some("gpt-4o")),
            (Some("literouter"), "free")
        );
        assert_eq!(
            parse_hive_catalog_profile_and_tier(
                "literouter:paid",
                Some("rout"),
                Some("deepseek:free")
            ),
            (Some("literouter"), "paid")
        );
        assert_eq!(
            parse_hive_catalog_profile_and_tier(
                "literouter",
                None,
                Some("glm-5.3-flash-cheap:free")
            ),
            (Some("literouter"), "free")
        );
        assert_eq!(
            parse_hive_catalog_profile_and_tier("literouter", None, Some("claude-3-7-sonnet")),
            (Some("literouter"), "paid")
        );
        assert_eq!(
            parse_hive_catalog_profile_and_tier("", Some("literouter"), None),
            (Some("literouter"), "free")
        );
    }
}
