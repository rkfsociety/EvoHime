use evohime_permissions::{Permission, PermissionDecision, PermissionEngine};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, time::Duration};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

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
pub struct ToolContext {
    pub workspace_root: PathBuf,
}

impl ToolContext {
    pub fn sandbox(&self) -> Result<crate::WorkspaceSandbox, ToolError> {
        crate::WorkspaceSandbox::new(&self.workspace_root)
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
        let mut registry = Self::new();
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
        registry
    }

    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            permissions: PermissionEngine::new(),
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
        let definition = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;

        for permission in definition.permissions {
            match self.permissions.check(*permission).await {
                PermissionDecision::Allowed => {}
                PermissionDecision::Denied => return Err(ToolError::PermissionDenied(*permission)),
                PermissionDecision::NeedsApproval => {
                    let approval = self
                        .permissions
                        .create_approval(uuid::Uuid::nil(), name, *permission, "workspace")
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
                tools::shell::NAME => {
                    tools::shell::execute(ctx, input, CancellationToken::new()).await
                }
                tools::git::STATUS_NAME => tools::git::status(ctx, input).await,
                tools::git::DIFF_NAME => tools::git::diff(ctx, input).await,
                tools::git::COMMIT_NAME => tools::git::commit(ctx, input).await,
                tools::git::PULL_NAME => tools::git::pull(ctx, input).await,
                tools::git::PUSH_NAME => tools::git::push(ctx, input).await,
                _ => Err(ToolError::UnknownTool(name.to_string())),
            }
        };

        match tokio::time::timeout(definition.timeout, execution).await {
            Ok(result) => result,
            Err(_) => Err(ToolError::TimedOut(definition.timeout)),
        }
    }

    pub async fn execute_cancellable(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        tokio::select! {
            _ = cancellation.cancelled() => Err(ToolError::Execution("tool cancelled".to_string())),
            result = self.execute(ctx, name, input) => result,
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
            async move { self.execute_cancellable(ctx, &name, input, token).await }
        });
        futures_util::future::join_all(futures).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::bootstrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_registers_filesystem_read() {
        let registry = ToolRegistry::bootstrap();
        let tools = registry.list();
        assert_eq!(tools.len(), 10);
        assert_eq!(tools[0].name, "filesystem.patch");
        assert_eq!(tools[1].name, "filesystem.read");
        assert_eq!(tools[2].name, "filesystem.search");
        assert_eq!(tools[3].name, "filesystem.write");
        assert_eq!(tools[4].name, "git.commit");
        assert_eq!(tools[5].name, "git.diff");
        assert_eq!(tools[6].name, "git.pull");
        assert_eq!(tools[7].name, "git.push");
        assert_eq!(tools[8].name, "git.status");
        assert_eq!(tools[9].name, "shell.execute");
    }

    #[tokio::test]
    async fn parallel_calls_complete_independently() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "a").expect("write a");
        std::fs::write(dir.path().join("b.txt"), "b").expect("write b");
        let registry = ToolRegistry::bootstrap();
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
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
        let registry = ToolRegistry::bootstrap();
        let context = ToolContext {
            workspace_root: dir.path().to_path_buf(),
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
}
