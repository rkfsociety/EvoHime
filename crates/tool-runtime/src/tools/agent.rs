use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::{json, Value};
use std::time::Duration;

/// Registry identifier for agent subtask delegation.
pub const NAME: &str = "agent_run";
/// Registry identifier for Hive role-based agent delegation.
pub const HIVE_DELEGATE_NAME: &str = "hive_delegate_agent";
/// Registry identifier for registering a new Hive role / sector.
pub const HIVE_REGISTER_ROLE_NAME: &str = "hive_register_role";
/// Input contract and fan-out guidance shown in tool catalogs.
pub const DESCRIPTION: &str = "Spawn a budgeted read-only subagent with its own prompt. Input: { prompt, max_steps?, timeout_ms?, model_route? }. The subagent may read files, git history and memory, but cannot edit, run commands, browse, or delegate further. Fan-out by scheduling multiple agent_run steps without depends_on.";
/// Description for Hive role-based agent delegation.
pub const HIVE_DELEGATE_DESCRIPTION: &str = "Delegate a step to a specialized Hive worker role (researcher, planner, implementer, tester, reviewer, releaser, architect, or a registered custom role). Input: { role, prompt, agent_name?, department?, plan_summary?, scope?, acceptance_criteria?, max_steps?, timeout_ms?, model_route? }. Always set agent_name to a short human-readable Russian name (e.g. 'Исследователь ядра', 'Разработчик Улья'). Read-only roles (researcher, planner, architect, reviewer) run without user approval — use them for all file reading, search, link checks, git inspection, and planning. Only call mutating/verification roles (implementer, tester, releaser) AFTER research and planning are complete, providing a clear ready-to-execute plan_summary for user approval.";
/// Description for registering a new Hive agent role and sector.
pub const HIVE_REGISTER_ROLE_DESCRIPTION: &str = "Register a new specialized Hive agent role and department/sector for the city map. Input: { role, display_name, department, objective, capabilities?, strategy? }.";
/// No additional tool permission is checked by this registry adapter.
///
/// Core still enforces the subagent's read-only permission session and depth
/// limit before the nested agent loop starts.
pub const PERMISSIONS: &[Permission] = &[];
/// Maximum advertised wall-clock budget for agent delegation.
pub const TIMEOUT: Duration = Duration::from_secs(180);

/// Registry adapter for `agent_run`.
///
/// The real nested loop lives in Core (`ToolAgent::execute_agent_run`), which
/// intercepts the call before the registry. This adapter is the fail-closed
/// fallback for direct registry consumers and tool simulation.
pub async fn execute(_ctx: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution(
        "agent_run is executed by the Core agent loop".into(),
    ))
}

/// Validated inputs passed from the tool registry to the subagent backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunInput {
    /// Nonempty prompt after leading and trailing whitespace are removed.
    pub prompt: String,
    /// Optional step budget, clamped to 1 through 32 when supplied.
    pub max_steps: Option<usize>,
    /// Optional positive timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Optional nonempty model route override.
    pub model_route: Option<String>,
}

/// Parses and normalizes the JSON input for an agent subtask.
///
/// # Errors
///
/// Returns [`ToolError::InvalidInput`] if the prompt is missing or blank.
pub fn parse_input(input: &Value) -> Result<AgentRunInput, ToolError> {
    let prompt = input
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: "prompt is required".into(),
        })?
        .to_string();
    let max_steps = input
        .get("max_steps")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 32) as usize);
    let timeout_ms = input
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0);
    let model_route = input
        .get("model_route")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(AgentRunInput {
        prompt,
        max_steps,
        timeout_ms,
        model_route,
    })
}

/// Validated inputs for Hive role-based agent delegation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiveDelegateInput {
    /// Target role identifier (built-in or custom registered role).
    pub role: String,
    /// Nonempty task prompt for the delegated worker.
    pub prompt: String,
    /// Optional human-readable Russian name for this worker.
    pub agent_name: Option<String>,
    /// Optional department / sector name.
    pub department: Option<String>,
    /// User-facing plan summary for approval preview before mutations.
    pub plan_summary: Option<String>,
    /// Optional target file or module scope.
    pub scope: Option<String>,
    /// Optional acceptance criteria list.
    pub acceptance_criteria: Vec<String>,
    /// Optional step budget, clamped to 1 through 32 when supplied.
    pub max_steps: Option<usize>,
    /// Optional positive timeout in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Optional nonempty model route override.
    pub model_route: Option<String>,
}

/// Parses and normalizes JSON input for `hive_delegate_agent`.
///
/// # Errors
///
/// Returns [`ToolError::InvalidInput`] if `role` or `prompt` is missing or blank.
pub fn parse_hive_delegate_input(input: &Value) -> Result<HiveDelegateInput, ToolError> {
    let opt_str = |key: &str| -> Option<String> {
        input
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let role = opt_str("role")
        .ok_or_else(|| ToolError::InvalidInput {
            tool: HIVE_DELEGATE_NAME.to_string(),
            message: "role is required".into(),
        })?
        .to_lowercase();
    let prompt = opt_str("prompt").ok_or_else(|| ToolError::InvalidInput {
        tool: HIVE_DELEGATE_NAME.to_string(),
        message: "prompt is required".into(),
    })?;
    let acceptance_criteria = input
        .get("acceptance_criteria")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .take(8)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let max_steps = input
        .get("max_steps")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 32) as usize);
    let timeout_ms = input
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0);
    Ok(HiveDelegateInput {
        role,
        prompt,
        agent_name: opt_str("agent_name"),
        department: opt_str("department"),
        plan_summary: opt_str("plan_summary"),
        scope: opt_str("scope"),
        acceptance_criteria,
        max_steps,
        timeout_ms,
        model_route: opt_str("model_route"),
    })
}

/// Validated inputs for registering a new Hive role and sector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiveRegisterRoleInput {
    /// Stable lowercase role identifier.
    pub role: String,
    /// Human-readable display title for UI and 3D sector labels.
    pub display_name: String,
    /// Department / sector identifier.
    pub department: String,
    /// Concise objective of the new role.
    pub objective: String,
    /// Requested capability names (bounded by Core policy).
    pub capabilities: Vec<String>,
    /// Strategy guidance for the role.
    pub strategy: String,
}

/// Parses and normalizes JSON input for `hive_register_role`.
///
/// # Errors
///
/// Returns [`ToolError::InvalidInput`] if required fields are missing or malformed.
pub fn parse_hive_register_role_input(input: &Value) -> Result<HiveRegisterRoleInput, ToolError> {
    let req_str = |key: &str| -> Result<String, ToolError> {
        input
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ToolError::InvalidInput {
                tool: HIVE_REGISTER_ROLE_NAME.to_string(),
                message: format!("{key} is required"),
            })
    };
    let role = req_str("role")?.to_lowercase();
    if role.len() > 48
        || !role.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
    {
        return Err(ToolError::InvalidInput {
            tool: HIVE_REGISTER_ROLE_NAME.to_string(),
            message: "role must be <= 48 chars of [a-z0-9_-]".into(),
        });
    }
    let display_name = req_str("display_name")?;
    let department = req_str("department")?.to_lowercase();
    let objective = req_str("objective")?;
    let strategy = input
        .get("strategy")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(
            "Выполнять профильные задачи строго в рамках выданных прав и проверять результат.",
        )
        .to_string();
    let capabilities = input
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .take(12)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_else(|| vec!["workspace.read".into(), "workspace.search".into()]);
    Ok(HiveRegisterRoleInput {
        role,
        display_name,
        department,
        objective,
        capabilities,
        strategy,
    })
}

/// Formats the delegated agent summary and its bounded execution metadata.
pub fn format_result(summary: &str, steps_run: usize, truncated: bool, depth: u32) -> ToolResult {
    ToolResult {
        output: format!(
            "agent_run (depth={depth}, steps={steps_run}{}):\n{summary}",
            if truncated { ", truncated" } else { "" }
        ),
        structured: json!({
            "summary": summary,
            "steps_run": steps_run,
            "truncated": truncated,
            "depth": depth,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_requires_prompt() {
        assert!(parse_input(&json!({})).is_err());
        let parsed = parse_input(&json!({
            "prompt": " dig into auth ",
            "max_steps": 4,
            "timeout_ms": 5000,
            "model_route": "planner"
        }))
        .expect("input");
        assert_eq!(parsed.prompt, "dig into auth");
        assert_eq!(parsed.max_steps, Some(4));
        assert_eq!(parsed.timeout_ms, Some(5000));
        assert_eq!(parsed.model_route.as_deref(), Some("planner"));
    }
}
