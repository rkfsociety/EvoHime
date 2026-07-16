use crate::worker::WorkerClient;
use anyhow::Result;
use evohime_model_gateway::providers::ProviderKind;
use evohime_model_gateway::{ModelConfigResponse, ModelGateway, ModelGatewayConfig};
use evohime_permissions::PermissionEngine;
use evohime_protocol::ServerEvent;
use evohime_tool_runtime::ToolRegistry;
use serde::{Deserialize, Serialize};
use serde_json::to_value;
use sqlx::PgPool;
use std::{collections::HashMap, env, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub bind_addr: String,
    pub demo_file_path: PathBuf,
    pub workspace_root: PathBuf,
    pub model_config: ModelGatewayConfig,
    pub mcp_servers: Vec<McpServerConfig>,
    pub worker_url: String,
    pub worker_retention_days: i64,
    pub worker_health_interval: Duration,
    pub worker_health_stale: Duration,
    pub worker_job_stall: Duration,
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
        let mcp_servers = match env::var("MCP_SERVERS_JSON") {
            Ok(value) => serde_json::from_str::<Vec<McpServerConfig>>(&value)?,
            Err(_) => Vec::new(),
        };
        let worker_url =
            env::var("PYTHON_WORKER_URL").unwrap_or_else(|_| "http://127.0.0.1:8090".to_string());
        let worker_retention_days = env::var("WORKER_JOB_RETENTION_DAYS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|days: &i64| *days > 0)
            .unwrap_or(7);
        let worker_health_interval = duration_secs_env("WORKER_HEALTH_INTERVAL_SECS", 5);
        let worker_health_stale = duration_secs_env("WORKER_HEALTH_STALE_SECS", 15);
        let worker_job_stall = duration_secs_env("WORKER_JOB_STALL_SECS", 30);

        Ok(Self {
            database_url,
            bind_addr,
            demo_file_path,
            workspace_root,
            model_config,
            mcp_servers,
            worker_url,
            worker_retention_days,
            worker_health_interval,
            worker_health_stale,
            worker_job_stall,
        })
    }
}

fn duration_secs_env(name: &str, default_secs: u64) -> Duration {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|secs: &u64| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(default_secs))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpServerConfig {
    pub name: String,
    pub url: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub workspace_root: PathBuf,
    pub tools: ToolRegistry,
    pub permissions: PermissionEngine,
    pub model_gateway: Arc<RwLock<Option<Arc<ModelGateway>>>>,
    pub model_config: Arc<RwLock<ModelGatewayConfig>>,
    pub mcp_servers: Arc<Mutex<Vec<McpServerConfig>>>,
    pub session_buses: Arc<Mutex<HashMap<Uuid, broadcast::Sender<ServerEvent>>>>,
    pub task_cancellations: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    pub worker: WorkerClient,
    pub worker_job_stall: Duration,
}

impl AppState {
    pub async fn model_config_response(&self) -> ModelConfigResponse {
        let config = self.model_config.read().await.clone();
        let client = reqwest::Client::new();
        let mut available_models = HashMap::new();

        for (name, route) in &config.routes {
            if !route.configured() || route.provider == ProviderKind::Mock {
                continue;
            }
            let endpoint = format!("{}/models", route.literouter.base_url.trim_end_matches('/'));
            let response = match client
                .get(endpoint)
                .bearer_auth(&route.literouter.api_key)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => response,
                _ => continue,
            };
            let payload = match response.json::<OpenAiModelsResponse>().await {
                Ok(payload) => payload,
                Err(_) => continue,
            };
            let billing_mode = route.provider == ProviderKind::LiteRouter
                && route.literouter.model.ends_with(":free");
            let mut models = payload
                .data
                .into_iter()
                .map(|model| model.id)
                .filter(|model| model.ends_with(":free") == billing_mode)
                .collect::<Vec<_>>();
            models.sort();
            models.dedup();
            available_models.insert(name.clone(), models);
        }

        ModelGateway::config_response_with_models(&config, &available_models)
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
        let sequence =
            evohime_storage::insert_event(&self.pool, session_id, &event_json, task_id).await?;
        let sender = self.session_bus(session_id).await;
        let _ = sender.send(event);
        Ok(sequence)
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    #[serde(default)]
    data: Vec<OpenAiModelEntry>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    id: String,
}
