//! Session CRUD and history HTTP API.
use crate::app::AppState;
use crate::ApiError;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use evohime_protocol::{HistoryItem, ServerEvent, SessionBootstrap};
use serde::Deserialize;
use serde_json::to_value;
use std::sync::Arc;
use uuid::Uuid;

pub(crate) async fn create_session(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
) -> Result<Json<SessionBootstrap>, ApiError> {
    state
        .rate_limiter
        .allow_session_create()
        .map_err(|error| ApiError::TooManyRequests(error.message))?;

    let session = evohime_storage::create_session_for_operator(&state.pool, identity.id)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let event = ServerEvent::SessionCreated {
        session_id: session.id,
        created_at: session.created_at,
    };
    let event_json = to_value(&event).map_err(|error| ApiError::Internal(error.to_string()))?;
    let (sequence, created_at) =
        evohime_storage::insert_event(&state.pool, session.id, &event_json, None)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

    Ok(Json(SessionBootstrap {
        session_id: session.id,
        created_at: session.created_at,
        events: vec![HistoryItem {
            sequence,
            created_at,
            event,
        }],
    }))
}

pub(crate) async fn delete_session(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted =
        evohime_storage::delete_session_for_operator(&state.pool, identity.id, session_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    if !deleted {
        return Err(ApiError::BadRequest("Чат не найден".to_string()));
    }

    state.session_buses.lock().await.remove(&session_id);
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn archive_session(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let archived =
        evohime_storage::archive_session_for_operator(&state.pool, identity.id, session_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    if !archived {
        return Err(ApiError::BadRequest(
            "Чат не найден или уже архивирован".to_string(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn unarchive_session(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let restored =
        evohime_storage::unarchive_session_for_operator(&state.pool, identity.id, session_id)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;
    if !restored {
        return Err(ApiError::BadRequest(
            "Чат не найден или не архивирован".to_string(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct SessionSummary {
    session_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    title: Option<String>,
    workspace_path: Option<String>,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    last_message: Option<String>,
    last_role: Option<String>,
}

pub(crate) fn session_summary(row: evohime_storage::SessionSummaryRow) -> SessionSummary {
    SessionSummary {
        session_id: row.id,
        created_at: row.created_at,
        title: row.title,
        workspace_path: row
            .workspace_path
            .map(|path| crate::task::helpers::public_fs_path_str(&path)),
        last_message_at: row.last_message_at,
        last_message: row.last_message,
        last_role: row.last_role,
    }
}

pub(crate) fn summarize_session_title(message: &str) -> String {
    let normalized = message
        .split("\n\nВложения:")
        .next()
        .unwrap_or(message)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = normalized.to_lowercase();
    let title = if lower.contains("разберись") && lower.contains("код") {
        "Разбор кода проекта".to_string()
    } else if lower.contains("запусти") && lower.contains("провер") {
        "Проверка проекта".to_string()
    } else if lower.contains("исправ") || lower.contains("почини") {
        "Исправление проекта".to_string()
    } else {
        normalized.chars().take(56).collect()
    };
    title
        .trim_end_matches([' ', '.', ',', ':', ';', '!', '?'])
        .to_string()
}

pub(crate) async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let rows = evohime_storage::list_sessions_for_operator(&state.pool, identity.id, 20)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let sessions = rows.into_iter().map(session_summary).collect();

    Ok(Json(sessions))
}

pub(crate) async fn list_archived_sessions(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    let rows = evohime_storage::list_archived_sessions_for_operator(&state.pool, identity.id, 100)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(rows.into_iter().map(session_summary).collect()))
}

pub(crate) async fn session_history(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Path(session_id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<PaginatedHistoryResponse>, ApiError> {
    // Prefer keyset pagination if no 'after' parameter
    if query.after.is_some() {
        // Legacy behavior: use after_sequence
        let after = query.after.unwrap_or(0).max(0);
        let rows = evohime_storage::list_session_events_after_for_operator(
            &state.pool,
            identity.id,
            session_id,
            after,
        )
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

        let mut history = Vec::with_capacity(rows.len());
        for row in rows {
            let event: ServerEvent = serde_json::from_value(row.event_json)
                .map_err(|error| ApiError::Internal(error.to_string()))?;
            history.push(HistoryItem {
                sequence: row.sequence,
                created_at: row.created_at,
                event,
            });
        }

        return Ok(Json(PaginatedHistoryResponse::legacy(history)));
    }

    // New keyset pagination
    let limit = query.limit.unwrap_or(50);
    let order = query.order.as_deref().unwrap_or("asc");
    let cursor = query.cursor.as_deref();

    let page = evohime_storage::list_session_events_paginated_for_operator(
        &state.pool,
        identity.id,
        session_id,
        limit,
        cursor,
        order,
    )
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;

    let mut items = Vec::with_capacity(page.items.len());
    for row in page.items {
        let event: ServerEvent = serde_json::from_value(row.event_json)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        items.push(HistoryItem {
            sequence: row.sequence,
            created_at: row.created_at,
            event,
        });
    }

    Ok(Json(PaginatedHistoryResponse {
        items,
        next_cursor: page.next_cursor,
        prev_cursor: page.prev_cursor,
        has_more: page.has_more,
        total_available: page.total_available,
    }))
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct PaginatedHistoryResponse {
    pub items: Vec<HistoryItem>,
    pub next_cursor: Option<String>,
    pub prev_cursor: Option<String>,
    pub has_more: bool,
    pub total_available: i64,
}

impl PaginatedHistoryResponse {
    fn legacy(items: Vec<HistoryItem>) -> Self {
        let total_available = items.len() as i64;
        Self {
            items,
            next_cursor: None,
            prev_cursor: None,
            has_more: false,
            total_available,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct HistoryQuery {
    after: Option<i64>,
    limit: Option<i64>,
    cursor: Option<String>,
    order: Option<String>,
}
