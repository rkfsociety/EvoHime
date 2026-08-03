use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use evohime_storage::{list_confidence_audit_for_session, list_confidence_audit_for_task};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfidenceThresholds {
    pub version: String,
    pub risk_none: ThresholdPair,
    pub risk_low: ThresholdPair,
    pub risk_medium: ThresholdPair,
    pub risk_high: ThresholdPair,
    pub missing_signal_ask_threshold: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThresholdPair {
    pub proceed: f32,
    pub ask: f32,
    pub require: Option<f32>,
}

impl Default for ConfidenceThresholds {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            risk_none: ThresholdPair {
                proceed: 0.65,
                ask: 0.40,
                require: None,
            },
            risk_low: ThresholdPair {
                proceed: 0.70,
                ask: 0.45,
                require: None,
            },
            risk_medium: ThresholdPair {
                proceed: 0.75,
                ask: 0.50,
                require: None,
            },
            risk_high: ThresholdPair {
                proceed: 0.85,
                ask: 0.65,
                require: Some(0.30),
            },
            missing_signal_ask_threshold: 0.5,
        }
    }
}

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
pub async fn get_confidence_thresholds() -> Json<ConfidenceThresholds> {
    Json(ConfidenceThresholds::default())
}

/// PUT /api/settings/confidence-thresholds
pub async fn update_confidence_thresholds(
    Json(thresholds): Json<ConfidenceThresholds>,
) -> Result<Json<ConfidenceThresholds>, StatusCode> {
    // TODO: Persist to settings table
    // For now, just validate and return
    if thresholds.risk_none.proceed < 0.0
        || thresholds.risk_none.proceed > 1.0
        || thresholds.risk_high.proceed > 1.0
        || thresholds.missing_signal_ask_threshold < 0.0
        || thresholds.missing_signal_ask_threshold > 1.0
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(Json(thresholds))
}
