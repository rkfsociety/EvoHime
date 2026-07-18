use evohime_permissions::{Permission, PermissionDecision, PermissionEngine};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, time::Duration};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::tools;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid input for {tool}: {message}")]
    InvalidInput { tool: String, message: String },
    #[error("permission denied: {0:?}")]
    PermissionDenied(Permission),
    #[error("approval required for {tool}: {approval_id}")]
    NeedsApproval {
        tool: String,
        permission: Permission,
        scope: String,
        approval_id: uuid::Uuid,
    },
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("tool timed out after {0:?}")]
    TimedOut(Duration),
}

#[derive(Debug, Clone)]
pub struct ToolProgress {
    pub stream: &'static str,
    pub delta: String,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_root: PathBuf,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    /// Optional live progress channel (e.g. shell stdout/stderr chunks).
    pub progress_tx: Option<tokio::sync::mpsc::UnboundedSender<ToolProgress>>,
}

impl ToolContext {
    pub fn sandbox(&self) -> Result<crate::WorkspaceSandbox, ToolError> {
        crate::WorkspaceSandbox::new(&self.workspace_root)
    }

    pub fn emit_progress(&self, stream: &'static str, delta: impl Into<String>) {
        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(ToolProgress {
                stream,
                delta: delta.into(),
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub structured: Value,
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub permissions: &'static [Permission],
    pub timeout: Duration,
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<&'static str, ToolDefinition>,
    permissions: PermissionEngine,
}

impl ToolRegistry {
    pub fn bootstrap() -> Self {
        Self::bootstrap_with_permissions(PermissionEngine::new())
    }

    pub fn bootstrap_with_permissions(permissions: PermissionEngine) -> Self {
        let mut registry = Self::with_permissions(permissions);
        registry.register(ToolDefinition {
            name: tools::agent::NAME,
            description: tools::agent::DESCRIPTION,
            permissions: tools::agent::PERMISSIONS,
            timeout: tools::agent::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::filesystem::NAME,
            description: tools::filesystem::DESCRIPTION,
            permissions: tools::filesystem::PERMISSIONS,
            timeout: tools::filesystem::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::write::NAME,
            description: tools::write::DESCRIPTION,
            permissions: tools::write::PERMISSIONS,
            timeout: tools::write::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::patch::NAME,
            description: tools::patch::DESCRIPTION,
            permissions: tools::patch::PERMISSIONS,
            timeout: tools::patch::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::search::NAME,
            description: tools::search::DESCRIPTION,
            permissions: tools::search::PERMISSIONS,
            timeout: tools::search::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::list::NAME,
            description: tools::list::DESCRIPTION,
            permissions: tools::list::PERMISSIONS,
            timeout: tools::list::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::shell::NAME,
            description: tools::shell::DESCRIPTION,
            permissions: tools::shell::PERMISSIONS,
            timeout: tools::shell::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::STATUS_NAME,
            description: tools::git::STATUS_DESCRIPTION,
            permissions: tools::git::STATUS_PERMISSIONS,
            timeout: tools::git::STATUS_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::DIFF_NAME,
            description: tools::git::DIFF_DESCRIPTION,
            permissions: tools::git::DIFF_PERMISSIONS,
            timeout: tools::git::DIFF_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::COMMIT_NAME,
            description: tools::git::COMMIT_DESCRIPTION,
            permissions: tools::git::COMMIT_PERMISSIONS,
            timeout: tools::git::COMMIT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::PULL_NAME,
            description: tools::git::PULL_DESCRIPTION,
            permissions: tools::git::PULL_PERMISSIONS,
            timeout: tools::git::PULL_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::PUSH_NAME,
            description: tools::git::PUSH_DESCRIPTION,
            permissions: tools::git::PUSH_PERMISSIONS,
            timeout: tools::git::PUSH_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::mcp::NAME,
            description: tools::mcp::DESCRIPTION,
            permissions: tools::mcp::PERMISSIONS,
            timeout: tools::mcp::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::memory::NAME,
            description: tools::memory::DESCRIPTION,
            permissions: tools::memory::PERMISSIONS,
            timeout: tools::memory::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::worker::NAME,
            description: tools::worker::DESCRIPTION,
            permissions: tools::worker::PERMISSIONS,
            timeout: tools::worker::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser::OPEN_NAME,
            description: tools::browser::OPEN_DESCRIPTION,
            permissions: tools::browser::OPEN_PERMISSIONS,
            timeout: tools::browser::OPEN_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser::EXTRACT_NAME,
            description: tools::browser::EXTRACT_DESCRIPTION,
            permissions: tools::browser::EXTRACT_PERMISSIONS,
            timeout: tools::browser::EXTRACT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::http::NAME,
            description: tools::http::DESCRIPTION,
            permissions: tools::http::PERMISSIONS,
            timeout: tools::http::TIMEOUT,
        });
        registry
    }

    pub fn new() -> Self {
        Self::with_permissions(PermissionEngine::new())
    }

    pub fn with_permissions(permissions: PermissionEngine) -> Self {
        Self {
            tools: HashMap::new(),
            permissions,
        }
    }

    pub fn register(&mut self, definition: ToolDefinition) {
        self.tools.insert(definition.name, definition);
    }

    pub fn list(&self) -> Vec<&ToolDefinition> {
        let mut items: Vec<_> = self.tools.values().collect();
        items.sort_by_key(|tool| tool.name);
        items
    }

    pub async fn execute(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
    ) -> Result<ToolResult, ToolError> {
        self.execute_with_cancellation(ctx, name, input, CancellationToken::new())
            .await
    }

    async fn execute_with_cancellation(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        let definition = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;

        for permission in definition.permissions {
            let scope = scope_from_input(name, &input);
            match self
                .permissions
                .check_scoped(
                    *permission,
                    &evohime_permissions::PermissionCheck {
                        session_id: ctx.session_id,
                        path: Some(scope.as_str()),
                    },
                )
                .await
            {
                PermissionDecision::Allowed => {}
                PermissionDecision::Denied => return Err(ToolError::PermissionDenied(*permission)),
                PermissionDecision::NeedsApproval => {
                    let approval = self
                        .permissions
                        .create_approval_scoped(
                            ctx.task_id,
                            ctx.session_id,
                            name,
                            *permission,
                            scope,
                        )
                        .await;
                    return Err(ToolError::NeedsApproval {
                        tool: name.to_string(),
                        permission: *permission,
                        scope: approval.scope,
                        approval_id: approval.id,
                    });
                }
            }
        }

        let execution = async {
            match name {
                tools::filesystem::NAME => tools::filesystem::execute(ctx, input).await,
                tools::write::NAME => tools::write::execute(ctx, input).await,
                tools::patch::NAME => tools::patch::execute(ctx, input).await,
                tools::search::NAME => tools::search::execute(ctx, input).await,
                tools::list::NAME => tools::list::execute(ctx, input).await,
                tools::shell::NAME => tools::shell::execute(ctx, input, cancellation.clone()).await,
                tools::git::STATUS_NAME => tools::git::status(ctx, input).await,
                tools::git::DIFF_NAME => tools::git::diff(ctx, input).await,
                tools::git::COMMIT_NAME => tools::git::commit(ctx, input).await,
                tools::git::PULL_NAME => tools::git::pull(ctx, input).await,
                tools::git::PUSH_NAME => tools::git::push(ctx, input).await,
                tools::mcp::NAME => tools::mcp::execute(ctx, input).await,
                tools::memory::NAME => tools::memory::execute(ctx, input).await,
                tools::worker::NAME => tools::worker::execute(ctx, input).await,
                tools::agent::NAME => tools::agent::execute(ctx, input).await,
                tools::browser::OPEN_NAME => tools::browser::open(ctx, input).await,
                tools::browser::EXTRACT_NAME => tools::browser::extract(ctx, input).await,
                tools::http::NAME => tools::http::fetch(ctx, input).await,
                _ => Err(ToolError::UnknownTool(name.to_string())),
            }
        };

        tokio::select! {
            _ = cancellation.cancelled() => Err(ToolError::Execution("tool cancelled".to_string())),
            result = tokio::time::timeout(definition.timeout, execution) => match result {
                Ok(result) => result,
                Err(_) => Err(ToolError::TimedOut(definition.timeout)),
            },
        }
    }

    pub async fn execute_cancellable(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        let cancellation_wait = cancellation.clone();
        tokio::select! {
            _ = cancellation_wait.cancelled() => Err(ToolError::Execution("tool cancelled".to_string())),
            result = self.execute_with_cancellation(ctx, name, input, cancellation) => result,
        }
    }

    pub async fn execute_parallel(
        &self,
        ctx: &ToolContext,
        calls: Vec<(String, Value)>,
        cancellation: CancellationToken,
    ) -> Vec<Result<ToolResult, ToolError>> {
        let futures = calls.into_iter().map(|(name, input)| {
            let token = cancellation.clone();
            async move {
                self.execute_with_cancellation(ctx, &name, input, token)
                    .await
            }
        });
        futures_util::future::join_all(futures).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::bootstrap()
    }
}

/// Derive a stable scope key from tool input (path, cwd, url, or `"workspace"`).
fn scope_from_input(tool_name: &str, input: &Value) -> String {
    if let Some(path) = input.get("path").and_then(Value::as_str) {
        return path.replace('\\', "/");
    }
    if let Some(cwd) = input.get("cwd").and_then(Value::as_str) {
        return cwd.replace('\\', "/");
    }
    if let Some(url) = input.get("url").and_then(Value::as_str) {
        return url.to_string();
    }
    if tool_name.starts_with("browser.") {
        return "browser".to_string();
    }
    "workspace".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_registers_filesystem_read() {
        let registry = ToolRegistry::bootstrap();
        let tools = registry.list();
        assert_eq!(tools.len(), 18);
        assert_eq!(tools[0].name, "agent.run");
        assert_eq!(tools[1].name, "browser.extract");
        assert_eq!(tools[2].name, "browser.open");
        assert_eq!(tools[3].name, "filesystem.list");
        assert_eq!(tools[4].name, "filesystem.patch");
        assert_eq!(tools[5].name, "filesystem.read");
        assert_eq!(tools[6].name, "filesystem.search");
        assert_eq!(tools[7].name, "filesystem.write");
        assert_eq!(tools[8].name, "git.commit");
        assert_eq!(tools[9].name, "git.diff");
        assert_eq!(tools[10].name, "git.pull");
        assert_eq!(tools[11].name, "git.push");
        assert_eq!(tools[12].name, "git.status");
        assert_eq!(tools[13].name, "http.fetch");
        assert_eq!(tools[14].name, "mcp.call");
        assert_eq!(tools[15].name, "memory.search");
        assert_eq!(tools[16].name, "shell.execute");
        assert_eq!(tools[17].name, "worker.run");
    }

    #[tokio::test]
    async fn parallel_calls_complete_independently() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "a").expect("write a");
        std::fs::write(dir.path().join("b.txt"), "b").expect("write b");
        let registry = ToolRegistry::bootstrap();
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let results = registry
            .execute_parallel(
                &context,
                vec![
                    (
                        "filesystem.read".into(),
                        serde_json::json!({"path":"a.txt"}),
                    ),
                    (
                        "filesystem.read".into(),
                        serde_json::json!({"path":"b.txt"}),
                    ),
                ],
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(Result::is_ok));
    }

    #[tokio::test]
    async fn cancellation_stops_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "a").expect("write");
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::ShellExecute,
                evohime_permissions::PermissionMode::Allow,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();
        let result = registry
            .execute_cancellable(
                &context,
                "filesystem.read",
                serde_json::json!({"path":"a.txt"}),
                token,
            )
            .await;
        assert!(
            matches!(result, Err(ToolError::Execution(message)) if message == "tool cancelled")
        );
    }

    #[tokio::test]
    async fn cancellation_propagates_into_shell_execution() {
        let dir = tempfile::tempdir().expect("tempdir");
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::ShellExecute,
                evohime_permissions::PermissionMode::Allow,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let token = CancellationToken::new();
        let (program, args) = if cfg!(windows) {
            ("ping", vec!["-n", "5", "127.0.0.1"])
        } else {
            ("sleep", vec!["2"])
        };
        let handle = tokio::spawn({
            let registry = registry.clone();
            let context = context.clone();
            let token = token.clone();
            async move {
                registry
                    .execute_cancellable(
                        &context,
                        "shell.execute",
                        serde_json::json!({"program":program,"args":args}),
                        token,
                    )
                    .await
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        token.cancel();
        let result = handle.await.expect("task join");
        assert!(
            matches!(result, Err(ToolError::Execution(message)) if message == "tool cancelled")
        );
    }

    #[tokio::test]
    async fn cancellation_stops_non_shell_tool_in_parallel_execution() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let _ssrf = crate::ssrf::lock_private_override(Some(true));
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("late")
                    .set_delay(std::time::Duration::from_secs(2)),
            )
            .mount(&server)
            .await;

        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::BrowserAccess,
                evohime_permissions::PermissionMode::Allow,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let token = CancellationToken::new();
        let cancel = token.clone();
        let handle = tokio::spawn(async move {
            registry
                .execute_parallel(
                    &context,
                    vec![(
                        "browser.open".into(),
                        serde_json::json!({ "url": format!("{}/slow", server.uri()) }),
                    )],
                    token,
                )
                .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();
        let results = handle.await.expect("parallel task");
        assert!(matches!(
            results.as_slice(),
            [Err(ToolError::Execution(message))] if message == "tool cancelled"
        ));
    }

    #[tokio::test]
    async fn registry_dispatches_browser_open_when_allowed() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let _ssrf = crate::ssrf::lock_private_override(Some(true));
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/page"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "<html><head><title>Hi</title></head><body><p>hello registry</p></body></html>",
            ))
            .mount(&server)
            .await;

        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::BrowserAccess,
                evohime_permissions::PermissionMode::Allow,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let result = registry
            .execute(
                &context,
                "browser.open",
                serde_json::json!({ "url": format!("{}/page", server.uri()) }),
            )
            .await
            .expect("browser.open should dispatch");
        assert!(result.output.to_lowercase().contains("hi") || result.output.contains("hello"));
    }

    #[test]
    fn scope_from_input_prefers_path_then_cwd_then_url() {
        assert_eq!(
            scope_from_input(
                "filesystem.write",
                &serde_json::json!({ "path": "src\\main.rs" })
            ),
            "src/main.rs"
        );
        assert_eq!(
            scope_from_input("shell.execute", &serde_json::json!({ "cwd": "scripts" })),
            "scripts"
        );
        assert_eq!(
            scope_from_input(
                "browser.open",
                &serde_json::json!({ "url": "https://example.com" })
            ),
            "https://example.com"
        );
        assert_eq!(
            scope_from_input("git.status", &serde_json::json!({})),
            "workspace"
        );
    }

    #[tokio::test]
    async fn ask_mode_creates_scoped_approval() {
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::FilesystemWrite,
                evohime_permissions::PermissionMode::Ask,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        let session_id = Uuid::new_v4();
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: Some(session_id),
            progress_tx: None,
        };
        let err = registry
            .execute(
                &context,
                "filesystem.write",
                serde_json::json!({ "path": "notes/todo.txt", "content": "x" }),
            )
            .await
            .expect_err("ask mode should require approval");
        match err {
            ToolError::NeedsApproval { scope, .. } => {
                assert_eq!(scope, "notes/todo.txt");
            }
            other => panic!("expected NeedsApproval, got {other:?}"),
        }
    }
}
