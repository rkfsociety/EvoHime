//! Permissions, tools, and MCP server HTTP API.
use crate::app::{AppState, McpServerConfig};
use crate::ApiError;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use evohime_permissions::{ApprovalAuditEntry, Permission, PermissionMode};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

pub(crate) async fn list_permissions(State(state): State<Arc<AppState>>) -> Json<Value> {
    let names = [
        ("filesystem_read", Permission::FilesystemRead),
        ("filesystem_write", Permission::FilesystemWrite),
        ("shell_execute", Permission::ShellExecute),
        ("git_read", Permission::GitRead),
        ("git_write", Permission::GitWrite),
        ("browser_access", Permission::BrowserAccess),
        ("mcp_call", Permission::McpCall),
        ("memory_search", Permission::MemorySearch),
    ];
    let mut result = serde_json::Map::new();
    for (name, permission) in names {
        result.insert(
            name.into(),
            json!({"mode": state.permissions.mode(permission).await}),
        );
    }
    Json(Value::Object(result))
}

#[derive(Debug, Deserialize)]
pub(crate) struct PermissionAuditQuery {
    #[serde(default = "default_permission_audit_limit")]
    limit: i64,
}

pub(crate) fn default_permission_audit_limit() -> i64 {
    200
}

pub(crate) fn approval_audit_to_row(
    entry: &ApprovalAuditEntry,
) -> evohime_storage::NewPermissionAudit {
    evohime_storage::NewPermissionAudit {
        approval_id: entry.approval_id,
        task_id: entry.task_id,
        session_id: entry.session_id,
        tool_name: entry.tool_name.clone(),
        permission: serde_json::to_value(entry.permission)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("{:?}", entry.permission)),
        scope: entry.scope.clone(),
        decision: serde_json::to_value(entry.decision)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("{:?}", entry.decision).to_ascii_lowercase()),
        at_ms: entry.at_ms as i64,
        remembered_path: entry.remembered_path,
    }
}

pub(crate) async fn list_permission_audit(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PermissionAuditQuery>,
) -> Result<Json<Value>, ApiError> {
    let rows = evohime_storage::list_permission_audit(&state.pool, query.limit)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let entries: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            json!({
                "approval_id": row.approval_id,
                "task_id": row.task_id,
                "session_id": row.session_id,
                "tool_name": row.tool_name,
                "permission": row.permission,
                "scope": row.scope,
                "decision": row.decision,
                "at_ms": row.at_ms,
                "remembered_path": row.remembered_path,
            })
        })
        .collect();
    Ok(Json(json!({ "entries": entries })))
}

pub(crate) async fn list_permission_scopes(State(state): State<Arc<AppState>>) -> Json<Value> {
    let session_overrides = state.permissions.list_session_overrides().await;
    let path_grants = state.permissions.list_path_grants().await;
    Json(json!({
        "session_overrides": session_overrides,
        "path_grants": path_grants,
    }))
}

#[derive(serde::Serialize)]
pub(crate) struct ToolSummary {
    name: String,
    description: String,
    permissions: Vec<String>,
    timeout_ms: u64,
}

pub(crate) async fn list_tools(State(state): State<Arc<AppState>>) -> Json<Vec<ToolSummary>> {
    let tools = state
        .tools
        .list()
        .into_iter()
        .map(|tool| ToolSummary {
            name: tool.name.to_string(),
            description: tool.description.to_string(),
            permissions: tool
                .permissions
                .iter()
                .map(|permission| permission_name(*permission).to_string())
                .collect(),
            timeout_ms: duration_to_ms(tool.timeout),
        })
        .collect();
    Json(tools)
}

pub(crate) async fn list_mcp_servers(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<McpServerConfig>> {
    Json(state.mcp_servers.lock().await.clone())
}

pub(crate) async fn update_mcp_servers(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Vec<McpServerConfig>>,
) -> Result<Json<Vec<McpServerConfig>>, ApiError> {
    let servers = body
        .into_iter()
        .map(validate_mcp_server)
        .collect::<Result<Vec<_>, _>>()?;
    let value =
        serde_json::to_value(&servers).map_err(|error| ApiError::Internal(error.to_string()))?;
    evohime_storage::save_setting(&state.pool, "mcp_servers", &value)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    *state.mcp_servers.lock().await = servers.clone();
    Ok(Json(servers))
}

pub(crate) async fn update_permission(
    State(state): State<Arc<AppState>>,
    Path(permission): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let permission = match permission.as_str() {
        "filesystem_read" => Permission::FilesystemRead,
        "filesystem_write" => Permission::FilesystemWrite,
        "shell_execute" => Permission::ShellExecute,
        "git_read" => Permission::GitRead,
        "git_write" => Permission::GitWrite,
        "browser_access" => Permission::BrowserAccess,
        "mcp_call" => Permission::McpCall,
        "memory_search" => Permission::MemorySearch,
        _ => return Err(ApiError::BadRequest("unknown permission".into())),
    };
    let mode: PermissionMode = serde_json::from_value(
        body.get("mode")
            .cloned()
            .ok_or_else(|| ApiError::BadRequest("mode is required".into()))?,
    )
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    state.permissions.set_mode(permission, mode).await;
    let settings = permission_settings_value(&state).await;
    evohime_storage::save_setting(&state.pool, "permissions", &settings)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(Json(
        json!({"permission": permission_name(permission), "mode": mode}),
    ))
}

pub(crate) fn permission_name(permission: Permission) -> &'static str {
    match permission {
        Permission::FilesystemRead => "filesystem_read",
        Permission::FilesystemWrite => "filesystem_write",
        Permission::ShellExecute => "shell_execute",
        Permission::GitRead => "git_read",
        Permission::GitWrite => "git_write",
        Permission::BrowserAccess => "browser_access",
        Permission::McpCall => "mcp_call",
        Permission::MemorySearch => "memory_search",
    }
}

pub(crate) fn parse_permission_name(name: &str) -> Option<Permission> {
    match name {
        "filesystem_read" => Some(Permission::FilesystemRead),
        "filesystem_write" => Some(Permission::FilesystemWrite),
        "shell_execute" => Some(Permission::ShellExecute),
        "git_read" => Some(Permission::GitRead),
        "git_write" => Some(Permission::GitWrite),
        "browser_access" => Some(Permission::BrowserAccess),
        "mcp_call" => Some(Permission::McpCall),
        "memory_search" => Some(Permission::MemorySearch),
        _ => None,
    }
}

pub(crate) async fn permission_settings_value(state: &AppState) -> Value {
    let mut settings = serde_json::Map::new();
    for permission in [
        Permission::FilesystemRead,
        Permission::FilesystemWrite,
        Permission::ShellExecute,
        Permission::GitRead,
        Permission::GitWrite,
        Permission::BrowserAccess,
        Permission::McpCall,
        Permission::MemorySearch,
    ] {
        settings.insert(
            permission_name(permission).to_string(),
            json!(state.permissions.mode(permission).await),
        );
    }
    Value::Object(settings)
}

pub(crate) async fn persist_permission_scopes(state: &AppState) -> Result<(), ApiError> {
    let snapshot = state.permissions.export_scopes().await;
    let value =
        serde_json::to_value(&snapshot).map_err(|error| ApiError::Internal(error.to_string()))?;
    evohime_storage::save_setting(&state.pool, "permission_scopes", &value)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    Ok(())
}

pub(crate) fn duration_to_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

pub(crate) fn validate_mcp_server(
    mut server: McpServerConfig,
) -> Result<McpServerConfig, ApiError> {
    server.name = server.name.trim().to_string();
    server.url = server.url.trim().to_string();
    if server.name.is_empty() {
        return Err(ApiError::BadRequest("mcp server name is required".into()));
    }

    let parsed = url::Url::parse(&server.url)
        .map_err(|error| ApiError::BadRequest(format!("invalid MCP server url: {error}")))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(ApiError::BadRequest(
                "mcp server url must use http or https".into(),
            ))
        }
    }

    if let Some(description) = server.description.as_mut() {
        let trimmed = description.trim().to_string();
        if trimmed.is_empty() {
            server.description = None;
        } else {
            *description = trimmed;
        }
    }

    Ok(server)
}
