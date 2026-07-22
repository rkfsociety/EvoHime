use crate::{app::AppState, scheduler, ApiError};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use evohime_storage::scheduled::{
    create_scheduled_task, delete_scheduled_task, dispatch_scheduled_task, get_scheduled_task,
    list_scheduled_tasks, resume_scheduled_task, set_scheduled_task_status, update_scheduled_task,
    ScheduledTaskRow,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ScheduledQuery {
    pub workspace_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ScheduledInput {
    pub title: String,
    pub prompt: String,
    pub cron_expr: String,
}

#[derive(Debug, Serialize)]
pub struct ScheduledResponse {
    pub id: Uuid,
    pub workspace_path: String,
    pub title: String,
    pub prompt: String,
    pub cron_expr: String,
    pub status: String,
    pub last_run_at: Option<String>,
    pub next_run_at: String,
    pub run_count: i64,
    pub failure_count: i64,
    pub last_run_status: Option<String>,
    pub last_run_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn scope(state: &AppState, requested: Option<&str>) -> Result<String, ApiError> {
    let default_path = state.workspace_root.to_string_lossy().to_string();
    let path = requested.unwrap_or(&default_path).trim();
    if path.is_empty() {
        return Err(ApiError::BadRequest("workspace_path обязателен".into()));
    }
    let candidate = std::path::Path::new(path)
        .canonicalize()
        .map_err(|_| ApiError::BadRequest("workspace_path должен существовать".into()))?;
    if candidate != state.workspace_root {
        return Err(ApiError::BadRequest(
            "workspace_path вне разрешённого workspace".into(),
        ));
    }
    Ok(candidate.to_string_lossy().to_string())
}

fn validate_input(input: &ScheduledInput) -> Result<(), ApiError> {
    if input.title.trim().is_empty() || input.title.len() > 200 {
        return Err(ApiError::BadRequest(
            "title должен быть от 1 до 200 символов".into(),
        ));
    }
    if input.prompt.trim().is_empty() || input.prompt.len() > 8000 {
        return Err(ApiError::BadRequest(
            "prompt должен быть от 1 до 8000 символов".into(),
        ));
    }
    scheduler::validate_cron(&input.cron_expr).map_err(ApiError::BadRequest)
}

fn response(row: ScheduledTaskRow) -> ScheduledResponse {
    ScheduledResponse {
        id: row.id,
        workspace_path: row.workspace_path,
        title: row.title,
        prompt: row.prompt,
        cron_expr: row.cron_expr,
        status: row.status,
        last_run_at: row.last_run_at.map(|t| t.to_rfc3339()),
        next_run_at: row.next_run_at.to_rfc3339(),
        run_count: row.run_count,
        failure_count: row.failure_count,
        last_run_status: row.last_run_status,
        last_run_error: row.last_run_error,
        created_at: row.created_at.to_rfc3339(),
        updated_at: row.updated_at.to_rfc3339(),
    }
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ScheduledQuery>,
) -> Result<Json<Vec<ScheduledResponse>>, ApiError> {
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    Ok(Json(
        list_scheduled_tasks(&state.pool, &workspace)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .into_iter()
            .map(response)
            .collect(),
    ))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ScheduledQuery>,
    Json(input): Json<ScheduledInput>,
) -> Result<(StatusCode, Json<ScheduledResponse>), ApiError> {
    validate_input(&input)?;
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    let next_run_at =
        scheduler::next_run_after_now(&input.cron_expr).map_err(ApiError::BadRequest)?;
    let row = create_scheduled_task(
        &state.pool,
        &workspace,
        input.title.trim(),
        input.prompt.trim(),
        &input.cron_expr,
        next_run_at,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok((StatusCode::CREATED, Json(response(row))))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ScheduledQuery>,
) -> Result<Json<ScheduledResponse>, ApiError> {
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    let row = get_scheduled_task(&state.pool, id, &workspace)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("scheduled task не найден".into()))?;
    Ok(Json(response(row)))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ScheduledQuery>,
    Json(input): Json<ScheduledInput>,
) -> Result<Json<ScheduledResponse>, ApiError> {
    validate_input(&input)?;
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    let next_run_at =
        scheduler::next_run_after_now(&input.cron_expr).map_err(ApiError::BadRequest)?;
    let row = update_scheduled_task(
        &state.pool,
        id,
        &workspace,
        input.title.trim(),
        input.prompt.trim(),
        &input.cron_expr,
        next_run_at,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::NotFound("scheduled task не найден".into()))?;
    Ok(Json(response(row)))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ScheduledQuery>,
) -> Result<StatusCode, ApiError> {
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    if !delete_scheduled_task(&state.pool, id, &workspace)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
    {
        return Err(ApiError::NotFound("scheduled task не найден".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn pause(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ScheduledQuery>,
) -> Result<Json<ScheduledResponse>, ApiError> {
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    let row = set_scheduled_task_status(&state.pool, id, &workspace, "paused")
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("scheduled task не найден".into()))?;
    Ok(Json(response(row)))
}

pub async fn resume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ScheduledQuery>,
) -> Result<Json<ScheduledResponse>, ApiError> {
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    // Recompute next_run_at from the cron expression so the task fires at the right time.
    let row = get_scheduled_task(&state.pool, id, &workspace)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("scheduled task не найден".into()))?;
    let next_run_at =
        scheduler::next_run_after_now(&row.cron_expr).map_err(ApiError::BadRequest)?;
    let row = resume_scheduled_task(&state.pool, id, &workspace, next_run_at)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("scheduled task не найден".into()))?;
    Ok(Json(response(row)))
}

/// Manual trigger: fire the task now and reset next_run_at.
pub async fn trigger(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<ScheduledQuery>,
) -> Result<Json<ScheduledResponse>, ApiError> {
    let workspace = scope(&state, query.workspace_path.as_deref())?;
    let row = get_scheduled_task(&state.pool, id, &workspace)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("scheduled task не найден".into()))?;

    let next_run_at =
        scheduler::next_run_after_now(&row.cron_expr).map_err(ApiError::BadRequest)?;
    let dispatch = dispatch_scheduled_task(
        &state.pool,
        id,
        row.next_run_at,
        next_run_at,
        "manual",
        false,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .ok_or_else(|| ApiError::Conflict("расписание уже запускается или изменено".into()))?;
    let state2 = state.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::scheduler::fire_scheduled_task_pub(&state2, dispatch).await {
            tracing::error!(id = %id, error = %e, "manual trigger failed");
        }
    });
    let updated = get_scheduled_task(&state.pool, id, &workspace)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("scheduled task не найден".into()))?;
    Ok(Json(response(updated)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_input_rejects_empty_title() {
        let bad = ScheduledInput {
            title: "  ".into(),
            prompt: "do something".into(),
            cron_expr: "* * * * *".into(),
        };
        assert!(validate_input(&bad).is_err());
    }

    #[test]
    fn validate_input_rejects_bad_cron() {
        let bad = ScheduledInput {
            title: "My task".into(),
            prompt: "do something".into(),
            cron_expr: "not-a-cron".into(),
        };
        assert!(validate_input(&bad).is_err());
    }

    #[test]
    fn validate_input_accepts_good_input() {
        let good = ScheduledInput {
            title: "My task".into(),
            prompt: "Summarize recent changes".into(),
            // 6-field cron: sec min hour day month weekday
            cron_expr: "0 0 8 * * 1-5".into(),
        };
        assert!(validate_input(&good).is_ok());
    }
}
