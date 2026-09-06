use super::*;

#[derive(Debug, thiserror::Error)]
pub enum AgentRunError {
    #[error("model request failed: {0}")]
    Provider(#[from] ProviderError),
    #[error("agent execution was cancelled")]
    Cancelled,
    #[error("agent execution timed out after {0} seconds")]
    Timeout(u64),
    #[error("agent runtime failed: {0}")]
    Internal(String),
    #[error("routing reroute approval was declined or expired")]
    RoutingApprovalDeclined,
    /// План 01.1: сборка контекста завершилась отказом. Это терминальный
    /// результат, а не обрыв соединения: model call не выполнялся, а
    /// автоматический retry запрещён на всех уровнях.
    #[error("context assembly refused ({stage}): {required_tokens} tokens required, {available_tokens} available, profile {profile_version}{missing}")]
    BudgetUnavailable {
        stage: String,
        required_tokens: u32,
        available_tokens: u32,
        profile_version: String,
        missing: String,
        context_ledger_hash: String,
    },
}

impl AgentRunError {
    /// Отказ сборки контекста в виде bounded ошибки без сырого prompt и памяти.
    pub fn from_budget_unavailable(
        refusal: &evohime_context_budget::budget::BudgetUnavailable,
    ) -> Self {
        Self::BudgetUnavailable {
            stage: refusal.stage.as_str().to_string(),
            required_tokens: refusal.required_tokens,
            available_tokens: refusal.available_tokens,
            profile_version: refusal.profile_version.clone(),
            missing: refusal
                .missing_part
                .map(|part| format!(", не поместилась часть {}", part.as_str()))
                .unwrap_or_default(),
            context_ledger_hash: refusal.context_ledger_hash.clone(),
        }
    }
}

#[derive(Clone, Default)]
pub struct ApprovalCoordinator {
    pending: Arc<Mutex<HashMap<uuid::Uuid, oneshot::Sender<bool>>>>,
    approved: Arc<Mutex<HashMap<uuid::Uuid, bool>>>,
    resolved: Arc<Mutex<HashSet<uuid::Uuid>>>,
}

#[derive(Clone, Default)]
pub struct RoutingApprovalRegistry {
    pending: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,
}

pub struct RoutingApprovalWait<'a> {
    pub task_id: &'a str,
    pub run_id: &'a str,
    pub trace_id: &'a str,
    pub route_id: &'a str,
    pub timeout_ms: u64,
    pub events: &'a broadcast::Sender<CoreEvent>,
    pub cancellation: &'a CancellationToken,
}

impl RoutingApprovalRegistry {
    pub async fn wait_for_decision(
        &self,
        wait: RoutingApprovalWait<'_>,
    ) -> Result<bool, AgentRunError> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(wait.trace_id.to_owned(), sender);
        let expires_at_ms = task_memory::now_millis().saturating_add(wait.timeout_ms);
        let _ = wait.events.send(CoreEvent::PendingRoutingApproval {
            task_id: wait.task_id.to_owned(),
            trace_id: wait.trace_id.to_owned(),
            run_id: wait.run_id.to_owned(),
            route_id: wait.route_id.to_owned(),
            expires_at_ms,
        });
        let outcome = tokio::select! {
            _ = wait.cancellation.cancelled() => Err(AgentRunError::Cancelled),
            result = tokio::time::timeout(std::time::Duration::from_millis(wait.timeout_ms.max(1)), receiver) =>
                Ok(result.ok().and_then(Result::ok).unwrap_or(false)),
        };
        self.pending.lock().await.remove(wait.trace_id);
        outcome
    }

    pub async fn resolve(&self, trace_id: &str, approve: bool) -> Result<bool, String> {
        let sender = self
            .pending
            .lock()
            .await
            .remove(trace_id)
            .ok_or_else(|| "routing approval is unknown or expired".to_owned())?;
        sender
            .send(approve)
            .map_err(|_| "routing approval is no longer pending".to_owned())?;
        Ok(true)
    }
}

impl ApprovalCoordinator {
    pub async fn register(&self, approval_id: uuid::Uuid) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(approval_id, sender);
        receiver
    }

    pub async fn resolve(&self, approval_id: uuid::Uuid, granted: bool) -> bool {
        if let Some(sender) = self.pending.lock().await.remove(&approval_id) {
            let delivered = sender.send(granted).is_ok();
            self.resolved.lock().await.insert(approval_id);
            return delivered;
        }

        let mut resolved = self.resolved.lock().await;
        if !resolved.insert(approval_id) {
            return false;
        }
        self.approved.lock().await.insert(approval_id, granted);
        true
    }

    pub async fn consume_approved(&self, approval_id: uuid::Uuid) -> bool {
        self.approved
            .lock()
            .await
            .remove(&approval_id)
            .unwrap_or(false)
    }
}

pub trait TaskExecutor: Send + Sync {
    fn execute(
        &self,
        task_id: String,
        prompt: String,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>>;

    fn execute_in_workspace(
        &self,
        task_id: String,
        prompt: String,
        workspace_root: PathBuf,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        let _ = workspace_root;
        self.execute(task_id, prompt, cancellation, events)
    }

    fn execute_in_workspace_with_routing_hint(
        &self,
        task_id: String,
        prompt: String,
        workspace_root: PathBuf,
        preferred_route_hint: Option<String>,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        let _ = preferred_route_hint;
        self.execute_in_workspace(task_id, prompt, workspace_root, cancellation, events)
    }

    fn execute_continuation_gate(
        &self,
        gate: crate::continuation::GateV1,
        task_id: String,
        workspace_root: PathBuf,
        cancellation: CancellationToken,
    ) -> BoxFuture<'static, crate::continuation::GateOutcome> {
        let _ = (gate, task_id, workspace_root, cancellation);
        Box::pin(async {
            crate::continuation::GateOutcome::Unavailable {
                code: "gate_executor_unavailable".into(),
            }
        })
    }

    /// Ambient-извлечение по закрытому эпизоду (04.6).
    ///
    /// Отдельный вход, а не задача: у эпизода нет ни промпта, ни воркспейса,
    /// ни отменяемого хода, и притворяться, будто есть, значило бы сломать
    /// смысл `user_asserted` в policy. Исполнитель без модели ничего не
    /// делает — это не ошибка, а отсутствие извлекателя.
    fn extract_ambient_memory(&self, episode_id: String) -> BoxFuture<'static, ()> {
        let _ = episode_id;
        Box::pin(async {})
    }
}

pub struct ModelAgent {
    gateway: Arc<ModelGateway>,
}

impl ModelAgent {
    pub fn new(gateway: Arc<ModelGateway>) -> Self {
        Self { gateway }
    }

    pub async fn run_once(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        events: &broadcast::Sender<CoreEvent>,
    ) -> Result<String, AgentRunError> {
        self.run_once_with_cancellation(task_id, prompt, events, CancellationToken::new())
            .await
    }

    async fn run_once_with_cancellation(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        events: &broadcast::Sender<CoreEvent>,
        cancellation: CancellationToken,
    ) -> Result<String, AgentRunError> {
        let task_id = task_id.into();
        let messages = [
            ChatMessage::text(ChatRole::System, AGENT_IDENTITY_PROMPT),
            ChatMessage::text(ChatRole::User, prompt),
        ];
        let provider_messages = messages
            .iter()
            .map(|message| {
                let mut message = message.clone();
                message.content = redact_boundary_text("model", &message.content)
                    .map_err(|_| AgentRunError::Internal("sensitive_data_blocked".into()))?;
                Ok(message)
            })
            .collect::<Result<Vec<_>, AgentRunError>>()?;
        let mut stream = self.gateway.stream_chat_with_policy(
            RoutingMode::Balanced,
            &RoutingRequest {
                required_capabilities: vec!["chat".into()],
                max_cost_micros_per_1k_tokens: None,
                max_latency_ms: None,
                required_privacy: PrivacyClass::Internal,
                allow_fallback: true,
                preferred_route: None,
                task_class: None,
                offline: false,
                allow_cloud: true,
                estimated_input_tokens: 0,
                quality_delta: 0.05,
            },
            &provider_messages,
        )?;
        let mut final_message = String::new();
        let mut redactor = sensitive_data_guardrails::StreamingRedactor::new(
            sensitive_data_guardrails::default_policy("stream"),
        );
        while let Some(item) = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
            item = stream.next() => item,
        } {
            match item? {
                evohime_model_gateway::ChatStreamItem::Delta(content) => {
                    let result = redactor
                        .push_chunk(&content)
                        .map_err(|_| AgentRunError::Internal("sensitive_data_blocked".into()))?;
                    if !result.value.is_empty() {
                        final_message.push_str(&result.value);
                        let _ = events.send(CoreEvent::AssistantDelta {
                            task_id: task_id.clone(),
                            content: result.value,
                        });
                    }
                }
                evohime_model_gateway::ChatStreamItem::Thinking(_)
                | evohime_model_gateway::ChatStreamItem::Usage(_) => {}
            }
        }
        let result = redactor
            .finish()
            .map_err(|_| AgentRunError::Internal("sensitive_data_blocked".into()))?;
        if !result.value.is_empty() {
            final_message.push_str(&result.value);
            let _ = events.send(CoreEvent::AssistantDelta {
                task_id: task_id.clone(),
                content: result.value,
            });
        }
        let _ = events.send(CoreEvent::TaskCompleted {
            task_id,
            final_message: final_message.clone(),
        });
        Ok(final_message)
    }
}

impl TaskExecutor for ModelAgent {
    fn execute(
        &self,
        task_id: String,
        prompt: String,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        let agent = Self {
            gateway: Arc::clone(&self.gateway),
        };
        Box::pin(async move {
            agent
                .run_once_with_cancellation(task_id, prompt, &events, cancellation)
                .await
        })
    }
}

/// Model the shell picked for the next request.
///
/// The gateway resolves the model per call, so a selection takes effect on the
/// following request without rebuilding the gateway or restarting Core. An
/// empty value means "whatever the route is configured with".
#[derive(Clone, Default)]
pub struct SelectedModel(Arc<std::sync::RwLock<Option<Arc<str>>>>);

impl SelectedModel {
    pub fn set(&self, model: &str) {
        if let Ok(mut current) = self.0.write() {
            *current = (!model.trim().is_empty()).then(|| Arc::<str>::from(model.trim()));
        }
    }

    pub fn get(&self) -> Option<Arc<str>> {
        self.0.read().ok().and_then(|value| value.clone())
    }
}

/// Executes an explicitly selected coding task through the user's authenticated
/// Codex CLI. The Core owns the workspace boundary and task lifecycle; the CLI
/// is only a bounded child process and never becomes an API provider.
pub(crate) async fn run_codex_cli(
    task_id: String,
    prompt: String,
    workspace_root: PathBuf,
    cancellation: CancellationToken,
    events: broadcast::Sender<CoreEvent>,
) -> Result<String, AgentRunError> {
    const MAX_PROMPT_BYTES: usize = 128 * 1024;
    const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(AgentRunError::Internal(
            "codex_cli: prompt exceeds 128 KiB".into(),
        ));
    }
    let model = std::env::var("CODEX_MODEL").unwrap_or_default();
    if model.trim().is_empty() {
        return Err(AgentRunError::Internal(
            "codex_cli: no selected model".into(),
        ));
    }

    let _ = events.send(CoreEvent::ToolStarted {
        task_id: task_id.clone(),
        tool_name: "codex.execute".into(),
    });
    let _ = events.send(CoreEvent::ToolOutput {
        task_id: task_id.clone(),
        tool_name: "codex.execute".into(),
        output: "Codex CLI запущен, выполняю задачу…".into(),
    });
    let executable = resolve_codex_executable();
    let mut command = tokio::process::Command::new(executable);
    command
        .args([
            "exec",
            "--json",
            "--approve-for-me",
            "--model",
            model.trim(),
        ])
        .arg(&prompt)
        .current_dir(&workspace_root)
        .env_clear();
    for name in [
        "PATH",
        "USERPROFILE",
        "HOME",
        "HOMEDRIVE",
        "HOMEPATH",
        "APPDATA",
        "LOCALAPPDATA",
        "PROGRAMDATA",
        "SystemRoot",
        "WINDIR",
        "ComSpec",
        "TEMP",
        "TMP",
        "CODEX_HOME",
    ] {
        if let Ok(value) = std::env::var(name) {
            command.env(name, value);
        }
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| AgentRunError::Internal(format!("codex_cli unavailable: {error}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AgentRunError::Internal("codex_cli stdout unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AgentRunError::Internal("codex_cli stderr unavailable".into()))?;
    let stdout_task = tokio::spawn(stream_codex_output(
        stdout,
        events.clone(),
        task_id.clone(),
        true,
    ));
    let stderr_task = tokio::spawn(stream_codex_output(
        stderr,
        events.clone(),
        task_id.clone(),
        false,
    ));
    let status = tokio::select! {
        _ = cancellation.cancelled() => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(AgentRunError::Cancelled);
        }
        output = child.wait() => output
            .map_err(|error| AgentRunError::Internal(format!("codex_cli process failed: {error}")))?,
    };
    let mut combined = stdout_task.await.unwrap_or_default();
    combined.extend_from_slice(&stderr_task.await.unwrap_or_default());
    if combined.len() > MAX_OUTPUT_BYTES {
        return Err(AgentRunError::Internal(
            "codex_cli: output limit exceeded".into(),
        ));
    }
    let text = String::from_utf8_lossy(&combined).into_owned();
    if !status.success() {
        return Err(AgentRunError::Internal(format!(
            "codex_cli exited with {}: {}",
            status,
            text.trim()
        )));
    }
    Ok(text)
}

pub(crate) async fn stream_codex_output<R>(
    mut reader: R,
    events: broadcast::Sender<CoreEvent>,
    task_id: String,
    parse_agent_messages: bool,
) -> Vec<u8>
where
    R: tokio::io::AsyncRead + Unpin,
{
    const CHUNK_BYTES: usize = 16 * 1024;
    let mut output = Vec::new();
    let mut line_buffer = String::new();
    let mut chunk = vec![0_u8; CHUNK_BYTES];
    while let Ok(read) = tokio::io::AsyncReadExt::read(&mut reader, &mut chunk).await {
        if read == 0 {
            break;
        }
        output.extend_from_slice(&chunk[..read]);
        let _ = events.send(CoreEvent::ToolOutput {
            task_id: task_id.clone(),
            tool_name: "codex.execute".into(),
            output: String::from_utf8_lossy(&chunk[..read]).into_owned(),
        });
        if parse_agent_messages {
            line_buffer.push_str(&String::from_utf8_lossy(&chunk[..read]));
            emit_codex_events(&mut line_buffer, &events, &task_id);
        }
    }
    if parse_agent_messages {
        emit_codex_events(&mut line_buffer, &events, &task_id);
    }
    output
}

/// Projects Codex CLI's JSONL into the normal Core transcript stream. Raw CLI
/// output remains available in the trace, while the chat receives real command
/// activities and separate assistant messages in their original order.
pub(crate) fn emit_codex_events(
    buffer: &mut String,
    events: &broadcast::Sender<CoreEvent>,
    task_id: &str,
) {
    while let Some(newline) = buffer.find('\n') {
        let line = buffer[..newline].trim();
        emit_codex_event(line, events, task_id);
        buffer.drain(..=newline);
    }
}

pub(crate) fn emit_codex_event(line: &str, events: &broadcast::Sender<CoreEvent>, task_id: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    let event_type = value.get("type").and_then(serde_json::Value::as_str);
    let Some(item) = value.get("item").and_then(serde_json::Value::as_object) else {
        return;
    };
    match (
        event_type,
        item.get("type").and_then(serde_json::Value::as_str),
    ) {
        (Some("item.started"), Some("command_execution")) => {
            if let Some(command) = item.get("command").and_then(serde_json::Value::as_str) {
                let _ = events.send(CoreEvent::ToolStarted {
                    task_id: task_id.to_string(),
                    tool_name: codex_command_tool_name(command),
                });
            }
        }
        (Some("item.completed"), Some("command_execution")) => {
            let command = item
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let output = item
                .get("aggregated_output")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(command);
            let output = if command.is_empty() || output == command {
                output.to_string()
            } else {
                format!("{command}\n{output}")
            };
            let _ = events.send(CoreEvent::ToolOutput {
                task_id: task_id.to_string(),
                tool_name: codex_command_tool_name(command),
                output,
            });
        }
        (Some("item.completed"), Some("agent_message")) => {
            if let Some(text) = item
                .get("text")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                let _ = events.send(CoreEvent::AssistantDelta {
                    task_id: task_id.to_string(),
                    content: text.to_string(),
                });
            }
        }
        _ => {}
    }
}

pub(crate) fn codex_command_tool_name(command: &str) -> String {
    let compact = command.split_whitespace().collect::<Vec<_>>().join(" ");
    let compact = if compact.len() > 240 {
        format!("{}…", &compact[..237])
    } else {
        compact
    };
    format!("shell.execute: {compact}")
}

pub(crate) fn resolve_codex_executable() -> PathBuf {
    if let Ok(value) = std::env::var("CODEX_EXECUTABLE") {
        let path = PathBuf::from(value);
        if path.is_absolute() && path.is_file() {
            return path;
        }
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        let bundled = PathBuf::from(&app_data).join(
            "npm/node_modules/@openai/codex/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe",
        );
        if bundled.is_file() {
            return bundled;
        }
        let path = PathBuf::from(app_data).join("npm/codex.cmd");
        if path.is_file() {
            return path;
        }
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let path = PathBuf::from(local_app_data).join("Programs/OpenAI/Codex/bin/codex.exe");
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from("codex")
}

pub(crate) fn effective_model_name(gateway_model: &str, selected_model: Option<&str>) -> String {
    selected_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or(gateway_model)
        .to_owned()
}

pub(crate) struct CoreReceiptSigner(pub(crate) Arc<ReceiptKeyManager>);

impl ReceiptSigner for CoreReceiptSigner {
    fn key_id(&self) -> Result<String, ReceiptRuntimeError> {
        self.0
            .load_signer()
            .map(|(metadata, _)| metadata.key_id)
            .map_err(|_| ReceiptRuntimeError::SignerUnavailable)
    }

    fn sign_payload_hash(&self, payload_hash: &str) -> Result<String, ReceiptRuntimeError> {
        self.0
            .sign_payload_hash(payload_hash)
            .map(|(_, signature)| signature)
            .map_err(|_| ReceiptRuntimeError::SignerUnavailable)
    }
}

impl evohime_local_storage::model_provenance::ProvenanceBundleSigner for CoreReceiptSigner {
    fn key_id(&self) -> String {
        // Export callers already run after receipt-key startup. The trait is
        // synchronous, so keep a bounded owned fallback for diagnostics.
        match self.0.load_signer().map(|(metadata, _)| metadata.key_id) {
            Ok(key_id) => key_id,
            Err(error) => {
                tracing::warn!(%error, "receipt signer key id unavailable");
                "unknown".into()
            }
        }
    }

    fn sign_manifest_digest(
        &self,
        digest: &[u8],
    ) -> Result<Vec<u8>, evohime_local_storage::model_provenance::ModelProvenanceError> {
        let digest_hex = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let (_, signature) = self.0.sign_payload_hash(&digest_hex).map_err(|error| {
            evohime_local_storage::model_provenance::ModelProvenanceError::CommitFailed(
                error.to_string(),
            )
        })?;
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|error| {
                evohime_local_storage::model_provenance::ModelProvenanceError::CommitFailed(
                    error.to_string(),
                )
            })
    }

    fn public_key_hex(&self) -> Option<String> {
        let transition = self.0.load_history().ok()?.last()?.new_public_key.clone();
        let public = evohime_receipts::key_lifecycle::public_key_bytes(&transition).ok()?;
        Some(public.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    fn key_history_jsonl(
        &self,
    ) -> Result<Vec<u8>, evohime_local_storage::model_provenance::ModelProvenanceError> {
        let mut output = Vec::new();
        for transition in self.0.load_history().map_err(|error| {
            evohime_local_storage::model_provenance::ModelProvenanceError::CommitFailed(
                error.to_string(),
            )
        })? {
            output.extend(serde_json::to_vec(&transition)?);
            output.push(b'\n');
        }
        Ok(output)
    }
}

pub struct ToolAgent {
    gateway: Arc<ModelGateway>,
    tools: Arc<ToolRegistry>,
    max_iterations: usize,
    approvals: ApprovalCoordinator,
    routing_approvals: Option<RoutingApprovalRegistry>,
    journal: Option<EventJournal>,
    selected_model: SelectedModel,
    receipt_keys: Option<Arc<ReceiptKeyManager>>,
    /// Per-workspace rate limit, token budget and circuit breaker for memory
    /// extraction. Shared across turns because the limits are hourly.
    extraction_guard: Arc<Mutex<crate::memory_extraction::ExtractionGuard>>,
    /// Потолок и счётчики ограниченной проактивности (04.7).
    ///
    /// `None` означает, что в этой сборке проактивности нет вовсе: предложение
    /// не создаётся, а не создаётся «без потолка».
    proactivity: Option<crate::ambient::AmbientProactivityRegistry>,
    workflow_registry: Arc<crate::workflow_registry::WorkflowRegistry>,
}

/// Жёсткий предел циклов `model -> tool` для одной задачи.
/// Maximum model-to-tool iterations allowed in one autonomous task.
const DEFAULT_TOOL_ITERATIONS: usize = 32;

struct ProvenancedModelResult {
    result: evohime_model_gateway::PolicyChatResult,
    request_id: Option<String>,
    request_envelope_hash: Option<String>,
    response_id: Option<String>,
}

struct ReceiptApprovalInput<'a> {
    task_id: &'a str,
    tool: &'a str,
    permission: &'a str,
    scope: &'a str,
    input: &'a serde_json::Value,
    preview: &'a evohime_permissions::ApprovalPreview,
    approval_id: Uuid,
}

struct ReceiptClaimInput<'a> {
    task_id: &'a str,
    tool: &'a str,
    permission: &'a str,
    permission_value: evohime_permissions::Permission,
    scope: &'a str,
    input: &'a serde_json::Value,
    preview: &'a evohime_permissions::ApprovalPreview,
    approval_id: Uuid,
}

struct ReceiptRefuseInput<'a> {
    task_id: &'a str,
    tool: &'a str,
    permission: &'a str,
    scope: &'a str,
    input: &'a serde_json::Value,
    preview: &'a evohime_permissions::ApprovalPreview,
    approval_id: Uuid,
    code: &'a str,
}

struct AssembleModelContextInput<'a> {
    runtime: &'a mut context_budget::ContextRuntime,
    task_id: &'a str,
    session_id: &'a str,
    iteration: usize,
    messages: &'a [ChatMessage],
    specs: &'a [ToolSpec],
    selected_model: Option<&'a str>,
}

struct CallModelInput<'a> {
    task_id: &'a str,
    messages: &'a [ChatMessage],
    specs: &'a [ToolSpec],
    source_refs: &'a [evohime_model_provenance::SourceRef],
    workspace_root: &'a std::path::Path,
    ledger: &'a evohime_context_budget::ledger::ContextLedgerEntry,
    config: &'a ProviderResilienceConfig,
    preferred_route: Option<&'a str>,
    task_class: Option<&'a str>,
    estimated_input_tokens: u32,
}

pub(crate) struct ModelRequestEnvelopeInput<'a> {
    logical_request_id: &'a str,
    request_id: String,
    attempt: u32,
    parent_request_id: Option<String>,
    previous_request_hash: Option<String>,
    ledger: &'a evohime_context_budget::ledger::ContextLedgerEntry,
    messages: &'a [ChatMessage],
    specs: &'a [ToolSpec],
    source_refs: &'a [evohime_model_provenance::SourceRef],
    route_snapshot_hash: &'a str,
}

pub(crate) fn model_request_envelope(
    input: ModelRequestEnvelopeInput<'_>,
) -> Result<evohime_model_provenance::ModelRequestEnvelopeV1, String> {
    let system_prompt = input
        .messages
        .iter()
        .find(|message| message.role == ChatRole::System)
        .map(|message| message.content.clone())
        .unwrap_or_default();
    let messages = input
        .messages
        .iter()
        .filter(|message| message.role != ChatRole::System)
        .map(|message| evohime_model_provenance::ModelMessage {
            role: message.role.as_str().to_string(),
            content: message.content.clone(),
        })
        .collect::<Vec<_>>();
    let tools = input
        .specs
        .iter()
        .map(|spec| evohime_model_provenance::ToolSchema {
            name: spec.function.name.clone(),
            description: spec.function.description.clone(),
            input_schema: spec.function.parameters.clone(),
        })
        .collect::<Vec<_>>();
    let selected_ids = input
        .ledger
        .selected_items
        .iter()
        .map(|item| item.id.clone());
    let dropped = input
        .ledger
        .dropped_items
        .iter()
        .map(|item| (item.id.clone(), item.drop_reason.as_str().to_string()));
    let mut summaries = input
        .ledger
        .compression
        .iter()
        .map(|record| (record.summary_id.clone(), Vec::new()))
        .collect::<Vec<_>>();
    if !input.source_refs.is_empty() {
        summaries.push(("workspace:evidence".into(), input.source_refs.to_vec()));
    }
    let projection = evohime_model_provenance::ContextProjection::from_ledger_parts(
        input.ledger.id.clone(),
        input.ledger.context_ledger_hash.clone(),
        selected_ids,
        summaries,
        dropped,
    )
    .map_err(|error| error.to_string())?;
    Ok(evohime_model_provenance::ModelRequestEnvelopeV1 {
        version: evohime_model_provenance::CONTRACT_VERSION,
        request_id: input.request_id,
        logical_request_id: input.logical_request_id.to_string(),
        attempt: input.attempt,
        parent_request_id: input.parent_request_id,
        ledger_id: input.ledger.id.clone(),
        request_kind: evohime_model_provenance::RequestKind::Agent,
        provider: input.ledger.provider.clone(),
        model: input.ledger.model.clone(),
        route_snapshot_hash: input.route_snapshot_hash.to_owned(),
        policy_snapshot_hash: input.route_snapshot_hash.to_owned(),
        route_policy_hash_shared: true,
        system_prompt,
        messages,
        tools,
        model_parameters: evohime_model_provenance::ModelParameters {
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            reasoning_mode: None,
            provider_options: serde_json::Map::new(),
        },
        context_projection: projection,
        previous_request_hash: input.previous_request_hash,
    })
}

#[path = "core_agent_context.rs"]
mod context;
#[path = "core_agent_execution.rs"]
mod execution;
#[path = "core_agent_memory.rs"]
mod memory;
#[path = "core_agent_receipts.rs"]
mod receipts;
#[path = "core_agent_tool_setup.rs"]
mod tool_setup;

impl TaskExecutor for ToolAgent {
    fn execute(
        &self,
        task_id: String,
        prompt: String,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        self.execute_in_workspace(
            task_id,
            prompt,
            std::env::current_dir().unwrap_or_default(),
            cancellation,
            events,
        )
    }

    fn execute_continuation_gate(
        &self,
        gate: crate::continuation::GateV1,
        task_id: String,
        workspace_root: PathBuf,
        cancellation: CancellationToken,
    ) -> BoxFuture<'static, crate::continuation::GateOutcome> {
        let tools = self.tools.clone();
        Box::pin(async move {
            if !matches!(gate.kind, crate::continuation::GateKind::Tool) {
                return crate::continuation::GateOutcome::Unavailable {
                    code: "gate_kind_not_supported_by_task_executor".into(),
                };
            }
            let input = match gate.args {
                crate::continuation::GateArgs::Empty => serde_json::json!({}),
                crate::continuation::GateArgs::Named { values } => {
                    let mut object = serde_json::Map::new();
                    for value in values {
                        object.insert(value.key, serde_json::Value::String(value.value));
                    }
                    serde_json::Value::Object(object)
                }
            };
            let context = ToolContext {
                workspace_root,
                task_id: match uuid::Uuid::parse_str(&task_id) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(%error, task_id = %task_id, "non-UUID continuation task id; generated runtime id");
                        uuid::Uuid::new_v4()
                    }
                },
                session_id: None,
                progress_tx: None,
            };
            match tools
                .execute_with_cancellation(&context, &gate.capability_ref, input, cancellation)
                .await
            {
                Ok(_) => crate::continuation::GateOutcome::Passed {
                    evidence_ref: format!("gate:{}", gate.id),
                },
                Err(evohime_tool_runtime::ToolError::NeedsApproval(details)) => {
                    crate::continuation::GateOutcome::PendingApproval {
                        approval_id: details.approval_id.to_string(),
                    }
                }
                Err(evohime_tool_runtime::ToolError::TimedOut(_))
                | Err(evohime_tool_runtime::ToolError::Execution(_)) => {
                    crate::continuation::GateOutcome::Failed {
                        retryable: true,
                        code: "gate_execution_failed".into(),
                    }
                }
                Err(error) => crate::continuation::GateOutcome::Failed {
                    retryable: false,
                    code: error.to_string(),
                },
            }
        })
    }

    fn execute_in_workspace(
        &self,
        task_id: String,
        prompt: String,
        workspace_root: PathBuf,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        let agent = Self {
            gateway: Arc::clone(&self.gateway),
            tools: Arc::clone(&self.tools),
            max_iterations: self.max_iterations,
            approvals: self.approvals.clone(),
            routing_approvals: self.routing_approvals.clone(),
            journal: self.journal.clone(),
            selected_model: self.selected_model.clone(),
            receipt_keys: self.receipt_keys.clone(),
            // Shared, not cloned: the hourly candidate/token limits and the
            // circuit breaker have to hold across concurrent tasks.
            extraction_guard: Arc::clone(&self.extraction_guard),
            proactivity: self.proactivity.clone(),
            workflow_registry: Arc::clone(&self.workflow_registry),
        };
        Box::pin(async move {
            agent
                .run_once_with_cancellation(
                    task_id,
                    prompt,
                    workspace_root,
                    &events,
                    cancellation,
                    None,
                )
                .await
        })
    }

    fn execute_in_workspace_with_routing_hint(
        &self,
        task_id: String,
        prompt: String,
        workspace_root: PathBuf,
        preferred_route_hint: Option<String>,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        if preferred_route_hint.as_deref() == Some("codex_cli") {
            return Box::pin(run_codex_cli(
                task_id,
                prompt,
                workspace_root,
                cancellation,
                events,
            ));
        }
        let agent = Self {
            gateway: Arc::clone(&self.gateway),
            tools: Arc::clone(&self.tools),
            max_iterations: self.max_iterations,
            approvals: self.approvals.clone(),
            routing_approvals: self.routing_approvals.clone(),
            journal: self.journal.clone(),
            selected_model: self.selected_model.clone(),
            receipt_keys: self.receipt_keys.clone(),
            extraction_guard: Arc::clone(&self.extraction_guard),
            proactivity: self.proactivity.clone(),
            workflow_registry: Arc::clone(&self.workflow_registry),
        };
        Box::pin(async move {
            agent
                .run_once_with_cancellation(
                    task_id,
                    prompt,
                    workspace_root,
                    &events,
                    cancellation,
                    preferred_route_hint,
                )
                .await
        })
    }

    fn extract_ambient_memory(&self, episode_id: String) -> BoxFuture<'static, ()> {
        let agent = Self {
            gateway: Arc::clone(&self.gateway),
            tools: Arc::clone(&self.tools),
            max_iterations: self.max_iterations,
            approvals: self.approvals.clone(),
            routing_approvals: self.routing_approvals.clone(),
            journal: self.journal.clone(),
            selected_model: self.selected_model.clone(),
            receipt_keys: self.receipt_keys.clone(),
            // Shared, not cloned: the ambient budgets and the malformed
            // breaker are hourly and have to hold across episodes.
            extraction_guard: Arc::clone(&self.extraction_guard),
            proactivity: self.proactivity.clone(),
            workflow_registry: Arc::clone(&self.workflow_registry),
        };
        Box::pin(async move {
            agent.run_ambient_memory_extraction(&episode_id).await;
        })
    }
}
