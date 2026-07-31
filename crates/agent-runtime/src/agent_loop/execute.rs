//! Plan step execution and tool input construction.
use super::parse::extract_code_block;
use super::tool_budget::{truncate_tool_result, ToolResultBudget};
use super::util::emit;
use super::{AgentConfig, AgentError};
use crate::subagent::execute_agent_run;
use evohime_model_gateway::ModelGateway;
use evohime_protocol::{PlanStep, ServerEvent};
use evohime_tool_runtime::{ToolContext, ToolError, ToolRegistry};
use serde_json::{json, Value};
use std::path::Path;
use tokio::sync::mpsc::UnboundedSender;

pub(crate) enum StepOutcome {
    SkippedAssistant,
    SkippedUnsupported { message: String },
    Completed { output: String },
}

pub(crate) async fn execute_single_plan_step(
    step: &PlanStep,
    config: &AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    event_tx: &UnboundedSender<ServerEvent>,
) -> Result<StepOutcome, AgentError> {
    let tool_name = match step.tool_name.as_str() {
        "read_file" => "filesystem.read",
        "list_files" => "filesystem.list",
        "search" => "filesystem.search",
        name => name,
    };
    if tool_name == "assistant.reply" {
        return Ok(StepOutcome::SkippedAssistant);
    }

    // Dependency gating is handled by batching; this path is for legacy sequential safety.
    let mut effective_tool_name = tool_name;
    let input = match tool_input(tool_name, &step.description, &config.workspace_root) {
        Some(input) => input,
        None => {
            if is_mutating_tool(tool_name) {
                return Err(AgentError::PlanStepFailed {
                    step_id: step.id.clone(),
                    tool_name: tool_name.to_string(),
                    message: "шаг изменения не содержит исполнимых входных данных".to_string(),
                });
            }
            return Ok(StepOutcome::SkippedUnsupported {
                message: format!(
                    "{}: шаг пропущен — инструмент не поддержан runtime",
                    step.id
                ),
            });
        }
    };
    if tool_name == "filesystem.read"
        && input
            .get("path")
            .and_then(Value::as_str)
            .map(|path| config.workspace_root.join(path).is_dir())
            .unwrap_or(false)
    {
        effective_tool_name = "filesystem.list";
    }

    emit(
        event_tx,
        ServerEvent::ToolStarted {
            task_id: config.task_id,
            tool_name: effective_tool_name.to_string(),
        },
    )?;

    let (progress_tx, mut progress_rx) =
        tokio::sync::mpsc::unbounded_channel::<evohime_tool_runtime::ToolProgress>();
    let forward_tx = event_tx.clone();
    let forward_task_id = config.task_id;
    let forward_tool = effective_tool_name.to_string();
    let forward = tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            let _ = emit(
                &forward_tx,
                ServerEvent::ToolOutputDelta {
                    task_id: forward_task_id,
                    tool_name: forward_tool.clone(),
                    stream: progress.stream.to_string(),
                    delta: progress.delta,
                },
            );
        }
    });

    let context = ToolContext {
        workspace_root: config.workspace_root.clone(),
        task_id: config.task_id,
        session_id: Some(config.session_id),
        progress_tx: Some(progress_tx),
    };
    let tool_result = if effective_tool_name == "memory.search" {
        execute_memory_search(config, tools, &input).await
    } else if effective_tool_name == "agent.run" {
        execute_agent_run(config, gateway, tools, event_tx, input).await
    } else {
        tools.execute(&context, effective_tool_name, input).await
    };
    drop(context);
    let _ = forward.await;

    match tool_result {
        Ok(result) => {
            emit(
                event_tx,
                ServerEvent::ToolOutput {
                    task_id: config.task_id,
                    tool_name: effective_tool_name.to_string(),
                    output: result.output.clone(),
                },
            )?;
            emit(
                event_tx,
                ServerEvent::ToolCompleted {
                    task_id: config.task_id,
                    tool_name: effective_tool_name.to_string(),
                    success: true,
                },
            )?;
            Ok(StepOutcome::Completed {
                output: truncate_tool_result(
                    &format!("{} ({effective_tool_name}):\n{}", step.id, result.output),
                    ToolResultBudget::from_env().per_result_chars,
                ),
            })
        }
        Err(error) => {
            if matches!(error, ToolError::NeedsApproval { .. }) {
                // Pause for operator approval — do not mark the step completed.
                return Err(AgentError::Tool(error));
            }
            let output = format!(
                "{} ({effective_tool_name}) завершился с ошибкой: {error}",
                step.id
            );
            emit(
                event_tx,
                ServerEvent::ToolOutput {
                    task_id: config.task_id,
                    tool_name: effective_tool_name.to_string(),
                    output: output.clone(),
                },
            )?;
            emit(
                event_tx,
                ServerEvent::ToolCompleted {
                    task_id: config.task_id,
                    tool_name: effective_tool_name.to_string(),
                    success: false,
                },
            )?;
            if matches!(
                effective_tool_name,
                "filesystem.read" | "filesystem.list" | "filesystem.search" | "memory.search"
            ) {
                return Ok(StepOutcome::Completed { output });
            }
            Err(AgentError::PlanStepFailed {
                step_id: step.id.clone(),
                tool_name: effective_tool_name.to_string(),
                message: error.to_string(),
            })
        }
    }
}

pub(crate) fn tool_input(
    tool_name: &str,
    description: &str,
    workspace_root: &Path,
) -> Option<Value> {
    if let Some(mut structured) = structured_json_input(tool_name, description) {
        if tool_name == "shell.execute" {
            normalize_shell_program_alias(&mut structured)?;
        }
        return Some(structured);
    }

    let path = extract_declared_path(description)
        .or_else(|| extract_backticked(description))
        .or_else(|| extract_pathlike_token(description))
        .map(|path| normalize_plan_path(&path, workspace_root));
    match tool_name {
        "filesystem.read" => {
            let resolved = path.unwrap_or_else(|| {
                if description_implies_workspace_root(description) {
                    ".".to_string()
                } else {
                    "docs/sample-context.md".to_string()
                }
            });
            Some(json!({ "path": resolved }))
        }
        "filesystem.list" => Some(json!({"path": path.unwrap_or_else(|| ".".to_string())})),
        "filesystem.search" => {
            let query = extract_backticked(description)
                .or_else(|| extract_search_query_token(description))
                .unwrap_or_else(|| "TODO".to_string());
            Some(json!({"query": query, "limit": 100}))
        }
        "memory.search" => Some(json!({
            "query": extract_backticked(description)
                .unwrap_or_else(|| description.trim().to_string()),
            "limit": 10,
        })),
        "agent.run" => Some(json!({
            "prompt": extract_backticked(description)
                .unwrap_or_else(|| description.trim().to_string()),
        })),
        "worker.run" => None, // requires structured JSON { task, payload }
        "filesystem.write" => Some(json!({
            "path": path?,
            "content": extract_code_block(description).unwrap_or_default(),
        })),
        "filesystem.patch" => Some(json!({
            "path": path?,
            "patch": extract_code_block(description).unwrap_or_default(),
        })),
        "shell.execute" => shell_input(description),
        "git.status" => Some(json!({})),
        "git.diff" => Some(json!({})),
        "git.commit" => Some(json!({"message": extract_commit_message(description)})),
        "git.pull" | "git.push" => Some(json!({})),
        "browser.open" => extract_url(description).map(|url| json!({ "url": url })),
        "http.fetch" => extract_url(description).map(|url| json!({ "url": url })),
        "browser.extract" => {
            let url = extract_url(description)?;
            let selector = extract_backticked(description)
                .filter(|value| value != &url && !value.starts_with("http"))
                .unwrap_or_else(|| "body".to_string());
            Some(json!({ "url": url, "selector": selector }))
        }
        "mcp.call" => None, // requires structured JSON (url + method)
        _ => None,
    }
}

/// Prefer structured JSON descriptions produced by `tool.call` / tagged calls.
pub(crate) fn structured_json_input(tool_name: &str, description: &str) -> Option<Value> {
    let trimmed = description.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value = serde_json::from_str::<Value>(trimmed).ok()?;
    match tool_name {
        "shell.execute" | "browser.open" | "browser.extract" | "http.fetch" | "mcp.call"
        | "memory.search" | "agent.run" | "worker.run" | "git.diff" | "git.pull" | "git.push"
        | "git.status" | "git.commit" | "filesystem.read" | "filesystem.list"
        | "filesystem.search" | "filesystem.write" | "filesystem.patch" => {
            if value.is_object() {
                Some(value)
            } else if value.is_null() {
                Some(json!({}))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) async fn execute_memory_search(
    config: &AgentConfig,
    tools: &ToolRegistry,
    input: &Value,
) -> Result<evohime_tool_runtime::ToolResult, ToolError> {
    use evohime_permissions::{Permission, PermissionCheck, PermissionDecision};

    let (query, limit) = evohime_tool_runtime::memory::parse_input(input)?;

    let permissions = tools.permissions();
    match permissions
        .check_scoped(
            Permission::MemorySearch,
            &PermissionCheck {
                session_id: Some(config.session_id),
                path: Some("workspace"),
            },
        )
        .await
    {
        PermissionDecision::Allowed => {}
        PermissionDecision::Denied => {
            return Err(ToolError::PermissionDenied(Permission::MemorySearch));
        }
        PermissionDecision::NeedsApproval => {
            let approval = permissions
                .create_approval_scoped(
                    config.task_id,
                    Some(config.session_id),
                    "memory.search",
                    Permission::MemorySearch,
                    "workspace",
                )
                .await;
            return Err(ToolError::NeedsApproval {
                tool: "memory.search".to_string(),
                permission: Permission::MemorySearch,
                scope: "workspace".to_string(),
                approval_id: approval.id,
                input: input.clone(),
            });
        }
    }

    let Some(pool) = &config.memory_pool else {
        return Ok(evohime_tool_runtime::ToolResult {
            output: "memory.search: memory backend not configured".into(),
            structured: json!({
                "query": query,
                "count": 0,
                "matches": [],
                "error": "not_configured",
            }),
        });
    };

    let ranked = evohime_memory::rank_for_query(
        pool,
        evohime_memory::RetrieveRequest {
            session_id: Some(config.session_id),
            workspace_key: &config.workspace_key,
            query: &query,
            max_chars: 16_000,
            max_items: limit,
        },
    )
    .await
    .map_err(|error| ToolError::Execution(error.to_string()))?;

    let entries: Vec<(String, String, String, f64)> = ranked
        .iter()
        .map(|ranked| {
            (
                ranked.item.scope.clone(),
                ranked.item.kind.clone(),
                ranked.item.content.clone(),
                ranked.score,
            )
        })
        .collect();
    Ok(evohime_tool_runtime::memory::format_results(
        &query, &entries,
    ))
}

pub(crate) fn extract_url(description: &str) -> Option<String> {
    if let Some(url) = extract_backticked(description)
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
    {
        return Some(url);
    }
    for token in description.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| {
            c == '"' || c == '\'' || c == ',' || c == ')' || c == '(' || c == '<' || c == '>'
        });
        if cleaned.starts_with("http://") || cleaned.starts_with("https://") {
            return Some(cleaned.to_string());
        }
    }
    None
}

pub(crate) fn is_mutating_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "filesystem.write"
            | "filesystem.patch"
            | "shell.execute"
            | "git.commit"
            | "git.pull"
            | "git.push"
    )
}

pub(crate) fn shell_input(description: &str) -> Option<Value> {
    let mut input = serde_json::from_str::<Value>(description).ok()?;
    normalize_shell_program_alias(&mut input)?;
    Some(input)
}

pub(crate) fn normalize_shell_program_alias(input: &mut Value) -> Option<()> {
    let object = input.as_object_mut()?;
    if !object.contains_key("program") {
        let command = object.remove("command")?;
        object.insert("program".to_string(), command);
    }
    Some(())
}

pub(crate) fn extract_commit_message(description: &str) -> String {
    for delimiter in ['"', '\''] {
        if let Some(start) = description.find(delimiter) {
            if let Some(end) = description[start + delimiter.len_utf8()..].find(delimiter) {
                let message = description
                    [start + delimiter.len_utf8()..start + delimiter.len_utf8() + end]
                    .trim();
                if !message.is_empty() {
                    return message.to_string();
                }
            }
        }
    }

    description
        .split_once(':')
        .map(|(_, message)| message.trim().trim_matches('`').to_string())
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "Обновление кода".to_string())
}

pub(crate) fn normalize_plan_path(path: &str, workspace_root: &Path) -> String {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        if let Ok(relative) = candidate.strip_prefix(workspace_root) {
            let normalized = relative.to_string_lossy().replace('\\', "/");
            return if normalized.is_empty() {
                ".".to_string()
            } else {
                normalized
            };
        }
    }
    path.replace('\\', "/")
}

pub(crate) fn extract_backticked(value: &str) -> Option<String> {
    let start = value.find('`')? + 1;
    let end = value[start..].find('`')? + start;
    let path = value[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// Pull a bare workspace path from free-text plan descriptions
/// (e.g. "Inspect crates/server/src/main.rs" or "Прочитать Cargo.toml, …").
pub(crate) fn extract_pathlike_token(description: &str) -> Option<String> {
    description
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '|'))
        .find_map(|token| {
            let candidate = clean_path_token(token);
            looks_like_workspace_path(candidate).then(|| candidate.to_string())
        })
}

fn clean_path_token(token: &str) -> &str {
    let trimmed = token.trim_matches(|c: char| {
        matches!(
            c,
            '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | ':' | '«' | '»'
        )
    });
    if trimmed.ends_with('.') {
        let without_dot = &trimmed[..trimmed.len() - '.'.len_utf8()];
        if looks_like_workspace_path(without_dot) {
            return without_dot;
        }
    }
    trimmed
}

fn looks_like_workspace_path(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with("http://")
        || value.starts_with("https://")
        || value.contains(' ')
    {
        return false;
    }
    if value.contains('/') || value.contains('\\') {
        return value.chars().any(|c| c.is_ascii_alphanumeric());
    }
    matches!(
        Path::new(value).extension().and_then(|ext| ext.to_str()),
        Some(
            "md" | "rs"
                | "toml"
                | "json"
                | "tsx"
                | "ts"
                | "jsx"
                | "js"
                | "py"
                | "css"
                | "sql"
                | "yml"
                | "yaml"
                | "ps1"
                | "sh"
                | "txt"
        )
    )
}

/// Prefer an explicit debt marker from search plan text when backticks are absent.
fn extract_search_query_token(description: &str) -> Option<String> {
    let lower = description.to_lowercase();
    for marker in [
        "todo",
        "fixme",
        "unimplemented",
        "stub",
        "fake",
        "placeholder",
        "panic!",
        "todo!",
    ] {
        if lower.contains(marker) {
            return Some(marker.to_string());
        }
    }
    None
}

pub(crate) fn description_implies_workspace_root(description: &str) -> bool {
    let lower = description.to_lowercase();
    [
        "root directory",
        "workspace root",
        "workspace context",
        "project structure",
        "project root",
        "корнев",
        "структур проекта",
        "структуру проекта",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

pub(crate) fn extract_declared_path(value: &str) -> Option<String> {
    value.lines().find_map(|line| {
        let trimmed = line.trim();
        let remainder = ["path", "file"].iter().find_map(|key| {
            let remainder = trimmed.strip_prefix(key)?;
            let remainder = if let Some(value) = remainder.strip_prefix(':') {
                value
            } else if remainder.len() != remainder.trim_start().len() {
                remainder.trim_start()
            } else {
                return None;
            };
            (!remainder.trim().is_empty()).then_some(remainder)
        })?;
        let raw_path = remainder.trim();
        let path = if let Some(quoted) = raw_path.strip_prefix('"') {
            quoted.split('"').next().unwrap_or_default()
        } else if let Some(quoted) = raw_path.strip_prefix('`') {
            quoted.split('`').next().unwrap_or_default()
        } else if let Some(quoted) = raw_path.strip_prefix('\'') {
            quoted.split('\'').next().unwrap_or_default()
        } else {
            raw_path.split_whitespace().next().unwrap_or_default()
        };
        (!path.is_empty()).then(|| path.to_string())
    })
}
