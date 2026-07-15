use evohime_permissions::Permission;
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
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("tool timed out after {0:?}")]
    TimedOut(Duration),
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_root: PathBuf,
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

        let execution = match name {
            tools::filesystem::NAME => tools::filesystem::execute(ctx, input),
            tools::git::STATUS_NAME => tools::git::status(ctx, input),
            tools::git::DIFF_NAME => tools::git::diff(ctx, input),
            tools::git::COMMIT_NAME => tools::git::commit(ctx, input),
            tools::git::PULL_NAME => tools::git::pull(ctx, input),
            tools::git::PUSH_NAME => tools::git::push(ctx, input),
            _ => return Err(ToolError::UnknownTool(name.to_string())),
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
            self.execute_cancellable(ctx, &name, input, cancellation.clone())
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
        assert_eq!(tools.len(), 6);
        assert_eq!(tools[0].name, "filesystem.read");
        assert_eq!(tools[1].name, "git.commit");
        assert_eq!(tools[2].name, "git.diff");
        assert_eq!(tools[3].name, "git.pull");
        assert_eq!(tools[4].name, "git.push");
        assert_eq!(tools[5].name, "git.status");
    }

    #[tokio::test]
    async fn parallel_calls_complete_independently() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "a").expect("write a");
        std::fs::write(dir.path().join("b.txt"), "b").expect("write b");
        let registry = ToolRegistry::bootstrap();
        let context = ToolContext { workspace_root: dir.path().to_path_buf() };
        let results = registry.execute_parallel(
            &context,
            vec![("filesystem.read".into(), serde_json::json!({"path":"a.txt"})), ("filesystem.read".into(), serde_json::json!({"path":"b.txt"}))],
            tokio_util::sync::CancellationToken::new(),
        ).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(Result::is_ok));
    }

    #[tokio::test]
    async fn cancellation_stops_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "a").expect("write");
        let registry = ToolRegistry::bootstrap();
        let context = ToolContext { workspace_root: dir.path().to_path_buf() };
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();
        let result = registry.execute_cancellable(&context, "filesystem.read", serde_json::json!({"path":"a.txt"}), token).await;
        assert!(matches!(result, Err(ToolError::Execution(message)) if message == "tool cancelled"));
    }
}
