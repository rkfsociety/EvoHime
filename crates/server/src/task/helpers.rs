//! Shared task helpers (events, paths, history, errors).
use crate::app::AppState;
use crate::ApiError;
use evohime_agent_runtime::AgentError;
use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_protocol::ServerEvent;
use sqlx::PgPool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
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

pub(crate) async fn claim_attachment_context(
    state: &Arc<AppState>,
    session_id: Uuid,
    task_id: Uuid,
    workspace_root: &std::path::Path,
) -> Result<Option<String>, ApiError> {
    let attachments =
        evohime_storage::claim_pending_session_attachments(&state.pool, session_id, task_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    if attachments.is_empty() {
        return Ok(None);
    }

    const MAX_FILE_CHARS: usize = 16_000;
    const MAX_TOTAL_CHARS: usize = 48_000;
    let mut remaining = MAX_TOTAL_CHARS;
    let mut sections = Vec::with_capacity(attachments.len());

    for attachment in attachments {
        let rel_path = attachment.stored_path.clone();
        let abs_path = workspace_root.join(&rel_path);
        let mut section = format!(
            "- name: {}\n  path: {}\n  mime: {}\n  bytes: {}\n",
            attachment.original_name,
            rel_path,
            attachment.mime_type.as_deref().unwrap_or("unknown"),
            attachment.size_bytes
        );

        let bytes = fs::read(&abs_path).await.map_err(|error| {
            ApiError::Internal(format!("не удалось прочитать вложение: {error}"))
        })?;
        match String::from_utf8(bytes) {
            Ok(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() && remaining > 0 {
                    let excerpt: String = trimmed
                        .chars()
                        .take(MAX_FILE_CHARS.min(remaining))
                        .collect();
                    section.push_str("  content: |\n");
                    for line in excerpt.lines() {
                        section.push_str("    ");
                        section.push_str(line);
                        section.push('\n');
                    }
                    remaining = remaining.saturating_sub(excerpt.chars().count());
                } else {
                    section.push_str("  content: <empty text file>\n");
                }
            }
            Err(_) => {
                section.push_str(
                    "  content: <binary or non-UTF8 file; inspect manually via filesystem.read>\n",
                );
            }
        }
        sections.push(section);
        if remaining == 0 {
            break;
        }
    }

    Ok(Some(format!(
        "Attached files were uploaded for this task. Use their paths directly if you need deeper inspection.\n\n{}",
        sections.join("\n")
    )))
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

/// Removes `task_id` from `task_cancellations` only if the task has reached
/// a terminal status. A task that merely paused (e.g. for
/// `approval.required`) still has unfinished business in its workspace and
/// must keep signaling `is_concurrent` to any task starting while it's
/// paused — removing it early would let a second task start unisolated in
/// the same directory the paused task will resume into later (7.107).
pub(crate) async fn release_task_cancellation_if_terminal(state: &Arc<AppState>, task_id: Uuid) {
    let is_terminal = match evohime_storage::load_task(&state.pool, task_id).await {
        Ok(Some(task)) => crate::task::worktree::TERMINAL_TASK_STATUSES.contains(&task.status.as_str()),
        Ok(None) => true, // task row is gone entirely — nothing left to track
        Err(error) => {
            tracing::warn!(%task_id, %error, "failed to check task status before releasing task_cancellations entry; leaving it in place");
            false
        }
    };
    if is_terminal {
        state.task_cancellations.lock().await.remove(&task_id);
    }
}

/// Guards against a panic inside `process_user_message`/`resume_task_run`
/// leaking a `task_cancellations` entry forever. Those calls run directly
/// inside a `tokio::spawn`'d block with no outer `.await` — a panic there
/// aborts that spawned task immediately; normal post-await cleanup code
/// (including `release_task_cancellation_if_terminal` above) never runs,
/// since Rust doesn't execute code *after* a panic within the same async
/// fn, only `Drop` impls of values already on the stack as it unwinds.
/// Construct one right after inserting into `task_cancellations`, and call
/// `.disarm()` immediately before invoking
/// `release_task_cancellation_if_terminal` on every normal (non-panicking)
/// exit path — that's what makes the forced removal fire *only* on a panic,
/// never on an ordinary pause. On an ordinary pause, disarming still lets
/// `release_task_cancellation_if_terminal`'s own terminal-status check
/// decide correctly whether to actually remove the entry; the guard itself
/// never makes that decision.
pub(crate) struct TaskCancellationGuard {
    state: Arc<AppState>,
    task_id: Uuid,
    armed: bool,
}

impl TaskCancellationGuard {
    pub(crate) fn new(state: Arc<AppState>, task_id: Uuid) -> Self {
        Self {
            state,
            task_id,
            armed: true,
        }
    }

    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TaskCancellationGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // Dropping during a panic unwind can't run `.await` directly, so
        // spawn a fire-and-forget task to do the actual removal. Forced and
        // unconditional (unlike `release_task_cancellation_if_terminal`):
        // a panic means the task's real status is unknown/inconsistent, and
        // leaving every future task on this workspace isolated forever is
        // worse than the (already-abnormal) alternative.
        let state = self.state.clone();
        let task_id = self.task_id;
        tokio::spawn(async move {
            state.task_cancellations.lock().await.remove(&task_id);
        });
    }
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
