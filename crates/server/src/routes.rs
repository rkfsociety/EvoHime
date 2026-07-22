//! HTTP router assembly for the EvoHime server.
use crate::app::AppState;
use crate::attachments_api;
use crate::auth;
use crate::cors;
use crate::memory_api;
use crate::plugins;
use crate::scheduled_api;
use crate::sites_api;
use crate::workspace;
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(crate::health::health))
        .route("/health/deep", get(crate::health::deep_health))
        .route("/openapi.json", get(crate::openapi::document))
        .route("/api/auth/status", get(crate::health::auth_status))
        .route(
            "/api/operators",
            get(crate::operators_api::list).post(crate::operators_api::create),
        )
        .route(
            "/api/operators/:operator_id/rotate",
            post(crate::operators_api::rotate),
        )
        .route(
            "/api/operators/:operator_id/revoke",
            post(crate::operators_api::revoke),
        )
        .route("/api/features", get(crate::features::list))
        .route(
            "/api/models/config",
            get(crate::models_api::model_config).put(crate::models_api::update_model_config),
        )
        .route(
            "/api/models/available",
            get(crate::models_api::available_models),
        )
        .route(
            "/api/sessions",
            get(crate::sessions_api::list_sessions).post(crate::sessions_api::create_session),
        )
        .route(
            "/api/sessions/archived",
            get(crate::sessions_api::list_archived_sessions),
        )
        .route(
            "/api/sessions/:session_id",
            delete(crate::sessions_api::delete_session),
        )
        .route(
            "/api/sessions/:session_id/archive",
            post(crate::sessions_api::archive_session),
        )
        .route(
            "/api/sessions/:session_id/unarchive",
            post(crate::sessions_api::unarchive_session),
        )
        .route(
            "/api/sessions/:session_id/history",
            get(crate::sessions_api::session_history),
        )
        .route(
            "/api/sessions/:session_id/attachments",
            get(attachments_api::list_attachments).post(attachments_api::upload_attachments),
        )
        .route("/api/auth/github", get(crate::github_api::github_auth))
        .route(
            "/api/github/pull-requests",
            get(crate::github_api::list_pull_requests).post(crate::github_api::create_pull_request),
        )
        .route(
            "/api/github/pull-requests/:number",
            get(crate::github_api::get_pull_request),
        )
        .route("/api/files", get(workspace::list_files))
        .route(
            "/api/projects",
            get(workspace::list_projects).post(workspace::create_project),
        )
        .route(
            "/api/files/content",
            get(workspace::read_file)
                .put(workspace::save_file)
                .post(workspace::save_file),
        )
        .route("/api/git/status", get(workspace::git_status))
        .route("/api/git/diff", get(workspace::git_diff))
        .route("/api/git/commit", post(workspace::git_commit))
        .route("/api/git/pull", post(workspace::git_pull))
        .route("/api/git/push", post(workspace::git_push))
        .route("/api/tasks", get(crate::task::list_tasks))
        .route(
            "/api/scheduled",
            get(scheduled_api::list).post(scheduled_api::create),
        )
        .route(
            "/api/scheduled/:id",
            get(scheduled_api::get)
                .put(scheduled_api::update)
                .delete(scheduled_api::delete),
        )
        .route("/api/scheduled/:id/pause", post(scheduled_api::pause))
        .route("/api/scheduled/:id/resume", post(scheduled_api::resume))
        .route("/api/scheduled/:id/trigger", post(scheduled_api::trigger))
        .route("/api/sites", get(sites_api::list).post(sites_api::create))
        .route("/api/sites/:id/preview", get(sites_api::preview))
        .route("/api/sites/:id/publish", post(sites_api::publish))
        .route(
            "/api/sites/:id",
            put(sites_api::update).delete(sites_api::delete),
        )
        .route(
            "/api/worker/jobs",
            get(crate::worker_api::list_worker_jobs).post(crate::worker_api::create_worker_job),
        )
        .route(
            "/api/worker/jobs/:job_id",
            get(crate::worker_api::get_worker_job),
        )
        .route(
            "/api/worker/jobs/:job_id/retry",
            post(crate::worker_api::retry_worker_job),
        )
        .route("/api/worker/status", get(crate::worker_api::worker_status))
        .route(
            "/api/permissions",
            get(crate::permissions_api::list_permissions),
        )
        .route(
            "/api/permissions/audit",
            get(crate::permissions_api::list_permission_audit),
        )
        .route(
            "/api/permissions/scopes",
            get(crate::permissions_api::list_permission_scopes),
        )
        .route(
            "/api/permissions/:permission",
            put(crate::permissions_api::update_permission),
        )
        .route("/api/tools", get(crate::permissions_api::list_tools))
        .route(
            "/api/memory",
            get(memory_api::list_memory).post(memory_api::create_memory),
        )
        .route("/api/memory/export", get(memory_api::export_memory))
        .route("/api/memory/import", post(memory_api::import_memory))
        .route(
            "/api/memory/:id",
            get(memory_api::get_memory)
                .patch(memory_api::update_memory)
                .post(memory_api::resolve_memory_conflict)
                .delete(memory_api::delete_memory),
        )
        .route("/api/metrics", get(crate::metrics_api::pipeline_metrics))
        .route(
            "/api/metrics/history",
            get(crate::metrics_api::pipeline_metrics_history),
        )
        .route("/metrics", get(crate::metrics_api::prometheus_metrics))
        .route("/api/plugins", get(plugins::list_plugins))
        .route("/api/plugins/catalog", get(plugins::list_plugin_catalog))
        .route("/api/plugins/install", post(plugins::install_plugin))
        .route("/api/plugins/update", post(plugins::update_plugin))
        .route("/api/plugins/uninstall", post(plugins::uninstall_plugin))
        .route(
            "/api/plugins/:name/skills",
            get(plugins::list_plugin_skills),
        )
        .route(
            "/api/mcp/servers",
            get(crate::permissions_api::list_mcp_servers)
                .put(crate::permissions_api::update_mcp_servers),
        )
        .route("/ws/:session_id", get(crate::ws::ws_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_local_auth,
        ))
        .layer(cors::cors_layer_from_env())
        .layer(middleware::from_fn(crate::request_id::request_id))
        .with_state(state)
}
