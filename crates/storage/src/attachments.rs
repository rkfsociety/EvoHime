use crate::StorageError;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct SessionAttachmentRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub task_id: Option<Uuid>,
    pub workspace_path: String,
    pub original_name: String,
    pub stored_path: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub consumed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

const COLUMNS: &str = "id, session_id, task_id, workspace_path, original_name, stored_path, mime_type, size_bytes, consumed_at, created_at";

pub async fn create_session_attachment(
    pool: &PgPool,
    session_id: Uuid,
    workspace_path: &str,
    original_name: &str,
    stored_path: &str,
    mime_type: Option<&str>,
    size_bytes: i64,
) -> Result<SessionAttachmentRow, StorageError> {
    Ok(sqlx::query_as::<_, SessionAttachmentRow>(&format!(
        "INSERT INTO session_attachments (id, session_id, workspace_path, original_name, stored_path, mime_type, size_bytes)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {COLUMNS}"
    ))
    .bind(Uuid::new_v4())
    .bind(session_id)
    .bind(workspace_path)
    .bind(original_name)
    .bind(stored_path)
    .bind(mime_type)
    .bind(size_bytes)
    .fetch_one(pool)
    .await?)
}

pub async fn list_pending_session_attachments(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<SessionAttachmentRow>, StorageError> {
    Ok(sqlx::query_as::<_, SessionAttachmentRow>(&format!(
        "SELECT {COLUMNS}
         FROM session_attachments
         WHERE session_id = $1 AND consumed_at IS NULL
         ORDER BY created_at ASC"
    ))
    .bind(session_id)
    .fetch_all(pool)
    .await?)
}

pub async fn list_session_attachments(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<SessionAttachmentRow>, StorageError> {
    Ok(sqlx::query_as::<_, SessionAttachmentRow>(&format!(
        "SELECT {COLUMNS}
         FROM session_attachments
         WHERE session_id = $1
         ORDER BY created_at DESC"
    ))
    .bind(session_id)
    .fetch_all(pool)
    .await?)
}

pub async fn claim_pending_session_attachments(
    pool: &PgPool,
    session_id: Uuid,
    task_id: Uuid,
) -> Result<Vec<SessionAttachmentRow>, StorageError> {
    Ok(sqlx::query_as::<_, SessionAttachmentRow>(&format!(
        "UPDATE session_attachments
         SET task_id = $2, consumed_at = now()
         WHERE id IN (
            SELECT id FROM session_attachments
            WHERE session_id = $1 AND consumed_at IS NULL
            ORDER BY created_at ASC
         )
         RETURNING {COLUMNS}"
    ))
    .bind(session_id)
    .bind(task_id)
    .fetch_all(pool)
    .await?)
}
