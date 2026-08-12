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
        input: Value,
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
            name: tools::browser_session::NAVIGATE_NAME,
            description: tools::browser_session::NAVIGATE_DESCRIPTION,
            permissions: tools::browser_session::NAVIGATE_PERMISSIONS,
            timeout: tools::browser_session::NAVIGATE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::READ_NAME,
            description: tools::browser_session::READ_DESCRIPTION,
            permissions: tools::browser_session::READ_PERMISSIONS,
            timeout: tools::browser_session::READ_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::CLICK_NAME,
            description: tools::browser_session::CLICK_DESCRIPTION,
            permissions: tools::browser_session::CLICK_PERMISSIONS,
            timeout: tools::browser_session::CLICK_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::SCREENSHOT_NAME,
            description: tools::browser_session::SCREENSHOT_DESCRIPTION,
            permissions: tools::browser_session::SCREENSHOT_PERMISSIONS,
            timeout: tools::browser_session::SCREENSHOT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::TYPE_NAME,
            description: tools::browser_session::TYPE_DESCRIPTION,
            permissions: tools::browser_session::TYPE_PERMISSIONS,
            timeout: tools::browser_session::TYPE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::CLOSE_NAME,
            description: tools::browser_session::CLOSE_DESCRIPTION,
            permissions: tools::browser_session::CLOSE_PERMISSIONS,
            timeout: tools::browser_session::CLOSE_TIMEOUT,
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

    pub async fn execute_with_cancellation(
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

        if name == tools::patch::NAME {
            tools::patch::validate_input(&input)?;
        }

        let scope = scope_from_input(name, &input);
        let command = command_from_input(name, &input);
        for permission in definition.permissions {
            match self
                .permissions
                .check_scoped(
                    *permission,
                    &evohime_permissions::PermissionCheck {
                        session_id: ctx.session_id,
                        path: Some(scope.as_str()),
                        command: command.as_deref(),
                    },
                )
                .await
            {
                PermissionDecision::Allowed => {}
                PermissionDecision::Denied => return Err(ToolError::PermissionDenied(*permission)),
                PermissionDecision::NeedsApproval => {
                    let approval = self
                        .permissions
                        .create_approval_scoped_for_call_with_command(
                            ctx.task_id,
                            ctx.session_id,
                            name,
                            *permission,
                            scope,
                            command.clone(),
                            &input,
                        )
                        .await;
                    return Err(ToolError::NeedsApproval {
                        tool: name.to_string(),
                        permission: *permission,
                        scope: approval.scope,
                        approval_id: approval.id,
                        input: input.clone(),
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
                tools::agent::NAME => tools::agent::execute(ctx, input).await,
                tools::browser::OPEN_NAME => tools::browser::open(ctx, input).await,
                tools::browser::EXTRACT_NAME => tools::browser::extract(ctx, input).await,
                tools::browser_session::NAVIGATE_NAME => {
                    tools::browser_session::navigate(ctx, input).await
                }
                tools::browser_session::READ_NAME => tools::browser_session::read(ctx, input).await,
                tools::browser_session::CLICK_NAME => {
                    tools::browser_session::click(ctx, input).await
                }
                tools::browser_session::SCREENSHOT_NAME => {
                    tools::browser_session::screenshot(ctx, input).await
                }
                tools::browser_session::TYPE_NAME => {
                    tools::browser_session::type_text(ctx, input).await
                }
                tools::browser_session::CLOSE_NAME => {
                    tools::browser_session::close(ctx, input).await
                }
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

    pub async fn execute_after_approval(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
        approval_id: Uuid,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        let definition = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;
        let scope = scope_from_input(name, &input);
        let command = command_from_input(name, &input);
        for permission in definition.permissions {
            match self
                .permissions
                .approval_matches_call(
                    approval_id,
                    ctx.task_id,
                    ctx.session_id,
                    name,
                    *permission,
                    &scope,
                    &input,
                )
                .await
            {
                Some(evohime_permissions::ApprovalState::Granted) => {}
                Some(evohime_permissions::ApprovalState::Denied) => {
                    return Err(ToolError::PermissionDenied(*permission));
                }
                Some(evohime_permissions::ApprovalState::Pending) | None => {
                    return Err(ToolError::Execution(
                        "approval is not granted for this call".to_string(),
                    ));
                }
            }
            if matches!(
                self.permissions
                    .check_scoped(
                        *permission,
                        &evohime_permissions::PermissionCheck {
                            session_id: ctx.session_id,
                            path: Some(scope.as_str()),
                            command: command.as_deref(),
                        },
                    )
                    .await,
                evohime_permissions::PermissionDecision::Denied
            ) {
                return Err(ToolError::PermissionDenied(*permission));
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
                tools::agent::NAME => tools::agent::execute(ctx, input).await,
                tools::browser::OPEN_NAME => tools::browser::open(ctx, input).await,
                tools::browser::EXTRACT_NAME => tools::browser::extract(ctx, input).await,
                tools::browser_session::NAVIGATE_NAME => {
                    tools::browser_session::navigate(ctx, input).await
                }
                tools::browser_session::READ_NAME => tools::browser_session::read(ctx, input).await,
                tools::browser_session::CLICK_NAME => {
                    tools::browser_session::click(ctx, input).await
                }
                tools::browser_session::SCREENSHOT_NAME => {
                    tools::browser_session::screenshot(ctx, input).await
                }
                tools::browser_session::TYPE_NAME => {
                    tools::browser_session::type_text(ctx, input).await
                }
                tools::browser_session::CLOSE_NAME => {
                    tools::browser_session::close(ctx, input).await
                }
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

    pub fn permissions(&self) -> &PermissionEngine {
        &self.permissions
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::bootstrap()
    }
}

/// Derive a stable scope key from tool input (path, cwd, url, or `"workspace"`).
fn scope_from_input(tool_name: &str, input: &Value) -> String {
    if tool_name == tools::shell::NAME {
        if let Some((_, _, cwd)) = tools::shell::resolve_invocation(input) {
            if let Some(cwd) = cwd {
                return cwd.replace('\\', "/");
            }
        }
    }
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

/// Build the subject matched by permission rules.
fn command_from_input(tool_name: &str, input: &Value) -> Option<String> {
    if tool_name == tools::shell::NAME {
        let (program, args, _) = tools::shell::resolve_invocation(input)?;
        return Some(
            std::iter::once(program)
                .chain(args)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let command = match tool_name {
        tools::git::STATUS_NAME => "git status",
        tools::git::DIFF_NAME => "git diff",
        tools::git::COMMIT_NAME => "git commit",
        tools::git::PULL_NAME => "git pull",
        tools::git::PUSH_NAME => "git push",
        _ => return None,
    };
    Some(command.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_registers_filesystem_read() {
        let registry = ToolRegistry::bootstrap();
        let tools = registry.list();
        assert_eq!(tools.len(), 23);
        assert_eq!(tools[0].name, "agent.run");
        assert_eq!(tools[1].name, "browser.extract");
        assert_eq!(tools[2].name, "browser.open");
        assert_eq!(tools[3].name, "browser.session.click");
        assert_eq!(tools[4].name, "browser.session.close");
        assert_eq!(tools[5].name, "browser.session.navigate");
        assert_eq!(tools[6].name, "browser.session.read");
        assert_eq!(tools[7].name, "browser.session.screenshot");
        assert_eq!(tools[8].name, "browser.session.type");
        assert_eq!(tools[9].name, "filesystem.list");
        assert_eq!(tools[10].name, "filesystem.patch");
        assert_eq!(tools[11].name, "filesystem.read");
        assert_eq!(tools[12].name, "filesystem.search");
        assert_eq!(tools[13].name, "filesystem.write");
        assert_eq!(tools[14].name, "git.commit");
        assert_eq!(tools[15].name, "git.diff");
        assert_eq!(tools[16].name, "git.pull");
        assert_eq!(tools[17].name, "git.push");
        assert_eq!(tools[18].name, "git.status");
        assert_eq!(tools[19].name, "http.fetch");
        assert_eq!(tools[20].name, "mcp.call");
        assert_eq!(tools[21].name, "memory.search");
        assert_eq!(tools[22].name, "shell.execute");
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
    async fn policy_denies_shell_subject_after_cd_prefix_is_resolved() {
        let permissions = evohime_permissions::PermissionEngine::new();
        permissions
            .set_policy_rules(evohime_permissions::PolicyRuleSet::new(vec![
                evohime_permissions::PolicyRule {
                    permission: evohime_permissions::Permission::ShellExecute,
                    pattern: "rm *".into(),
                    mode: evohime_permissions::PermissionMode::Deny,
                },
            ]))
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("workspace");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let error = registry
            .execute(
                &context,
                "shell.execute",
                serde_json::json!({"command": "cd nested && rm -rf target"}),
            )
            .await
            .expect_err("policy deny must happen before approval or spawn");
        assert!(matches!(
            error,
            ToolError::PermissionDenied(Permission::ShellExecute)
        ));
    }

    #[tokio::test]
    async fn policy_uses_a_separate_subject_for_git_push() {
        let permissions = evohime_permissions::PermissionEngine::new();
        permissions
            .set_policy_rules(evohime_permissions::PolicyRuleSet::new(vec![
                evohime_permissions::PolicyRule {
                    permission: evohime_permissions::Permission::GitWrite,
                    pattern: "git push*".into(),
                    mode: evohime_permissions::PermissionMode::Deny,
                },
            ]))
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("workspace");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let error = registry
            .execute(
                &context,
                "git.push",
                serde_json::json!({"remote": "origin", "branch": "main"}),
            )
            .await
            .expect_err("git subject policy must deny before git runs");
        assert!(matches!(
            error,
            ToolError::PermissionDenied(Permission::GitWrite)
        ));
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

    #[tokio::test]
    async fn approval_is_bound_to_exact_call_and_rechecks_deny() {
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::FilesystemWrite,
                evohime_permissions::PermissionMode::Ask,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions.clone());
        let dir = tempfile::tempdir().expect("tempdir");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::new_v4(),
            session_id: Some(Uuid::new_v4()),
            progress_tx: None,
        };
        let original = serde_json::json!({
            "path": "notes/todo.txt",
            "content": "approved"
        });
        let approval_id = match registry
            .execute(&context, "filesystem.write", original.clone())
            .await
            .expect_err("ask mode should require approval")
        {
            ToolError::NeedsApproval { approval_id, .. } => approval_id,
            other => panic!("expected NeedsApproval, got {other:?}"),
        };
        permissions
            .resolve(approval_id, true)
            .await
            .expect("granted");

        let changed = serde_json::json!({
            "path": "notes/todo.txt",
            "content": "tampered"
        });
        assert!(matches!(
            registry
                .execute_after_approval(
                    &context,
                    "filesystem.write",
                    changed,
                    approval_id,
                    CancellationToken::new(),
                )
                .await,
            Err(ToolError::Execution(message)) if message.contains("this call")
        ));
        assert!(!dir.path().join("notes/todo.txt").exists());

        permissions
            .set_mode(
                evohime_permissions::Permission::FilesystemWrite,
                evohime_permissions::PermissionMode::Deny,
            )
            .await;
        assert!(matches!(
            registry
                .execute_after_approval(
                    &context,
                    "filesystem.write",
                    original,
                    approval_id,
                    CancellationToken::new(),
                )
                .await,
            Err(ToolError::PermissionDenied(
                evohime_permissions::Permission::FilesystemWrite
            ))
        ));
        assert!(!dir.path().join("notes/todo.txt").exists());
    }

    #[tokio::test]
    async fn denied_approval_is_rejected_when_rechecked() {
        let permissions = PermissionEngine::new();
        let input = serde_json::json!({ "path": "notes/todo.txt", "content": "x" });
        let request = permissions
            .create_approval_scoped_for_call(
                Uuid::new_v4(),
                None,
                "filesystem.write",
                evohime_permissions::Permission::FilesystemWrite,
                "notes/todo.txt",
                &input,
            )
            .await;
        permissions
            .resolve(request.id, false)
            .await
            .expect("denied");
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: request.task_id,
            session_id: request.session_id,
            progress_tx: None,
        };

        assert!(matches!(
            registry
                .execute_after_approval(
                    &context,
                    "filesystem.write",
                    input,
                    request.id,
                    CancellationToken::new(),
                )
                .await,
            Err(ToolError::PermissionDenied(
                evohime_permissions::Permission::FilesystemWrite
            ))
        ));
        assert!(!dir.path().join("notes/todo.txt").exists());
    }

    #[tokio::test]
    async fn oversized_patch_is_rejected_before_approval() {
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(
                evohime_permissions::Permission::FilesystemWrite,
                evohime_permissions::PermissionMode::Ask,
            )
            .await;
        let registry = ToolRegistry::bootstrap_with_permissions(permissions);
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("file.txt"), "old\n").expect("write fixture");
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: Some(Uuid::new_v4()),
            progress_tx: None,
        };

        let error = registry
            .execute(
                &context,
                "filesystem.patch",
                serde_json::json!({
                    "path": "file.txt",
                    "patch": "a".repeat(tools::patch::MAX_PATCH_BYTES + 1)
                }),
            )
            .await
            .expect_err("preflight must reject oversized input");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }
}
