use crate::app::AppState;
use crate::task::helpers::{public_fs_path, resolve_workspace_path};
use crate::ApiError;
use axum::{
    extract::{Multipart, Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use tokio::fs;
use uuid::Uuid;

const MAX_ATTACHMENT_BYTES: usize = 512 * 1024;
const MAX_ATTACHMENTS_PER_REQUEST: usize = 8;

#[derive(Debug, Deserialize)]
pub struct AttachmentQuery {
    pub workspace_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AttachmentResponse {
    pub id: Uuid,
    pub session_id: Uuid,
    pub task_id: Option<Uuid>,
    pub workspace_path: String,
    pub original_name: String,
    pub stored_path: String,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub consumed_at: Option<String>,
    pub created_at: String,
}

fn response(row: evohime_storage::SessionAttachmentRow) -> AttachmentResponse {
    AttachmentResponse {
        id: row.id,
        session_id: row.session_id,
        task_id: row.task_id,
        workspace_path: row.workspace_path,
        original_name: row.original_name,
        stored_path: row.stored_path,
        mime_type: row.mime_type,
        size_bytes: row.size_bytes,
        consumed_at: row.consumed_at.map(|v| v.to_rfc3339()),
        created_at: row.created_at.to_rfc3339(),
    }
}

fn sanitize_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        "attachment.bin".to_string()
    } else {
        cleaned
    }
}

fn relative_stored_path(workspace_root: &StdPath, stored_path: &StdPath) -> Result<String, ApiError> {
    let relative = stored_path
        .strip_prefix(workspace_root)
        .map_err(|_| ApiError::Internal("attachment path escaped workspace".into()))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

pub async fn list_attachments(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Vec<AttachmentResponse>>, ApiError> {
    let rows = evohime_storage::list_session_attachments(&state.pool, session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(rows.into_iter().map(response).collect()))
}

pub async fn upload_attachments(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
    Query(query): Query<AttachmentQuery>,
    mut multipart: Multipart,
) -> Result<Json<Vec<AttachmentResponse>>, ApiError> {
    let Some(_) = evohime_storage::load_session(&state.pool, session_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    else {
        return Err(ApiError::NotFound("сессия не найдена".into()));
    };

    let workspace_root = resolve_workspace_path(&state, query.workspace_path.clone())?;
    let workspace_path = public_fs_path(&workspace_root);
    let attachment_root = workspace_root
        .join(".evohime")
        .join("attachments")
        .join(session_id.to_string());
    fs::create_dir_all(&attachment_root)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let mut uploaded = Vec::new();
    let mut count = 0usize;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::BadRequest(error.to_string()))?
    {
        if field.name() != Some("files") {
            continue;
        }
        count += 1;
        if count > MAX_ATTACHMENTS_PER_REQUEST {
            return Err(ApiError::BadRequest(format!(
                "можно загрузить не больше {} файлов за раз",
                MAX_ATTACHMENTS_PER_REQUEST
            )));
        }
        let original_name = sanitize_name(field.file_name().unwrap_or("attachment.bin"));
        let mime_type = field.content_type().map(str::to_string);
        let bytes = field
            .bytes()
            .await
            .map_err(|error| ApiError::BadRequest(error.to_string()))?;
        if bytes.is_empty() {
            return Err(ApiError::BadRequest(format!(
                "файл '{}' пустой",
                original_name
            )));
        }
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err(ApiError::BadRequest(format!(
                "файл '{}' слишком большой: максимум {} KB",
                original_name,
                MAX_ATTACHMENT_BYTES / 1024
            )));
        }
        let file_id = Uuid::new_v4();
        let stored_name = format!("{}-{}", file_id, original_name);
        let stored_path: PathBuf = attachment_root.join(stored_name);
        fs::write(&stored_path, &bytes)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        let relative = relative_stored_path(&workspace_root, &stored_path)?;
        let row = evohime_storage::create_session_attachment(
            &state.pool,
            session_id,
            &workspace_path,
            &original_name,
            &relative,
            mime_type.as_deref(),
            bytes.len() as i64,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
        uploaded.push(response(row));
    }

    if uploaded.is_empty() {
        return Err(ApiError::BadRequest("не переданы файлы в поле 'files'".into()));
    }

    Ok(Json(uploaded))
}
