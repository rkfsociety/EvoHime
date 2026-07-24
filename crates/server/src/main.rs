//! EvoHime HTTP + WebSocket server entrypoint.
mod api_error;
mod app;
mod attachments_api;
mod auth;
mod cors;
mod features;
mod github_api;
mod health;
mod llm_telemetry;
mod log_safety;
mod memory_api;
mod metrics_api;
mod metrics_export;
mod models_api;
mod observability;
mod operators_api;
mod openapi;
mod otel;
mod permissions_api;
mod plugin_lock;
mod plugin_trust;
mod plugins;
mod rate_limit;
mod request_id;
mod routes;
mod scheduled_api;
mod scheduler;
mod sessions_api;
mod sites_api;
mod startup;
mod sync_api;
mod task;
mod worker;
mod worker_api;
mod worker_observability;
mod workspace;
mod ws;

pub use api_error::ApiError;

use anyhow::Context;
use startup::prepare;
use std::net::SocketAddr;
use tracing::{info, warn};

use crate::app::AppConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _otel = otel::init_tracing()?;

    let config = AppConfig::from_env()?;
    let prepared = prepare(&config).await?;
    let state = prepared.state;

    let app = routes::build_router(state.clone());

    let addr: SocketAddr = config.bind_addr.parse().context("parse bind address")?;
    let auth_info = auth::status_payload(&config.auth);
    info!(
        workspace_root = %config.workspace_root.display(),
        demo_file = %config.demo_file_path.display(),
        model = %prepared.default_model_name,
        provider = %prepared.default_provider_name,
        llm_configured = %state.model_gateway.read().await.is_some(),
        auth_mode = auth_info.mode,
        token_configured = auth_info.token_configured,
        "listening on {}",
        addr
    );
    if !auth_info.token_configured {
        warn!(
            "EVOHIME_API_TOKEN unset: non-loopback clients get 401; set a token before exposing the bind address"
        );
    }
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
