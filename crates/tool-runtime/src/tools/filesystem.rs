use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

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

    let (file_ref, content) = crate::revision_safe_workspace_files::read(ctx, &input.path)
        .await
        .map_err(|error| {
            if matches!(error, crate::revision_safe_workspace_files::RevisionError::Io(ref io) if io.kind() == std::io::ErrorKind::NotFound) {
                return ToolError::NotFound { tool: NAME.into(), path: input.path.clone(), hint: String::new() };
            }
            crate::revision_safe_workspace_files::permission(
                error,
                NAME,
                Permission::FilesystemRead,
            )
        })?;

    let display = truncate_for_display(&content);
    Ok(ToolResult {
        output: display.clone(),
        structured: json!({
            "path": input.path,
            "bytes": content.len(),
            "content_hash": file_ref.content_hash,
            "revision": file_ref.revision,
            "namespace": file_ref.namespace.as_str(),
            "preview": display,
        }),
    })
}

fn truncate_for_display(content: &str) -> String {
    let mut lines = content.lines().take(200).collect::<Vec<_>>().join("\n");
    if content.lines().count() > 200 {
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
            progress_tx: None,
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
            progress_tx: None,
        };

        let error = execute(&ctx, json!({ "path": ".." }))
            .await
            .expect_err("path traversal blocked");

        match error {
            ToolError::PermissionDenied(Permission::FilesystemRead) => {}
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn classifies_missing_file_as_not_found() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let error = execute(&ctx, json!({ "path": "missing.txt" }))
            .await
            .expect_err("missing file must fail");
        assert!(matches!(
            error,
            ToolError::NotFound { ref tool, ref path, .. }
                if tool == NAME && path == "missing.txt"
        ));
    }
}
