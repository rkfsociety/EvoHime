use evohime_model_gateway::{ModelConfigResponse, ModelGateway, ModelGatewayConfig};
use evohime_tool_runtime::ToolRegistry;
use anyhow::Result;
use sqlx::PgPool;
use std::{collections::HashMap, env, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;
use evohime_protocol::ServerEvent;
use serde_json::to_value;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub bind_addr: String,
    pub demo_file_path: PathBuf,
    pub workspace_root: PathBuf,
    pub model_config: ModelGatewayConfig,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://evohime:evohime@localhost:5432/evohime".to_string());
        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
        let workspace_root = env::var("WORKSPACE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
        let workspace_root = workspace_root.canonicalize()?;
        let demo_file_path = env::var("DEMO_FILE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| workspace_root.join("docs/sample-context.md"));
        let model_config = ModelGatewayConfig::from_env()?;

        Ok(Self {
            database_url,
            bind_addr,
            demo_file_path,
            workspace_root,
            model_config,
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub demo_file_path: PathBuf,
    pub workspace_root: PathBuf,
    pub tools: ToolRegistry,
    pub model_gateway: Option<Arc<ModelGateway>>,
    pub model_config: ModelGatewayConfig,
    pub session_buses: Arc<Mutex<HashMap<Uuid, broadcast::Sender<ServerEvent>>>>,
    pub task_cancellations: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
}

impl AppState {
    pub fn model_config_response(&self) -> ModelConfigResponse {
        ModelGateway::config_response(&self.model_config)
    }

    pub async fn session_bus(&self, session_id: Uuid) -> broadcast::Sender<ServerEvent> {
        let mut buses = self.session_buses.lock().await;
        buses
            .entry(session_id)
            .or_insert_with(|| {
                let (sender, _receiver) = broadcast::channel(128);
                sender
            })
            .clone()
    }

    pub async fn publish_event(
        &self,
        session_id: Uuid,
        task_id: Option<Uuid>,
        event: ServerEvent,
    ) -> Result<i64> {
        let event_json = to_value(&event)?;
        let sequence = evohime_storage::insert_event(&self.pool, session_id, &event_json, task_id).await?;
        let sender = self.session_bus(session_id).await;
        let _ = sender.send(event);
        Ok(sequence)
    }
}
