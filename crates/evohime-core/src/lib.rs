pub struct CoreVersion;

impl CoreVersion {
    pub const fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

use std::{collections::HashMap, sync::Arc};

use evohime_model_gateway::{
    providers::{ChatMessage, ChatRole, ProviderError},
    ModelGateway,
};
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
    use super::{CoreCommand, CoreEvent, CoreVersion, ModelAgent, TaskCoordinator};
    use evohime_model_gateway::{providers::mock::MockProvider, ModelGateway};
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
}
