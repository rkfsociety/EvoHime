//! OpenAI-compatible tool specs for native planning (Stage 7.28).

use evohime_model_gateway::{NativeToolCall, ToolSpec};
use evohime_protocol::PlanStep;
use evohime_tool_runtime::ToolRegistry;
use serde_json::{json, Value};

const NATIVE_PLANNING_PROMPT: &str = "You are EvoHime's task planner. Call one or more tools to fulfill the user request. Prefer the fewest tools. Use assistant.reply only when no tool is needed. Do not invent file paths — use relative workspace paths. For mutating tools, include complete executable arguments.";

pub fn native_tool_calls_enabled() -> bool {
    match std::env::var("EVOHIME_NATIVE_TOOL_CALLS") {
        Ok(value) => {
            let trimmed = value.trim().to_ascii_lowercase();
            !(trimmed.is_empty()
                || trimmed == "0"
                || trimmed == "false"
                || trimmed == "off"
                || trimmed == "no")
        }
        Err(_) => true,
    }
}

pub fn native_planning_prompt() -> String {
    NATIVE_PLANNING_PROMPT.to_string()
}

pub fn openai_tools_for_registry(tools: &ToolRegistry) -> Vec<ToolSpec> {
    let mut specs: Vec<ToolSpec> = tools
        .list()
        .into_iter()
        .filter_map(|tool| tool_spec_for_name(tool.name, tool.description))
        .collect();
    specs.push(ToolSpec::function(
        "assistant.reply",
        "Respond to the user without further tools",
        json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Final answer for the user" }
            },
            "required": ["message"]
        }),
    ));
    specs.sort_by(|left, right| left.function.name.cmp(&right.function.name));
    specs.dedup_by(|left, right| left.function.name == right.function.name);
    specs
}

pub fn plan_from_native_tool_calls(calls: &[NativeToolCall]) -> Vec<PlanStep> {
    let steps: Vec<PlanStep> = calls
        .iter()
        .enumerate()
        .map(|(index, call)| {
            let description = normalize_arguments_description(&call.name, &call.arguments);
            PlanStep {
                id: format!("step-{}", index + 1),
                tool_name: call.name.clone(),
                description,
                depends_on: Vec::new(),
            }
        })
        .collect();
    steps
}

fn normalize_arguments_description(tool_name: &str, arguments: &str) -> String {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return "{}".into();
    }
    if tool_name == "assistant.reply" {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if let Some(message) = value.get("message").and_then(Value::as_str) {
                return message.to_string();
            }
        }
    }
    trimmed.to_string()
}

fn tool_spec_for_name(name: &str, description: &str) -> Option<ToolSpec> {
    let parameters = match name {
        "filesystem.read" | "filesystem.list" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative path inside the workspace" }
            },
            "required": ["path"]
        }),
        "filesystem.search" => json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "path": { "type": "string" },
                "glob": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["query"]
        }),
        "filesystem.write" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        }),
        "filesystem.patch" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "patch": { "type": "string" }
            },
            "required": ["path", "patch"]
        }),
        "shell.execute" => json!({
            "type": "object",
            "properties": {
                "program": { "type": "string" },
                "args": { "type": "array", "items": { "type": "string" } },
                "cwd": { "type": "string" },
                "timeout_ms": { "type": "integer" }
            },
            "required": ["program"]
        }),
        "git.status" | "git.diff" => json!({
            "type": "object",
            "properties": {}
        }),
        "git.commit" => json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            },
            "required": ["message"]
        }),
        "git.pull" | "git.push" => json!({
            "type": "object",
            "properties": {
                "remote": { "type": "string" },
                "branch": { "type": "string" }
            }
        }),
        "browser.open" => json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_chars": { "type": "integer" }
            },
            "required": ["url"]
        }),
        "browser.extract" => json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "selector": { "type": "string" },
                "attribute": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["url", "selector"]
        }),
        "mcp.call" => json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "method": { "type": "string" },
                "params": { "type": "object" }
            },
            "required": ["url", "method"]
        }),
        "memory.search" => json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "integer" }
            },
            "required": ["query"]
        }),
        "agent.run" => json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "Subagent user prompt" },
                "max_steps": { "type": "integer" },
                "timeout_ms": { "type": "integer" },
                "model_route": { "type": "string" }
            },
            "required": ["prompt"]
        }),
        _ => return None,
    };
    Some(ToolSpec::function(name, description, parameters))
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_permissions::PermissionEngine;
    use evohime_tool_runtime::ToolRegistry;

    #[test]
    fn builds_openai_tools_including_assistant_reply() {
        let tools = ToolRegistry::bootstrap_with_permissions(PermissionEngine::new());
        let specs = openai_tools_for_registry(&tools);
        assert!(specs.iter().any(|spec| spec.function.name == "filesystem.read"));
        assert!(specs.iter().any(|spec| spec.function.name == "assistant.reply"));
    }

    #[test]
    fn plan_from_native_preserves_json_arguments() {
        let plan = plan_from_native_tool_calls(&[NativeToolCall {
            id: "1".into(),
            name: "filesystem.read".into(),
            arguments: r#"{"path":"docs/a.md"}"#.into(),
        }]);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tool_name, "filesystem.read");
        assert!(plan[0].description.contains("docs/a.md"));
    }

    #[test]
    fn assistant_reply_uses_message_field() {
        let plan = plan_from_native_tool_calls(&[NativeToolCall {
            id: "1".into(),
            name: "assistant.reply".into(),
            arguments: r#"{"message":"hello"}"#.into(),
        }]);
        assert_eq!(plan[0].description, "hello");
    }
}
