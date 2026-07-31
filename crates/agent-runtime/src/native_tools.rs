//! OpenAI-compatible tool specs for native planning (Stage 7.28).

use evohime_model_gateway::ToolSpec;
use evohime_tool_runtime::ToolRegistry;
use serde_json::json;

pub fn openai_tools_for_registry(tools: &ToolRegistry) -> Vec<ToolSpec> {
    let mut specs: Vec<ToolSpec> = tools
        .list()
        .into_iter()
        .filter_map(|tool| tool_spec_for_name(tool.name, tool.description))
        .collect();
    specs.push(ToolSpec::function(
        provider_tool_name("assistant.reply"),
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

fn provider_tool_name(name: &str) -> String {
    name.replace('.', "_")
}

pub(crate) fn canonical_tool_name(name: &str) -> String {
    match name {
        "agent_run" => "agent.run",
        "filesystem_read" => "filesystem.read",
        "filesystem_write" => "filesystem.write",
        "filesystem_patch" => "filesystem.patch",
        "filesystem_search" => "filesystem.search",
        "filesystem_list" => "filesystem.list",
        "shell_execute" => "shell.execute",
        "git_status" => "git.status",
        "git_diff" => "git.diff",
        "git_commit" => "git.commit",
        "git_pull" => "git.pull",
        "git_push" => "git.push",
        "browser_open" => "browser.open",
        "browser_extract" => "browser.extract",
        "browser_session_navigate" => "browser.session.navigate",
        "browser_session_read" => "browser.session.read",
        "browser_session_click" => "browser.session.click",
        "browser_session_screenshot" => "browser.session.screenshot",
        "browser_session_type" => "browser.session.type",
        "browser_session_close" => "browser.session.close",
        "http_fetch" => "http.fetch",
        "mcp_call" => "mcp.call",
        "memory_search" => "memory.search",
        "worker_run" => "worker.run",
        "assistant_reply" => "assistant.reply",
        other => other,
    }
    .to_string()
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
        "browser.session.navigate" => json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_chars": { "type": "integer" },
                "timeout_ms": { "type": "integer" }
            },
            "required": ["url"]
        }),
        "browser.session.read" => json!({
            "type": "object",
            "properties": {
                "max_chars": { "type": "integer" }
            }
        }),
        "browser.session.click" => json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string" },
                "max_chars": { "type": "integer" },
                "settle_ms": { "type": "integer" }
            },
            "required": ["selector"]
        }),
        "browser.session.screenshot" => json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Workspace-relative PNG path" },
                "full_page": { "type": "boolean" }
            }
        }),
        "browser.session.type" => json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string" },
                "text": { "type": "string" },
                "max_chars": { "type": "integer" },
                "settle_ms": { "type": "integer" }
            },
            "required": ["selector", "text"]
        }),
        "browser.session.close" => json!({
            "type": "object",
            "properties": {}
        }),
        "http.fetch" => json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_chars": { "type": "integer" },
                "timeout_ms": { "type": "integer" }
            },
            "required": ["url"]
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
        "worker.run" => json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Worker task name, e.g. text.summarize" },
                "payload": { "type": "object" },
                "timeout_ms": { "type": "integer" },
                "poll_ms": { "type": "integer" }
            },
            "required": ["task", "payload"]
        }),
        _ => return None,
    };
    Some(ToolSpec::function(
        provider_tool_name(name),
        description,
        parameters,
    ))
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
        assert!(specs
            .iter()
            .any(|spec| spec.function.name == "filesystem_read"));
        assert!(specs
            .iter()
            .any(|spec| spec.function.name == "assistant_reply"));
    }

    #[test]
    fn native_tool_names_are_provider_safe() {
        let tools = ToolRegistry::bootstrap_with_permissions(PermissionEngine::new());
        let specs = openai_tools_for_registry(&tools);

        assert!(specs.iter().all(|spec| {
            spec.function
                .name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        }));
        assert!(specs
            .iter()
            .any(|spec| spec.function.name == "filesystem_read"));
    }

    #[test]
    fn canonical_tool_name_round_trips_provider_names() {
        assert_eq!(canonical_tool_name("filesystem_read"), "filesystem.read");
        assert_eq!(canonical_tool_name("assistant_reply"), "assistant.reply");
    }
}
