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
