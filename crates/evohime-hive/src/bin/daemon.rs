//! Standalone entry point for the `evohime-hive-daemon` process.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use evohime_desktop_ipc::{generated, session, transport};
use evohime_hive::city_map::{
    assemble_snapshot, canonical_path_string, embedded_capital_island, offline_snapshot,
    repository_fingerprint, resolve_repo_root, satellite_id_for, scan_island, unavailable_island,
    CityIsland, CityIslandRole, CityIslandSource, CityIslandUnavailableReason, HiveCityMapSnapshot,
    RepoFingerprint, CAPITAL_ISLAND_ID, MAX_REPO_SCAN_DURATION, MAX_SATELLITES,
    MAX_TOTAL_SCAN_DURATION,
};
#[cfg(unix)]
use evohime_hive::pulse::HIVE_OBSERVATION_CONTROL_SOCKET_ENV;
use evohime_hive::pulse::{
    HiveObservationControlRequest, HiveObservationController, HiveObservationDaemon,
    DEFAULT_HIVE_OBSERVATION_CONTROL_PIPE, HIVE_OBSERVATION_CONTROL_PIPE_ENV,
};
use evohime_hive::settings::HiveSettings;
use evohime_hive::state::HiveStateSnapshot;
use evohime_hive_contract::{
    BoxFuture, HiveContractError, HiveCoordinator, HiveDispatchWindows, HiveEventRecord,
    HiveJournalProvider, HiveModelUsage, HiveTaskDispatchRequest, HiveTelemetrySink,
    HiveWorktreeLeaseRequest, WorkspaceProvider,
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
            build_start_task(request),
        ));
        session.write_command(command).await.map_err(|error| {
            HiveContractError::Dispatch(format!("failed to dispatch task over IPC: {error}"))
        })
    }

    async fn cancel_task(&self, task_id: String) -> Result<(), HiveContractError> {
        let mut session = self.session.lock().await;
        let command = session.command_envelope(generated::command_envelope::Command::StopTask(
            generated::StopTask { task_id },
        ));
        session.write_command(command).await.map_err(|error| {
            HiveContractError::Dispatch(format!("failed to cancel task over IPC: {error}"))
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

fn build_start_task(request: HiveTaskDispatchRequest) -> generated::StartTask {
    let client_message_id = request.task_id.clone();
    generated::StartTask {
        task_id: request.task_id,
        prompt: request.prompt,
        workspace_path: request.workspace_root.unwrap_or_default(),
        preferred_route_hint: request.preferred_route_hint.unwrap_or_default(),
        execution_kind: "agent".into(),
        conversation_id: "hive-system-chat".into(),
        client_message_id,
        goal_id: String::new(),
        provider: String::new(),
        profile_id: String::new(),
        model: String::new(),
        permission_mode: String::new(),
        isolation_mode: String::new(),
    }
}

fn snapshot_to_settings(snapshot: &HiveStateSnapshot) -> HiveSettings {
    HiveSettings {
        local_observation_interval_secs: snapshot.local_observation_interval_secs,
        llm_dispatch_interval_secs: snapshot.llm_dispatch_interval_secs,
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

    fn cancel_task(&self, task_id: String) -> BoxFuture<'_, Result<(), HiveContractError>> {
        Box::pin(async move { self.client.cancel_task(task_id).await })
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

struct FileHiveTelemetrySink {
    file: std::sync::Mutex<BufWriter<File>>,
}

impl FileHiveTelemetrySink {
    fn open_default() -> Option<Arc<Self>> {
        let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .filter(|value| !value.is_empty())
                    .map(|value| PathBuf::from(value).join("EvoHime"))
            })?;
        let log_path = data_dir.join("logs").join("hive.jsonl");
        if let Some(parent) = log_path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return None;
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .ok()?;
        Some(Arc::new(Self {
            file: std::sync::Mutex::new(BufWriter::new(file)),
        }))
    }
}

impl HiveTelemetrySink for FileHiveTelemetrySink {
    fn log_event(&self, level: &str, event: &str, fields: serde_json::Value) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let record = serde_json::json!({
            "timestamp_ms": timestamp_ms,
            "level": level,
            "event": event,
            "fields": fields,
        });
        let Ok(mut writer) = self.file.lock() else {
            return;
        };
        if serde_json::to_writer(&mut *writer, &record).is_ok() {
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("evohime_hive=info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

// ---------------------------------------------------------------------------
// City archipelago map runtime (plan 199.2)
// ---------------------------------------------------------------------------

/// Read timeout for one observation control connection request, agreed with
/// the 1500 ms client timeout on the shell side.
const OBSERVATION_CONTROL_READ_TIMEOUT: Duration = Duration::from_millis(1000);

/// Write timeout for one observation control connection response.
const OBSERVATION_CONTROL_WRITE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Fallback idle delay between city-map maintenance passes when the daemon
/// carries no real observation settings yet.
const CITY_MAP_FALLBACK_POLL: Duration = Duration::from_secs(5);

/// One queued city-map recompute job: immutable settings projection plus the
/// wall-clock instant it was requested at.
struct CityMapJob {
    eva_workspace_path: Option<String>,
    watched_repositories: Vec<String>,
    now_ms: i64,
}

/// Mutable single-flight state of the city-map recompute worker.
#[derive(Default)]
struct CityMapShared {
    /// Exactly one `spawn_blocking` worker runs while this flag is set.
    recomputing: bool,
    /// Latest queued job (replaced wholesale by newer settings); the worker
    /// drains it serially, so a request during an active scan becomes exactly
    /// one repeat pass after the current one.
    job: Option<CityMapJob>,
    /// Publish-ready islands of the last successful pass, keyed by island id.
    satellites: BTreeMap<String, CityIsland>,
    /// Repository fingerprints of the last full pass, keyed by canonical path.
    fingerprints: BTreeMap<String, RepoFingerprint>,
    /// Capital island of the last successful pass (live scan or embedded).
    capital_island: Option<CityIsland>,
    /// `(canonical path, fingerprint)` of the capital's last live scan.
    capital_fingerprint: Option<(String, RepoFingerprint)>,
}

/// Shared runtime of the city-archipelago map: keeps the latest published
/// snapshot, per-repository fingerprints, and a single-flight guard around
/// `spawn_blocking` recomputes. [`GetCityMap`](HiveObservationControlRequest::GetCityMap)
/// never blocks on the file system: it clones the last valid snapshot or
/// answers with the instant offline one on a cold start.
struct CityMapRuntime {
    shared: Arc<std::sync::Mutex<CityMapShared>>,
    published: Arc<RwLock<Option<Arc<HiveCityMapSnapshot>>>>,
    /// Wakes the maintenance loop right after a settings change.
    wake: Arc<tokio::sync::Notify>,
}

impl Clone for CityMapRuntime {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            published: Arc::clone(&self.published),
            wake: Arc::clone(&self.wake),
        }
    }
}

impl Default for CityMapRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl CityMapRuntime {
    fn new() -> Self {
        Self {
            shared: Arc::new(std::sync::Mutex::new(CityMapShared::default())),
            published: Arc::new(RwLock::new(None)),
            wake: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Returns the last published snapshot without touching the file system;
    /// the instant offline snapshot (embedded capital only) on a cold start.
    fn current(&self, now_ms: i64) -> Arc<HiveCityMapSnapshot> {
        match self.published.read() {
            Ok(guard) => match guard.as_ref() {
                Some(snapshot) => Arc::clone(snapshot),
                None => Arc::new(offline_snapshot(now_ms)),
            },
            Err(_) => Arc::new(offline_snapshot(now_ms)),
        }
    }

    /// Queues a recompute with the given settings projection. At most one
    /// `spawn_blocking` worker runs; a request during an active scan is
    /// coalesced into serial repeat passes (never a second worker).
    fn request_rebuild(&self, job: CityMapJob) {
        match self.shared.lock() {
            Ok(mut shared) => {
                shared.job = Some(job);
                if shared.recomputing {
                    return;
                }
                shared.recomputing = true;
            }
            Err(_) => return,
        }
        let this = self.clone();
        tokio::task::spawn_blocking(move || CityMapRuntime::worker(this));
    }

    /// Wakes the maintenance loop (settings changed on the shell side).
    fn kick(&self) {
        self.wake.notify_one();
    }

    /// Waits for a kick or the idle poll delay; returns `true` on a kick.
    async fn idle_wait(&self, idle_poll: Duration) -> bool {
        let wake = Arc::clone(&self.wake);
        tokio::select! {
            _ = wake.notified() => true,
            _ = tokio::time::sleep(idle_poll) => false,
        }
    }

    /// Single-flight recompute worker: drains the queued job and, when a
    /// rescan was requested meanwhile, repeats exactly once.
    fn worker(this: CityMapRuntime) {
        loop {
            enum Step {
                Run(CityMapJob),
                Done,
            }
            let step = match this.shared.lock() {
                Ok(mut shared) => match shared.job.take() {
                    Some(job) => Step::Run(job),
                    None => {
                        shared.recomputing = false;
                        Step::Done
                    }
                },
                Err(_) => return,
            };
            match step {
                Step::Run(job) => {
                    if let Err(error) = run_city_map_job(&this, &job) {
                        tracing::warn!(error = %error, "city map recompute failed");
                    }
                }
                Step::Done => return,
            }
        }
    }
}

/// Last-segment name of a canonical path (the island display name).
fn last_path_segment(canonical: &str) -> String {
    canonical.rsplit('/').next().unwrap_or(canonical).to_owned()
}

/// Rescans the capital from the live workspace; any failure falls back to the
/// embedded canonical manifest (mirrors `build_archipelago`).
fn rescan_capital(path: &str, total_deadline: Instant) -> CityIsland {
    let remaining = total_deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return embedded_capital_island();
    }
    let deadline = Instant::now() + remaining.min(MAX_REPO_SCAN_DURATION);
    match scan_island(
        Path::new(path),
        CAPITAL_ISLAND_ID.to_owned(),
        "EvoHime",
        CityIslandRole::Capital,
        Some(deadline),
    ) {
        Ok(island) => island,
        Err(reason) => {
            tracing::warn!(
                reason = ?reason,
                "city map: capital scan failed; using embedded manifest"
            );
            embedded_capital_island()
        }
    }
}

/// Computes one maintenance pass: per-repository fingerprint comparison, a
/// bounded rescan of the changed repositories only, and a conditional publish
/// (a pass with an unchanged revision set keeps the previous snapshot
/// instance, including its `generated_at_ms`).
fn run_city_map_job(runtime: &CityMapRuntime, job: &CityMapJob) -> Result<(), String> {
    let total_deadline = Instant::now() + MAX_TOTAL_SCAN_DURATION;
    let (prev_satellites, prev_fingerprints, prev_capital, prev_capital_fingerprint) = {
        match runtime.shared.lock() {
            Ok(shared) => (
                shared.satellites.clone(),
                shared.fingerprints.clone(),
                shared.capital_island.clone(),
                shared.capital_fingerprint.clone(),
            ),
            Err(_) => return Err("city map state lock poisoned".to_owned()),
        }
    };

    // --- Capital: live scan of the Eva workspace, or the embedded manifest.
    let capital_path = job
        .eva_workspace_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty());
    let mut satellite_cache: BTreeMap<String, CityIsland> = BTreeMap::new();
    let mut capital_fingerprint_entry: Option<(String, RepoFingerprint)> = None;
    let capital = match capital_path {
        Some(path) => {
            let key = canonical_path_string(path);
            match repository_fingerprint(Path::new(path)) {
                Ok(fingerprint) => {
                    capital_fingerprint_entry = Some((key, fingerprint));
                }
                Err(reason) => {
                    tracing::warn!(
                        reason = ?reason,
                        "city map: capital fingerprint failed; rescanning"
                    );
                }
            }
            let reusable = matches!((&capital_fingerprint_entry, &prev_capital_fingerprint, &prev_capital),
                    (Some((key, fp)), Some((prev_key, prev_fp)), Some(island))
                        if prev_key == key && prev_fp == fp
                            && island.source == CityIslandSource::LiveWorkspace);
            match (reusable, prev_capital.clone()) {
                (true, Some(island)) => island,
                _ => rescan_capital(path, total_deadline),
            }
        }
        None => embedded_capital_island(),
    };

    // --- Satellites: canonical identity, bounded rescan of changed repos.
    // The capital already renders the Eva workspace checkout: a watched
    // repository pointing at the same directory must not duplicate it as a
    // second city (mirrors the guard of `build_archipelago_with_budgets`).
    let capital_canonical = capital_path.map(resolve_repo_root);
    let mut entries: Vec<(String, String, PathBuf)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for raw in &job.watched_repositories {
        let canonical = resolve_repo_root(raw);
        if canonical.is_empty()
            || Some(canonical.as_str()) == capital_canonical.as_deref()
            || !seen.insert(canonical.clone())
        {
            continue;
        }
        entries.push((
            satellite_id_for(&canonical),
            last_path_segment(&canonical),
            PathBuf::from(canonical),
        ));
    }
    entries.sort_by(|a, b| (a.0.as_str(), a.1.as_str()).cmp(&(b.0.as_str(), b.1.as_str())));
    entries.truncate(MAX_SATELLITES);

    let mut satellites: Vec<CityIsland> = Vec::with_capacity(entries.len());
    let mut published_fingerprints: BTreeMap<String, RepoFingerprint> = BTreeMap::new();
    for (id, name, root) in entries {
        let canonical = root.to_string_lossy().into_owned();
        let fingerprint = repository_fingerprint(&root);
        if let Ok(fp) = &fingerprint {
            if let Some(prev_fp) = prev_fingerprints.get(&canonical) {
                if prev_fp == fp {
                    if let Some(prev_island) = prev_satellites.get(&id) {
                        satellites.push(prev_island.clone());
                        published_fingerprints.insert(canonical.clone(), fp.clone());
                        satellite_cache.insert(id.clone(), prev_island.clone());
                        continue;
                    }
                }
            }
        }
        // The repository is new or its on-disk signature changed.
        let remaining = total_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            // Budget exhausted: serve the cached island, else an explicit
            // unavailable placeholder; the fingerprint stays untouched so the
            // next pass rescans properly.
            match prev_satellites.get(&id) {
                Some(prev_island) => satellites.push(prev_island.clone()),
                None => satellites.push(unavailable_island(
                    id.clone(),
                    name.clone(),
                    CityIslandRole::Satellite,
                    CityIslandUnavailableReason::ScanTimeout,
                )),
            }
            continue;
        }
        let deadline = Instant::now() + remaining.min(MAX_REPO_SCAN_DURATION);
        let (island, fresh_fingerprint) = match scan_island(
            &root,
            id.clone(),
            &name,
            CityIslandRole::Satellite,
            Some(deadline),
        ) {
            Ok(island) => match &fingerprint {
                Ok(fp) => (island, Some(fp.clone())),
                Err(_) => (island, None),
            },
            Err(reason) => {
                tracing::warn!(
                    island = %id,
                    reason = ?reason,
                    "city map: satellite scan failed"
                );
                match prev_satellites.get(&id) {
                    // Keep the previous structure; the fresh fingerprint is
                    // recorded so the next unchanged pass reuses it.
                    Some(prev_island) => match &fingerprint {
                        Ok(fp) => (prev_island.clone(), Some(fp.clone())),
                        Err(_) => (prev_island.clone(), None),
                    },
                    None => (
                        unavailable_island(
                            id.clone(),
                            name.clone(),
                            CityIslandRole::Satellite,
                            reason,
                        ),
                        None,
                    ),
                }
            }
        };
        if let Some(fp) = fresh_fingerprint {
            published_fingerprints.insert(canonical.clone(), fp);
        }
        satellite_cache.insert(id.clone(), island.clone());
        satellites.push(island);
    }

    // --- Cache-hit fast path: identical revision set keeps the old snapshot.
    {
        let published = match runtime.published.read() {
            Ok(guard) => guard,
            Err(_) => return Err("city map published lock poisoned".to_owned()),
        };
        if let Some(prev_snapshot) = published.as_ref() {
            let same_capital = prev_snapshot.capital.revision_hash == capital.revision_hash
                && prev_snapshot.capital.source == capital.source;
            let same_satellites =
                prev_snapshot.satellites.len() == satellites.len()
                    && prev_snapshot.satellites.iter().zip(satellites.iter()).all(
                        |(prev, next)| {
                            prev.id == next.id && prev.revision_hash == next.revision_hash
                        },
                    );
            if same_capital && same_satellites {
                if let Ok(mut shared) = runtime.shared.lock() {
                    shared.fingerprints = published_fingerprints;
                    shared.satellites = satellite_cache;
                    shared.capital_island = Some(capital);
                    shared.capital_fingerprint = capital_fingerprint_entry;
                }
                return Ok(());
            }
        }
    }

    // --- Publish the fresh snapshot.
    let snapshot = Arc::new(assemble_snapshot(capital, satellites, job.now_ms));
    match runtime.published.write() {
        Ok(mut guard) => *guard = Some(Arc::clone(&snapshot)),
        Err(_) => return Err("city map published lock poisoned".to_owned()),
    }
    if let Ok(mut shared) = runtime.shared.lock() {
        shared.fingerprints = published_fingerprints;
        shared.satellites = satellite_cache;
    }
    tracing::info!(
        revision = %snapshot.revision_hash,
        satellites = snapshot.satellites.len(),
        "city map snapshot published"
    );
    Ok(())
}

/// Maintenance loop of the city map: fetches Hive settings from Core at the
/// local observation cadence (plus kicks from `NotifySettingsChanged`), and
/// recomputes the archipelago in a bounded background worker.
async fn run_city_map_maintenance(
    city_map: CityMapRuntime,
    honeycomb: HoneycombRuntime,
    client: Arc<CoreIpcClient>,
) {
    loop {
        let mut idle_poll = CITY_MAP_FALLBACK_POLL;
        let mut job = None;
        match client.fetch_hive_state().await {
            Ok(state) => {
                let settings = snapshot_to_settings(&state);
                let now_ms = unix_now_ms();
                idle_poll = Duration::from_secs(settings.local_observation_interval_secs.max(1));
                honeycomb.sync_maintenance(&state, &settings, now_ms);
                job = Some(CityMapJob {
                    eva_workspace_path: settings.eva_workspace_path.clone(),
                    watched_repositories: settings.watched_repositories.clone(),
                    now_ms,
                });
            }
            Err(error) => {
                tracing::warn!(error = %error, "city map: failed to fetch hive state");
            }
        }
        if let Some(job) = job {
            city_map.request_rebuild(job);
        }
        // A kick shortens the wait: settings just changed on the shell side.
        if city_map.idle_wait(idle_poll).await {
            continue;
        }
    }
}

/// Current Unix time in milliseconds; `0` if the clock is before the epoch.
fn unix_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|delta| i64::try_from(delta.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    tracing::info!("evohime-hive-daemon starting");
    let file_telemetry = FileHiveTelemetrySink::open_default();

    let has_hive_key = std::env::var("HIVE_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let has_fallback_key = std::env::var("HIVE_FALLBACK_API_KEY")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if !has_hive_key {
        tracing::info!(
            "HIVE_API_KEY is not set; continuing in case a local Hive provider (e.g. ollama) is configured"
        );
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
        if let Some(sink) = file_telemetry.as_ref() {
            sink.log_event(
                "error",
                "hive.connect_failed",
                serde_json::json!({ "reason": "core_ipc_unavailable" }),
            );
        }
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

    let telemetry: Option<Arc<dyn HiveTelemetrySink>> = file_telemetry
        .as_ref()
        .map(|sink| Arc::clone(sink) as Arc<dyn HiveTelemetrySink>);
    let daemon = HiveObservationDaemon::start(
        journal,
        gateway_config,
        workspace_provider,
        coordinator,
        telemetry,
    );
    let control_daemon = daemon.clone();
    let city_map_runtime = CityMapRuntime::new();
    let control_city_map = city_map_runtime.clone();
    let honeycomb_runtime = HoneycombRuntime::open_default(
        settings.eva_workspace_path.as_deref(),
        &settings.watched_repositories,
    );
    let control_honeycomb = honeycomb_runtime.clone();
    let control_task = tokio::spawn(async move {
        if let Err(error) =
            run_observation_control_server(control_daemon, control_city_map, control_honeycomb)
                .await
        {
            tracing::error!(error = %error, "observation control IPC server stopped");
        }
    });
    // Bounded background maintenance of the city-archipelago map (plan 199.2) and Honeycomb engine (plan 200).
    let city_map_maintenance = tokio::spawn(run_city_map_maintenance(
        city_map_runtime.clone(),
        honeycomb_runtime.clone(),
        Arc::clone(&client),
    ));
    let status = daemon.status();
    tracing::info!(
        status = %status.status,
        enabled = status.enabled,
        "evohime-hive-daemon started"
    );
    if let Some(sink) = file_telemetry.as_ref() {
        sink.log_event(
            "info",
            "hive.daemon_started",
            serde_json::json!({
                "status": status.status,
                "enabled": status.enabled,
                "has_hive_key": has_hive_key,
                "has_fallback_key": has_fallback_key,
                "proactive_evolution_enabled": settings.proactive_evolution_enabled,
            }),
        );
    }

    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(error = %error, "failed to listen for ctrl-c signal");
    }

    control_task.abort();
    city_map_maintenance.abort();
    daemon.shutdown();
    ExitCode::SUCCESS
}

#[derive(Clone)]
struct HoneycombRuntime {
    engine: Arc<std::sync::Mutex<evohime_hive::honeycomb_engine::HoneycombEngine>>,
}

impl Default for HoneycombRuntime {
    fn default() -> Self {
        Self::in_memory(None, &[])
    }
}

impl HoneycombRuntime {
    fn open_default(eva_workspace_path: Option<&str>, watched_repositories: &[String]) -> Self {
        let base_dir = std::env::var_os("EVOHIME_DATA_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .filter(|v| !v.is_empty())
                    .map(|root| PathBuf::from(root).join("EvoHime"))
            });
        if let Some(dir) = base_dir {
            let db_path = dir.join("hive.db");
            if let Ok(db) = evohime_hive::honeycomb_db::HiveDatabase::open(&db_path) {
                if let Ok(engine) = evohime_hive::honeycomb_engine::HoneycombEngine::open(
                    db,
                    eva_workspace_path,
                    watched_repositories,
                ) {
                    return Self {
                        engine: Arc::new(std::sync::Mutex::new(engine)),
                    };
                }
            }
        }
        Self::in_memory(eva_workspace_path, watched_repositories)
    }

    fn in_memory(eva_workspace_path: Option<&str>, watched_repositories: &[String]) -> Self {
        let db = match evohime_hive::honeycomb_db::HiveDatabase::open_in_memory() {
            Ok(db) => db,
            Err(_) => {
                let fallback = std::env::temp_dir().join("evohime-hive-fallback.db");
                #[allow(clippy::expect_used)]
                evohime_hive::honeycomb_db::HiveDatabase::open(&fallback).expect("temp hive db")
            }
        };
        #[allow(clippy::expect_used)]
        let engine = evohime_hive::honeycomb_engine::HoneycombEngine::open(
            db,
            eva_workspace_path,
            watched_repositories,
        )
        .expect("in-memory honeycomb engine");
        Self {
            engine: Arc::new(std::sync::Mutex::new(engine)),
        }
    }

    fn snapshot_world(
        &self,
        force_pause: bool,
    ) -> Option<evohime_hive_contract::honeycomb::HoneycombWorldState> {
        let mut guard = self.engine.lock().ok()?;
        if force_pause {
            let _ = guard.set_active_mode(false, false, false);
        }
        Some(guard.world().clone())
    }

    fn delta_since(
        &self,
        from_revision: u64,
    ) -> Option<evohime_hive_contract::honeycomb::HoneycombWorldDelta> {
        let guard = self.engine.lock().ok()?;
        guard.database().read_delta_since(from_revision, 256).ok()
    }

    fn sync_maintenance(&self, state: &HiveStateSnapshot, settings: &HiveSettings, now_ms: i64) {
        let Ok(mut guard) = self.engine.lock() else {
            return;
        };
        let _ = guard.sync_projects(
            settings.eva_workspace_path.as_deref(),
            &settings.watched_repositories,
        );
        let (hive_active, waiting_for_limits, queen_working) = resolve_honeycomb_mode(state);
        let _ = guard.set_active_mode(hive_active, waiting_for_limits, queen_working);

        if hive_active && !waiting_for_limits {
            // Сканирование и перестройка модулей проектов (с кулдауном 5 мин внутри движка).
            let mut project_paths: Vec<(String, PathBuf)> = Vec::new();
            if let Some(eva_path) = settings
                .eva_workspace_path
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            {
                project_paths.push(("city-eva".to_string(), PathBuf::from(eva_path)));
            }
            for (idx, repo) in settings.watched_repositories.iter().enumerate() {
                if !repo.trim().is_empty() {
                    project_paths.push((format!("city-repo-{idx}"), PathBuf::from(repo)));
                }
            }
            for (city_id, path) in project_paths {
                let _ = guard.remodel_city_from_checkout(&city_id, &path, now_ms);
            }

            // Стартовый триггер разведки запускается только при явно включённой автономке.
            let primary_has_sectors = guard
                .world()
                .cities
                .first()
                .map(|c| !c.sectors.is_empty())
                .unwrap_or(false);
            if state.proactive_evolution_enabled && !primary_has_sectors {
                let _ = guard.dispatch_relay_step(
                    evohime_hive::honeycomb_engine::RelayStage::Execution,
                    "разведка проекта",
                    None,
                    now_ms,
                );
            } else if !state.active_agents.is_empty() {
                for agent in state.active_agents.iter().take(4) {
                    let stage = if agent.role.contains("аналит") || agent.role.contains("analyst")
                    {
                        evohime_hive::honeycomb_engine::RelayStage::Analysis
                    } else if agent.role.contains("стратег") || agent.role.contains("plan") {
                        evohime_hive::honeycomb_engine::RelayStage::Strategy
                    } else {
                        evohime_hive::honeycomb_engine::RelayStage::Execution
                    };
                    let topic = if agent.current_action.trim().is_empty() {
                        agent.role.as_str()
                    } else {
                        agent.current_action.as_str()
                    };
                    let _ = guard.dispatch_relay_step(stage, topic, None, now_ms);
                }
            }
        }
    }
}

/// Resolves `(hive_active, waiting_for_limits, queen_working)` for the Honeycomb engine.
/// When proactive autonomy is disabled and there are no active tasks/agents, the Hive
/// stays paused with zero vehicles on the roads.
fn resolve_honeycomb_mode(state: &HiveStateSnapshot) -> (bool, bool, bool) {
    let obs_status = state.observation_status.as_str();
    let waiting_for_limits =
        obs_status == evohime_hive::state::HIVE_OBSERVATION_WAITING_FOR_LIMITS;
    let observation_alive = obs_status != evohime_hive::state::HIVE_OBSERVATION_DISABLED
        && obs_status != evohime_hive::state::HIVE_OBSERVATION_SLEEPING;
    let has_active_work = state.queen.status == evohime_hive::state::HIVE_QUEEN_ACTIVE
        || !state.active_agents.is_empty();
    let hive_active = observation_alive
        && !waiting_for_limits
        && (state.proactive_evolution_enabled || has_active_work);
    let queen_working = hive_active && has_active_work;
    (hive_active, waiting_for_limits, queen_working)
}

async fn handle_observation_control_connection<S>(
    mut stream: S,
    daemon: HiveObservationDaemon,
    city_map: CityMapRuntime,
    honeycomb: HoneycombRuntime,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // Explicit timeouts keep a slow reader or writer from pinning a control
    // socket and from affecting other in-flight connections (each connection
    // also runs on its own tokio task).
    let payload = match tokio::time::timeout(
        OBSERVATION_CONTROL_READ_TIMEOUT,
        transport::read_frame(&mut stream),
    )
    .await
    {
        Ok(Ok(payload)) => payload,
        Ok(Err(error)) => {
            tracing::debug!(error = %error, "observation control IPC read failed");
            return;
        }
        Err(_) => {
            tracing::debug!("observation control IPC read timed out");
            return;
        }
    };
    let request = match serde_json::from_slice::<HiveObservationControlRequest>(&payload) {
        Ok(request) => request,
        // Canonical old-protocol behavior: an invalid/unknown request body is
        // answered with the `invalid_request` error frame; a new shell treats
        // it as `unavailable` while the daemon module has not caught up.
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
    match request {
        HiveObservationControlRequest::GetCityMap => {
            // Never blocks on the file system: the last valid snapshot or the
            // instant offline one; the payload is bounded by
            // MAX_CITY_MAP_PAYLOAD_BYTES by construction.
            let snapshot = city_map.current(unix_now_ms());
            let response_bytes = match serde_json::to_vec(&*snapshot) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::error!(error = %error, "failed to serialize HiveCityMapSnapshot");
                    return;
                }
            };
            if let Err(error) = tokio::time::timeout(
                OBSERVATION_CONTROL_WRITE_TIMEOUT,
                transport::write_frame(&mut stream, &response_bytes),
            )
            .await
            {
                let _ = error;
                tracing::debug!("observation control IPC city-map write failed or timed out");
            }
        }
        HiveObservationControlRequest::GetHoneycombWorld => {
            let status = daemon.status();
            let force_pause = !status.enabled
                || status.status == evohime_hive::state::HIVE_OBSERVATION_WAITING_FOR_LIMITS
                || status.status == evohime_hive::state::HIVE_OBSERVATION_DISABLED
                || status.status == evohime_hive::state::HIVE_OBSERVATION_SLEEPING;
            let Some(world) = honeycomb.snapshot_world(force_pause) else {
                return;
            };
            let response_bytes = match serde_json::to_vec(&world) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::error!(error = %error, "failed to serialize HoneycombWorldState");
                    return;
                }
            };
            let _ = tokio::time::timeout(
                OBSERVATION_CONTROL_WRITE_TIMEOUT,
                transport::write_frame(&mut stream, &response_bytes),
            )
            .await;
        }
        HiveObservationControlRequest::GetHoneycombDelta { from_revision } => {
            let Some(delta) = honeycomb.delta_since(from_revision) else {
                return;
            };
            let response_bytes = match serde_json::to_vec(&delta) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::error!(error = %error, "failed to serialize HoneycombWorldDelta");
                    return;
                }
            };
            let _ = tokio::time::timeout(
                OBSERVATION_CONTROL_WRITE_TIMEOUT,
                transport::write_frame(&mut stream, &response_bytes),
            )
            .await;
        }
        HiveObservationControlRequest::NotifySettingsChanged => {
            let controller = HiveObservationController::new(daemon);
            controller.notify_settings_changed();
            // The shell changed settings: shorten the city-map wait instead of
            // waiting for the next scheduled maintenance pass.
            city_map.kick();
            let snapshot = controller.snapshot();
            let response_bytes = match serde_json::to_vec(&snapshot) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::error!(error = %error, "failed to serialize HiveObservationSnapshot");
                    return;
                }
            };
            if let Err(error) = tokio::time::timeout(
                OBSERVATION_CONTROL_WRITE_TIMEOUT,
                transport::write_frame(&mut stream, &response_bytes),
            )
            .await
            {
                let _ = error;
                tracing::debug!("observation control IPC write failed or timed out");
            }
        }
        HiveObservationControlRequest::GetObservationStatus => {
            let controller = HiveObservationController::new(daemon);
            let snapshot = controller.snapshot();
            let response_bytes = match serde_json::to_vec(&snapshot) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::error!(error = %error, "failed to serialize HiveObservationSnapshot");
                    return;
                }
            };
            if let Err(error) = tokio::time::timeout(
                OBSERVATION_CONTROL_WRITE_TIMEOUT,
                transport::write_frame(&mut stream, &response_bytes),
            )
            .await
            {
                let _ = error;
                tracing::debug!("observation control IPC write failed or timed out");
            }
        }
    }
}

#[cfg(windows)]
async fn run_observation_control_server(
    daemon: HiveObservationDaemon,
    city_map: CityMapRuntime,
    honeycomb: HoneycombRuntime,
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
        let city_map = city_map.clone();
        let honeycomb = honeycomb.clone();
        tokio::spawn(async move {
            handle_observation_control_connection(server, daemon, city_map, honeycomb).await;
        });
    }
}

#[cfg(unix)]
async fn run_observation_control_server(
    daemon: HiveObservationDaemon,
    city_map: CityMapRuntime,
    honeycomb: HoneycombRuntime,
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
        let city_map = city_map.clone();
        let honeycomb = honeycomb.clone();
        tokio::spawn(async move {
            handle_observation_control_connection(stream, daemon, city_map, honeycomb).await;
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

    #[test]
    fn build_start_task_sets_client_message_id_for_hive_conversation() {
        let start = build_start_task(HiveTaskDispatchRequest {
            task_id: "eva-pulse-12345678-1234-1234-1234-1234567890ab".into(),
            prompt: "Run proactive evolution".into(),
            workspace_root: Some("D:\\github\\EvoHime".into()),
            preferred_route_hint: Some("hive_fallback".into()),
        });
        assert_eq!(start.conversation_id, "hive-system-chat");
        assert_eq!(
            start.client_message_id,
            "eva-pulse-12345678-1234-1234-1234-1234567890ab"
        );
        assert_eq!(start.preferred_route_hint, "hive_fallback");
    }

    #[test]
    fn snapshot_to_settings_preserves_observation_intervals() {
        let settings = HiveSettings {
            local_observation_interval_secs: 15,
            llm_dispatch_interval_secs: 600,
            ..HiveSettings::default()
        };
        let snapshot = HiveStateSnapshot::empty(&settings);
        let restored = snapshot_to_settings(&snapshot);
        assert_eq!(restored.local_observation_interval_secs, 15);
        assert_eq!(restored.llm_dispatch_interval_secs, 600);
    }

    // -------------------------------------------------------------------
    // City archipelago map (plan 199.2)
    // -------------------------------------------------------------------

    use evohime_hive::pulse::{
        FailedWorkerContext, HiveObservationOptions, ObservationCycleReport, ObservationFuture,
        ObservationProbe, ObservationTaskSink, RoadmapItem,
    };

    /// Probe seam that never observes successfully.
    struct NoopProbe;

    impl ObservationProbe for NoopProbe {
        fn observe(&self) -> ObservationFuture<'_, Result<ObservationCycleReport, String>> {
            Box::pin(async { Err("noop".to_owned()) })
        }
    }

    /// Sink seam that never dispatches tasks.
    struct NoopSink;

    impl ObservationTaskSink for NoopSink {
        fn dispatch_recovery(
            &self,
            _failed_workers: &[FailedWorkerContext],
            _eva_workspace_path: Option<&str>,
        ) -> ObservationFuture<'_, Result<String, String>> {
            Box::pin(async { Err("noop".to_owned()) })
        }

        fn dispatch_evolution(
            &self,
            _items: &[RoadmapItem],
            _eva_workspace_path: Option<&str>,
        ) -> ObservationFuture<'_, Result<String, String>> {
            Box::pin(async { Err("noop".to_owned()) })
        }
    }

    /// Spawns an observation daemon with inert seams for control-connection tests.
    fn noop_daemon() -> HiveObservationDaemon {
        HiveObservationDaemon::spawn(
            Arc::new(NoopProbe),
            Arc::new(NoopSink),
            HiveObservationOptions::default(),
            None,
        )
    }

    /// Sends one framed control request against an in-memory stream and returns
    /// the framed response; the connection task is joined to surface panics.
    async fn exchange_control_frame(request: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let (mut client, server) = tokio::io::duplex(256 * 1024);
        let connection = tokio::spawn(handle_observation_control_connection(
            server,
            noop_daemon(),
            CityMapRuntime::default(),
            HoneycombRuntime::default(),
        ));
        transport::write_frame(&mut client, request).await?;
        let response =
            tokio::time::timeout(Duration::from_secs(5), transport::read_frame(&mut client))
                .await??;
        connection.await.expect("connection task");
        Ok(response)
    }

    #[test]
    fn city_map_cache_hit_keeps_snapshot_instance() {
        let runtime = CityMapRuntime::default();
        let job = CityMapJob {
            eva_workspace_path: None,
            watched_repositories: Vec::new(),
            now_ms: unix_now_ms(),
        };
        run_city_map_job(&runtime, &job).expect("first city map pass");
        let first = runtime.current(unix_now_ms());
        run_city_map_job(&runtime, &job).expect("second city map pass");
        let second = runtime.current(unix_now_ms());
        assert!(
            Arc::ptr_eq(&first, &second),
            "unchanged pass must republish the same snapshot instance"
        );
        assert_eq!(first.revision_hash, second.revision_hash);
        assert_eq!(first.generated_at_ms, second.generated_at_ms);
        assert_eq!(first.capital.id, CAPITAL_ISLAND_ID);
        assert!(first.satellites.is_empty());
    }

    #[test]
    fn city_map_new_satellite_changes_revision() {
        let runtime = CityMapRuntime::default();
        let empty_job = CityMapJob {
            eva_workspace_path: None,
            watched_repositories: Vec::new(),
            now_ms: unix_now_ms(),
        };
        run_city_map_job(&runtime, &empty_job).expect("empty city map pass");
        let baseline = runtime.current(unix_now_ms());
        assert!(baseline.satellites.is_empty());

        let temp = tempfile::tempdir().expect("tempdir");
        let src = temp.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir");
        std::fs::write(src.join("main.rs"), "fn main() {}\n").expect("source file");

        let job = CityMapJob {
            eva_workspace_path: None,
            watched_repositories: vec![temp.path().to_string_lossy().into_owned()],
            now_ms: unix_now_ms(),
        };
        run_city_map_job(&runtime, &job).expect("satellite city map pass");
        let updated = runtime.current(unix_now_ms());
        assert_eq!(updated.satellites.len(), 1);
        let satellite = &updated.satellites[0];
        assert_eq!(satellite.role, CityIslandRole::Satellite);
        assert_eq!(satellite.source, CityIslandSource::LiveWorkspace);
        assert_eq!(satellite.status, city_map_status_ready());
        assert_ne!(updated.revision_hash, baseline.revision_hash);
    }

    fn city_map_status_ready() -> evohime_hive::city_map::CityIslandStatus {
        evohime_hive::city_map::CityIslandStatus::Ready
    }

    #[tokio::test]
    async fn control_connection_get_city_map_returns_snapshot_frame() {
        let request = serde_json::json!({ "command": "get_city_map" }).to_string();
        let response = exchange_control_frame(request.as_bytes())
            .await
            .expect("city map exchange");
        let value: serde_json::Value = serde_json::from_slice(&response).expect("city map JSON");
        assert!(value.get("error").is_none(), "get_city_map must not error");
        let snapshot: HiveCityMapSnapshot = serde_json::from_value(value).expect("typed snapshot");
        assert_eq!(
            snapshot.capital.id, CAPITAL_ISLAND_ID,
            "cold start publishes the embedded capital"
        );
        assert!(snapshot.satellites.is_empty());
        assert!(!snapshot.revision_hash.is_empty());
    }

    #[tokio::test]
    async fn control_connection_get_observation_status_keeps_contract() {
        let request = serde_json::json!({ "command": "get_observation_status" }).to_string();
        let response = exchange_control_frame(request.as_bytes())
            .await
            .expect("status exchange");
        let value: serde_json::Value = serde_json::from_slice(&response).expect("status JSON");
        for key in [
            "schema_version",
            "owner",
            "status",
            "enabled",
            "next_wake_at_ms",
        ] {
            assert!(value.get(key).is_some(), "missing `{key}` in {value}");
        }
        assert_eq!(value["status"], "observing");
        assert_eq!(value["enabled"], true);
    }

    #[tokio::test]
    async fn control_connection_unknown_command_answers_invalid_request() {
        let request = serde_json::json!({ "command": "definitely_unknown" }).to_string();
        let response = exchange_control_frame(request.as_bytes())
            .await
            .expect("unknown command exchange");
        let value: serde_json::Value = serde_json::from_slice(&response).expect("error JSON");
        let error = value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .expect("error string");
        assert!(
            error.starts_with("invalid_request:"),
            "unexpected error payload: {error}"
        );
    }

    #[test]
    fn honeycomb_stays_paused_without_vehicles_when_autonomy_is_disabled_and_idle() {
        let settings = HiveSettings::default();
        let mut state = HiveStateSnapshot::empty(&settings);
        state.observation_status = evohime_hive::state::HIVE_OBSERVATION_WAITING_FOR_TASKS.to_string();
        state.proactive_evolution_enabled = false;

        let dir = tempfile::tempdir().expect("tempdir");
        let db = evohime_hive::honeycomb_db::HiveDatabase::open(&dir.path().join("hive.db"))
            .expect("open db");
        let engine = evohime_hive::honeycomb_engine::HoneycombEngine::open(
            db,
            None,
            &["C:/Projects/EvoHime".to_string()],
        )
        .expect("open engine");
        let runtime = HoneycombRuntime {
            engine: Arc::new(std::sync::Mutex::new(engine)),
        };

        runtime.sync_maintenance(&state, &settings, 1_000);
        let world = runtime.snapshot_world(false).expect("world");
        assert!(world.paused);
        assert!(!world.queen_working);
        assert!(world.active_dispatches.is_empty());
        assert!(world.cities[0].sectors.is_empty());
    }

    #[test]
    fn honeycomb_parks_vehicles_when_manual_task_finishes_with_autonomy_disabled() {
        let settings = HiveSettings::default();
        let mut state = HiveStateSnapshot::empty(&settings);
        state.observation_status = evohime_hive::state::HIVE_OBSERVATION_WAITING_FOR_TASKS.to_string();
        state.proactive_evolution_enabled = false;
        state.queen.status = evohime_hive::state::HIVE_QUEEN_ACTIVE.to_string();
        state.active_agents.push(evohime_hive::state::HiveActiveAgent {
            id: "agent-1".to_string(),
            display_name: Some("Исполнитель #1".to_string()),
            role: "executor".to_string(),
            current_action: "проверка кода".to_string(),
            depth: 1,
            spawned_subagents: 0,
            ceiling: 4,
        });

        let dir = tempfile::tempdir().expect("tempdir");
        let db = evohime_hive::honeycomb_db::HiveDatabase::open(&dir.path().join("hive.db"))
            .expect("open db");
        let engine = evohime_hive::honeycomb_engine::HoneycombEngine::open(
            db,
            None,
            &["C:/Projects/EvoHime".to_string()],
        )
        .expect("open engine");
        let runtime = HoneycombRuntime {
            engine: Arc::new(std::sync::Mutex::new(engine)),
        };

        runtime.sync_maintenance(&state, &settings, 2_000);
        let active_world = runtime.snapshot_world(false).expect("active world");
        assert!(!active_world.paused);
        assert!(active_world.queen_working);
        assert_eq!(active_world.active_dispatches.len(), 1);

        state.queen.status = evohime_hive::state::HIVE_QUEEN_IDLE.to_string();
        state.active_agents.clear();
        runtime.sync_maintenance(&state, &settings, 3_000);
        let idle_world = runtime.snapshot_world(false).expect("idle world");
        assert!(idle_world.paused);
        assert!(!idle_world.queen_working);
        assert!(idle_world.active_dispatches.is_empty());
    }
}
