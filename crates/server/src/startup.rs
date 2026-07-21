//! Server bootstrap: pool, AppState, settings restore, crash recovery, background loops.
use crate::app::{AppConfig, AppState, McpServerConfig};
use crate::metrics_api::metrics_persist_loop;
use crate::scheduler;
use crate::metrics_export;
use crate::models_api::{build_model_config, ModelSettingsRequest};
use crate::observability;
use crate::permissions_api::{approval_audit_to_row, parse_permission_name};
use crate::plugins;
use crate::rate_limit;
use crate::task::{emit_event, resume_task_run};
use crate::worker;
use crate::worker_api::{recover_worker_jobs, worker_health_loop, worker_retention_loop};
use crate::worker_observability;
use anyhow::Context;
use std::time::Duration;
use evohime_model_gateway::ModelGateway;
use evohime_permissions::{ApprovalAuditEntry, PermissionMode};
use evohime_protocol::ServerEvent;
use evohime_task_engine::{fail_task, resume_task};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

fn duration_secs_env_local(name: &str, default_secs: u64) -> Duration {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|s: &u64| *s > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(default_secs))
}

pub struct StartupInfo {
    pub state: Arc<AppState>,
    pub default_model_name: String,
    pub default_provider_name: String,
}

pub async fn prepare(config: &AppConfig) -> anyhow::Result<StartupInfo> {
    let pool_config = evohime_storage::PoolConfig::from_env();
    info!(
        max_connections = pool_config.max_connections,
        min_connections = pool_config.min_connections,
        acquire_timeout_secs = pool_config.acquire_timeout.as_secs(),
        idle_timeout_secs = ?pool_config.idle_timeout.map(|d| d.as_secs()),
        max_lifetime_secs = ?pool_config.max_lifetime.map(|d| d.as_secs()),
        "postgres pool configured"
    );
    let pool = evohime_storage::connect_pool(&config.database_url, &pool_config)
        .await
        .context("connect to postgres")?;

    evohime_storage::run_migrations(&pool)
        .await
        .context("run migrations")?;

    let active_model_config = match evohime_storage::load_setting(&pool, "model_config").await? {
        Some(value) => match serde_json::from_value::<ModelSettingsRequest>(value) {
            Ok(request) => build_model_config(request, &config.model_config).unwrap_or_else(|error| {
                warn!(error = %error, "stored model settings are invalid; using environment defaults");
                config.model_config.clone()
            }),
            Err(error) => {
                warn!(error = %error, "stored model settings could not be read; using environment defaults");
                config.model_config.clone()
            }
        },
        None => config.model_config.clone(),
    };
    let default_route_name = active_model_config.default_route.clone();
    let default_route_config = active_model_config.routes.get(&default_route_name).cloned();
    let default_model_name = default_route_config
        .as_ref()
        .map(|route| route.literouter.model.clone())
        .unwrap_or_default();
    let default_provider_name = default_route_config
        .as_ref()
        .map(|route| route.provider.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let model_gateway = match default_route_config {
        Some(ref route) if !route.configured() => {
            warn!(
                "default model route is not configured — LLM requests will fail until a key is set"
            );
            None
        }
        Some(_) => Some(Arc::new(
            ModelGateway::from_config(&active_model_config).context("init model gateway")?,
        )),
        None => {
            warn!("default model route '{}' is missing", default_route_name);
            None
        }
    };

    let permissions = evohime_permissions::PermissionEngine::new();
    let (audit_tx, mut audit_rx) = mpsc::unbounded_channel::<ApprovalAuditEntry>();
    permissions.attach_audit_sender(audit_tx).await;
    let audit_pool = pool.clone();
    tokio::spawn(async move {
        while let Some(entry) = audit_rx.recv().await {
            let row = approval_audit_to_row(&entry);
            if let Err(error) = evohime_storage::insert_permission_audit(&audit_pool, &row).await {
                warn!(error = %error, approval_id = %entry.approval_id, "failed to persist permission audit");
            }
        }
    });
    let state = Arc::new(AppState {
        pool,
        workspace_root: config.workspace_root.clone(),
        auth: config.auth.clone(),
        tools: evohime_tool_runtime::ToolRegistry::bootstrap_with_permissions(permissions.clone()),
        permissions,
        model_gateway: Arc::new(RwLock::new(model_gateway)),
        model_config: Arc::new(RwLock::new(active_model_config)),
        mcp_servers: Arc::new(Mutex::new(config.mcp_servers.clone())),
        session_buses: Arc::new(Mutex::new(HashMap::new())),
        task_cancellations: Arc::new(Mutex::new(HashMap::new())),
        worker: worker::WorkerClient::new(config.worker_url.clone())?,
        worker_job_stall: config.worker_job_stall,
        plugin_catalog_cache: plugins::PluginCatalogCache::default(),
        metrics: Arc::new(observability::PipelineMetrics::new()),
        worker_metrics: Arc::new(worker_observability::WorkerMetrics::new()),
        rate_limiter: Arc::new(rate_limit::RateLimiter::from_env()),
    });

    let retention_state = state.clone();
    let retention_days = config.worker_retention_days;
    tokio::spawn(async move {
        worker_retention_loop(retention_state, retention_days).await;
    });
    let health_state = state.clone();
    let health_interval = config.worker_health_interval;
    let health_stale = config.worker_health_stale;
    tokio::spawn(async move {
        worker_health_loop(health_state, health_interval, health_stale).await;
    });
    recover_worker_jobs(state.clone()).await;
    {
        let sched_state = state.clone();
        let sched_interval = duration_secs_env_local("EVOHIME_SCHEDULER_INTERVAL_SECS", 30);
        info!(interval_secs = sched_interval.as_secs(), "starting cron scheduler");
        tokio::spawn(async move {
            scheduler::scheduler_loop(sched_state, sched_interval).await;
        });
    }

    let metrics_persist = metrics_export::MetricsPersistConfig::from_env();
    if metrics_persist.enabled() {
        info!(
            interval_secs = metrics_persist.interval.as_secs(),
            history_limit = metrics_persist.history_limit,
            "metrics snapshot persistence enabled"
        );
        let persist_state = state.clone();
        tokio::spawn(async move {
            metrics_persist_loop(persist_state, metrics_persist).await;
        });
    } else {
        info!("metrics snapshot persistence disabled (EVOHIME_METRICS_PERSIST_INTERVAL_SECS=0)");
    }
    match evohime_storage::import_legacy_memory_notes(&state.pool).await {
        Ok(imported) => {
            if imported > 0 {
                info!(
                    imported,
                    "imported legacy session/global memory notes into memory_items"
                );
            } else {
                info!("legacy memory import: nothing new (already migrated or empty)");
            }
        }
        Err(error) => {
            warn!(error = %error, "legacy memory import failed; continuing without it");
        }
    }
    if let Some(value) = evohime_storage::load_setting(&state.pool, "permissions").await? {
        if let Some(settings) = value.as_object() {
            for (name, mode) in settings {
                if let (Some(permission), Ok(mode)) = (
                    parse_permission_name(name),
                    serde_json::from_value::<PermissionMode>(mode.clone()),
                ) {
                    state.permissions.set_mode(permission, mode).await;
                }
            }
        }
    }
    if let Some(value) = evohime_storage::load_setting(&state.pool, "permission_scopes").await? {
        match serde_json::from_value::<evohime_permissions::PermissionScopesSnapshot>(value) {
            Ok(snapshot) => {
                let sessions = snapshot.session_overrides.len();
                let grants = snapshot.path_grants.len();
                state.permissions.import_scopes(snapshot).await;
                info!(sessions, grants, "restored permission session/path scopes");
            }
            Err(error) => {
                warn!(error = %error, "stored permission_scopes could not be read; ignoring");
            }
        }
    }
    if let Some(value) = evohime_storage::load_setting(&state.pool, "mcp_servers").await? {
        if let Ok(servers) = serde_json::from_value::<Vec<McpServerConfig>>(value) {
            *state.mcp_servers.lock().await = servers;
        }
    }

    let recovered = evohime_task_engine::recover_after_restart(&state.pool)
        .await
        .context("recover tasks after restart")?;
    let resume_policy = evohime_task_engine::RestartResumePolicy::from_env();
    if resume_policy.auto_resume_mutating {
        info!("EVOHIME_AUTO_RESUME_ON_RESTART enabled: mutating tasks may auto-resume");
    }
    if !recovered.is_empty() {
        info!(
            count = recovered.len(),
            "tasks paused after crash (were running/cancelling)"
        );
        for task in recovered {
            let task_id = task.id;
            let session_id = task.session_id;
            let mutating = evohime_task_engine::task_has_mutating_work(&state.pool, task_id)
                .await
                .unwrap_or(true);
            let _ = evohime_storage::merge_checkpoint(
                &state.pool,
                task_id,
                None,
                &json!({
                    "pause_reason": "server_restart",
                    "mutating": mutating,
                }),
            )
            .await;

            if !evohime_task_engine::should_auto_resume_after_restart(resume_policy, mutating) {
                warn!(
                    %task_id,
                    "deferring auto-resume for mutating task; set EVOHIME_AUTO_RESUME_ON_RESTART=1 to override"
                );
                emit_event(
                    &state,
                    session_id,
                    Some(task_id),
                    ServerEvent::TaskStatusChanged {
                        task_id,
                        status: "paused".to_string(),
                    },
                )
                .await
                .map_err(|(_, error)| error)?;
                emit_event(
                    &state,
                    session_id,
                    Some(task_id),
                    ServerEvent::ActionLogged {
                        task_id,
                        action: "task.recovery_deferred".to_string(),
                        detail: "Mutating task left paused after server restart; resume manually or set EVOHIME_AUTO_RESUME_ON_RESTART=1".to_string(),
                        created_at: chrono::Utc::now(),
                    },
                )
                .await
                .map_err(|(_, error)| error)?;
                continue;
            }

            let _ = resume_task(&state.pool, task_id).await;
            emit_event(
                &state,
                session_id,
                Some(task_id),
                ServerEvent::TaskStatusChanged {
                    task_id,
                    status: "running".to_string(),
                },
            )
            .await
            .map_err(|(_, error)| error)?;
            emit_event(
                &state,
                session_id,
                Some(task_id),
                ServerEvent::ActionLogged {
                    task_id,
                    action: "task.recovered".to_string(),
                    detail: if mutating {
                        "Mutating task auto-resumed after server restart (EVOHIME_AUTO_RESUME_ON_RESTART)".to_string()
                    } else {
                        "Task restored after server restart".to_string()
                    },
                    created_at: chrono::Utc::now(),
                },
            )
            .await
            .map_err(|(_, error)| error)?;
            let state_for_task = state.clone();
            let token = CancellationToken::new();
            state
                .task_cancellations
                .lock()
                .await
                .insert(task_id, token.clone());
            tokio::spawn(async move {
                if let Err((failed_task_id, error)) =
                    resume_task_run(&state_for_task, task, token, false).await
                {
                    let _ = emit_event(
                        &state_for_task,
                        session_id,
                        Some(failed_task_id),
                        ServerEvent::TaskFailed {
                            task_id: failed_task_id,
                            error: error.to_string(),
                        },
                    )
                    .await;
                    let _ = fail_task(&state_for_task.pool, failed_task_id).await;
                }
                state_for_task
                    .task_cancellations
                    .lock()
                    .await
                    .remove(&task_id);
            });
        }
    }

    Ok(StartupInfo {
        state,
        default_model_name,
        default_provider_name,
    })
}
