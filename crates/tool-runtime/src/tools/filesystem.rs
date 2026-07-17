use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::fs;

pub const NAME: &str = "filesystem.read";
pub const DESCRIPTION: &str = "Read a UTF-8 text file from the workspace";
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct Input {
    path: String,
}

pub async fn execute(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(input).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;

    let resolved = ctx.sandbox()?.resolve_existing(&input.path)?;
    let content = fs::read_to_string(&resolved)
        .await
        .map_err(|error| ToolError::Execution(format!("read failed: {error}")))?;

    let display = truncate_for_display(&content);
    Ok(ToolResult {
        output: display.clone(),
        structured: json!({
            "path": resolved.display().to_string(),
            "bytes": content.len(),
            "preview": display,
        }),
    })
}

fn truncate_for_display(content: &str) -> String {
    let mut lines = content.lines().take(8).collect::<Vec<_>>().join("\n");
    if content.lines().count() > 8 {
        lines.push_str("\n...");
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn reads_file_inside_workspace() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("demo.txt");
        std_fs::write(&file_path, "hello\nworld").expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
        };
        let result = execute(&ctx, json!({ "path": "demo.txt" }))
            .await
            .expect("read succeeds");

        assert!(result.output.contains("hello"));
        assert_eq!(result.structured["bytes"], 11);
    }

    #[tokio::test]
    async fn rejects_path_outside_workspace() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
        };

        let error = execute(&ctx, json!({ "path": ".." }))
            .await
            .expect_err("path traversal blocked");

        match error {
            ToolError::PermissionDenied(Permission::FilesystemRead) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
