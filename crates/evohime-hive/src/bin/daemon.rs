//! Standalone entry point for the `evohime-hive-daemon` process.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use evohime_desktop_ipc::{generated, session, transport};
use evohime_hive::pulse::EvaPulseDaemon;
use evohime_hive::settings::HiveSettings;
use evohime_hive::state::HiveStateSnapshot;
use evohime_hive_contract::{
    BoxFuture, HiveContractError, HiveCoordinator, HiveDispatchWindows, HiveEventRecord,
    HiveJournalProvider, HiveModelUsage, HiveTaskDispatchRequest, HiveWorktreeLeaseRequest,
    WorkspaceProvider,
};
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
    async fn connect(stream: S, context: &session::LaunchContext) -> Result<Self, HiveContractError> {
        context
            .validate()
            .map_err(|error| HiveContractError::Unavailable(format!("invalid launch context: {error}")))?;
        let client_id = format!("hive-{}", uuid::Uuid::new_v4());
        let mut session = Self {
            stream,
            client_id: client_id.clone(),
            core_instance_id: String::new(),
            session_epoch: 0,
        };
        let challenge = session.read_event().await?;
        let nonce = challenge_nonce(&challenge)?;
        let proof = context.secret.proof("cli", &client_id, &nonce);
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
                        client_role: "cli".into(),
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
}

impl CoreIpcClient {
    async fn connect() -> Result<Self, HiveContractError> {
        let (stream, context) = open_core_stream().await?;
        let session = CoreIpcSession::connect(stream, &context).await?;
        Ok(Self {
            session: Mutex::new(session),
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
        let command = session.command_envelope(
            generated::command_envelope::Command::SetEvaWorkspacePath(
                generated::SetEvaWorkspacePathCommand {
                    schema_version: 1,
                    request_id: uuid::Uuid::new_v4().to_string(),
                    path: path.to_owned(),
                },
            ),
        );
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
                conversation_id: String::new(),
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

struct IpcJournalProvider {
    client: Arc<CoreIpcClient>,
}

impl HiveJournalProvider for IpcJournalProvider {
    fn latest_sequence(&self) -> BoxFuture<'_, i64> {
        Box::pin(async { 0 })
    }

    fn replay_events(
        &self,
        _after_sequence: i64,
        _limit: usize,
    ) -> BoxFuture<'_, Result<Vec<HiveEventRecord>, HiveContractError>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn record_audit<'a>(
        &'a self,
        _task_id: &'a str,
        _event_type: &'a str,
        _payload: &'a [u8],
    ) -> BoxFuture<'a, Result<(), HiveContractError>> {
        Box::pin(async { Ok(()) })
    }

    fn prune_signal_records<'a>(
        &'a self,
        _event_types: &'a [&'a str],
        _keep_limit: usize,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    fn load_hive_settings_json(
        &self,
    ) -> BoxFuture<'_, Result<Option<serde_json::Value>, HiveContractError>> {
        Box::pin(async move {
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
            let parsed: HiveSettings = serde_json::from_value(settings.clone()).map_err(|error| {
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

    fn filter_hive_root_tasks(&self, _roots: Vec<String>) -> BoxFuture<'_, Vec<String>> {
        Box::pin(async { Vec::new() })
    }

    fn model_dispatch_windows<'a>(
        &'a self,
        _provider: &'a str,
        _model: &'a str,
        _now_ms: i64,
        _minute_window_ms: i64,
        _hour_window_ms: i64,
        _day_window_ms: i64,
    ) -> BoxFuture<'a, Result<HiveDispatchWindows, HiveContractError>> {
        Box::pin(async { Ok(HiveDispatchWindows::default()) })
    }

    fn model_usage_since<'a>(
        &'a self,
        _start_ms: i64,
        _provider: &'a str,
        _model: &'a str,
    ) -> BoxFuture<'a, Result<Vec<HiveModelUsage>, HiveContractError>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn route_tokens_since<'a>(
        &'a self,
        _start_ms: i64,
        _route_scope: &'a str,
    ) -> BoxFuture<'a, Result<u64, HiveContractError>> {
        Box::pin(async { Ok(0) })
    }

    fn register_worktree_lease(&self, _request: HiveWorktreeLeaseRequest) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    fn has_active_workflow_run<'a>(&'a self, _task_id: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async { false })
    }

    fn mark_worktree_cleanup_pending<'a>(&'a self, _worktree_id: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async {})
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

    let client = match CoreIpcClient::connect().await {
        Ok(client) => Arc::new(client),
        Err(error) => {
            tracing::error!(error = %error, "failed to connect to Core IPC; exiting");
            return ExitCode::FAILURE;
        }
    };

    let journal: Arc<dyn HiveJournalProvider> = Arc::new(IpcJournalProvider {
        client: Arc::clone(&client),
    });
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

    let daemon = EvaPulseDaemon::start(journal, None, workspace_provider, coordinator, None);
    let status = daemon.status();
    tracing::info!(
        status = %status.status,
        enabled = status.enabled,
        "evohime-hive-daemon started"
    );

    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!(error = %error, "failed to listen for ctrl-c signal");
    }

    daemon.shutdown();
    ExitCode::SUCCESS
}
