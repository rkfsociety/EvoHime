use crate::{StorageError, TaskRow};
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
    pub failure_count: i64,
    pub last_run_status: Option<String>,
    pub last_run_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const COLUMNS: &str = "id, workspace_path, title, prompt, cron_expr, status, last_run_at, next_run_at, run_count, failure_count, last_run_status, last_run_error, created_at, updated_at";

#[derive(Debug, Clone)]
pub struct ScheduledDispatch {
    pub scheduled_task_id: Uuid,
    pub run_id: Uuid,
    pub due_at: DateTime<Utc>,
    pub trigger_kind: String,
    pub task: TaskRow,
}

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

/// Dispatch one scheduled task exactly once for the expected schedule slot.
///
/// The schedule advance, session/task creation, and run-history insert share one
/// transaction. Concurrent scheduler processes therefore observe one winner and
/// cannot create duplicate agent tasks for the same `(id, next_run_at)` slot.
pub async fn dispatch_scheduled_task(
    pool: &PgPool,
    id: Uuid,
    expected_next_run_at: DateTime<Utc>,
    next_run_at: DateTime<Utc>,
    trigger_kind: &str,
    due_only: bool,
) -> Result<Option<ScheduledDispatch>, StorageError> {
    let mut tx = pool.begin().await?;
    let status_clause = if due_only {
        "status = 'active' AND next_run_at <= now()"
    } else {
        "status IN ('active', 'paused')"
    };
    let scheduled = sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "UPDATE scheduled_tasks
         SET last_run_at = now(), run_count = run_count + 1,
             next_run_at = $3, last_run_status = 'dispatched',
             last_run_error = NULL, updated_at = now()
         WHERE id = $1 AND next_run_at = $2 AND {status_clause}
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(expected_next_run_at)
    .bind(next_run_at)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(scheduled) = scheduled else {
        tx.rollback().await?;
        return Ok(None);
    };

    let session = sqlx::query_as::<_, crate::SessionRow>(
        "INSERT INTO sessions DEFAULT VALUES RETURNING id, created_at",
    )
    .fetch_one(&mut *tx)
    .await?;
    let task = sqlx::query_as::<_, TaskRow>(
        "INSERT INTO tasks (session_id, user_message, workspace_path, status)
         VALUES ($1, $2, $3, 'running')
         RETURNING id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at",
    )
    .bind(session.id)
    .bind(&scheduled.prompt)
    .bind(&scheduled.workspace_path)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE sessions SET workspace_path = $2 WHERE id = $1 AND workspace_path IS NULL")
        .bind(session.id)
        .bind(&scheduled.workspace_path)
        .execute(&mut *tx)
        .await?;
    let run_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO scheduled_task_runs
            (scheduled_task_id, due_at, trigger_kind, status, session_id, task_id, completed_at)
         VALUES ($1, $2, $3, 'dispatched', $4, $5, now())
         RETURNING id",
    )
    .bind(scheduled.id)
    .bind(expected_next_run_at)
    .bind(trigger_kind)
    .bind(session.id)
    .bind(task.id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Some(ScheduledDispatch {
        scheduled_task_id: scheduled.id,
        run_id,
        due_at: expected_next_run_at,
        trigger_kind: trigger_kind.to_string(),
        task,
    }))
}

pub async fn resume_scheduled_task(
    pool: &PgPool,
    id: Uuid,
    workspace_path: &str,
    next_run_at: DateTime<Utc>,
) -> Result<Option<ScheduledTaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "UPDATE scheduled_tasks
         SET status = 'active', next_run_at = $3, updated_at = now()
         WHERE id = $1 AND workspace_path = $2 AND status = 'paused'
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(workspace_path)
    .bind(next_run_at)
    .fetch_optional(pool)
    .await?)
}

pub async fn record_scheduled_task_failure(
    pool: &PgPool,
    id: Uuid,
    expected_next_run_at: DateTime<Utc>,
    next_run_at: DateTime<Utc>,
    trigger_kind: &str,
    error: &str,
) -> Result<Option<ScheduledTaskRow>, StorageError> {
    let mut tx = pool.begin().await?;
    let updated = sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "UPDATE scheduled_tasks
         SET last_run_at = now(), failure_count = failure_count + 1,
             last_run_status = 'failed', last_run_error = $4,
             next_run_at = $3, updated_at = now()
         WHERE id = $1 AND next_run_at = $2 AND status = 'active'
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(expected_next_run_at)
    .bind(next_run_at)
    .bind(error)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(updated) = updated else {
        tx.rollback().await?;
        return Ok(None);
    };
    sqlx::query(
        "INSERT INTO scheduled_task_runs
            (scheduled_task_id, due_at, trigger_kind, status, error, completed_at)
         VALUES ($1, $2, $3, 'failed', $4, now())",
    )
    .bind(id)
    .bind(expected_next_run_at)
    .bind(trigger_kind)
    .bind(error)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(updated))
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

/// Fetch all active tasks whose `next_run_at` is in the past (ready to fire).
pub async fn due_scheduled_tasks(pool: &PgPool) -> Result<Vec<ScheduledTaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, ScheduledTaskRow>(&format!(
        "SELECT {COLUMNS} FROM scheduled_tasks WHERE status = 'active' AND next_run_at <= now() ORDER BY next_run_at ASC"
    ))
    .fetch_all(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::connect_integration_pool;
    use chrono::Duration;

    #[tokio::test]
    async fn concurrent_due_dispatch_creates_one_run_and_one_task() {
        let Some(pool) = connect_integration_pool().await else {
            return;
        };
        let workspace = std::env::current_dir()
            .expect("current dir")
            .to_string_lossy()
            .to_string();
        let scheduled = create_scheduled_task(
            &pool,
            &workspace,
            "Concurrent dispatch",
            "summarize the workspace",
            "0 * * * * *",
            Utc::now() - Duration::minutes(1),
        )
        .await
        .expect("scheduled task");
        let expected_due = scheduled.next_run_at;
        let next_run = Utc::now() + Duration::minutes(1);

        let (first, second) = tokio::join!(
            dispatch_scheduled_task(&pool, scheduled.id, expected_due, next_run, "cron", true,),
            dispatch_scheduled_task(&pool, scheduled.id, expected_due, next_run, "cron", true,),
        );
        let successes = [first, second]
            .into_iter()
            .filter_map(Result::ok)
            .filter(Option::is_some)
            .count();
        assert_eq!(successes, 1);

        let run_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM scheduled_task_runs WHERE scheduled_task_id = $1",
        )
        .bind(scheduled.id)
        .fetch_one(&pool)
        .await
        .expect("run count");
        assert_eq!(run_count, 1);

        let task_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tasks WHERE user_message = $1 AND workspace_path = $2",
        )
        .bind("summarize the workspace")
        .bind(&workspace)
        .fetch_one(&pool)
        .await
        .expect("task count");
        assert_eq!(task_count, 1);

        sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
            .bind(scheduled.id)
            .execute(&pool)
            .await
            .expect("cleanup scheduled task");
    }

    #[tokio::test]
    async fn failed_dispatch_advances_schedule_and_records_error() {
        let Some(pool) = connect_integration_pool().await else {
            return;
        };
        let workspace = std::env::current_dir()
            .expect("current dir")
            .to_string_lossy()
            .to_string();
        let scheduled = create_scheduled_task(
            &pool,
            &workspace,
            "Failed dispatch",
            "record a failed run",
            "0 * * * * *",
            Utc::now() - Duration::minutes(1),
        )
        .await
        .expect("scheduled task");
        let expected_due = scheduled.next_run_at;
        let next_run = Utc::now() + Duration::minutes(1);

        let updated = record_scheduled_task_failure(
            &pool,
            scheduled.id,
            expected_due,
            next_run,
            "cron",
            "database unavailable",
        )
        .await
        .expect("record failure")
        .expect("scheduled task updated");
        assert_eq!(updated.failure_count, 1);
        assert_eq!(updated.last_run_status.as_deref(), Some("failed"));
        assert_eq!(
            updated.last_run_error.as_deref(),
            Some("database unavailable")
        );

        let (status, error): (String, Option<String>) = sqlx::query_as(
            "SELECT status, error FROM scheduled_task_runs WHERE scheduled_task_id = $1",
        )
        .bind(scheduled.id)
        .fetch_one(&pool)
        .await
        .expect("failed run");
        assert_eq!(status, "failed");
        assert_eq!(error.as_deref(), Some("database unavailable"));

        sqlx::query("DELETE FROM scheduled_tasks WHERE id = $1")
            .bind(scheduled.id)
            .execute(&pool)
            .await
            .expect("cleanup scheduled task");
    }
}
