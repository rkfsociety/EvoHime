//! Pipeline metrics HTTP + persist loop.
use crate::app::AppState;
use crate::metrics_export;
use crate::ApiError;
use axum::{
    extract::{Query, State},
    http::header,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::warn;

pub(crate) async fn pipeline_metrics(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, ApiError> {
    let persist = metrics_export::MetricsPersistConfig::from_env();
    let last = evohime_storage::latest_metrics_snapshot(&state.pool)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(json!({
        "pipeline": state.metrics.snapshot(),
        "worker": state.worker_metrics.snapshot(),
        "persist": metrics_export::MetricsPersistStatus {
            enabled: persist.enabled(),
            interval_secs: persist.interval.as_secs(),
            history_limit: persist.history_limit,
            last_persisted_at: last.map(|row| row.captured_at.to_rfc3339()),
        },
    })))
}

#[derive(Debug, Deserialize)]
pub(crate) struct MetricsHistoryQuery {
    #[serde(default = "default_metrics_history_limit")]
    limit: i64,
}

pub(crate) fn default_metrics_history_limit() -> i64 {
    60
}

pub(crate) async fn pipeline_metrics_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MetricsHistoryQuery>,
) -> Result<Json<Value>, ApiError> {
    let rows = evohime_storage::list_metrics_snapshots(&state.pool, query.limit)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let entries: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            json!({
                "id": row.id,
                "captured_at": row.captured_at,
                "pipeline": row.pipeline,
                "worker": row.worker,
            })
        })
        .collect();
    Ok(Json(json!({ "entries": entries })))
}

pub(crate) async fn prometheus_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let body = metrics_export::render_prometheus(
        &state.metrics.snapshot(),
        &state.worker_metrics.snapshot(),
    );
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

pub(crate) async fn metrics_persist_loop(
    state: Arc<AppState>,
    config: metrics_export::MetricsPersistConfig,
) {
    let mut ticker = tokio::time::interval(config.interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        let pipeline = match serde_json::to_value(state.metrics.snapshot()) {
            Ok(value) => value,
            Err(error) => {
                warn!(error = %error, "failed to serialize pipeline metrics for persist");
                continue;
            }
        };
        let worker = match serde_json::to_value(state.worker_metrics.snapshot()) {
            Ok(value) => value,
            Err(error) => {
                warn!(error = %error, "failed to serialize worker metrics for persist");
                continue;
            }
        };
        match evohime_storage::insert_metrics_snapshot(&state.pool, &pipeline, &worker).await {
            Ok(row) => {
                if let Err(error) =
                    evohime_storage::prune_metrics_snapshots(&state.pool, config.history_limit)
                        .await
                {
                    warn!(error = %error, "failed to prune metrics snapshots");
                } else {
                    tracing::debug!(
                        snapshot_id = row.id,
                        captured_at = %row.captured_at,
                        "persisted metrics snapshot"
                    );
                }
            }
            Err(error) => {
                warn!(error = %error, "failed to persist metrics snapshot");
            }
        }
    }
}
