pub struct CoreVersion;

impl CoreVersion {
    pub const fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCommand {
    StartTask { task_id: String, prompt: String },
    StopTask { task_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    TaskStarted { task_id: String, prompt: String },
    TaskStopped { task_id: String },
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
    use super::{CoreCommand, CoreEvent, CoreVersion, TaskCoordinator};

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
}
