use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub const NAME: &str = "filesystem.write";
pub const DESCRIPTION: &str = "Write UTF-8 text inside the workspace";
pub const PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
struct Input {
    path: String,
    content: String,
    #[serde(default)]
    expected_hash: Option<String>,
}

pub async fn execute(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|e| ToolError::InvalidInput {
        tool: NAME.into(),
        message: e.to_string(),
    })?;
    let existed = crate::revision_safe_workspace_files::read(ctx, &input.path)
        .await
        .is_ok();
    let file_ref = crate::revision_safe_workspace_files::write(
        ctx,
        &input.path,
        input.content.as_bytes(),
        input.expected_hash.as_deref(),
    )
    .await
    .map_err(|error| {
        crate::revision_safe_workspace_files::permission(error, NAME, Permission::FilesystemWrite)
    })?;
    let change = if existed { "updated" } else { "created" };
    Ok(ToolResult {
        output: format!("{change} {}", input.path),
        structured: json!({"path": input.path, "bytes": input.content.len(), "change": change, "content_hash": file_ref.content_hash, "revision": file_ref.revision, "namespace": file_ref.namespace.as_str(), "change_set": {"status": "observed", "path": file_ref.path}}),
    })
}
