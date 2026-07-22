//! Liveness and auth status endpoints.
use crate::app::AppState;
use crate::auth;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::timeout;

const DEEP_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AggregateStatus {
    status: &'static str,
    http_status: u16,
}

fn aggregate_status(database_ok: bool, worker_ok: bool, disk_ok: bool) -> AggregateStatus {
    if !database_ok || !disk_ok {
        AggregateStatus {
            status: "failed",
            http_status: 503,
        }
    } else if !worker_ok {
        AggregateStatus {
            status: "degraded",
            http_status: 200,
        }
    } else {
        AggregateStatus {
            status: "ok",
            http_status: 200,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ProbeResult {
    ok: bool,
    latency_ms: u64,
}

#[derive(Debug, Serialize)]
struct DeepHealthResponse {
    status: &'static str,
    components: DeepHealthComponents,
}

#[derive(Debug, Serialize)]
struct DeepHealthComponents {
    database: ComponentHealth,
    worker: ComponentHealth,
    disk: ComponentHealth,
}

#[derive(Debug, Serialize)]
struct ComponentHealth {
    status: &'static str,
    latency_ms: u64,
}

impl From<ProbeResult> for ComponentHealth {
    fn from(result: ProbeResult) -> Self {
        Self {
            status: if result.ok { "ok" } else { "failed" },
            latency_ms: result.latency_ms,
        }
    }
}

pub(crate) async fn auth_status(State(state): State<Arc<AppState>>) -> Json<auth::AuthStatus> {
    Json(auth::status_payload(&state.auth))
}

pub(crate) async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub(crate) async fn deep_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let (database, worker, disk) = tokio::join!(
        timed_database_probe(&state.pool),
        timed_worker_probe(&state.worker),
        timed_disk_probe(&state.workspace_root),
    );
    let aggregate = aggregate_status(database.ok, worker.ok, disk.ok);
    let response = DeepHealthResponse {
        status: aggregate.status,
        components: DeepHealthComponents {
            database: database.into(),
            worker: worker.into(),
            disk: disk.into(),
        },
    };

    (
        StatusCode::from_u16(aggregate.http_status).expect("health status is valid"),
        Json(response),
    )
}

async fn timed_database_probe(pool: &sqlx::PgPool) -> ProbeResult {
    let started = Instant::now();
    let ok = timeout(DEEP_HEALTH_TIMEOUT, sqlx::query("SELECT 1").execute(pool))
        .await
        .is_ok_and(|result| result.is_ok());
    ProbeResult {
        ok,
        latency_ms: started.elapsed().as_millis() as u64,
    }
}

async fn timed_worker_probe(worker: &crate::worker::WorkerClient) -> ProbeResult {
    let started = Instant::now();
    let ok = timeout(DEEP_HEALTH_TIMEOUT, worker.health())
        .await
        .is_ok_and(|result| result.is_ok_and(|health| health.status == "ok"));
    ProbeResult {
        ok,
        latency_ms: started.elapsed().as_millis() as u64,
    }
}

async fn timed_disk_probe(workspace_root: &Path) -> ProbeResult {
    let started = Instant::now();
    let ok = timeout(DEEP_HEALTH_TIMEOUT, tokio::fs::metadata(workspace_root))
        .await
        .is_ok_and(|result| result.is_ok_and(|metadata| metadata.is_dir()));
    ProbeResult {
        ok,
        latency_ms: started.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_health_is_ok_when_all_probes_are_healthy() {
        let result = aggregate_status(true, true, true);
        assert_eq!(result.status, "ok");
        assert_eq!(result.http_status, 200);
    }

    #[test]
    fn aggregate_health_is_degraded_when_worker_is_down() {
        let result = aggregate_status(true, false, true);
        assert_eq!(result.status, "degraded");
        assert_eq!(result.http_status, 200);
    }

    #[test]
    fn aggregate_health_fails_when_database_or_disk_is_down() {
        assert_eq!(aggregate_status(false, true, true).http_status, 503);
        assert_eq!(aggregate_status(true, true, false).http_status, 503);
    }
}
