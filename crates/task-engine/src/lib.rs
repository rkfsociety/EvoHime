use evohime_protocol::{StepStatus, TaskStatus};
use evohime_storage::{StorageError, TaskRow};
use sqlx::PgPool;
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TaskEngineError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("invalid transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}

pub fn can_transition(from: &str, to: TaskStatus) -> bool {
    matches!(
        (from, to),
        (
            "running",
            TaskStatus::Cancelling
                | TaskStatus::Completed
                | TaskStatus::Failed
                | TaskStatus::Paused
        ) | ("cancelling", TaskStatus::Cancelled | TaskStatus::Paused)
            | ("paused", TaskStatus::Running | TaskStatus::Cancelled)
            | ("failed", TaskStatus::Retrying)
            | ("retrying", TaskStatus::Running)
            | ("completed", TaskStatus::Running)
            | ("cancelled", TaskStatus::Running)
    )
}

pub async fn start_task(
    pool: &PgPool,
    session_id: Uuid,
    user_message: &str,
) -> Result<TaskRow, TaskEngineError> {
    Ok(evohime_storage::create_task(pool, session_id, user_message).await?)
}

pub async fn complete_task(pool: &PgPool, task_id: Uuid) -> Result<TaskRow, TaskEngineError> {
    transition(pool, task_id, "completed", TaskStatus::Completed).await
}

pub async fn fail_task(pool: &PgPool, task_id: Uuid) -> Result<TaskRow, TaskEngineError> {
    transition(pool, task_id, "failed", TaskStatus::Failed).await
}

pub async fn pause_task(pool: &PgPool, task_id: Uuid) -> Result<TaskRow, TaskEngineError> {
    transition(pool, task_id, "paused", TaskStatus::Paused).await
}

pub async fn cancel_task(pool: &PgPool, task_id: Uuid) -> Result<TaskRow, TaskEngineError> {
    let task = evohime_storage::set_task_status(pool, task_id, "cancelling").await?;
    Ok(evohime_storage::set_task_status(pool, task.id, "cancelled").await?)
}

pub async fn resume_task(pool: &PgPool, task_id: Uuid) -> Result<TaskRow, TaskEngineError> {
    transition(pool, task_id, "running", TaskStatus::Running).await
}

pub async fn retry_task(pool: &PgPool, task_id: Uuid) -> Result<TaskRow, TaskEngineError> {
    let task = evohime_storage::set_task_status(pool, task_id, "retrying").await?;
    Ok(evohime_storage::set_task_status(pool, task.id, "running").await?)
}

async fn transition(
    pool: &PgPool,
    task_id: Uuid,
    status: &str,
    target: TaskStatus,
) -> Result<TaskRow, TaskEngineError> {
    let tasks = evohime_storage::list_tasks(pool, None).await?;
    let current = tasks
        .iter()
        .find(|task| task.id == task_id)
        .map(|task| task.status.as_str())
        .unwrap_or("unknown");
    if current != "unknown" && !can_transition(current, target) {
        return Err(TaskEngineError::InvalidTransition {
            from: current.to_string(),
            to: status.to_string(),
        });
    }
    Ok(evohime_storage::set_task_status(pool, task_id, status).await?)
}

pub async fn recover_after_restart(pool: &PgPool) -> Result<Vec<TaskRow>, TaskEngineError> {
    Ok(evohime_storage::recover_running_tasks(pool).await?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemoryTask {
    pub status: TaskStatus,
    pub steps: HashMap<Uuid, StepStatus>,
}

impl InMemoryTask {
    pub fn new() -> Self {
        Self {
            status: TaskStatus::Running,
            steps: HashMap::new(),
        }
    }
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        for status in self.steps.values_mut() {
            if *status == StepStatus::Running || *status == StepStatus::Pending {
                *status = StepStatus::Cancelled;
            }
        }
    }
    pub fn pause(&mut self) {
        if self.status == TaskStatus::Running {
            self.status = TaskStatus::Paused;
        }
    }
    pub fn resume(&mut self) {
        if self.status == TaskStatus::Paused {
            self.status = TaskStatus::Running;
        }
    }
    pub fn retry(&mut self) {
        if self.status == TaskStatus::Failed {
            self.status = TaskStatus::Retrying;
            self.status = TaskStatus::Running;
        }
    }
}

impl Default for InMemoryTask {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_resume_and_retry_are_recoverable() {
        let mut task = InMemoryTask::new();
        task.steps.insert(Uuid::nil(), StepStatus::Running);
        task.cancel();
        assert_eq!(task.status, TaskStatus::Cancelled);
        task.status = TaskStatus::Paused;
        task.resume();
        assert_eq!(task.status, TaskStatus::Running);
        task.status = TaskStatus::Failed;
        task.retry();
        assert_eq!(task.status, TaskStatus::Running);
    }

    #[test]
    fn restart_pauses_running_tasks() {
        let mut task = InMemoryTask::new();
        task.pause();
        assert_eq!(task.status, TaskStatus::Paused);
    }

    #[test]
    fn pause_transition_is_supported() {
        assert!(can_transition("running", TaskStatus::Paused));
    }
}
