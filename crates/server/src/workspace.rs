use crate::{app::AppState, ApiError};
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::Utc;
use evohime_protocol::ServerEvent;
use serde::{Deserialize, Serialize};
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio::fs;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct FileQuery {
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveFileRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FileListing {
    pub path: String,
    pub entries: Vec<FileNode>,
}

#[derive(Debug, Serialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SaveResponse {
    pub path: String,
    pub bytes: usize,
    pub change: String,
}

#[derive(Debug, Serialize)]
pub struct GitSnapshot {
    pub status: String,
    pub diff: String,
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FileListing>, ApiError> {
    let path = resolve_relative_path(query.path.as_deref())?;
    let directory = directory_path(&state.workspace_root, path.as_deref())?;
    let mut entries = read_directory(&directory, &state.workspace_root).await?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Json(FileListing {
        path: relative_label(&state.workspace_root, &directory),
        entries,
    }))
}

pub async fn read_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FileContent>, ApiError> {
    let path = resolve_relative_path(query.path.as_deref())?
        .ok_or_else(|| ApiError::BadRequest("path is required".to_string()))?;
    let file = workspace_path(&state.workspace_root, &path)?;
    let content = fs::read_to_string(&file)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Json(FileContent {
        path: path.to_string_lossy().replace('\\', "/"),
        content,
    }))
}

pub async fn save_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SaveQuery>,
    Json(payload): Json<SaveFileRequest>,
) -> Result<Json<SaveResponse>, ApiError> {
    let path = resolve_relative_path(query.path.as_deref())?
        .ok_or_else(|| ApiError::BadRequest("path is required".to_string()))?;
    let file = writable_workspace_path(&state.workspace_root, &path)?;
    let existed_before = file.exists();

    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    }

    fs::write(&file, payload.content.as_bytes())
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let change = if existed_before { "updated" } else { "created" };
    let relative = path.to_string_lossy().replace('\\', "/");
    let now = Utc::now();

    let _ = state
        .publish_event(
            query.session_id,
            None,
            ServerEvent::FileChanged {
                path: relative.clone(),
                change: change.to_string(),
                created_at: now,
            },
        )
        .await;

    if let Ok(snapshot) = git_snapshot(&state, query.session_id).await {
        let _ = state
            .publish_event(
                query.session_id,
                None,
                ServerEvent::GitDiffChanged {
                    status: snapshot.status,
                    diff: snapshot.diff,
                    created_at: Utc::now(),
                },
            )
            .await;
    }

    Ok(Json(SaveResponse {
        path: relative,
        bytes: payload.content.as_bytes().len(),
        change: change.to_string(),
    }))
}

pub async fn git_status(State(state): State<Arc<AppState>>) -> Result<Json<GitSnapshot>, ApiError> {
    git_snapshot_with_path(&state, None).await.map(Json)
}

pub async fn git_diff(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
) -> Result<Json<GitSnapshot>, ApiError> {
    git_snapshot_with_path(&state, query.path.as_deref())
        .await
        .map(Json)
}

#[derive(Debug, Deserialize)]
pub struct SaveQuery {
    pub path: Option<String>,
    pub session_id: Uuid,
}

async fn git_snapshot(state: &Arc<AppState>, _session_id: Uuid) -> Result<GitSnapshot, ApiError> {
    git_snapshot_with_path_and_session(state, None).await
}

async fn git_snapshot_with_path(
    state: &Arc<AppState>,
    path: Option<&str>,
) -> Result<GitSnapshot, ApiError> {
    git_snapshot_with_path_and_session(state, path).await
}

async fn git_snapshot_with_path_and_session(
    state: &Arc<AppState>,
    path: Option<&str>,
) -> Result<GitSnapshot, ApiError> {
    let ctx = evohime_tool_runtime::ToolContext {
        workspace_root: state.workspace_root.clone(),
        task_id: Uuid::nil(),
    };

    let status = state
        .tools
        .execute(&ctx, "git.status", serde_json::Value::Null)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let diff_input = match path {
        Some(path) => serde_json::json!({ "path": path }),
        None => serde_json::Value::Null,
    };
    let diff = state
        .tools
        .execute(&ctx, "git.diff", diff_input)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(GitSnapshot {
        status: status.output,
        diff: diff.output,
    })
}

fn resolve_relative_path(input: Option<&str>) -> Result<Option<PathBuf>, ApiError> {
    match input {
        None | Some("") | Some(".") => Ok(None),
        Some(path) => {
            let mut relative = PathBuf::new();
            for component in Path::new(path).components() {
                match component {
                    Component::Normal(part) => relative.push(part),
                    Component::CurDir => {}
                    _ => return Err(ApiError::BadRequest("invalid path".to_string())),
                }
            }
            if relative.as_os_str().is_empty() {
                Ok(None)
            } else {
                Ok(Some(relative))
            }
        }
    }
}

fn workspace_path(root: &Path, relative: &Path) -> Result<PathBuf, ApiError> {
    let candidate = root.join(relative);
    let canonical_root = root
        .canonicalize()
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(ApiError::BadRequest("path outside workspace".to_string()));
    }

    Ok(canonical_candidate)
}

fn writable_workspace_path(root: &Path, relative: &Path) -> Result<PathBuf, ApiError> {
    let candidate = root.join(relative);
    if !candidate.starts_with(root) {
        return Err(ApiError::BadRequest("path outside workspace".to_string()));
    }

    Ok(candidate)
}

fn directory_path(root: &Path, relative: Option<&Path>) -> Result<PathBuf, ApiError> {
    match relative {
        None => Ok(root.to_path_buf()),
        Some(relative) => workspace_path(root, relative),
    }
}

fn relative_label(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .map(|value| {
            let text = value.to_string_lossy().replace('\\', "/");
            if text.is_empty() {
                ".".to_string()
            } else {
                text
            }
        })
        .unwrap_or_else(|| ".".to_string())
}

async fn read_directory(directory: &Path, root: &Path) -> Result<Vec<FileNode>, ApiError> {
    let mut result = Vec::new();
    let mut entries = fs::read_dir(directory)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
    {
        let metadata = entry
            .metadata()
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs().to_string());

        result.push(FileNode {
            name: entry.file_name().to_string_lossy().to_string(),
            path: relative,
            kind: if metadata.is_dir() {
                "dir".to_string()
            } else {
                "file".to_string()
            },
            size: metadata.len(),
            modified_at,
        });
    }

    Ok(result)
}
