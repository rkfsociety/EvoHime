//! Server bootstrap: pool, AppState, settings restore, crash recovery, background loops.
use crate::app::{AppConfig, AppState, McpServerConfig};
use crate::metrics_api::metrics_persist_loop;
use crate::metrics_export;
use crate::models_api::{build_model_config, ModelSettingsRequest};
use crate::observability;
use crate::permissions_api::{approval_audit_to_row, parse_permission_name};
use crate::plugins;
use crate::rate_limit;
use crate::scheduler;
use crate::task::{emit_event, resume_task_run};
use crate::worker;
use crate::worker_api::{recover_worker_jobs, worker_health_loop, worker_retention_loop};
use crate::worker_observability;
use anyhow::Context;
use evohime_model_gateway::ModelGateway;
use evohime_permissions::{ApprovalAuditEntry, PermissionMode};
use evohime_protocol::ServerEvent;
use evohime_task_engine::{fail_task, resume_task};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
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
    pub shutdown_token: CancellationToken,
}

pub async fn prepare(config: &AppConfig) -> anyhow::Result<StartupInfo> {
    let shutdown_token = CancellationToken::new();

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
            Ok(request) => {
                // Decrypt API keys when loading from database (Phase 5.7)
                let workspace_root = config.workspace_root.to_string_lossy().to_string();
                let decrypted_request =
                    crate::models_api::decrypt_model_config(request, &workspace_root);
                build_model_config(decrypted_request, &config.model_config).unwrap_or_else(|error| {
                    warn!(error = %error, "stored model settings are invalid; using environment defaults");
                    config.model_config.clone()
                })
            }
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
        workspace_merge_locks: Arc::new(Mutex::new(HashMap::new())),
        worker: worker::WorkerClient::new(config.worker_url.clone())?,
        worker_job_stall: config.worker_job_stall,
        plugin_catalog_cache: plugins::PluginCatalogCache::default(),
        metrics: Arc::new(observability::PipelineMetrics::new()),
        worker_metrics: Arc::new(worker_observability::WorkerMetrics::new()),
        rate_limiter: Arc::new(rate_limit::RateLimiter::from_env()),
        shutdown_token: shutdown_token.clone(),
        local_shutdown_secret: config.local_shutdown_secret.clone(),
    });

    let retention_state = state.clone();
    let retention_days = config.worker_retention_days;
    tokio::spawn(async move {
        worker_retention_loop(retention_state, retention_days).await;
    });
    let worktree_retention =
        duration_secs_env_local("EVOHIME_WORKTREE_RETENTION_SECS", 24 * 60 * 60);
    let worktree_cleanup_interval =
        duration_secs_env_local("EVOHIME_WORKTREE_CLEANUP_INTERVAL_SECS", 60 * 60);
    let worktree_cleanup_state = state.clone();
    tokio::spawn(async move {
        crate::task::worktree::worktree_cleanup_loop(
            worktree_cleanup_state,
            worktree_cleanup_interval,
            worktree_retention,
        )
        .await;
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
        let sched_shutdown = shutdown_token.clone();
        info!(
            interval_secs = sched_interval.as_secs(),
            "starting cron scheduler"
        );
        tokio::spawn(async move {
            scheduler::scheduler_loop(sched_state, sched_interval, sched_shutdown).await;
        });
    }

    match crate::sync_api::auto_sync_minutes() {
        Some(minutes) => {
            info!(interval_minutes = minutes, "cloud sync auto push enabled");
            let sync_state = state.clone();
            let interval = std::time::Duration::from_secs(minutes * 60);
            tokio::spawn(async move {
                crate::sync_api::auto_sync_loop(sync_state, interval).await;
            });
        }
        None => info!("cloud sync auto push disabled (EVOHIME_SYNC_AUTO_MINUTES=0)"),
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

    // Spawn session bus cleanup loop
    {
        let cleanup_state = state.clone();
        let cleanup_shutdown = shutdown_token.clone();
        let max_session_buses = std::env::var("EVOHIME_MAX_SESSION_BUSES")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|n: &usize| *n > 0)
            .unwrap_or(500);
        let cleanup_interval = duration_secs_env_local("EVOHIME_CLEANUP_INTERVAL_SECS", 300);
        info!(
            max_buses = max_session_buses,
            interval_secs = cleanup_interval.as_secs(),
            "starting session bus cleanup loop"
        );
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(cleanup_interval);
            loop {
                tokio::select! {
                    _ = cleanup_shutdown.cancelled() => {
                        info!("cleanup loop received shutdown signal");
                        break;
                    }
                    _ = ticker.tick() => {
                        cleanup_state.cleanup_session_buses(max_session_buses).await;
                    }
                }
            }
        });
    }

    // Spawn planning history TTL cleanup loop
    {
        let planning_pool = state.pool.clone();
        let planning_shutdown = shutdown_token.clone();
        let planning_retention_days = config.planning_retention_days;
        info!(
            retention_days = planning_retention_days,
            "starting planning history TTL cleanup loop"
        );
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(86400)); // 24h
            loop {
                tokio::select! {
                    _ = planning_shutdown.cancelled() => {
                        info!("planning history cleanup loop shutting down");
                        break;
                    }
                    _ = ticker.tick() => {
                        match evohime_storage::planning_history::cleanup_old_planning_history(&planning_pool, planning_retention_days).await {
                            Ok(count) => info!("planning history cleanup: {} rows deleted", count),
                            Err(e) => warn!("planning history cleanup failed: {}", e),
                        }
                    }
                }
            }
        });
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

    crate::task::worktree::cleanup_stale_worktrees(&state, worktree_retention).await;
    crate::task::worktree::cleanup_orphaned_worktree_directories(&state).await;

    let non_terminal_tasks = evohime_storage::list_tasks(&state.pool, None)
        .await
        .context("list tasks for task_cancellations startup seed")?
        .into_iter()
        .filter(|task| {
            !crate::task::worktree::TERMINAL_TASK_STATUSES.contains(&task.status.as_str())
        });
    {
        let mut cancellations = state.task_cancellations.lock().await;
        for task in non_terminal_tasks {
            // A fresh token here is never wired to anything that can
            // actually cancel this specific task mid-flight — it exists
            // purely so this entry's *presence* makes `is_concurrent` true
            // for any task starting before this one resumes or is
            // cancelled. Real cancellation of an already-paused task goes
            // through `evohime_task_engine::cancel_task` directly (see
            // `TaskCancel`, Task 5 Step 4), which doesn't depend on this
            // token at all.
            cancellations
                .entry(task.id)
                .or_insert_with(tokio_util::sync::CancellationToken::new);
        }
    }

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
                        correlation_id: Some(task_id),
                        duration_ms: None,
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
                    correlation_id: Some(task_id),
                    duration_ms: None,
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
                            duration_ms: None,
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
        shutdown_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_protocol::planning::{PlanCandidate, ScoreBreakdown};
    use evohime_storage::planning_history::{insert_planning_history, NewPlanningHistory};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_cleanup_deletes_old_planning_history() {
        let Some(pool) = evohime_storage::test_db::connect_integration_pool().await else {
            eprintln!("skipping cleanup test: database unavailable");
            return;
        };

        // Create test session and task
        let session = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000000'::uuid) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("create session");

        let task = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(&pool)
        .await
        .expect("create task");

        // Insert an old entry (100 days ago)
        sqlx::query(
            r#"
            INSERT INTO planning_history (task_id, session_id, candidates, chosen_plan_id, reasoning, created_at)
            VALUES ($1, $2, $3, $4, $5, now() - interval '100 days')
            "#,
        )
        .bind(task)
        .bind(session)
        .bind(vec![serde_json::json!({"id":"plan-1"})])
        .bind("plan-1")
        .bind("old reasoning")
        .execute(&pool)
        .await
        .expect("insert old entry");

        // Insert a recent entry
        let recent_entry = NewPlanningHistory {
            task_id: task,
            session_id: session,
            candidates: vec![PlanCandidate {
                id: "plan-2".to_string(),
                description: "Test plan".to_string(),
                confidence: 0.85,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.9,
                    tool_success_rate: 0.85,
                    complexity_penalty: 0.1,
                    feedback_adjustment: 0.0,
                    final_score: 0.8,
                },
            }],
            chosen_plan_id: Some("plan-2".to_string()),
            reasoning: "recent reasoning".to_string(),
        };

        insert_planning_history(&pool, recent_entry)
            .await
            .expect("insert recent entry");

        // Run cleanup with 30-day retention
        let deleted = evohime_storage::planning_history::cleanup_old_planning_history(&pool, 30)
            .await
            .expect("cleanup should succeed");

        // Should have deleted 1 old entry
        assert_eq!(deleted, 1u64, "should delete exactly 1 old entry");

        // Verify only 1 entry remains
        let remaining = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM planning_history WHERE task_id = $1",
        )
        .bind(task)
        .fetch_one(&pool)
        .await
        .expect("count entries");

        assert_eq!(remaining, 1i64, "should retain exactly 1 recent entry");
    }

    #[tokio::test]
    async fn test_cleanup_retains_recent_rows() {
        let Some(pool) = evohime_storage::test_db::connect_integration_pool().await else {
            eprintln!("skipping retention test: database unavailable");
            return;
        };

        // Create test session and task
        let session = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000000'::uuid) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("create session");

        let task = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(&pool)
        .await
        .expect("create task");

        // Insert entry created 10 days ago (within retention window)
        sqlx::query(
            r#"
            INSERT INTO planning_history (task_id, session_id, candidates, chosen_plan_id, reasoning, created_at)
            VALUES ($1, $2, $3, $4, $5, now() - interval '10 days')
            "#,
        )
        .bind(task)
        .bind(session)
        .bind(vec![serde_json::json!({"id":"plan-1"})])
        .bind("plan-1")
        .bind("recent reasoning")
        .execute(&pool)
        .await
        .expect("insert recent entry");

        // Run cleanup with 30-day retention
        let deleted = evohime_storage::planning_history::cleanup_old_planning_history(&pool, 30)
            .await
            .expect("cleanup should succeed");

        // Should delete 0 entries
        assert_eq!(
            deleted, 0u64,
            "should not delete entries within retention window"
        );

        // Verify entry still exists
        let remaining = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM planning_history WHERE task_id = $1",
        )
        .bind(task)
        .fetch_one(&pool)
        .await
        .expect("count entries");

        assert_eq!(
            remaining, 1i64,
            "should retain entry within retention window"
        );
    }

    #[tokio::test]
    async fn test_cleanup_loop_shutdown() {
        let Some(_pool) = evohime_storage::test_db::connect_integration_pool().await else {
            eprintln!("skipping shutdown test: database unavailable");
            return;
        };

        // Create cancellation token and immediately cancel it
        let shutdown_token = CancellationToken::new();
        let shutdown_clone = shutdown_token.clone();

        // Spawn cleanup loop that will immediately receive shutdown
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(86400));
            let mut iterations = 0;
            loop {
                tokio::select! {
                    _ = shutdown_clone.cancelled() => {
                        break;
                    }
                    _ = ticker.tick() => {
                        iterations += 1;
                    }
                }
            }
            iterations
        });

        // Give task a moment to start
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Send shutdown signal
        shutdown_token.cancel();

        // Wait for task to complete
        let iterations = tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("task should complete quickly")
            .expect("task should not panic");

        // Should not have ticked since we canceled immediately
        assert_eq!(
            iterations, 0,
            "cleanup should exit cleanly on shutdown signal"
        );
    }
}
