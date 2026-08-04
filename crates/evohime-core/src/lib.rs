pub struct CoreVersion;

impl CoreVersion {
    pub const fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

use std::{collections::HashMap, sync::Arc};

use evohime_model_gateway::{
    providers::{ChatMessage, ChatRole, ProviderError},
    ModelGateway, ToolSpec,
};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::StreamExt;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCommand {
    StartTask { task_id: String, prompt: String },
    StopTask { task_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
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

#[derive(Debug, thiserror::Error)]
pub enum AgentRunError {
    #[error("model request failed: {0}")]
    Provider(#[from] ProviderError),
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
        let task_id = task_id.into();
        let messages = [ChatMessage::text(ChatRole::User, prompt)];
        let mut stream = self.gateway.stream_chat(&messages);
        let mut final_message = String::new();
        while let Some(item) = stream.next().await {
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

pub struct ToolAgent {
    gateway: Arc<ModelGateway>,
    tools: Arc<ToolRegistry>,
    max_iterations: usize,
}

impl ToolAgent {
    pub fn new(gateway: Arc<ModelGateway>, tools: Arc<ToolRegistry>) -> Self {
        Self {
            gateway,
            tools,
            max_iterations: 8,
        }
    }

    pub async fn run_once(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        workspace_root: impl Into<std::path::PathBuf>,
        events: &broadcast::Sender<CoreEvent>,
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
                ToolSpec::function(
                    tool.name,
                    tool.description,
                    serde_json::json!({"type": "object", "additionalProperties": true}),
                )
            })
            .collect::<Vec<_>>();
        let mut messages = vec![ChatMessage::text(ChatRole::User, prompt)];

        for _ in 0..self.max_iterations {
            let result = self
                .gateway
                .chat_with_tools_for_route("default", None, &messages, &specs)
                .await?;
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
                let output = match self.tools.execute(&context, &call.name, input).await {
                    Ok(result) => result.output,
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

#[derive(Clone)]
pub struct TaskCoordinator {
    commands: mpsc::Sender<CoreCommand>,
}

struct CoordinatorState {
    tasks: HashMap<String, oneshot::Sender<()>>,
    events: broadcast::Sender<CoreEvent>,
}

impl TaskCoordinator {
    pub fn new(buffer: usize) -> (Self, broadcast::Receiver<CoreEvent>) {
        let (commands, mut command_rx) = mpsc::channel(buffer.max(1));
        let (events, event_rx) = broadcast::channel(buffer.max(1));
        let state = Arc::new(Mutex::new(CoordinatorState {
            tasks: HashMap::new(),
            events,
        }));
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
            CoreCommand::StartTask { task_id, prompt } => {
                let (stop_tx, mut stop_rx) = oneshot::channel();
                let mut state_guard = state.lock().await;
                if state_guard.tasks.insert(task_id.clone(), stop_tx).is_some() {
                    return;
                }
                let _ = state_guard.events.send(CoreEvent::TaskStarted {
                    task_id: task_id.clone(),
                    prompt,
                });
                drop(state_guard);
                tokio::spawn(async move {
                    let _ = (&mut stop_rx).await;
                    let mut state_guard = state.lock().await;
                    state_guard.tasks.remove(&task_id);
                    let _ = state_guard.events.send(CoreEvent::TaskStopped { task_id });
                });
            }
            CoreCommand::StopTask { task_id } => {
                let mut state_guard = state.lock().await;
                if let Some(stop_tx) = state_guard.tasks.remove(&task_id) {
                    let _ = stop_tx.send(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CoreCommand, CoreEvent, CoreVersion, ModelAgent, TaskCoordinator, ToolAgent};
    use evohime_model_gateway::{
        providers::mock::MockProvider, ChatResult, ModelGateway, NativeToolCall,
    };
    use evohime_tool_runtime::ToolRegistry;
    use std::sync::Arc;

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
}
