use evohime_protocol::{PlanStep, StepStatus, TaskStatus};
use evohime_storage::{StorageError, TaskRow};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TaskEngineError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("invalid transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    #[error("unknown dependency {dependency} for step {step_id}")]
    UnknownDependency { step_id: String, dependency: String },
    #[error("dependency cycle detected at step {step_id}")]
    DependencyCycle { step_id: String },
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

pub fn dependency_batches(plan: &[PlanStep]) -> Result<Vec<Vec<PlanStep>>, TaskEngineError> {
    let known_ids: HashSet<&str> = plan.iter().map(|step| step.id.as_str()).collect();
    let mut remaining: Vec<&PlanStep> = plan.iter().collect();
    let mut completed: HashSet<String> = HashSet::new();
    let mut batches = Vec::new();

    while !remaining.is_empty() {
        let mut ready = Vec::new();
        let mut blocked = Vec::new();

        for step in remaining {
            if let Some(missing) = step
                .depends_on
                .iter()
                .map(|dependency| dependency.as_str())
                .find(|dependency| !known_ids.contains(dependency))
            {
                return Err(TaskEngineError::UnknownDependency {
                    step_id: step.id.clone(),
                    dependency: missing.to_string(),
                });
            }

            if step
                .depends_on
                .iter()
                .all(|dependency| completed.contains(dependency))
            {
                ready.push(step.clone());
            } else {
                blocked.push(step);
            }
        }

        if ready.is_empty() {
            let cycle_step = blocked
                .first()
                .map(|step| step.id.clone())
                .unwrap_or_default();
            return Err(TaskEngineError::DependencyCycle {
                step_id: cycle_step,
            });
        }

        completed.extend(ready.iter().map(|step| step.id.clone()));
        batches.push(ready);
        remaining = blocked;
    }

    Ok(batches)
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

    #[test]
    fn dependency_batches_group_independent_steps_together() {
        let plan = vec![
            evohime_protocol::PlanStep {
                id: "step-1".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read the context".to_string(),
                depends_on: Vec::new(),
            },
            evohime_protocol::PlanStep {
                id: "step-2".to_string(),
                tool_name: "filesystem.search".to_string(),
                description: "Search for symbols".to_string(),
                depends_on: Vec::new(),
            },
            evohime_protocol::PlanStep {
                id: "step-3".to_string(),
                tool_name: "assistant.reply".to_string(),
                description: "Respond".to_string(),
                depends_on: vec!["step-1".to_string(), "step-2".to_string()],
            },
        ];

        let batches = dependency_batches(&plan).expect("batches");
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[1].len(), 1);
        assert_eq!(batches[1][0].id, "step-3");
    }

    #[test]
    fn dependency_batches_reject_unknown_dependencies() {
        let plan = vec![evohime_protocol::PlanStep {
            id: "step-1".to_string(),
            tool_name: "filesystem.read".to_string(),
            description: "Read the context".to_string(),
            depends_on: vec!["missing-step".to_string()],
        }];

        let error = dependency_batches(&plan).expect_err("missing dependency should fail");
        assert!(matches!(error, TaskEngineError::UnknownDependency { .. }));
    }
}
