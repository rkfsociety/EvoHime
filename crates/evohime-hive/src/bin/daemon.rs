//! Standalone entry point for the `evohime-hive-daemon` process.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use evohime_desktop_ipc::{generated, session, transport};
use evohime_hive::pulse::{
    HiveObservationControlRequest, HiveObservationController, HiveObservationDaemon,
    DEFAULT_HIVE_OBSERVATION_CONTROL_PIPE, HIVE_OBSERVATION_CONTROL_PIPE_ENV,
    HIVE_OBSERVATION_CONTROL_SOCKET_ENV,
};
use evohime_hive::settings::HiveSettings;
use evohime_hive::state::HiveStateSnapshot;
use evohime_hive_contract::{
    BoxFuture, HiveContractError, HiveCoordinator, HiveDispatchWindows, HiveEventRecord,
    HiveJournalProvider, HiveModelUsage, HiveTaskDispatchRequest, HiveWorktreeLeaseRequest,
    WorkspaceProvider,
};
use evohime_model_gateway::ModelGatewayConfig;
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;

const MAX_INTERLEAVED_EVENTS: usize = 128;

#[cfg(windows)]
type CoreIpcStream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type CoreIpcStream = tokio::net::UnixStream;

struct CoreIpcSession<S> {
    stream: S,
    client_id: String,
    core_instance_id: String,
    session_epoch: u64,
}

impl<S> CoreIpcSession<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    async fn connect(
        stream: S,
        context: &session::LaunchContext,
    ) -> Result<Self, HiveContractError> {
        context.validate().map_err(|error| {
            HiveContractError::Unavailable(format!("invalid launch context: {error}"))
        })?;
        let client_id = format!("hive-{}", uuid::Uuid::new_v4());
        let mut session = Self {
            stream,
            client_id: client_id.clone(),
            core_instance_id: String::new(),
            session_epoch: 0,
        };
        let challenge = session.read_event().await?;
        let nonce = challenge_nonce(&challenge)?;
        let proof = context.secret.proof("shell", &client_id, &nonce);
        session
            .write_command(generated::CommandEnvelope {
                protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                request_id: uuid::Uuid::new_v4().to_string(),
                client_id: client_id.clone(),
                core_instance_id: String::new(),
                session_epoch: 0,
                command: Some(generated::command_envelope::Command::Handshake(
                    generated::Handshake {
                        protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
                        client_id: client_id.clone(),
                        session_id: client_id,
                        session_epoch: 0,
                        last_event_sequence: 0,
                        capabilities: vec!["hive-daemon".into()],
                        client_role: "shell".into(),
                        nonce,
                        proof,
                    },
                )),
            })
            .await?;
        let ready = session.read_event().await?;
        (session.core_instance_id, session.session_epoch) = ready_generation(&ready)?;
        Ok(session)
    }

    fn command_envelope(
        &self,
        command: generated::command_envelope::Command,
    ) -> generated::CommandEnvelope {
        generated::CommandEnvelope {
            protocol: Some(generated::ProtocolVersion { major: 1, minor: 0 }),
            request_id: uuid::Uuid::new_v4().to_string(),
            client_id: self.client_id.clone(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            command: Some(command),
        }
    }

    async fn write_command(
        &mut self,
        command: generated::CommandEnvelope,
    ) -> Result<(), HiveContractError> {
        transport::write_frame(&mut self.stream, &command.encode_to_vec())
            .await
            .map_err(|error| HiveContractError::Unavailable(format!("IPC write failed: {error}")))
    }

    async fn read_event(&mut self) -> Result<generated::EventEnvelope, HiveContractError> {
        let payload = transport::read_frame(&mut self.stream)
            .await
            .map_err(|error| HiveContractError::Unavailable(format!("IPC read failed: {error}")))?;
        let event = generated::EventEnvelope::decode(payload.as_slice()).map_err(|error| {
            HiveContractError::Serialization(format!("IPC event decode failed: {error}"))
        })?;
        validate_event_generation(&event, &self.core_instance_id, self.session_epoch)?;
        Ok(event)
    }

    async fn wait_for_event_type(
        &mut self,
        expected_event_type: &str,
    ) -> Result<generated::EventEnvelope, HiveContractError> {
        for _ in 0..=MAX_INTERLEAVED_EVENTS {
            let event = self.read_event().await?;
            if event.event_type == expected_event_type {
                return Ok(event);
            }
        }
        Err(HiveContractError::Unavailable(format!(
            "timed out waiting for IPC response '{expected_event_type}'"
        )))
    }
}

fn challenge_nonce(event: &generated::EventEnvelope) -> Result<String, HiveContractError> {
    let Some(generated::event_envelope::Event::AuthChallenge(challenge)) = &event.event else {
        return Err(HiveContractError::Unavailable(
            "authentication failed: challenge missing".into(),
        ));
    };
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok());
    if challenge.expires_at_ms == 0
        || now_ms.is_none()
        || now_ms.is_some_and(|now| challenge.expires_at_ms <= now)
        || challenge.nonce.len() != session::NONCE_BYTES * 2
        || !challenge.nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(HiveContractError::Unavailable(
            "authentication failed: challenge invalid".into(),
        ));
    }
    Ok(challenge.nonce.clone())
}

fn ready_generation(event: &generated::EventEnvelope) -> Result<(String, u64), HiveContractError> {
    if !matches!(
        &event.event,
        Some(generated::event_envelope::Event::Ready(_))
    ) {
        return Err(HiveContractError::Unavailable(
            "authentication failed: Core did not become ready".into(),
        ));
    }
    if event.core_instance_id.is_empty() || event.session_epoch == 0 {
        return Err(HiveContractError::Unavailable(
            "authentication failed: Core generation is invalid".into(),
        ));
    }
    Ok((event.core_instance_id.clone(), event.session_epoch))
}

fn validate_event_generation(
    event: &generated::EventEnvelope,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), HiveContractError> {
    if core_instance_id.is_empty() {
        return Ok(());
    }
    if event.core_instance_id != core_instance_id || event.session_epoch != session_epoch {
        return Err(HiveContractError::Unavailable(
            "Core generation changed during IPC session".into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
async fn open_core_stream() -> Result<(CoreIpcStream, session::LaunchContext), HiveContractError> {
    use tokio::net::windows::named_pipe::ClientOptions;

    let context_path = std::env::var_os("EVOHIME_LAUNCH_CONTEXT")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|value| PathBuf::from(value).join("EvoHime/runtime/session.json"))
        })
        .ok_or_else(|| {
            HiveContractError::Unavailable("launch context path is not configured".into())
        })?;
    let context = session::read_launch_context(&context_path).map_err(|error| {
        HiveContractError::Unavailable(format!(
            "failed to read launch context '{}': {error}",
            context_path.display()
        ))
    })?;
    let pipe = ClientOptions::new()
        .open(&context.pipe_name)
        .map_err(|error| {
            HiveContractError::Unavailable(format!(
                "failed to open Core named pipe '{}': {error}",
                context.pipe_name
            ))
        })?;
    Ok((pipe, context))
}

#[cfg(unix)]
async fn open_core_stream() -> Result<(CoreIpcStream, session::LaunchContext), HiveContractError> {
    use evohime_desktop_ipc::unix_runtime::{read_private_launch_context, UnixRuntimePaths};

    let paths = UnixRuntimePaths::current().map_err(|error| {
        HiveContractError::Unavailable(format!("Linux runtime directory is unavailable: {error}"))
    })?;
    let context_path = std::env::var_os("EVOHIME_LAUNCH_CONTEXT")
        .map(PathBuf::from)
        .unwrap_or(paths.launch_context);
    let context = read_private_launch_context(&context_path).map_err(|error| {
        HiveContractError::Unavailable(format!(
            "failed to read launch context '{}': {error}",
            context_path.display()
        ))
    })?;
    let stream = tokio::net::UnixStream::connect(&paths.socket_path)
        .await
        .map_err(|error| {
            HiveContractError::Unavailable(format!(
                "failed to connect to Core socket '{}': {error}",
                paths.socket_path.display()
            ))
        })?;
    Ok((stream, context))
}

struct CoreIpcClient {
    session: Mutex<CoreIpcSession<CoreIpcStream>>,
    last_rate_limit_sync: std::sync::Mutex<Option<std::time::Instant>>,
}

impl CoreIpcClient {
    async fn connect() -> Result<Self, HiveContractError> {
        let (stream, context) = open_core_stream().await?;
        let session = CoreIpcSession::connect(stream, &context).await?;
        Ok(Self {
            session: Mutex::new(session),
            last_rate_limit_sync: std::sync::Mutex::new(None),
        })
    }

    async fn fetch_hive_state(&self) -> Result<HiveStateSnapshot, HiveContractError> {
        let mut session = self.session.lock().await;
        let command = session.command_envelope(generated::command_envelope::Command::HiveState(
            generated::HiveStateCommand {
                schema_version: 1,
                request_id: uuid::Uuid::new_v4().to_string(),
            },
        ));
        session.write_command(command).await?;
        let event = session.wait_for_event_type("hive_state.snapshot").await?;
        serde_json::from_slice::<HiveStateSnapshot>(&event.payload).map_err(|error| {
            HiveContractError::Serialization(format!(
                "failed to decode hive_state.snapshot: {error}"
            ))
        })
    }

    async fn set_eva_workspace_path(&self, path: &str) -> Result<(), HiveContractError> {
        let mut session = self.session.lock().await;
        let command =
            session.command_envelope(generated::command_envelope::Command::SetEvaWorkspacePath(
                generated::SetEvaWorkspacePathCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    path: path.to_owned(),
                },
            ));
        session.write_command(command).await?;
        let _ = session.wait_for_event_type("hive_settings.saved").await?;
        Ok(())
    }

    async fn dispatch_task(
        &self,
        request: HiveTaskDispatchRequest,
    ) -> Result<(), HiveContractError> {
        let mut session = self.session.lock().await;
        let command = session.command_envelope(generated::command_envelope::Command::StartTask(
            generated::StartTask {
                task_id: request.task_id,
                prompt: request.prompt,
                workspace_path: request.workspace_root.unwrap_or_default(),
                preferred_route_hint: request.preferred_route_hint.unwrap_or_default(),
                execution_kind: "agent".into(),
                conversation_id: "hive-system-chat".into(),
                client_message_id: String::new(),
                goal_id: String::new(),
                provider: String::new(),
                profile_id: String::new(),
                model: String::new(),
                permission_mode: String::new(),
                isolation_mode: String::new(),
            },
        ));
        session.write_command(command).await.map_err(|error| {
            HiveContractError::Dispatch(format!("failed to dispatch task over IPC: {error}"))
        })
    }

    async fn latest_sequence(&self) -> Result<i64, HiveContractError> {
        let mut session = self.session.lock().await;
        let command = session.command_envelope(
            generated::command_envelope::Command::ResyncRequest(generated::ResyncRequest {
                after_sequence: i64::MAX as u64,
                max_events: 1,
                include_full_snapshot: false,
            }),
        );
        session.write_command(command).await?;
        let event = session.wait_for_event_type("resync.end").await?;
        let payload: serde_json::Value =
            serde_json::from_slice(&event.payload).map_err(|error| {
                HiveContractError::Serialization(format!("failed to decode resync.end: {error}"))
            })?;
        let latest = payload
            .get("latest_sequence")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                HiveContractError::Serialization("resync.end missing latest_sequence".into())
            })?;
        i64::try_from(latest).map_err(|_| {
            HiveContractError::Serialization(format!("latest_sequence overflows i64: {latest}"))
        })
    }

    async fn replay_events(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<HiveEventRecord>, HiveContractError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut session = self.session.lock().await;
        let after = u64::try_from(after_sequence.max(0)).unwrap_or(0);
        let command = session.command_envelope(generated::command_envelope::Command::ReplayEvents(
            generated::ReplayEvents {
                after_sequence: after,
            },
        ));
        session.write_command(command).await?;
        let mut records = Vec::new();
        for _ in 0..=(1_000 + MAX_INTERLEAVED_EVENTS) {
            let event = session.read_event().await?;
            if event.event_type == "replay.end" {
                return Ok(records);
            }
            if matches!(
                event.event,
                Some(generated::event_envelope::Event::ReplayGap(_))
            ) || event.event_type == "replay.gap"
            {
                tracing::debug!(
                    after_sequence = after,
                    "received replay_gap during Core IPC replay_events"
                );
                continue;
            }
            if event.sequence_id > 0 && records.len() < limit {
                if let Ok(sequence_id) = i64::try_from(event.sequence_id) {
                    records.push(HiveEventRecord {
                        sequence_id,
                        task_id: event.task_id,
                        event_type: event.event_type,
                        payload: event.payload,
                        created_at_ms: 0,
                    });
                }
            }
        }
        Err(HiveContractError::Unavailable(
            "timed out waiting for IPC response 'replay.end'".into(),
        ))
    }

    async fn submit_hive_audit(
        &self,
        task_id: &str,
        event_type: &str,
        payload: &[u8],
    ) -> Result<(), HiveContractError> {
        let mut session = self.session.lock().await;
        let command =
            session.command_envelope(generated::command_envelope::Command::SubmitHiveAudit(
                generated::SubmitHiveAuditCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    task_id: task_id.to_owned(),
                    event_type: event_type.to_owned(),
                    payload: payload.to_vec(),
                },
            ));
        session.write_command(command).await?;
        let event = session.wait_for_event_type("hive_audit.submitted").await?;
        let response: serde_json::Value =
            serde_json::from_slice(&event.payload).map_err(|error| {
                HiveContractError::Serialization(format!(
                    "failed to decode hive_audit.submitted: {error}"
                ))
            })?;
        if response
            .get("accepted")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            Ok(())
        } else {
            let reason = response
                .get("error_code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            Err(HiveContractError::Storage(format!(
                "submit_hive_audit rejected: {reason}"
            )))
        }
    }

    async fn prune_signal_records(
        &self,
        event_types: &[&str],
        keep_limit: usize,
    ) -> Result<(), HiveContractError> {
        let mut session = self.session.lock().await;
        let command = session.command_envelope(
            generated::command_envelope::Command::PruneHiveSignalRecords(
                generated::PruneHiveSignalRecordsCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    event_types: event_types.iter().map(|s| (*s).to_owned()).collect(),
                    keep_limit: u32::try_from(keep_limit).unwrap_or(u32::MAX),
                },
            ),
        );
        session.write_command(command).await?;
        let _ = session.wait_for_event_type("hive_signals.pruned").await?;
        Ok(())
    }

    async fn get_hive_model_usage(
        &self,
        request: generated::GetHiveModelUsageCommand,
    ) -> Result<serde_json::Value, HiveContractError> {
        let mut session = self.session.lock().await;
        let command = session.command_envelope(
            generated::command_envelope::Command::GetHiveModelUsage(request),
        );
        session.write_command(command).await?;
        let event = session
            .wait_for_event_type("hive_model_usage.snapshot")
            .await?;
        let response: serde_json::Value =
            serde_json::from_slice(&event.payload).map_err(|error| {
                HiveContractError::Serialization(format!(
                    "failed to decode hive_model_usage.snapshot: {error}"
                ))
            })?;
        sync_rate_limits_from_payload(&response);
        if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
            let reason = response
                .get("error_code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unavailable");
            return Err(HiveContractError::Unavailable(format!(
                "get_hive_model_usage failed: {reason}"
            )));
        }
        Ok(response)
    }

    async fn manage_worktree_lease(
        &self,
        request: generated::ManageWorktreeLeaseCommand,
    ) -> Result<serde_json::Value, HiveContractError> {
        let mut session = self.session.lock().await;
        let command = session.command_envelope(
            generated::command_envelope::Command::ManageWorktreeLease(request),
        );
        session.write_command(command).await?;
        let event = session.wait_for_event_type("hive_worktree.managed").await?;
        serde_json::from_slice(&event.payload).map_err(|error| {
            HiveContractError::Serialization(format!(
                "failed to decode hive_worktree.managed: {error}"
            ))
        })
    }

    async fn filter_hive_root_tasks(
        &self,
        roots: Vec<String>,
    ) -> Result<Vec<String>, HiveContractError> {
        let mut session = self.session.lock().await;
        let command =
            session.command_envelope(generated::command_envelope::Command::FilterHiveRootTasks(
                generated::FilterHiveRootTasksCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    roots,
                },
            ));
        session.write_command(command).await?;
        let event = session
            .wait_for_event_type("hive_root_tasks.filtered")
            .await?;
        let response: serde_json::Value =
            serde_json::from_slice(&event.payload).map_err(|error| {
                HiveContractError::Serialization(format!(
                    "failed to decode hive_root_tasks.filtered: {error}"
                ))
            })?;
        let items = response
            .get("roots")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                HiveContractError::Serialization("hive_root_tasks.filtered missing roots".into())
            })?;
        Ok(items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect())
    }

    async fn sync_rate_limits_from_core(&self) {
        const SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
        {
            let Ok(mut guard) = self.last_rate_limit_sync.lock() else {
                return;
            };
            if guard
                .as_ref()
                .is_some_and(|last| last.elapsed() < SYNC_INTERVAL)
            {
                return;
            }
            *guard = Some(std::time::Instant::now());
        }
        let mut session = self.session.lock().await;
        let command =
            session.command_envelope(generated::command_envelope::Command::HiveObservationStatus(
                generated::HiveObservationStatusCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                },
            ));
        if session.write_command(command).await.is_err() {
            return;
        }
        if let Ok(event) = session.wait_for_event_type("hive.observation_status").await {
            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&event.payload) {
                sync_rate_limits_from_payload(&payload);
            }
        }
    }
}

fn sync_rate_limits_from_payload(payload: &serde_json::Value) {
    let Some(rate_limits) = payload
        .get("rate_limits")
        .and_then(serde_json::Value::as_object)
    else {
        return;
    };
    let active = evohime_model_gateway::rate_limit_signal::active_rate_limits();
    for model in active.keys() {
        if !rate_limits.contains_key(model) {
            evohime_model_gateway::clear_rate_limit(model);
        }
    }
    for (model, until) in rate_limits {
        if let Some(until_ms) = until.as_i64() {
            evohime_model_gateway::record_rate_limit(model, until_ms);
        }
    }
}

fn snapshot_to_settings(snapshot: &HiveStateSnapshot) -> HiveSettings {
    HiveSettings {
        daily_budget_usd: snapshot.daily_budget_usd,
        eva_workspace_path: snapshot.eva_workspace_path.clone(),
        watched_repositories: snapshot
            .watched_repositories
            .iter()
            .map(|repo| repo.path.clone())
            .collect(),
        hive_provider_profile: snapshot.hive_provider_profile.clone(),
        hive_model: snapshot.hive_model.clone(),
        hive_fallback_provider_profile: snapshot.hive_fallback_provider_profile.clone(),
        hive_fallback_model: snapshot.hive_fallback_model.clone(),
        hive_provider_limits: snapshot.hive_provider_limits,
        hive_fallback_provider_limits: snapshot.hive_fallback_provider_limits,
        hive_daily_token_quota: snapshot.hive_daily_token_quota,
        hive_fallback_daily_token_quota: snapshot.hive_fallback_daily_token_quota,
        hive_token_quota_configured: snapshot.hive_token_quota_configured,
        hive_fallback_token_quota_configured: snapshot.hive_fallback_token_quota_configured,
        hive_quota_reset_utc_midnight: snapshot.hive_quota_reset_utc_midnight,
        hive_fallback_quota_reset_utc_midnight: snapshot.hive_fallback_quota_reset_utc_midnight,
        proactive_evolution_enabled: snapshot.proactive_evolution_enabled,
        ..HiveSettings::default()
    }
}

const MAX_FALLBACK_AUDIT_CACHE: usize = 512;

struct RealIpcJournalProvider {
    client: Arc<CoreIpcClient>,
    fallback_audit_cache: std::sync::Mutex<VecDeque<(String, String, Vec<u8>)>>,
}

impl RealIpcJournalProvider {
    fn new(client: Arc<CoreIpcClient>) -> Self {
        Self {
            client,
            fallback_audit_cache: std::sync::Mutex::new(VecDeque::new()),
        }
    }

    fn push_fallback_audit(&self, task_id: &str, event_type: &str, payload: &[u8]) {
        let Ok(mut cache) = self.fallback_audit_cache.lock() else {
            return;
        };
        if cache.len() >= MAX_FALLBACK_AUDIT_CACHE {
            cache.pop_front();
        }
        cache.push_back((task_id.to_owned(), event_type.to_owned(), payload.to_vec()));
    }

    async fn flush_fallback_audit(&self) {
        let pending: Vec<(String, String, Vec<u8>)> = {
            let Ok(mut cache) = self.fallback_audit_cache.lock() else {
                return;
            };
            cache.drain(..).collect()
        };
        for (idx, (task_id, event_type, payload)) in pending.iter().enumerate() {
            if let Err(error) = self
                .client
                .submit_hive_audit(task_id, event_type, payload)
                .await
            {
                tracing::warn!(
                    error = %error,
                    "failed to flush buffered fallback audit record to Core IPC"
                );
                for (rem_task, rem_type, rem_payload) in pending.into_iter().skip(idx) {
                    self.push_fallback_audit(&rem_task, &rem_type, &rem_payload);
                }
                break;
            }
        }
    }
}

impl HiveJournalProvider for RealIpcJournalProvider {
    fn latest_sequence(&self) -> BoxFuture<'_, i64> {
        Box::pin(async move {
            match self.client.latest_sequence().await {
                Ok(sequence) => sequence,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to fetch latest_sequence from Core IPC");
                    0
                }
            }
        })
    }

    fn replay_events(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> BoxFuture<'_, Result<Vec<HiveEventRecord>, HiveContractError>> {
        Box::pin(async move { self.client.replay_events(after_sequence, limit).await })
    }

    fn record_audit<'a>(
        &'a self,
        task_id: &'a str,
        event_type: &'a str,
        payload: &'a [u8],
    ) -> BoxFuture<'a, Result<(), HiveContractError>> {
        Box::pin(async move {
            match self
                .client
                .submit_hive_audit(task_id, event_type, payload)
                .await
            {
                Ok(()) => {
                    self.flush_fallback_audit().await;
                    Ok(())
                }
                Err(error) => {
                    tracing::warn!(
                        task_id,
                        event_type,
                        error = %error,
                        "Core IPC record_audit unavailable; buffering in local session fallback cache"
                    );
                    self.push_fallback_audit(task_id, event_type, payload);
                    Ok(())
                }
            }
        })
    }

    fn prune_signal_records<'a>(
        &'a self,
        event_types: &'a [&'a str],
        keep_limit: usize,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Err(error) = self
                .client
                .prune_signal_records(event_types, keep_limit)
                .await
            {
                tracing::warn!(
                    error = %error,
                    "Core IPC prune_signal_records failed; skipping prune this cycle"
                );
            }
        })
    }

    fn load_hive_settings_json(
        &self,
    ) -> BoxFuture<'_, Result<Option<serde_json::Value>, HiveContractError>> {
        Box::pin(async move {
            self.client.sync_rate_limits_from_core().await;
            let snapshot = self.client.fetch_hive_state().await?;
            let settings = snapshot_to_settings(&snapshot);
            let value = serde_json::to_value(&settings).map_err(|error| {
                HiveContractError::Serialization(format!(
                    "failed to serialize HiveSettings: {error}"
                ))
            })?;
            Ok(Some(value))
        })
    }

    fn save_hive_settings_json<'a>(
        &'a self,
        settings: &'a serde_json::Value,
    ) -> BoxFuture<'a, Result<(), HiveContractError>> {
        Box::pin(async move {
            let parsed: HiveSettings =
                serde_json::from_value(settings.clone()).map_err(|error| {
                    HiveContractError::Serialization(format!(
                        "invalid HiveSettings JSON payload: {error}"
                    ))
                })?;
            let path = parsed.eva_workspace_path.as_deref().unwrap_or("");
            self.client.set_eva_workspace_path(path).await
        })
    }

    fn persistent_agent_roles(&self) -> BoxFuture<'_, Vec<String>> {
        Box::pin(async move {
            match self.client.fetch_hive_state().await {
                Ok(snapshot) => snapshot
                    .departments
                    .into_iter()
                    .map(|department| department.role)
                    .collect(),
                Err(_) => Vec::new(),
            }
        })
    }

    fn task_metrics(&self, _now_ms: i64) -> BoxFuture<'_, (u32, u64)> {
        Box::pin(async move {
            match self.client.fetch_hive_state().await {
                Ok(snapshot) => (
                    snapshot.metrics.tasks_today,
                    snapshot.metrics.average_task_time_sec,
                ),
                Err(_) => (0, 0),
            }
        })
    }

    fn filter_hive_root_tasks(&self, roots: Vec<String>) -> BoxFuture<'_, Vec<String>> {
        Box::pin(async move {
            match self.client.filter_hive_root_tasks(roots.clone()).await {
                Ok(filtered) => filtered,
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "Core IPC filter_hive_root_tasks failed; keeping unfiltered roots"
                    );
                    roots
                }
            }
        })
    }

    fn model_dispatch_windows<'a>(
        &'a self,
        provider: &'a str,
        model: &'a str,
        now_ms: i64,
        minute_window_ms: i64,
        hour_window_ms: i64,
        day_window_ms: i64,
    ) -> BoxFuture<'a, Result<HiveDispatchWindows, HiveContractError>> {
        Box::pin(async move {
            let response = self
                .client
                .get_hive_model_usage(generated::GetHiveModelUsageCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    operation: "dispatch_windows".into(),
                    provider: provider.to_owned(),
                    model: model.to_owned(),
                    route_scope: String::new(),
                    start_ms: 0,
                    now_ms,
                    minute_window_ms,
                    hour_window_ms,
                    day_window_ms,
                })
                .await?;
            let windows_val = response.get("windows").cloned().ok_or_else(|| {
                HiveContractError::Serialization("hive_model_usage.snapshot missing windows".into())
            })?;
            serde_json::from_value::<HiveDispatchWindows>(windows_val).map_err(|error| {
                HiveContractError::Serialization(format!(
                    "failed to decode HiveDispatchWindows: {error}"
                ))
            })
        })
    }

    fn model_usage_since<'a>(
        &'a self,
        start_ms: i64,
        provider: &'a str,
        model: &'a str,
    ) -> BoxFuture<'a, Result<Vec<HiveModelUsage>, HiveContractError>> {
        Box::pin(async move {
            let response = self
                .client
                .get_hive_model_usage(generated::GetHiveModelUsageCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    operation: "model_usage".into(),
                    provider: provider.to_owned(),
                    model: model.to_owned(),
                    route_scope: String::new(),
                    start_ms,
                    now_ms: 0,
                    minute_window_ms: 0,
                    hour_window_ms: 0,
                    day_window_ms: 0,
                })
                .await?;
            let usage_val = response.get("usage").cloned().ok_or_else(|| {
                HiveContractError::Serialization("hive_model_usage.snapshot missing usage".into())
            })?;
            serde_json::from_value::<Vec<HiveModelUsage>>(usage_val).map_err(|error| {
                HiveContractError::Serialization(format!(
                    "failed to decode HiveModelUsage list: {error}"
                ))
            })
        })
    }

    fn route_tokens_since<'a>(
        &'a self,
        start_ms: i64,
        route_scope: &'a str,
    ) -> BoxFuture<'a, Result<u64, HiveContractError>> {
        Box::pin(async move {
            let response = self
                .client
                .get_hive_model_usage(generated::GetHiveModelUsageCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    operation: "route_tokens".into(),
                    provider: String::new(),
                    model: String::new(),
                    route_scope: route_scope.to_owned(),
                    start_ms,
                    now_ms: 0,
                    minute_window_ms: 0,
                    hour_window_ms: 0,
                    day_window_ms: 0,
                })
                .await?;
            response
                .get("tokens")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| {
                    HiveContractError::Serialization(
                        "hive_model_usage.snapshot missing tokens".into(),
                    )
                })
        })
    }

    fn register_worktree_lease(&self, request: HiveWorktreeLeaseRequest) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            if let Err(error) = self
                .client
                .manage_worktree_lease(generated::ManageWorktreeLeaseCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    operation: "register".into(),
                    worktree_id: request.worktree_id,
                    task_id: request.task_id,
                    base_root: request.base_root,
                    branch: request.branch,
                    root_ref: request.root_ref,
                    base_commit: request.base_commit,
                })
                .await
            {
                tracing::warn!(
                    error = %error,
                    "Core IPC register_worktree_lease failed; continuing without durable lease"
                );
            }
        })
    }

    fn has_active_workflow_run<'a>(&'a self, task_id: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async move {
            match self
                .client
                .manage_worktree_lease(generated::ManageWorktreeLeaseCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    operation: "has_active_run".into(),
                    worktree_id: String::new(),
                    task_id: task_id.to_owned(),
                    base_root: String::new(),
                    branch: String::new(),
                    root_ref: String::new(),
                    base_commit: String::new(),
                })
                .await
            {
                Ok(response) => response
                    .get("active")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                Err(error) => {
                    tracing::warn!(
                        task_id,
                        error = %error,
                        "Core IPC has_active_workflow_run failed; assuming active"
                    );
                    true
                }
            }
        })
    }

    fn mark_worktree_cleanup_pending<'a>(&'a self, worktree_id: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Err(error) = self
                .client
                .manage_worktree_lease(generated::ManageWorktreeLeaseCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    operation: "cleanup_pending".into(),
                    worktree_id: worktree_id.to_owned(),
                    task_id: String::new(),
                    base_root: String::new(),
                    branch: String::new(),
                    root_ref: String::new(),
                    base_commit: String::new(),
                })
                .await
            {
                tracing::warn!(
                    worktree_id,
                    error = %error,
                    "Core IPC mark_worktree_cleanup_pending failed; skipping"
                );
            }
        })
    }
}

struct IpcCoordinator {
    client: Arc<CoreIpcClient>,
}

impl HiveCoordinator for IpcCoordinator {
    fn dispatch_task(
        &self,
        request: HiveTaskDispatchRequest,
    ) -> BoxFuture<'_, Result<(), HiveContractError>> {
        Box::pin(async move { self.client.dispatch_task(request).await })
    }
}

struct IpcWorkspaceProvider {
    client: Arc<CoreIpcClient>,
    selected_model: RwLock<Option<String>>,
}

impl IpcWorkspaceProvider {
    fn new(client: Arc<CoreIpcClient>) -> Self {
        Self {
            client,
            selected_model: RwLock::new(None),
        }
    }
}

impl WorkspaceProvider for IpcWorkspaceProvider {
    fn eva_workspace_path(&self) -> BoxFuture<'_, Option<String>> {
        Box::pin(async move {
            match self.client.fetch_hive_state().await {
                Ok(snapshot) => {
                    if let Ok(mut guard) = self.selected_model.write() {
                        *guard = snapshot.hive_model.clone();
                    }
                    snapshot.eva_workspace_path
                }
                Err(_) => None,
            }
        })
    }

    fn selected_model_name(&self) -> Option<String> {
        self.selected_model
            .read()
            .ok()
            .and_then(|guard| guard.clone())
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("evohime_hive=info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    tracing::info!("evohime-hive-daemon starting");

    let has_hive_key = std::env::var("HIVE_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_fallback_key = std::env::var("HIVE_FALLBACK_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if !has_hive_key {
        tracing::error!("HIVE_API_KEY is missing or empty; exiting gracefully");
        return ExitCode::SUCCESS;
    }
    if !has_fallback_key {
        tracing::warn!("HIVE_FALLBACK_API_KEY is not set; continuing without fallback key");
    }

    let mut client_opt = None;
    for attempt in 1..=10 {
        match CoreIpcClient::connect().await {
            Ok(client) => {
                client_opt = Some(Arc::new(client));
                break;
            }
            Err(error) => {
                tracing::warn!(attempt, error = %error, "failed to connect to Core IPC; retrying");
                if attempt < 10 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    }
    let Some(client) = client_opt else {
        tracing::error!("failed to connect to Core IPC after retries; exiting");
        return ExitCode::FAILURE;
    };

    let journal: Arc<dyn HiveJournalProvider> =
        Arc::new(RealIpcJournalProvider::new(Arc::clone(&client)));
    let coordinator: Arc<dyn HiveCoordinator> = Arc::new(IpcCoordinator {
        client: Arc::clone(&client),
    });
    let workspace_provider: Arc<dyn WorkspaceProvider> =
        Arc::new(IpcWorkspaceProvider::new(Arc::clone(&client)));

    let settings = match journal.load_hive_settings_json().await {
        Ok(Some(value)) => match serde_json::from_value::<HiveSettings>(value) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::error!(error = %error, "failed to deserialize HiveSettings from Core IPC");
                return ExitCode::FAILURE;
            }
        },
        Ok(None) => HiveSettings::default(),
        Err(error) => {
            tracing::error!(error = %error, "failed to load HiveSettings from Core IPC");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(
        eva_workspace_path = ?settings.eva_workspace_path,
        "connected to Core IPC and loaded HiveSettings"
    );

    let gateway_config = match ModelGatewayConfig::from_env() {
        Ok(config) => Some(config),
        Err(error) => {
            tracing::warn!(error = %error, "failed to load ModelGatewayConfig from env; starting without gateway config");
            None
        }
    };

    let daemon = HiveObservationDaemon::start(
        journal,
        gateway_config,
        workspace_provider,
        coordinator,
        None,
    );
    let control_daemon = daemon.clone();
    let control_task = tokio::spawn(async move {
        if let Err(error) = run_observation_control_server(control_daemon).await {
            tracing::error!(error = %error, "observation control IPC server stopped");
        }
    });
    let status = daemon.status();
    tracing::info!(
        status = %status.status,
        enabled = status.enabled,
        "evohime-hive-daemon started"
    );

    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(error = %error, "failed to listen for ctrl-c signal");
    }

    control_task.abort();
    daemon.shutdown();
    ExitCode::SUCCESS
}

async fn handle_observation_control_connection<S>(mut stream: S, daemon: HiveObservationDaemon)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let payload = match transport::read_frame(&mut stream).await {
        Ok(payload) => payload,
        Err(error) => {
            tracing::debug!(error = %error, "observation control IPC read failed");
            return;
        }
    };
    let request = match serde_json::from_slice::<HiveObservationControlRequest>(&payload) {
        Ok(request) => request,
        Err(error) => {
            let error_payload = match serde_json::to_vec(&serde_json::json!({
                "error": format!("invalid_request: {error}"),
            })) {
                Ok(bytes) => bytes,
                Err(_) => return,
            };
            let _ = transport::write_frame(&mut stream, &error_payload).await;
            return;
        }
    };
    let controller = HiveObservationController::new(daemon);
    let snapshot = match request {
        HiveObservationControlRequest::GetObservationStatus => controller.snapshot(),
    };
    let response_bytes = match serde_json::to_vec(&snapshot) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::error!(error = %error, "failed to serialize HiveObservationSnapshot");
            return;
        }
    };
    if let Err(error) = transport::write_frame(&mut stream, &response_bytes).await {
        tracing::debug!(error = %error, "observation control IPC write failed");
    }
}

#[cfg(windows)]
async fn run_observation_control_server(
    daemon: HiveObservationDaemon,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let pipe_name = std::env::var(HIVE_OBSERVATION_CONTROL_PIPE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_HIVE_OBSERVATION_CONTROL_PIPE.to_owned());
    let sid = evohime_desktop_ipc::windows_security::current_user_sid()?;
    tracing::info!(pipe = %pipe_name, "observation control IPC server listening");

    loop {
        let mut security = evohime_desktop_ipc::windows_security::PipeSecurity::owner_only(&sid)?;
        let server = unsafe {
            ServerOptions::new().create_with_security_attributes_raw(&pipe_name, security.as_raw())
        }?;
        server.connect().await?;
        let daemon = daemon.clone();
        tokio::spawn(async move {
            handle_observation_control_connection(server, daemon).await;
        });
    }
}

#[cfg(unix)]
async fn run_observation_control_server(
    daemon: HiveObservationDaemon,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let socket_path = match std::env::var_os(HIVE_OBSERVATION_CONTROL_SOCKET_ENV) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => {
            let paths = evohime_desktop_ipc::unix_runtime::UnixRuntimePaths::current()?;
            paths.directory.join("hive-observation.sock")
        }
    };
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }
    let listener = tokio::net::UnixListener::bind(&socket_path)?;
    tracing::info!(socket = %socket_path.display(), "observation control IPC server listening");

    loop {
        let (stream, _) = listener.accept().await?;
        let daemon = daemon.clone();
        tokio::spawn(async move {
            handle_observation_control_connection(stream, daemon).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_rate_limits_from_payload_records_and_clears() {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or_default();
        let future_ms = now_ms + 60_000;
        let model = "test-ipc-rate-limit-sync-model";

        evohime_model_gateway::clear_rate_limit(model);
        sync_rate_limits_from_payload(&serde_json::json!({
            "rate_limits": {
                "test-ipc-rate-limit-sync-model": future_ms
            }
        }));
        assert_eq!(
            evohime_model_gateway::rate_limited_until(model),
            Some(future_ms)
        );

        sync_rate_limits_from_payload(&serde_json::json!({
            "rate_limits": {}
        }));
        assert_eq!(evohime_model_gateway::rate_limited_until(model), None);
    }
}
