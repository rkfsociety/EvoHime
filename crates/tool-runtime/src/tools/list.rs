use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::json;
use std::{io::ErrorKind, time::Duration};
use tokio::fs;

pub const NAME: &str = "filesystem.list";
pub const DESCRIPTION: &str = "List files and directories in the workspace";
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct Input {
    #[serde(default = "default_path")]
    path: String,
}

fn default_path() -> String {
    ".".to_string()
}

pub async fn execute(ctx: &ToolContext, value: serde_json::Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;
    let directory = ctx.sandbox()?.resolve_existing(&input.path)?;
    if !directory.is_dir() {
        return Err(ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: "path must be a directory".to_string(),
        });
    }

    let mut entries = fs::read_dir(&directory)
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => ToolError::NotFound {
                tool: NAME.to_string(),
                path: input.path.clone(),
                hint: String::new(),
            },
            _ => ToolError::Execution(format!("list failed: {error}")),
        })?;
    let mut names = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| ToolError::Execution(format!("list failed: {error}")))?
    {
        names.push(entry.file_name().to_string_lossy().to_string());
    }
    names.sort();

    Ok(ToolResult {
        output: names.join("\n"),
        structured: json!({ "path": input.path, "entries": names }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[tokio::test]
    async fn lists_files_in_directory() {
        let dir = tempdir().expect("tempdir");
        std_fs::write(dir.path().join("file1.txt"), "content1").expect("write");
        std_fs::write(dir.path().join("file2.txt"), "content2").expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = execute(&ctx, json!({})).await.expect("list succeeds");

        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
    }
}
