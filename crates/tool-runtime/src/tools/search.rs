use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::process::Command;

pub const NAME: &str = "filesystem.search";
pub const DESCRIPTION: &str = "Search text in workspace files using ripgrep";
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Deserialize)]
struct Input {
    query: String,
    path: Option<String>,
    glob: Option<String>,
    limit: Option<usize>,
}

pub async fn execute(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|e| ToolError::InvalidInput {
        tool: NAME.into(),
        message: e.to_string(),
    })?;
    let base = match input.path.as_deref() {
        Some(path) => ctx.sandbox()?.resolve_existing(path)?,
        None => ctx.sandbox()?.root().to_path_buf(),
    };
    let mut command = Command::new("rg");
    command
        .args(["--json", "--no-heading", "--color", "never", &input.query])
        .arg(&base)
        .current_dir(ctx.sandbox()?.root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(glob) = input.glob.as_deref() {
        if glob.starts_with('-') {
            return Err(ToolError::InvalidInput {
                tool: NAME.into(),
                message: "invalid glob".into(),
            });
        }
        command.args(["--glob", glob]);
    }
    let output = command
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("ripgrep failed: {e}")))?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(ToolError::Execution(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    let limit = input.limit.unwrap_or(100).min(1000);
    let mut matches = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Ok(item) = serde_json::from_str::<Value>(line) {
            if item["type"] == "match" {
                for sub in item["data"]["lines"]["text"].as_str().unwrap_or("").lines() {
                    matches.push(json!({"path": item["data"]["path"]["text"], "line": item["data"]["line_number"], "text": sub}));
                    if matches.len() >= limit {
                        break;
                    }
                }
            }
        }
        if matches.len() >= limit {
            break;
        }
    }
    Ok(ToolResult {
        output: serde_json::to_string(&matches).unwrap_or_default(),
        structured: json!({"matches": matches, "count": matches.len()}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[tokio::test]
    async fn searches_recursively_and_honors_limit() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("nested")).expect("nested");
        std::fs::write(dir.path().join("a.txt"), "needle one").expect("write a");
        std::fs::write(dir.path().join("nested").join("b.md"), "needle two").expect("write b");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
        };

        let result = execute(
            &ctx,
            json!({
                "query": "needle",
                "limit": 1
            }),
        )
        .await
        .expect("search succeeds");

        assert_eq!(result.structured["count"], 1);
        assert_eq!(result.structured["matches"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn search_can_filter_by_glob() {
        let dir = tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("nested")).expect("nested");
        std::fs::write(dir.path().join("a.txt"), "needle one").expect("write a");
        std::fs::write(dir.path().join("nested").join("b.md"), "needle two").expect("write b");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
        };

        let result = execute(
            &ctx,
            json!({
                "query": "needle",
                "glob": "*.md"
            }),
        )
        .await
        .expect("search succeeds");

        let matches = result.structured["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["path"].as_str().unwrap().ends_with("b.md"));
    }

    #[tokio::test]
    async fn search_rejects_path_traversal() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
        };

        let error = execute(
            &ctx,
            json!({
                "query": "needle",
                "path": ".."
            }),
        )
        .await
        .expect_err("traversal rejected");

        assert!(matches!(
            error,
            ToolError::PermissionDenied(Permission::FilesystemRead)
        ));
    }
}
