//! Shared task helpers (events, paths, history, errors).
use crate::app::AppState;
use crate::ApiError;
use evohime_agent_runtime::AgentError;
use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_protocol::ServerEvent;
use sqlx::PgPool;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

pub(crate) async fn load_chat_history(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<ChatMessage>, evohime_storage::StorageError> {
    let rows = evohime_storage::list_session_messages(pool, session_id).await?;
    let mut messages = Vec::with_capacity(rows.len());

    for row in rows {
        let role = match row.role.as_str() {
            "system" => ChatRole::System,
            "assistant" => ChatRole::Assistant,
            _ => ChatRole::User,
        };
        messages.push(ChatMessage::text(role, row.content));
    }

    Ok(messages)
}

pub(crate) fn map_agent_error(error: AgentError) -> ApiError {
    ApiError::Internal(error.to_string())
}

pub(crate) fn resolve_model_route(model_route: Option<&str>, default_route: &str) -> String {
    model_route
        .map(|route| route.to_string())
        .unwrap_or_else(|| default_route.to_string())
}

pub(crate) fn resolve_workspace_path(
    state: &Arc<AppState>,
    requested_path: Option<String>,
) -> Result<PathBuf, ApiError> {
    let root = state.workspace_root.canonicalize().map_err(|error| {
        ApiError::Internal(format!("не удалось определить корень workspace: {error}"))
    })?;
    let projects_root = root
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let requested = requested_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.clone());
    let candidate = if requested.is_absolute() {
        requested
    } else if requested.as_os_str() == "." {
        root.clone()
    } else {
        projects_root.join(requested)
    };
    let resolved = candidate
        .canonicalize()
        .map_err(|error| ApiError::BadRequest(format!("проект не найден: {error}")))?;
    if !resolved.starts_with(&projects_root) {
        return Err(ApiError::BadRequest(
            "проект должен находиться внутри workspace".to_string(),
        ));
    }
    if !resolved.is_dir() {
        return Err(ApiError::BadRequest(
            "путь проекта должен быть папкой".to_string(),
        ));
    }
    Ok(resolved)
}

/// Stable path for DB / UI matching. Strips Windows `\\?\` / `//?/` prefixes.
pub(crate) fn public_fs_path(path: &std::path::Path) -> String {
    public_fs_path_str(&path.to_string_lossy())
}

pub(crate) fn public_fs_path_str(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("//?/")
        .trim_start_matches("//./")
        .to_string()
}

pub(crate) async fn emit_event(
    state: &Arc<AppState>,
    session_id: Uuid,
    task_id: Option<Uuid>,
    event: ServerEvent,
) -> Result<(), (Uuid, ApiError)> {
    state
        .publish_event(session_id, task_id, event)
        .await
        .map_err(|error| {
            (
                task_id.unwrap_or(Uuid::nil()),
                ApiError::Internal(error.to_string()),
            )
        })?;
    Ok(())
}

pub(crate) async fn find_session_for_task(
    state: &Arc<AppState>,
    task_id: Uuid,
) -> Result<Uuid, ApiError> {
    let task = evohime_storage::load_task(&state.pool, task_id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("unknown task".to_string()))?;
    Ok(task.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_model_route_with_default_fallback() {
        assert_eq!(resolve_model_route(Some("planner"), "default"), "planner");
        assert_eq!(resolve_model_route(None, "default"), "default");
    }

    #[test]
    fn public_fs_path_strips_windows_extended_prefix() {
        assert_eq!(
            public_fs_path_str(r"\\?\F:\github\EvoHimeSmoke-20260719"),
            "F:/github/EvoHimeSmoke-20260719"
        );
        assert_eq!(
            public_fs_path_str("//?/F:/github/EvoHimeSmoke-20260719"),
            "F:/github/EvoHimeSmoke-20260719"
        );
        assert_eq!(
            public_fs_path_str("F:/github/EvoHimeSmoke-20260719"),
            "F:/github/EvoHimeSmoke-20260719"
        );
    }
}
