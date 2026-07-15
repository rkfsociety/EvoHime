use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_message: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskStepRow {
    pub id: Uuid,
    pub task_id: Uuid,
    pub step_index: i32,
    pub tool_name: String,
    pub input_json: Value,
    pub depends_on: Vec<Uuid>,
    pub status: String,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TaskCheckpointRow {
    pub task_id: Uuid,
    pub next_step: i32,
    pub state_json: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct EventRow {
    pub sequence: i64,
    pub created_at: DateTime<Utc>,
    pub event_json: Value,
}

#[derive(Debug, Clone, FromRow)]
pub struct MessageRow {
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}

pub async fn create_session(pool: &PgPool) -> Result<SessionRow, StorageError> {
    let row = sqlx::query_as::<_, SessionRow>(
        r#"
        INSERT INTO sessions DEFAULT VALUES
        RETURNING id, created_at
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn load_session(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Option<SessionRow>, StorageError> {
    let row = sqlx::query_as::<_, SessionRow>(
        r#"
        SELECT id, created_at
        FROM sessions
        WHERE id = $1
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

pub async fn list_session_events(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<EventRow>, StorageError> {
    let rows = sqlx::query_as::<_, EventRow>(
        r#"
        SELECT sequence, created_at, event_json
        FROM session_events
        WHERE session_id = $1
        ORDER BY sequence ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn create_task(
    pool: &PgPool,
    session_id: Uuid,
    user_message: &str,
) -> Result<TaskRow, StorageError> {
    let row = sqlx::query_as::<_, TaskRow>(
        r#"
        INSERT INTO tasks (session_id, user_message, status)
        VALUES ($1, $2, 'running')
        RETURNING id, session_id, user_message, status, created_at, completed_at
        "#,
    )
    .bind(session_id)
    .bind(user_message)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn complete_task(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<TaskRow, StorageError> {
    let row = sqlx::query_as::<_, TaskRow>(
        r#"
        UPDATE tasks
        SET status = 'completed',
            completed_at = now()
        WHERE id = $1
        RETURNING id, session_id, user_message, status, created_at, completed_at
        "#,
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn fail_task(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<TaskRow, StorageError> {
    let row = sqlx::query_as::<_, TaskRow>(
        r#"
        UPDATE tasks
        SET status = 'failed',
            completed_at = now()
        WHERE id = $1
        RETURNING id, session_id, user_message, status, created_at, completed_at
        "#,
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn set_task_status(pool: &PgPool, task_id: Uuid, status: &str) -> Result<TaskRow, StorageError> {
    Ok(sqlx::query_as::<_, TaskRow>(
        "UPDATE tasks SET status = $2, completed_at = CASE WHEN $2 IN ('completed','failed','cancelled') THEN now() ELSE NULL END WHERE id = $1 RETURNING id, session_id, user_message, status, created_at, completed_at",
    ).bind(task_id).bind(status).fetch_one(pool).await?)
}

pub async fn list_tasks(pool: &PgPool, session_id: Option<Uuid>) -> Result<Vec<TaskRow>, StorageError> {
    let rows = if let Some(session_id) = session_id {
        sqlx::query_as::<_, TaskRow>("SELECT id, session_id, user_message, status, created_at, completed_at FROM tasks WHERE session_id = $1 ORDER BY created_at DESC").bind(session_id).fetch_all(pool).await?
    } else {
        sqlx::query_as::<_, TaskRow>("SELECT id, session_id, user_message, status, created_at, completed_at FROM tasks ORDER BY created_at DESC").fetch_all(pool).await?
    };
    Ok(rows)
}

pub async fn recover_running_tasks(pool: &PgPool) -> Result<Vec<TaskRow>, StorageError> {
    sqlx::query("UPDATE tasks SET status = 'paused' WHERE status IN ('running','cancelling')").execute(pool).await?;
    Ok(sqlx::query_as::<_, TaskRow>("SELECT id, session_id, user_message, status, created_at, completed_at FROM tasks WHERE status = 'paused' ORDER BY created_at ASC").fetch_all(pool).await?)
}

pub async fn upsert_checkpoint(pool: &PgPool, task_id: Uuid, next_step: i32, state_json: &Value) -> Result<(), StorageError> {
    sqlx::query("INSERT INTO task_checkpoints(task_id,next_step,state_json) VALUES ($1,$2,$3) ON CONFLICT(task_id) DO UPDATE SET next_step=EXCLUDED.next_step,state_json=EXCLUDED.state_json,updated_at=now()")
        .bind(task_id).bind(next_step).bind(state_json).execute(pool).await?;
    Ok(())
}

pub async fn create_task_step(pool: &PgPool, task_id: Uuid, step_index: i32, tool_name: &str, input_json: &Value, depends_on: &[Uuid]) -> Result<TaskStepRow, StorageError> {
    Ok(sqlx::query_as::<_, TaskStepRow>("INSERT INTO task_steps(task_id,step_index,tool_name,input_json,depends_on) VALUES ($1,$2,$3,$4,$5) RETURNING id,task_id,step_index,tool_name,input_json,depends_on,status,output,error")
        .bind(task_id).bind(step_index).bind(tool_name).bind(input_json).bind(depends_on).fetch_one(pool).await?)
}

pub async fn list_task_steps(pool: &PgPool, task_id: Uuid) -> Result<Vec<TaskStepRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskStepRow>("SELECT id,task_id,step_index,tool_name,input_json,depends_on,status,output,error FROM task_steps WHERE task_id=$1 ORDER BY step_index").bind(task_id).fetch_all(pool).await?)
}

pub async fn set_step_status(pool: &PgPool, step_id: Uuid, status: &str, output: Option<&str>, error: Option<&str>) -> Result<(), StorageError> {
    sqlx::query("UPDATE task_steps SET status=$2, output=COALESCE($3,output), error=COALESCE($4,error), started_at=CASE WHEN $2='running' THEN now() ELSE started_at END, completed_at=CASE WHEN $2 IN ('completed','failed','cancelled') THEN now() ELSE completed_at END WHERE id=$1")
        .bind(step_id).bind(status).bind(output).bind(error).execute(pool).await?;
    Ok(())
}

pub async fn insert_event(
    pool: &PgPool,
    session_id: Uuid,
    event_json: &Value,
    task_id: Option<Uuid>,
) -> Result<i64, StorageError> {
    let sequence = next_sequence(pool, session_id).await?;

    sqlx::query(
        r#"
        INSERT INTO session_events (session_id, task_id, sequence, event_json)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(session_id)
    .bind(task_id)
    .bind(sequence)
    .bind(event_json)
    .execute(pool)
    .await?;

    Ok(sequence)
}

pub async fn insert_message(
    pool: &PgPool,
    session_id: Uuid,
    task_id: Option<Uuid>,
    role: &str,
    content: &str,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        INSERT INTO session_messages (session_id, task_id, role, content)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(session_id)
    .bind(task_id)
    .bind(role)
    .bind(content)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_session_messages(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<MessageRow>, StorageError> {
    let rows = sqlx::query_as::<_, MessageRow>(
        r#"
        SELECT role, content, created_at
        FROM session_messages
        WHERE session_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

async fn next_sequence(pool: &PgPool, session_id: Uuid) -> Result<i64, StorageError> {
    let sequence = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COALESCE(MAX(sequence), 0) + 1
        FROM session_events
        WHERE session_id = $1
        "#,
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;

    Ok(sequence)
}
