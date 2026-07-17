//! Liveness and auth status endpoints.
use crate::app::AppState;
use crate::auth;
use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) async fn auth_status(State(state): State<Arc<AppState>>) -> Json<auth::AuthStatus> {
    Json(auth::status_payload(&state.auth))
}

pub(crate) async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
