use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::Value;
use std::time::Duration;

/// Registry identifier for reading the Core-owned workflow template catalog.
pub const CATALOG_NAME: &str = "workflow_catalog";
/// Registry identifier for requesting a fixed Core-owned workflow template.
pub const START_NAME: &str = "workflow_start";
/// Registry identifier for reading a workflow run in the current workspace.
pub const STATUS_NAME: &str = "workflow_status";
/// Registry identifier for cooperatively cancelling a run in the current workspace.
pub const CANCEL_NAME: &str = "workflow_cancel";

const PERMISSIONS: &[Permission] = &[];
const TIMEOUT: Duration = Duration::from_secs(10);

/// Fails closed when a workflow call is made outside Core's agent runtime.
///
/// The Core agent intercepts these calls and applies workspace ownership,
/// graph validation, durable state, and cancellation policy. This registry
/// adapter deliberately cannot execute workflows on its own.
pub async fn execute(_ctx: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution(
        "workflow tools require the Core agent workflow runtime".into(),
    ))
}

pub(crate) fn definition(name: &'static str) -> crate::ToolDefinition {
    crate::ToolDefinition {
        name,
        description: match name {
            CATALOG_NAME => "List fixed Core workflow templates and their runtime availability",
            START_NAME => "Request a fixed Core workflow template after Core validation",
            STATUS_NAME => "Read bounded status for a workflow run in the current workspace",
            _ => "Request cooperative cancellation of a workflow run in the current workspace",
        },
        permissions: PERMISSIONS,
        timeout: TIMEOUT,
    }
}
