use chrono::{DateTime, Utc};
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

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TaskRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_message: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct EventRow {
    pub sequence: i64,
    pub created_at: DateTime<Utc>,
    pub event_json: Value,
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

