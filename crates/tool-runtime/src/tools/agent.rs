use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::{json, Value};
use std::time::Duration;

pub const NAME: &str = "agent.run";
pub const DESCRIPTION: &str = "Spawn a budgeted subagent with its own prompt. Input: { prompt, max_steps?, timeout_ms?, model_route? }. Fan-out by scheduling multiple agent.run steps without depends_on.";
pub const PERMISSIONS: &[Permission] = &[];
pub const TIMEOUT: Duration = Duration::from_secs(180);

/// Registry stub — real subagent runs in agent-runtime.
pub async fn execute(_ctx: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution(
        "agent.run is executed by the agent subagent backend".into(),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunInput {
    pub prompt: String,
    pub max_steps: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub model_route: Option<String>,
}

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

pub fn format_result(summary: &str, steps_run: usize, truncated: bool, depth: u32) -> ToolResult {
    ToolResult {
        output: format!(
            "agent.run (depth={depth}, steps={steps_run}{}):\n{summary}",
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
