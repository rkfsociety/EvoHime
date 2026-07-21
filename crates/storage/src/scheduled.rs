use crate::StorageError;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ScheduledTaskRow {
    pub id: Uuid,
    pub workspace_path: String,
    pub title: String,
    pub prompt: String,
    pub cron_expr: String,
    pub status: String,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: DateTime<Utc>,
    pub run_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const COLUMNS: &str = "id, workspace_path, title, prompt, cron_expr, status, last_run_at, next_run_at, run_count, created_at, updated_at";

pub async fn list_scheduled_tasks(
    pool: &PgPool,
    workspace_path: &str,
) -> Result<Vec<ScheduledTaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "SELECT {COLUMNS} FROM scheduled_tasks WHERE workspace_path = $1 ORDER BY next_run_at ASC"
    ))
    .bind(workspace_path)
    .fetch_all(pool)
    .await?)
}

pub async fn get_scheduled_task(
    pool: &PgPool,
    id: Uuid,
    workspace_path: &str,
) -> Result<Option<ScheduledTaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "SELECT {COLUMNS} FROM scheduled_tasks WHERE id = $1 AND workspace_path = $2"
    ))
    .bind(id)
    .bind(workspace_path)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_scheduled_task(
    pool: &PgPool,
    workspace_path: &str,
    title: &str,
    prompt: &str,
    cron_expr: &str,
    next_run_at: DateTime<Utc>,
) -> Result<ScheduledTaskRow, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "INSERT INTO scheduled_tasks (id, workspace_path, title, prompt, cron_expr, next_run_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLUMNS}"
    ))
    .bind(Uuid::new_v4())
    .bind(workspace_path)
    .bind(title)
    .bind(prompt)
    .bind(cron_expr)
    .bind(next_run_at)
    .fetch_one(pool)
    .await?)
}

pub async fn update_scheduled_task(
    pool: &PgPool,
    id: Uuid,
    workspace_path: &str,
    title: &str,
    prompt: &str,
    cron_expr: &str,
    next_run_at: DateTime<Utc>,
) -> Result<Option<ScheduledTaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "UPDATE scheduled_tasks
         SET title = $3, prompt = $4, cron_expr = $5, next_run_at = $6, updated_at = now()
         WHERE id = $1 AND workspace_path = $2
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(workspace_path)
    .bind(title)
    .bind(prompt)
    .bind(cron_expr)
    .bind(next_run_at)
    .fetch_optional(pool)
    .await?)
}

pub async fn set_scheduled_task_status(
    pool: &PgPool,
    id: Uuid,
    workspace_path: &str,
    status: &str,
) -> Result<Option<ScheduledTaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "UPDATE scheduled_tasks
         SET status = $3, updated_at = now()
         WHERE id = $1 AND workspace_path = $2
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(workspace_path)
    .bind(status)
    .fetch_optional(pool)
    .await?)
}

pub async fn delete_scheduled_task(
    pool: &PgPool,
    id: Uuid,
    workspace_path: &str,
) -> Result<bool, StorageError> {
    Ok(
        sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1 AND workspace_path = $2")
            .bind(id)
            .bind(workspace_path)
            .execute(pool)
            .await?
            .rows_affected()
            == 1,
    )
}

/// Called by the scheduler after running a task: sets `last_run_at`, increments counter,
/// and sets the next scheduled time.
pub async fn record_scheduled_task_run(
    pool: &PgPool,
    id: Uuid,
    next_run_at: DateTime<Utc>,
) -> Result<Option<ScheduledTaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "UPDATE scheduled_tasks
         SET last_run_at = now(), run_count = run_count + 1,
             next_run_at = $2, updated_at = now()
         WHERE id = $1
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(next_run_at)
    .fetch_optional(pool)
    .await?)
}

/// Fetch all active tasks whose `next_run_at` is in the past (ready to fire).
pub async fn due_scheduled_tasks(pool: &PgPool) -> Result<Vec<ScheduledTaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "SELECT {COLUMNS} FROM scheduled_tasks WHERE status = 'active' AND next_run_at <= now() ORDER BY next_run_at ASC"
    ))
    .fetch_all(pool)
    .await?)
}
