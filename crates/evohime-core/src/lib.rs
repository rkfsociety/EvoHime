pub struct CoreVersion;

pub const AGENT_IDENTITY_PROMPT: &str =
    "Ты — Ева, AI-агент приложения EvoHime. Ева — короткое имя EvoHime; понимай обращения к тебе «Ева» и «EvoHime» как к одному агенту.";

fn build_agent_system_prompt(workspace: &std::path::Path, tool_names: &[String]) -> String {
    format!(
        "{AGENT_IDENTITY_PROMPT}\n\n\
Ты работаешь автономно внутри уже выбранного рабочего пространства.\n\
Текущий workspace: {}\n\
Этот путь доступен инструментам как их корневая папка; не проси пользователя сообщать его повторно.\n\n\
Правила выполнения:\n\
- Выполняй задачу самостоятельно и используй инструменты, когда они нужны для фактической проверки.\n\
- Если пользователь просит изучить, проверить, найти или объяснить проект, сначала вызови filesystem.list с path точкой (.).\n\
- Затем прочитай подходящие manifest-файлы и документацию (например Cargo.toml, *.csproj, package.json, README и архитектурные документы), а для поиска по коду используй filesystem.search.\n\
- Не проси пользователя прислать структуру проекта, путь или команды, если workspace уже указан.\n\
- Не утверждай, что изучила файл или выполнила действие, пока соответствующий инструмент не вернул результат.\n\
- Для чтения используй безопасные read-only инструменты. Перед изменениями и опасными действиями учитывай approval.\n\
- После исследования дай отчёт: что обнаружено, какие файлы проверены, какие проблемы найдены и что предлагается сделать дальше.\n\n\
Доступные инструменты в этой сессии:\n{}",
        workspace.display(),
        tool_names
            .iter()
            .map(|name| format!("- {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn tool_parameters(name: &str) -> serde_json::Value {
    match name {
        "filesystem.list" => serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Workspace-relative directory, usually ." } }
        }),
        "filesystem.read" => serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Workspace-relative UTF-8 file path" } },
            "required": ["path"]
        }),
        "filesystem.search" => serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Text or pattern to search for" },
                "path": { "type": "string", "description": "Optional workspace-relative directory" },
                "glob": { "type": "string", "description": "Optional file glob" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
            },
            "required": ["query"]
        }),
        "git.diff" => serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Optional workspace-relative path" } }
        }),
        _ => serde_json::json!({ "type": "object", "additionalProperties": true }),
    }
}

mod ipc_bridge;
pub use ipc_bridge::{IpcBridge, IpcBridgeError, ModelConfigSnapshot};
mod logging;
pub use logging::StructuredLogger;

#[cfg(windows)]
pub async fn run_windows_pipe(
    pipe_name: &str,
    bridge: IpcBridge,
    logger: std::sync::Arc<StructuredLogger>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::split;
    use tokio::net::windows::named_pipe::ServerOptions;

    loop {
        let server = ServerOptions::new().create(pipe_name)?;
        server.connect().await?;
        let (mut reader, mut writer) = split(server);
        loop {
            if let Err(error) = bridge.process_once(&mut reader, &mut writer).await {
                let _ = logger.write(
                    "warn",
                    "ipc.connection_closed",
                    serde_json::json!({"error": error.to_string()}),
                );
                break;
            }
        }
    }
}

impl CoreVersion {
    pub const fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use evohime_local_storage::{EventRecord, LocalDatabase, StorageError};
use evohime_model_gateway::{
    providers::{ChatMessage, ChatRole, ProviderError},
    ModelGateway, ToolSpec,
};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCommand {
    StartTask {
        task_id: String,
        prompt: String,
        workspace_root: Option<PathBuf>,
    },
    StopTask {
        task_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CoreEvent {
    ModelContext {
        task_id: String,
        workspace_path: String,
        model: String,
        system_prompt: String,
        user_prompt: String,
        tools: Vec<String>,
        estimated_tokens: usize,
    },
    TaskStarted {
        task_id: String,
        prompt: String,
    },
    AssistantDelta {
        task_id: String,
        content: String,
    },
    ToolStarted {
        task_id: String,
        tool_name: String,
    },
    ToolOutput {
        task_id: String,
        tool_name: String,
        output: String,
    },
    ApprovalRequired {
        task_id: String,
        approval_id: String,
        tool_name: String,
        permission: String,
        scope: String,
        input: serde_json::Value,
    },
    TaskCompleted {
        task_id: String,
        final_message: String,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },
    TaskStopped {
        task_id: String,
    },
}

#[derive(Clone)]
pub struct EventJournal {
    database: Arc<Mutex<LocalDatabase>>,
}

impl EventJournal {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        Ok(Self {
            database: Arc::new(Mutex::new(LocalDatabase::open(path)?)),
        })
    }

    pub async fn record(&self, event: &CoreEvent) -> Result<i64, StorageError> {
        let task_id = match event {
            CoreEvent::ModelContext { task_id, .. }
            | CoreEvent::TaskStarted { task_id, .. }
            | CoreEvent::AssistantDelta { task_id, .. }
            | CoreEvent::ToolStarted { task_id, .. }
            | CoreEvent::ToolOutput { task_id, .. }
            | CoreEvent::ApprovalRequired { task_id, .. }
            | CoreEvent::TaskCompleted { task_id, .. }
            | CoreEvent::TaskFailed { task_id, .. }
            | CoreEvent::TaskStopped { task_id } => task_id,
        };
        let event_type = match event {
            CoreEvent::ModelContext { .. } => "model.context",
            CoreEvent::TaskStarted { .. } => "task.started",
            CoreEvent::AssistantDelta { .. } => "agent.message.delta",
            CoreEvent::ToolStarted { .. } => "tool.started",
            CoreEvent::ToolOutput { .. } => "tool.output",
            CoreEvent::ApprovalRequired { .. } => "approval.required",
            CoreEvent::TaskCompleted { .. } => "task.completed",
            CoreEvent::TaskFailed { .. } => "task.failed",
            CoreEvent::TaskStopped { .. } => "task.stopped",
        };
        let payload = serde_json::to_vec(event).expect("core events serialize");
        let database = self.database.lock().await;
        database.append_event(task_id, event_type, &payload)
    }

    pub async fn replay(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let database = self.database.lock().await;
        database.read_events_after(after_sequence, limit)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunError {
    #[error("model request failed: {0}")]
    Provider(#[from] ProviderError),
    #[error("agent execution was cancelled")]
    Cancelled,
    #[error("agent execution timed out after {0} seconds")]
    Timeout(u64),
}

#[derive(Clone, Default)]
pub struct ApprovalCoordinator {
    pending: Arc<Mutex<HashMap<uuid::Uuid, oneshot::Sender<bool>>>>,
}

impl ApprovalCoordinator {
    pub async fn register(&self, approval_id: uuid::Uuid) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(approval_id, sender);
        receiver
    }

    pub async fn resolve(&self, approval_id: uuid::Uuid, granted: bool) -> bool {
        self.pending
            .lock()
            .await
            .remove(&approval_id)
            .map(|sender| sender.send(granted).is_ok())
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
        let mut stream = self.gateway.stream_chat(&messages);
        let mut final_message = String::new();
        while let Some(item) = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
            item = stream.next() => item,
        } {
            match item? {
                evohime_model_gateway::ChatStreamItem::Delta(content) => {
                    final_message.push_str(&content);
                    let _ = events.send(CoreEvent::AssistantDelta {
                        task_id: task_id.clone(),
                        content,
                    });
                }
                evohime_model_gateway::ChatStreamItem::Thinking(_)
                | evohime_model_gateway::ChatStreamItem::Usage(_) => {}
            }
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

pub struct ToolAgent {
    gateway: Arc<ModelGateway>,
    tools: Arc<ToolRegistry>,
    max_iterations: usize,
    approvals: ApprovalCoordinator,
}

impl ToolAgent {
    pub fn new(gateway: Arc<ModelGateway>, tools: Arc<ToolRegistry>) -> Self {
        Self::new_with_approvals(gateway, tools, ApprovalCoordinator::default())
    }

    pub fn new_with_approvals(
        gateway: Arc<ModelGateway>,
        tools: Arc<ToolRegistry>,
        approvals: ApprovalCoordinator,
    ) -> Self {
        Self {
            gateway,
            tools,
            max_iterations: 8,
            approvals,
        }
    }

    pub async fn run_once(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        workspace_root: impl Into<std::path::PathBuf>,
        events: &broadcast::Sender<CoreEvent>,
    ) -> Result<String, AgentRunError> {
        self.run_once_with_cancellation(
            task_id,
            prompt,
            workspace_root,
            events,
            CancellationToken::new(),
        )
        .await
    }

    async fn run_once_with_cancellation(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        workspace_root: impl Into<std::path::PathBuf>,
        events: &broadcast::Sender<CoreEvent>,
        cancellation: CancellationToken,
    ) -> Result<String, AgentRunError> {
        let task_id = task_id.into();
        let task_uuid = uuid::Uuid::parse_str(&task_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
        let context = ToolContext {
            workspace_root: workspace_root.into(),
            task_id: task_uuid,
            session_id: None,
            progress_tx: None,
        };
        let specs = self
            .tools
            .list()
            .into_iter()
            .map(|tool| {
                let name = tool.name.to_string();
                ToolSpec::function(
                    name,
                    tool.description,
                    tool_parameters(tool.name),
                )
            })
            .collect::<Vec<_>>();
        let tool_names = specs
            .iter()
            .map(|spec| spec.function.name.clone())
            .collect::<Vec<_>>();
        let system_prompt = build_agent_system_prompt(&context.workspace_root, &tool_names);
        let mut messages = vec![
            ChatMessage::text(ChatRole::System, system_prompt.clone()),
            ChatMessage::text(ChatRole::User, prompt),
        ];

        let user_prompt = messages[1].content.clone();
        let context_text = format!("{system_prompt}\n{user_prompt}\n{}", tool_names.join("\n"));
        let _ = events.send(CoreEvent::ModelContext {
            task_id: task_id.clone(),
            workspace_path: context.workspace_root.display().to_string(),
            model: self.gateway.model_name().to_string(),
            system_prompt,
            user_prompt,
            tools: tool_names,
            estimated_tokens: context_text.chars().count().div_ceil(4),
        });

        for _ in 0..self.max_iterations {
            let result = tokio::select! {
                _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
                result = self.gateway.chat_with_tools_for_route("default", None, &messages, &specs) => result?,
            };
            if result.tool_calls.is_empty() {
                let _ = events.send(CoreEvent::TaskCompleted {
                    task_id,
                    final_message: result.content.clone(),
                });
                return Ok(result.content);
            }

            messages.push(ChatMessage::assistant_tool_calls(
                result.content,
                result.tool_calls.clone(),
            ));
            for call in result.tool_calls {
                let _ = events.send(CoreEvent::ToolStarted {
                    task_id: task_id.clone(),
                    tool_name: call.name.clone(),
                });
                let input = serde_json::from_str(&call.arguments)
                    .unwrap_or_else(|_| serde_json::Value::Null);
                let output = match tokio::select! {
                    _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
                    result = self.tools.execute_with_cancellation(&context, &call.name, input, cancellation.clone()) => result,
                } {
                    Ok(result) => result.output,
                    Err(evohime_tool_runtime::ToolError::NeedsApproval {
                        tool,
                        permission,
                        scope,
                        approval_id,
                        input,
                    }) => {
                        let receiver = self.approvals.register(approval_id).await;
                        let _ = events.send(CoreEvent::ApprovalRequired {
                            task_id: task_id.clone(),
                            approval_id: approval_id.to_string(),
                            tool_name: tool.clone(),
                            permission: format!("{permission:?}"),
                            scope,
                            input: input.clone(),
                        });
                        let granted = tokio::select! {
                            _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
                            result = receiver => result.unwrap_or(false),
                        };
                        if !granted {
                            "approval denied".to_string()
                        } else {
                            match self.tools.execute_after_approval(
                                &context,
                                &tool,
                                input,
                                approval_id,
                                cancellation.clone(),
                            ).await {
                                Ok(result) => result.output,
                                Err(error) => error.to_string(),
                            }
                        }
                    }
                    Err(error) => error.to_string(),
                };
                let _ = events.send(CoreEvent::ToolOutput {
                    task_id: task_id.clone(),
                    tool_name: call.name,
                    output: output.clone(),
                });
                messages.push(ChatMessage::tool_observation(call.id, output));
            }
        }

        let message = "agent exceeded the tool iteration limit".to_string();
        let _ = events.send(CoreEvent::TaskFailed {
            task_id,
            error: message.clone(),
        });
        Ok(message)
    }
}

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
        };
        Box::pin(async move {
            agent
                .run_once_with_cancellation(task_id, prompt, workspace_root, &events, cancellation)
                .await
        })
    }
}

#[derive(Clone)]
pub struct TaskCoordinator {
    commands: mpsc::Sender<CoreCommand>,
}

struct CoordinatorState {
    tasks: HashMap<String, CancellationToken>,
    events: broadcast::Sender<CoreEvent>,
    executor: Option<Arc<dyn TaskExecutor>>,
}

impl TaskCoordinator {
    pub fn new(buffer: usize) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, None, None)
    }

    pub fn new_with_executor(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, executor, None)
    }

    pub fn new_with_journal(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
        journal: EventJournal,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, executor, Some(journal))
    }

    fn build(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
        journal: Option<EventJournal>,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        let (commands, mut command_rx) = mpsc::channel(buffer.max(1));
        let (events, event_rx) = broadcast::channel(buffer.max(1));
        let state = Arc::new(Mutex::new(CoordinatorState {
            tasks: HashMap::new(),
            events: events.clone(),
            executor,
        }));
        if let Some(journal) = journal {
            let mut journal_receiver = events.subscribe();
            tokio::spawn(async move {
                while let Ok(event) = journal_receiver.recv().await {
                    let _ = journal.record(&event).await;
                }
            });
        }
        let worker_state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                Self::handle_command(Arc::clone(&worker_state), command).await;
            }
        });
        (Self { commands }, event_rx)
    }

    pub async fn dispatch(
        &self,
        command: CoreCommand,
    ) -> Result<(), mpsc::error::SendError<CoreCommand>> {
        self.commands.send(command).await
    }

    async fn handle_command(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
        match command {
            CoreCommand::StartTask {
                task_id,
                prompt,
                workspace_root,
            } => {
                let cancellation = CancellationToken::new();
                let mut state_guard = state.lock().await;
                if state_guard
                    .tasks
                    .insert(task_id.clone(), cancellation.clone())
                    .is_some()
                {
                    return;
                }
                let _ = state_guard.events.send(CoreEvent::TaskStarted {
                    task_id: task_id.clone(),
                    prompt: prompt.clone(),
                });
                let events = state_guard.events.clone();
                let executor = state_guard.executor.clone();
                drop(state_guard);
                tokio::spawn(async move {
                    let result = match executor {
                        Some(executor) => match timeout(
                            Duration::from_secs(60),
                            executor.execute_in_workspace(
                                task_id.clone(),
                                prompt,
                                workspace_root.unwrap_or_else(|| {
                                    std::env::current_dir().unwrap_or_default()
                                }),
                                cancellation.clone(),
                                events.clone(),
                            ),
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(_) => Err(AgentRunError::Timeout(60)),
                        },
                        None => {
                            cancellation.cancelled().await;
                            Err(AgentRunError::Cancelled)
                        }
                    };
                    let mut state_guard = state.lock().await;
                    state_guard.tasks.remove(&task_id);
                    match result {
                        Err(AgentRunError::Cancelled) => {
                            let _ = state_guard.events.send(CoreEvent::TaskStopped { task_id });
                        }
                        Err(error) => {
                            let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: error.to_string(),
                            });
                        }
                        Ok(_) => {}
                    }
                });
            }
            CoreCommand::StopTask { task_id } => {
                let mut state_guard = state.lock().await;
                if let Some(cancellation) = state_guard.tasks.remove(&task_id) {
                    cancellation.cancel();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentRunError, CoreCommand, CoreEvent, CoreVersion, EventJournal, ModelAgent,
        TaskCoordinator, TaskExecutor, ToolAgent,
    };
    use evohime_model_gateway::{
        providers::mock::MockProvider, ChatResult, ModelGateway, NativeToolCall,
    };
    use evohime_tool_runtime::ToolRegistry;
    use futures_util::future::BoxFuture;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    struct NeverExecutor;

    #[tokio::test]
    async fn approval_coordinator_resolves_pending_request_once() {
        let coordinator = super::ApprovalCoordinator::default();
        let approval_id = uuid::Uuid::new_v4();
        let receiver = coordinator.register(approval_id).await;

        assert!(coordinator.resolve(approval_id, true).await);
        assert!(!coordinator.resolve(approval_id, false).await);
        assert!(receiver.await.expect("approval response"));
    }

    #[test]
    fn agent_identity_includes_short_name() {
        assert!(super::AGENT_IDENTITY_PROMPT.contains("Ева"));
        assert!(super::AGENT_IDENTITY_PROMPT.contains("EvoHime"));
    }

    #[test]
    fn agent_system_prompt_explains_workspace_research_flow() {
        let prompt = super::build_agent_system_prompt(
            std::path::Path::new(r"C:\Projects\demo"),
            &["filesystem.list".into(), "filesystem.read".into()],
        );
        assert!(prompt.contains(r"C:\Projects\demo"));
        assert!(prompt.contains("filesystem.list"));
        assert!(prompt.contains("Не проси пользователя прислать структуру"));
    }

    impl TaskExecutor for NeverExecutor {
        fn execute(
            &self,
            _task_id: String,
            _prompt: String,
            cancellation: CancellationToken,
            _events: tokio::sync::broadcast::Sender<CoreEvent>,
        ) -> BoxFuture<'static, Result<String, AgentRunError>> {
            Box::pin(async move {
                cancellation.cancelled().await;
                Err(AgentRunError::Cancelled)
            })
        }
    }

    #[test]
    fn core_exposes_version() {
        assert!(!CoreVersion::current().is_empty());
    }

    #[tokio::test]
    async fn starts_and_stops_a_task_without_blocking_the_core() {
        let (coordinator, mut events) = TaskCoordinator::new(8);
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-1".into(),
                prompt: "hello".into(),
                workspace_root: None,
            })
            .await
            .expect("start dispatches");
        assert_eq!(
            events.recv().await.expect("started event"),
            CoreEvent::TaskStarted {
                task_id: "task-1".into(),
                prompt: "hello".into()
            }
        );
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-1".into(),
            })
            .await
            .expect("stop dispatches");
        assert_eq!(
            events.recv().await.expect("stopped event"),
            CoreEvent::TaskStopped {
                task_id: "task-1".into()
            }
        );
    }

    #[tokio::test]
    async fn stop_cancels_an_active_executor() {
        let (coordinator, mut events) =
            TaskCoordinator::new_with_executor(8, Some(Arc::new(NeverExecutor)));
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-cancel".into(),
                prompt: "wait".into(),
                workspace_root: None,
            })
            .await
            .expect("start dispatches");
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskStarted { .. })
        ));
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-cancel".into(),
            })
            .await
            .expect("stop dispatches");
        assert_eq!(
            events.recv().await.expect("stopped event"),
            CoreEvent::TaskStopped {
                task_id: "task-cancel".into()
            }
        );
    }

    #[tokio::test]
    async fn streams_a_model_response_as_core_events() {
        let gateway = ModelGateway::from_provider(Arc::new(MockProvider::new(
            "mock",
            vec!["hello ".into(), "from core".into()],
        )));
        let agent = ModelAgent::new(Arc::new(gateway));
        let (events, mut receiver) = tokio::sync::broadcast::channel(8);
        let result = agent
            .run_once("task-2", "say hello", &events)
            .await
            .expect("mock model succeeds");
        assert_eq!(result, "hello from core");
        assert_eq!(
            receiver.recv().await.expect("first delta"),
            CoreEvent::AssistantDelta {
                task_id: "task-2".into(),
                content: "hello ".into()
            }
        );
        assert_eq!(
            receiver.recv().await.expect("second delta"),
            CoreEvent::AssistantDelta {
                task_id: "task-2".into(),
                content: "from core".into()
            }
        );
        assert_eq!(
            receiver.recv().await.expect("completed event"),
            CoreEvent::TaskCompleted {
                task_id: "task-2".into(),
                final_message: "hello from core".into()
            }
        );
    }

    #[tokio::test]
    async fn executes_a_safe_filesystem_tool_and_returns_to_the_model() {
        let workspace =
            std::env::temp_dir().join(format!("evohime-core-tool-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&workspace);
        std::fs::write(workspace.join("needle.txt"), "needle in a file").expect("fixture writes");
        let provider = MockProvider::with_tool_call_sequence(
            "mock",
            vec![
                ChatResult {
                    content: String::new(),
                    thinking: None,
                    tool_calls: vec![NativeToolCall {
                        id: "call-1".into(),
                        name: "filesystem.search".into(),
                        arguments: r#"{"query":"needle"}"#.into(),
                    }],
                    usage: None,
                },
                ChatResult {
                    content: "found it".into(),
                    ..ChatResult::default()
                },
            ],
        );
        let agent = ToolAgent::new(
            Arc::new(ModelGateway::from_provider(Arc::new(provider))),
            Arc::new(ToolRegistry::bootstrap()),
        );
        let (events, mut receiver) = tokio::sync::broadcast::channel(16);
        let result = agent
            .run_once("task-tools", "find needle", &workspace, &events)
            .await
            .expect("tool loop succeeds");
        assert_eq!(result, "found it");
        assert!(matches!(
            receiver.recv().await,
            Ok(CoreEvent::ModelContext { workspace_path, .. }) if workspace_path == workspace.display().to_string()
        ));
        assert!(matches!(
            receiver.recv().await,
            Ok(CoreEvent::ToolStarted { .. })
        ));
        assert!(
            matches!(receiver.recv().await, Ok(CoreEvent::ToolOutput { output, .. }) if output.contains("needle"))
        );
        assert!(
            matches!(receiver.recv().await, Ok(CoreEvent::TaskCompleted { final_message, .. }) if final_message == "found it")
        );
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[tokio::test]
    async fn journals_core_events_and_replays_after_a_sequence() {
        let path =
            std::env::temp_dir().join(format!("evohime-core-journal-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let first = journal
            .record(&CoreEvent::TaskStarted {
                task_id: "task-journal".into(),
                prompt: "persist me".into(),
            })
            .await
            .expect("event records");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-journal".into(),
                final_message: "done".into(),
            })
            .await
            .expect("second event records");
        let replay = journal.replay(first, 10).await.expect("events replay");
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].event_type, "task.completed");
        assert_eq!(replay[0].task_id, "task-journal");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn coordinator_journal_captures_lifecycle_events() {
        let path = std::env::temp_dir().join(format!(
            "evohime-core-coordinator-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, mut events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-persisted".into(),
                prompt: "persist lifecycle".into(),
                workspace_root: None,
            })
            .await
            .expect("start dispatches");
        let _ = events.recv().await.expect("started event");
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-persisted".into(),
            })
            .await
            .expect("stop dispatches");
        let _ = events.recv().await.expect("stopped event");
        let mut replay = Vec::new();
        for _ in 0..20 {
            replay = journal.replay(0, 10).await.expect("replay works");
            if replay.len() >= 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].event_type, "task.started");
        assert_eq!(replay[1].event_type, "task.stopped");
        let _ = std::fs::remove_file(path);
    }
}
