use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use evohime_storage::{
    get_confidence_thresholds, list_confidence_audit_for_session, list_confidence_audit_for_task,
    set_confidence_thresholds, ConfidenceThresholds,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;

/// GET /api/confidence/audit?task_id=...
pub async fn get_confidence_audit_for_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    list_confidence_audit_for_task(&state.pool, task_id)
        .await
        .map(|records| {
            let items = records
                .into_iter()
                .map(|record| serde_json::to_value(record).unwrap_or(serde_json::json!({})))
                .collect();
            Json(items)
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /api/confidence/audit/session/:session_id
pub async fn get_confidence_audit_for_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Vec<serde_json::Value>>, StatusCode> {
    list_confidence_audit_for_session(&state.pool, session_id)
        .await
        .map(|records| {
            let items = records
                .into_iter()
                .map(|record| serde_json::to_value(record).unwrap_or(serde_json::json!({})))
                .collect();
            Json(items)
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// GET /api/settings/confidence-thresholds
pub async fn get_confidence_thresholds_endpoint(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
) -> Result<Json<ConfidenceThresholds>, StatusCode> {
    get_confidence_thresholds(&state.pool, identity.id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// PUT /api/settings/confidence-thresholds
pub async fn update_confidence_thresholds_endpoint(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<crate::auth::OperatorIdentity>,
    Json(thresholds): Json<ConfidenceThresholds>,
) -> Result<Json<ConfidenceThresholds>, StatusCode> {
    // Validate thresholds
    if thresholds.risk_none.proceed < 0.0
        || thresholds.risk_none.proceed > 1.0
        || thresholds.risk_high.proceed > 1.0
        || thresholds.missing_signal_ask_threshold < 0.0
        || thresholds.missing_signal_ask_threshold > 1.0
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    set_confidence_thresholds(&state.pool, identity.id, &thresholds)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(thresholds))
}
