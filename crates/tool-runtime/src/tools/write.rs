use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::fs;

pub const NAME: &str = "filesystem.write";
pub const DESCRIPTION: &str = "Write UTF-8 text inside the workspace";
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
struct Input {
    path: String,
    content: String,
}

pub async fn execute(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|e| ToolError::InvalidInput {
        tool: NAME.into(),
        message: e.to_string(),
    })?;
    let path = ctx.sandbox()?.resolve_for_write(&input.path)?;
    let change = if fs::try_exists(&path)
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?
    {
        "updated"
    } else {
        "created"
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| ToolError::Execution(format!("create parent failed: {e}")))?;
    }
    fs::write(&path, input.content.as_bytes())
        .await
        .map_err(|e| ToolError::Execution(format!("write failed: {e}")))?;
    Ok(ToolResult {
        output: format!("{change} {}", input.path),
        structured: json!({"path": input.path, "bytes": input.content.len(), "change": change}),
    })
}
