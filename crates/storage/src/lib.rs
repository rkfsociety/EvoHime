use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

pub mod memory;
pub mod metrics_snapshots;
pub mod permission_audit;
pub mod pool;

pub use memory::{
    apply_memory_item_feedback, delete_memory_item, get_memory_item, import_legacy_memory_notes,
    insert_memory_item, list_idle_memory_for_decay, list_memory_items, list_memory_items_overview,
    update_memory_item_embedding, update_memory_item_fields,
    update_memory_item_fields_with_embedding, update_memory_item_status, MemoryItemRow, MemoryKind,
    MemoryScope, MemoryStatus, NewMemoryItem, LOCAL_OPERATOR_SCOPE_KEY,
};
pub use metrics_snapshots::{
    insert_metrics_snapshot, latest_metrics_snapshot, list_metrics_snapshots,
    prune_metrics_snapshots, MetricsSnapshotRow,
};
pub use permission_audit::{
    insert_permission_audit, list_permission_audit, NewPermissionAudit, PermissionAuditRow,
};
pub use pool::{connect_pool, PoolConfig};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid memory item: {0}")]
    InvalidMemory(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SessionSummaryRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub title: Option<String>,
    pub workspace_path: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message: Option<String>,
    pub last_role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub user_message: String,
    pub model_route: Option<String>,
    pub model: Option<String>,
    pub workspace_path: Option<String>,
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

#[derive(Debug, Clone, FromRow)]
pub struct MemoryRow {
    pub note: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WorkerJobRow {
    pub id: Uuid,
    pub worker_job_id: Option<String>,
    pub task: String,
    pub payload_json: Value,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub result_json: Option<Value>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}

pub async fn create_worker_job(
    pool: &PgPool,
    task: &str,
    payload_json: &Value,
) -> Result<WorkerJobRow, StorageError> {
    Ok(sqlx::query_as::<_, WorkerJobRow>("INSERT INTO worker_jobs (task, payload_json) VALUES ($1, $2) RETURNING id, worker_job_id, task, payload_json, status, attempts, max_attempts, result_json, error, created_at, updated_at, completed_at")
        .bind(task).bind(payload_json).fetch_one(pool).await?)
}

pub async fn load_worker_job(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<WorkerJobRow>, StorageError> {
    Ok(sqlx::query_as::<_, WorkerJobRow>("SELECT id, worker_job_id, task, payload_json, status, attempts, max_attempts, result_json, error, created_at, updated_at, completed_at FROM worker_jobs WHERE id = $1")
        .bind(id).fetch_optional(pool).await?)
}

pub async fn set_worker_job_submitted(
    pool: &PgPool,
    id: Uuid,
    worker_job_id: &str,
    attempts: i32,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE worker_jobs SET worker_job_id=$2, status='running', attempts=$3, updated_at=now() WHERE id=$1")
        .bind(id).bind(worker_job_id).bind(attempts).execute(pool).await?;
    Ok(())
}

pub async fn complete_worker_job(
    pool: &PgPool,
    id: Uuid,
    status: &str,
    result_json: Option<&Value>,
    error: Option<&str>,
) -> Result<WorkerJobRow, StorageError> {
    Ok(sqlx::query_as::<_, WorkerJobRow>("UPDATE worker_jobs SET status=$2, result_json=$3, error=$4, updated_at=now(), completed_at=CASE WHEN $2 IN ('completed','failed') THEN now() ELSE completed_at END WHERE id=$1 RETURNING id, worker_job_id, task, payload_json, status, attempts, max_attempts, result_json, error, created_at, updated_at, completed_at")
        .bind(id).bind(status).bind(result_json).bind(error).fetch_one(pool).await?)
}

pub async fn list_recoverable_worker_jobs(
    pool: &PgPool,
) -> Result<Vec<WorkerJobRow>, StorageError> {
    Ok(sqlx::query_as::<_, WorkerJobRow>("SELECT id, worker_job_id, task, payload_json, status, attempts, max_attempts, result_json, error, created_at, updated_at, completed_at FROM worker_jobs WHERE status IN ('queued', 'running', 'retrying') ORDER BY created_at ASC")
        .fetch_all(pool).await?)
}

pub async fn list_recent_worker_jobs(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<WorkerJobRow>, StorageError> {
    let limit = limit.clamp(1, 200);
    Ok(sqlx::query_as::<_, WorkerJobRow>(
        "SELECT id, worker_job_id, task, payload_json, status, attempts, max_attempts, result_json, error, created_at, updated_at, completed_at \
         FROM worker_jobs ORDER BY created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn count_worker_jobs_by_status(
    pool: &PgPool,
) -> Result<Vec<(String, i64)>, StorageError> {
    Ok(sqlx::query_as::<_, (String, i64)>(
        "SELECT status, COUNT(*)::bigint FROM worker_jobs GROUP BY status ORDER BY status",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn prune_worker_jobs(
    pool: &PgPool,
    older_than: DateTime<Utc>,
) -> Result<u64, StorageError> {
    let result = sqlx::query(
        "DELETE FROM worker_jobs WHERE status IN ('completed', 'failed') AND completed_at IS NOT NULL AND completed_at < $1",
    )
    .bind(older_than)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn load_setting(pool: &PgPool, key: &str) -> Result<Option<Value>, StorageError> {
    let row = sqlx::query_scalar::<_, Value>("SELECT value_json FROM app_settings WHERE key = $1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn save_setting(pool: &PgPool, key: &str, value: &Value) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO app_settings (key, value_json) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value_json = EXCLUDED.value_json, updated_at = now()",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
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

pub async fn delete_session(pool: &PgPool, session_id: Uuid) -> Result<bool, StorageError> {
    let result = sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_sessions(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<SessionSummaryRow>, StorageError> {
    let rows = sqlx::query_as::<_, SessionSummaryRow>(
        r#"
        SELECT
            s.id,
            s.created_at,
            s.title,
            s.workspace_path,
            last_message.created_at AS last_message_at,
            last_message.content AS last_message,
            last_message.role AS last_role
        FROM sessions AS s
        LEFT JOIN LATERAL (
            SELECT role, content, created_at
            FROM session_messages
            WHERE session_id = s.id
            ORDER BY created_at DESC
            LIMIT 1
        ) AS last_message ON TRUE
        WHERE s.archived_at IS NULL
        ORDER BY s.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn list_archived_sessions(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<SessionSummaryRow>, StorageError> {
    let rows = sqlx::query_as::<_, SessionSummaryRow>(
        r#"
        SELECT
            s.id,
            s.created_at,
            s.title,
            s.workspace_path,
            last_message.created_at AS last_message_at,
            last_message.content AS last_message,
            last_message.role AS last_role
        FROM sessions AS s
        LEFT JOIN LATERAL (
            SELECT role, content, created_at
            FROM session_messages
            WHERE session_id = s.id
            ORDER BY created_at DESC
            LIMIT 1
        ) AS last_message ON TRUE
        WHERE s.archived_at IS NOT NULL
        ORDER BY s.archived_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn archive_session(pool: &PgPool, session_id: Uuid) -> Result<bool, StorageError> {
    let result = sqlx::query(
        "UPDATE sessions SET archived_at = now() WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_session_title_if_empty(
    pool: &PgPool,
    session_id: Uuid,
    title: &str,
) -> Result<bool, StorageError> {
    let result = sqlx::query("UPDATE sessions SET title = $2 WHERE id = $1 AND title IS NULL")
        .bind(session_id)
        .bind(title)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
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
    list_session_events_after(pool, session_id, 0).await
}

pub async fn list_session_events_after(
    pool: &PgPool,
    session_id: Uuid,
    after_sequence: i64,
) -> Result<Vec<EventRow>, StorageError> {
    let rows = sqlx::query_as::<_, EventRow>(
        r#"
        SELECT sequence, created_at, event_json
        FROM session_events
        WHERE session_id = $1 AND sequence > $2
        ORDER BY sequence ASC
        "#,
    )
    .bind(session_id)
    .bind(after_sequence)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn create_task(
    pool: &PgPool,
    session_id: Uuid,
    user_message: &str,
    model_route: Option<&str>,
    model: Option<&str>,
    workspace_path: Option<&str>,
) -> Result<TaskRow, StorageError> {
    let row = sqlx::query_as::<_, TaskRow>(
        r#"
        INSERT INTO tasks (session_id, user_message, model_route, model, workspace_path, status)
        VALUES ($1, $2, $3, $4, $5, 'running')
        RETURNING id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at
        "#,
    )
    .bind(session_id)
    .bind(user_message)
    .bind(model_route)
    .bind(model)
    .bind(workspace_path)
    .fetch_one(pool)
    .await?;

    sqlx::query("UPDATE sessions SET workspace_path = $2 WHERE id = $1 AND workspace_path IS NULL")
        .bind(session_id)
        .bind(workspace_path)
        .execute(pool)
        .await?;

    Ok(row)
}

pub async fn complete_task(pool: &PgPool, task_id: Uuid) -> Result<TaskRow, StorageError> {
    let row = sqlx::query_as::<_, TaskRow>(
        r#"
        UPDATE tasks
        SET status = 'completed',
            completed_at = now()
        WHERE id = $1
        RETURNING id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at
        "#,
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn fail_task(pool: &PgPool, task_id: Uuid) -> Result<TaskRow, StorageError> {
    let row = sqlx::query_as::<_, TaskRow>(
        r#"
        UPDATE tasks
        SET status = 'failed',
            completed_at = now()
        WHERE id = $1
        RETURNING id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at
        "#,
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn set_task_status(
    pool: &PgPool,
    task_id: Uuid,
    status: &str,
) -> Result<TaskRow, StorageError> {
    Ok(sqlx::query_as::<_, TaskRow>(
    "UPDATE tasks SET status = $2, completed_at = CASE WHEN $2 IN ('completed','failed','cancelled') THEN now() ELSE NULL END WHERE id = $1 RETURNING id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at",
    ).bind(task_id).bind(status).fetch_one(pool).await?)
}

pub async fn list_tasks(
    pool: &PgPool,
    session_id: Option<Uuid>,
) -> Result<Vec<TaskRow>, StorageError> {
    let rows = if let Some(session_id) = session_id {
        sqlx::query_as::<_, TaskRow>("SELECT id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at FROM tasks WHERE session_id = $1 ORDER BY created_at DESC").bind(session_id).fetch_all(pool).await?
    } else {
        sqlx::query_as::<_, TaskRow>("SELECT id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at FROM tasks ORDER BY created_at DESC").fetch_all(pool).await?
    };
    Ok(rows)
}

pub async fn load_task(pool: &PgPool, task_id: Uuid) -> Result<Option<TaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskRow>(
        "SELECT id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at FROM tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn recover_running_tasks(pool: &PgPool) -> Result<Vec<TaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskRow>(
        r#"
        UPDATE tasks
        SET status = 'paused'
        WHERE status IN ('running', 'cancelling')
        RETURNING id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at
        "#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn load_checkpoint(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Option<TaskCheckpointRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskCheckpointRow>(
        "SELECT task_id, next_step, state_json, updated_at FROM task_checkpoints WHERE task_id = $1",
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn upsert_checkpoint(
    pool: &PgPool,
    task_id: Uuid,
    next_step: i32,
    state_json: &Value,
) -> Result<(), StorageError> {
    sqlx::query("INSERT INTO task_checkpoints(task_id,next_step,state_json) VALUES ($1,$2,$3) ON CONFLICT(task_id) DO UPDATE SET next_step=EXCLUDED.next_step,state_json=EXCLUDED.state_json,updated_at=now()")
        .bind(task_id).bind(next_step).bind(state_json).execute(pool).await?;
    Ok(())
}

/// Shallow-merge object keys from `patch` into `existing` checkpoint state.
pub fn merge_checkpoint_state(existing: &Value, patch: &Value) -> Value {
    let mut merged = match existing.as_object() {
        Some(object) => object.clone(),
        None => serde_json::Map::new(),
    };
    if let Some(object) = patch.as_object() {
        for (key, value) in object {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

pub async fn merge_checkpoint(
    pool: &PgPool,
    task_id: Uuid,
    next_step: Option<i32>,
    patch: &Value,
) -> Result<Value, StorageError> {
    let existing = load_checkpoint(pool, task_id).await?;
    let current_state = existing
        .as_ref()
        .map(|row| row.state_json.clone())
        .unwrap_or_else(|| json!({}));
    let merged = merge_checkpoint_state(&current_state, patch);
    let step = next_step
        .or_else(|| existing.as_ref().map(|row| row.next_step))
        .unwrap_or(0);
    upsert_checkpoint(pool, task_id, step, &merged).await?;
    Ok(merged)
}

pub async fn create_task_step(
    pool: &PgPool,
    task_id: Uuid,
    step_index: i32,
    tool_name: &str,
    input_json: &Value,
    depends_on: &[Uuid],
) -> Result<TaskStepRow, StorageError> {
    Ok(sqlx::query_as::<_, TaskStepRow>("INSERT INTO task_steps(task_id,step_index,tool_name,input_json,depends_on) VALUES ($1,$2,$3,$4,$5) RETURNING id,task_id,step_index,tool_name,input_json,depends_on,status,output,error")
        .bind(task_id).bind(step_index).bind(tool_name).bind(input_json).bind(depends_on).fetch_one(pool).await?)
}

pub async fn list_task_steps(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Vec<TaskStepRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskStepRow>("SELECT id,task_id,step_index,tool_name,input_json,depends_on,status,output,error FROM task_steps WHERE task_id=$1 ORDER BY step_index").bind(task_id).fetch_all(pool).await?)
}

pub async fn set_step_status(
    pool: &PgPool,
    step_id: Uuid,
    status: &str,
    output: Option<&str>,
    error: Option<&str>,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE task_steps SET status=$2, output=COALESCE($3,output), error=COALESCE($4,error), started_at=CASE WHEN $2='running' THEN now() ELSE started_at END, completed_at=CASE WHEN $2 IN ('completed','failed','cancelled') THEN now() ELSE completed_at END WHERE id=$1")
        .bind(step_id).bind(status).bind(output).bind(error).execute(pool).await?;
    Ok(())
}

pub async fn insert_event(
    pool: &PgPool,
    session_id: Uuid,
    event_json: &Value,
    task_id: Option<Uuid>,
) -> Result<(i64, DateTime<Utc>), StorageError> {
    let sequence = next_sequence(pool, session_id).await?;

    let created_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        r#"
        INSERT INTO session_events (session_id, task_id, sequence, event_json)
        VALUES ($1, $2, $3, $4)
        RETURNING created_at
        "#,
    )
    .bind(session_id)
    .bind(task_id)
    .bind(sequence)
    .bind(event_json)
    .fetch_one(pool)
    .await?;

    Ok((sequence, created_at))
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

pub async fn insert_session_memory(
    pool: &PgPool,
    session_id: Uuid,
    source_task_id: Option<Uuid>,
    note: &str,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        INSERT INTO session_memory (session_id, source_task_id, note)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(session_id)
    .bind(source_task_id)
    .bind(note)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_session_memory(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<MemoryRow>, StorageError> {
    let rows = sqlx::query_as::<_, MemoryRow>(
        r#"
        SELECT note, created_at
        FROM session_memory
        WHERE session_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn insert_global_memory(
    pool: &PgPool,
    scope_key: &str,
    source_task_id: Option<Uuid>,
    note: &str,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        INSERT INTO global_memory (scope_key, source_task_id, note)
        VALUES ($1, $2, $3)
        ON CONFLICT (scope_key, note) DO NOTHING
        "#,
    )
    .bind(scope_key)
    .bind(source_task_id)
    .bind(note)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_global_memory(
    pool: &PgPool,
    scope_key: &str,
    limit: i64,
) -> Result<Vec<MemoryRow>, StorageError> {
    let rows = sqlx::query_as::<_, MemoryRow>(
        r#"
        SELECT note, created_at
        FROM global_memory
        WHERE scope_key = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(scope_key)
    .bind(limit)
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
