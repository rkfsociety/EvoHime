use chrono::{DateTime, Utc};
use evohime_model_gateway::{providers::ChatMessage, providers::ChatRole, ModelGateway};
use evohime_project_index::ProjectIndex;
use evohime_protocol::{PlanStep, ServerEvent};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

pub fn parse_plan(raw: &str) -> Vec<PlanStep> {
    let normalized = unwrap_code_fence(raw);
    if normalized.is_empty() {
        return default_plan();
    }

    if let Some(plan) = parse_plan_json(&normalized) {
        return normalize_plan(plan);
    }
    if let Some(tool_calls) = parse_model_tool_calls(&normalized) {
        return tool_calls;
    }

    let source_lines = normalized.lines().collect::<Vec<_>>();
    let mut logical_lines = Vec::new();
    let mut index = 0;
    while index < source_lines.len() {
        let line = source_lines[index];
        let is_write_step = line.contains("filesystem.write") || line.contains("filesystem.patch");
        if is_write_step {
            let mut combined = line.to_string();
            let mut cursor = index + 1;
            let mut saw_code_fence = false;
            while cursor < source_lines.len() {
                let next = source_lines[cursor];
                combined.push('\n');
                combined.push_str(next);
                if next.trim_start().starts_with("```") {
                    if saw_code_fence {
                        cursor += 1;
                        break;
                    }
                    saw_code_fence = true;
                }
                cursor += 1;
                if cursor > index + 12 && !saw_code_fence {
                    break;
                }
            }
            logical_lines.push(combined);
            index = cursor;
        } else {
            logical_lines.push(line.to_string());
            index += 1;
        }
    }

    let parsed: Vec<PlanStep> = logical_lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| parse_plan_line(index, line))
        .collect();

    if parsed.is_empty() {
        parse_model_tool_calls(&normalized).unwrap_or_else(default_plan)
    } else {
        normalize_plan(parsed)
    }
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub user_message: String,
    pub created_at: DateTime<Utc>,
    pub demo_file_path: PathBuf,
    pub workspace_root: PathBuf,
    pub model_route: String,
    pub model: Option<String>,
    pub planning_model_route: String,
    pub planning_model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRunResult {
    pub final_message: String,
}

#[derive(Debug, Clone)]
pub struct AgentResumeContext {
    pub workspace_context: Option<String>,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("tool error: {0}")]
    Tool(#[from] evohime_tool_runtime::ToolError),
    #[error("model error: {0}")]
    Model(#[from] evohime_model_gateway::providers::ProviderError),
    #[error("event channel closed")]
    EventChannel,
    #[error("{phase} model request timed out after {timeout_seconds} seconds")]
    ModelTimeout {
        phase: &'static str,
        timeout_seconds: u64,
    },
    #[error("plan step {step_id} ({tool_name}) failed: {message}")]
    PlanStepFailed {
        step_id: String,
        tool_name: String,
        message: String,
    },
}

pub async fn run_agent_loop(
    config: AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    history: Vec<ChatMessage>,
    memory_notes: Vec<String>,
    event_tx: UnboundedSender<ServerEvent>,
) -> Result<AgentRunResult, AgentError> {
    run_agent_loop_inner(
        config,
        gateway,
        tools,
        history,
        memory_notes,
        event_tx,
        true,
        None,
    )
    .await
}

pub async fn run_agent_loop_resumed(
    config: AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    history: Vec<ChatMessage>,
    memory_notes: Vec<String>,
    event_tx: UnboundedSender<ServerEvent>,
    resume: AgentResumeContext,
) -> Result<AgentRunResult, AgentError> {
    run_agent_loop_inner(
        config,
        gateway,
        tools,
        history,
        memory_notes,
        event_tx,
        false,
        resume.workspace_context,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_agent_loop_inner(
    config: AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    history: Vec<ChatMessage>,
    memory_notes: Vec<String>,
    event_tx: UnboundedSender<ServerEvent>,
    emit_started: bool,
    workspace_context: Option<String>,
) -> Result<AgentRunResult, AgentError> {
    if emit_started {
        emit(
            &event_tx,
            ServerEvent::TaskStarted {
                task_id: config.task_id,
                session_id: config.session_id,
                user_message: config.user_message.clone(),
                created_at: config.created_at,
            },
        )?;
    }

    let tool_output = match workspace_context {
        Some(output) => output,
        None => {
            if !config.demo_file_path.is_file() {
                "Контекстный файл проекта отсутствует; проект может быть пустым.".to_string()
            } else {
                emit(
                    &event_tx,
                    ServerEvent::ToolStarted {
                        task_id: config.task_id,
                        tool_name: "filesystem.read".to_string(),
                    },
                )?;

                let relative_path =
                    relative_workspace_path(&config.workspace_root, &config.demo_file_path);
                let tool_ctx = ToolContext {
                    workspace_root: config.workspace_root.clone(),
                    task_id: config.task_id,
                };
                let tool_result = tools
                    .execute(
                        &tool_ctx,
                        "filesystem.read",
                        json!({ "path": relative_path }),
                    )
                    .await?;

                emit(
                    &event_tx,
                    ServerEvent::ToolOutput {
                        task_id: config.task_id,
                        tool_name: "filesystem.read".to_string(),
                        output: tool_result.output.clone(),
                    },
                )?;

                emit(
                    &event_tx,
                    ServerEvent::ToolCompleted {
                        task_id: config.task_id,
                        tool_name: "filesystem.read".to_string(),
                        success: true,
                    },
                )?;

                tool_result.output
            }
        }
    };
    let project_context =
        ProjectIndex::new(config.workspace_root.clone()).build_context(&config.user_message, 5);
    let memory_context = build_memory_context(&memory_notes);
    let rules_context = build_workspace_rules(&config.workspace_root);

    let mut planning_messages = Vec::with_capacity(history.len() + 4);
    planning_messages.push(ChatMessage {
        role: ChatRole::System,
        content: PLANNING_PROMPT.to_string(),
    });
    if let Some(context) = &rules_context {
        planning_messages.push(ChatMessage {
            role: ChatRole::System,
            content: context.clone(),
        });
    }
    if let Some(context) = &project_context {
        planning_messages.push(ChatMessage {
            role: ChatRole::System,
            content: context.clone(),
        });
    }
    if let Some(context) = &memory_context {
        planning_messages.push(ChatMessage {
            role: ChatRole::System,
            content: context.clone(),
        });
    }
    planning_messages.extend(history.clone());
    planning_messages.push(ChatMessage {
        role: ChatRole::User,
        content: format!(
            "User request:\n{}\n\nWorkspace context from `{}`:\n```\n{}\n```",
            config.user_message,
            config.demo_file_path.display(),
            tool_output
        ),
    });

    let raw_plan = collect_stream_text_with_timeout(
        gateway.stream_chat_for_route_with_model(
            &config.planning_model_route,
            config.planning_model.as_deref(),
            &planning_messages,
        )?,
        PLANNING_TIMEOUT,
        "planning",
    )
    .await?;
    let plan = parse_plan(&raw_plan);

    emit(
        &event_tx,
        ServerEvent::AgentPlanUpdated {
            task_id: config.task_id,
            plan: plan.clone(),
        },
    )?;

    let plan_outputs = execute_plan_steps(&plan, &config, tools, &event_tx).await?;

    let mut messages = Vec::with_capacity(history.len() + 4);
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: SYSTEM_PROMPT.to_string(),
    });
    if let Some(context) = &rules_context {
        messages.push(ChatMessage {
            role: ChatRole::System,
            content: context.clone(),
        });
    }
    if let Some(context) = &project_context {
        messages.push(ChatMessage {
            role: ChatRole::System,
            content: context.clone(),
        });
    }
    if let Some(context) = &memory_context {
        messages.push(ChatMessage {
            role: ChatRole::System,
            content: context.clone(),
        });
    }
    messages.extend(history);
    let context = format!(
        "{}\n\nPlan tool results:\n{}",
        tool_output,
        plan_outputs.join("\n\n")
    );
    messages.push(ChatMessage {
        role: ChatRole::User,
        content: format!(
            "{}\n\nPlan:\n{}\n\nContext from `{}`:\n```\n{}\n```",
            config.user_message,
            format_plan(&plan),
            config.demo_file_path.display(),
            context
        ),
    });

    let mut final_message = String::new();
    let mut stream = gateway.stream_chat_for_route_with_model(
        &config.model_route,
        config.model.as_deref(),
        &messages,
    )?;

    tokio::time::timeout(RESPONSE_TIMEOUT, async {
        while let Some(chunk) = stream.next().await {
            let delta = chunk?;
            final_message.push_str(&delta);
            emit(
                &event_tx,
                ServerEvent::AgentMessageDelta {
                    task_id: config.task_id,
                    delta,
                },
            )?;
        }
        Ok::<(), AgentError>(())
    })
    .await
    .map_err(|_| AgentError::ModelTimeout {
        phase: "response",
        timeout_seconds: RESPONSE_TIMEOUT.as_secs(),
    })??;

    if let Some(tool_plan) = parse_model_tool_calls(&final_message) {
        let _ = execute_plan_steps(&tool_plan, &config, tools, &event_tx).await?;
    }

    if final_message.trim().is_empty() {
        final_message = "No response from the model.".to_string();
        emit(
            &event_tx,
            ServerEvent::AgentMessageDelta {
                task_id: config.task_id,
                delta: final_message.clone(),
            },
        )?;
    }

    emit(
        &event_tx,
        ServerEvent::TaskCompleted {
            task_id: config.task_id,
            final_message: final_message.clone(),
            completed_at: Utc::now(),
        },
    )?;

    Ok(AgentRunResult { final_message })
}

const PLANNING_TIMEOUT: Duration = Duration::from_secs(90);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
const SYSTEM_PROMPT: &str = "You are EvoHime, a helpful AI coding assistant. Follow the workspace rules supplied in the system context, preserve user intent, never claim a change was made unless a tool result confirms it, and answer concisely using the provided workspace context. When an action is required, return an explicit JSON tool.call object with type, tool, and input; do not merely describe the call.";
const PLANNING_PROMPT: &str = "You are EvoHime's task planner. Return only JSON: an array of objects with fields id, tool_name, description, and depends_on. Use only these tool names: filesystem.read, filesystem.list, filesystem.search, filesystem.write, filesystem.patch, shell.execute, git.status, git.diff, git.commit, git.pull, git.push, assistant.reply. Use stable step ids like step-1, step-2, and keep depends_on empty unless a step truly depends on another step. Put exact relative paths in backticks. For filesystem.write, include complete content in a fenced code block. For filesystem.patch, include complete patch text in a fenced code block. For shell.execute, include a JSON object with program, args, cwd, and timeout_ms in the description. For git.commit, include the requested commit message in quotes. Use git.pull and git.push only when explicitly asked. If no tool call is needed, use assistant.reply.";

async fn execute_plan_steps(
    plan: &[PlanStep],
    config: &AgentConfig,
    tools: &ToolRegistry,
    event_tx: &UnboundedSender<ServerEvent>,
) -> Result<Vec<String>, AgentError> {
    let context = ToolContext {
        workspace_root: config.workspace_root.clone(),
        task_id: config.task_id,
    };
    let mut outputs = Vec::new();
    let mut successful_steps = HashMap::new();
    for step in plan {
        let tool_name = match step.tool_name.as_str() {
            "read_file" => "filesystem.read",
            "list_files" => "filesystem.list",
            "search" => "filesystem.search",
            name => name,
        };
        if tool_name == "assistant.reply" {
            successful_steps.insert(step.id.clone(), true);
            continue;
        }
        if step
            .depends_on
            .iter()
            .any(|dependency| !successful_steps.get(dependency).copied().unwrap_or(false))
        {
            outputs.push(format!(
                "{} ({tool_name}) пропущен: не выполнена зависимость {}",
                step.id,
                step.depends_on.join(", ")
            ));
            successful_steps.insert(step.id.clone(), false);
            continue;
        }
        let mut effective_tool_name = tool_name;
        let input = match tool_input(tool_name, &step.description, &config.workspace_root) {
            Some(input) => input,
            None => {
                outputs.push(format!(
                    "{}: шаг пропущен — инструмент не поддержан runtime",
                    step.id
                ));
                successful_steps.insert(step.id.clone(), false);
                continue;
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
        match tools.execute(&context, effective_tool_name, input).await {
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
                outputs.push(format!(
                    "{} ({effective_tool_name}):\n{}",
                    step.id, result.output
                ));
                successful_steps.insert(step.id.clone(), true);
            }
            Err(error) => {
                emit(
                    event_tx,
                    ServerEvent::ToolCompleted {
                        task_id: config.task_id,
                        tool_name: effective_tool_name.to_string(),
                        success: false,
                    },
                )?;
                outputs.push(format!(
                    "{} ({effective_tool_name}) завершился с ошибкой: {error}",
                    step.id
                ));
                successful_steps.insert(step.id.clone(), false);
                if matches!(
                    effective_tool_name,
                    "filesystem.read" | "filesystem.list" | "filesystem.search"
                ) {
                    continue;
                }
                return Err(AgentError::PlanStepFailed {
                    step_id: step.id.clone(),
                    tool_name: effective_tool_name.to_string(),
                    message: error.to_string(),
                });
            }
        }
    }
    Ok(outputs)
}

fn tool_input(tool_name: &str, description: &str, workspace_root: &Path) -> Option<Value> {
    let path = extract_declared_path(description)
        .or_else(|| extract_backticked(description))
        .map(|path| normalize_plan_path(&path, workspace_root));
    match tool_name {
        "filesystem.read" => {
            Some(json!({"path": path.unwrap_or_else(|| "docs/sample-context.md".to_string())}))
        }
        "filesystem.list" => Some(json!({"path": path.unwrap_or_else(|| ".".to_string())})),
        "filesystem.search" => Some(
            json!({"query": extract_backticked(description).unwrap_or_else(|| "TODO".to_string()), "limit": 100}),
        ),
        "filesystem.write" => Some(json!({
            "path": path?,
            "content": extract_code_block(description).unwrap_or_default(),
        })),
        "filesystem.patch" => Some(json!({
            "path": path?,
            "patch": extract_code_block(description).unwrap_or_default(),
        })),
        "shell.execute" => shell_input(description),
        "git.status" => Some(Value::Null),
        "git.diff" => Some(json!({})),
        "git.commit" => Some(json!({"message": extract_commit_message(description)})),
        "git.pull" | "git.push" => Some(json!({})),
        _ => None,
    }
}

fn shell_input(description: &str) -> Option<Value> {
    let mut input = serde_json::from_str::<Value>(description).ok()?;
    let object = input.as_object_mut()?;
    if !object.contains_key("program") {
        let command = object.remove("command")?;
        object.insert("program".to_string(), command);
    }
    Some(input)
}

fn extract_commit_message(description: &str) -> String {
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

fn normalize_plan_path(path: &str, workspace_root: &Path) -> String {
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

fn extract_backticked(value: &str) -> Option<String> {
    let start = value.find('`')? + 1;
    let end = value[start..].find('`')? + start;
    let path = value[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

fn extract_declared_path(value: &str) -> Option<String> {
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

fn parse_markup_tool_calls(raw: &str) -> Option<Vec<PlanStep>> {
    let mut steps = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = raw[cursor..].find("<invoke name=\"") {
        let start = cursor + relative_start + "<invoke name=\"".len();
        let Some(relative_name_end) = raw[start..].find('\"') else {
            break;
        };
        let name_end = relative_name_end + start;
        let tool_name = &raw[start..name_end];
        let Some(relative_body_start) = raw[name_end..].find('>') else {
            break;
        };
        let body_start = relative_body_start + name_end + 1;
        let Some(relative_body_end) = raw[body_start..].find("</invoke>") else {
            break;
        };
        let body_end = relative_body_end + body_start;
        let body = &raw[body_start..body_end];
        let path = markup_parameter(body, "path");
        let content = markup_parameter(body, "content").or_else(|| markup_parameter(body, "patch"));
        let description = match (path, content) {
            (Some(path), Some(content)) => format!("path: {path}\n```\n{content}\n```"),
            (Some(path), None) => format!("path: {path}"),
            (None, _) => body.trim().to_string(),
        };
        steps.push(PlanStep {
            id: format!("step-{}", steps.len() + 1),
            tool_name: tool_name.to_string(),
            description,
            depends_on: Vec::new(),
        });
        cursor = body_end + "</invoke>".len();
    }
    (!steps.is_empty()).then(|| normalize_plan(steps))
}

fn parse_model_tool_calls(raw: &str) -> Option<Vec<PlanStep>> {
    parse_markup_tool_calls(raw)
        .or_else(|| parse_json_tool_calls(raw))
        .or_else(|| parse_tagged_tool_calls(raw))
}

fn parse_json_tool_calls(raw: &str) -> Option<Vec<PlanStep>> {
    let mut candidates = extract_json_blocks(raw);
    if candidates.is_empty() && serde_json::from_str::<Value>(raw.trim()).is_ok() {
        candidates.push(raw.trim().to_string());
    }
    let mut steps = Vec::new();

    for candidate in candidates {
        let values = match serde_json::from_str::<Value>(&candidate) {
            Ok(Value::Array(values)) => values,
            Ok(value) => vec![value],
            Err(_) => continue,
        };

        for value in values {
            let Some(object) = value.as_object() else {
                continue;
            };
            let Some(type_name) = object.get("type").and_then(Value::as_str) else {
                continue;
            };
            let (tool_name, input) = if type_name == "tool.call" {
                let Some(tool_name) = object.get("tool").and_then(Value::as_str) else {
                    continue;
                };
                let Some(input) = object.get("input").and_then(Value::as_object) else {
                    continue;
                };
                (tool_name.to_string(), input.clone())
            } else if is_supported_tool(type_name) {
                let input = object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "type")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                (type_name.to_string(), input)
            } else {
                continue;
            };

            let description = tool_call_description(&tool_name, &input);
            steps.push(PlanStep {
                id: format!("step-{}", steps.len() + 1),
                tool_name,
                description,
                depends_on: Vec::new(),
            });
        }
    }

    (!steps.is_empty()).then(|| normalize_plan(steps))
}

fn is_supported_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "filesystem.read"
            | "filesystem.list"
            | "filesystem.search"
            | "filesystem.write"
            | "filesystem.patch"
            | "shell.execute"
            | "git.status"
            | "git.diff"
            | "git.commit"
            | "git.pull"
            | "git.push"
    )
}

fn parse_tagged_tool_calls(raw: &str) -> Option<Vec<PlanStep>> {
    let mut steps = Vec::new();
    let mut cursor = 0;
    const OPEN: &str = "<tool_call>";
    const CLOSE: &str = "</tool_call>";

    while let Some(relative_start) = raw[cursor..].find(OPEN) {
        let body_start = cursor + relative_start + OPEN.len();
        let Some(relative_end) = raw[body_start..].find(CLOSE) else {
            break;
        };
        let body_end = relative_end + body_start;
        let Ok(value) = serde_json::from_str::<Value>(raw[body_start..body_end].trim()) else {
            cursor = body_end + CLOSE.len();
            continue;
        };
        let Some(object) = value.as_object() else {
            cursor = body_end + CLOSE.len();
            continue;
        };
        let Some(tool_name) = object
            .get("tool")
            .and_then(Value::as_str)
            .or_else(|| object.get("type").and_then(Value::as_str))
        else {
            cursor = body_end + CLOSE.len();
            continue;
        };
        let input = object
            .get("input")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(|| {
                object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "type" && key.as_str() != "tool")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            });
        let description = tool_call_description(tool_name, &input);
        steps.push(PlanStep {
            id: format!("step-{}", steps.len() + 1),
            tool_name: tool_name.to_string(),
            description,
            depends_on: Vec::new(),
        });
        cursor = body_end + CLOSE.len();
    }

    (!steps.is_empty()).then(|| normalize_plan(steps))
}

fn tool_call_description(tool_name: &str, input: &serde_json::Map<String, Value>) -> String {
    match tool_name {
        "filesystem.write" => format!(
            "path: {}\n```\n{}\n```",
            input
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            input
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
        "filesystem.patch" => format!(
            "path: {}\n```\n{}\n```",
            input
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            input
                .get("patch")
                .and_then(Value::as_str)
                .unwrap_or_default()
        ),
        _ => serde_json::to_string(input).unwrap_or_default(),
    }
}

fn extract_json_blocks(raw: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = raw[cursor..].find("```") {
        let start = cursor + relative_start;
        let Some(line_end) = raw[start..].find('\n') else {
            break;
        };
        let body_start = start + line_end + 1;
        let Some(relative_end) = raw[body_start..].find("```") else {
            break;
        };
        let language = raw[start + 3..start + line_end].trim();
        if language.is_empty() || language.eq_ignore_ascii_case("json") {
            blocks.push(
                raw[body_start..body_start + relative_end]
                    .trim()
                    .to_string(),
            );
        }
        cursor = body_start + relative_end + 3;
    }
    blocks
}

fn markup_parameter(body: &str, name: &str) -> Option<String> {
    let marker = format!("<parameter name=\"{name}\">");
    let start = body.find(&marker)? + marker.len();
    let end = body[start..].find("</parameter>")? + start;
    let value = body[start..end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn extract_code_block(value: &str) -> Option<String> {
    let start = value.find("```")?;
    let body_start = value[start..].find('\n').map(|offset| start + offset + 1)?;
    let end = value[body_start..].find("```")? + body_start;
    let body = &value[body_start..end];
    (!body.is_empty()).then(|| body.to_string())
}

#[derive(Debug, serde::Deserialize)]
struct PlanEnvelope {
    #[serde(default)]
    steps: Vec<PlanStepDraft>,
    #[serde(default)]
    plan: Vec<PlanStepDraft>,
}

#[derive(Debug, serde::Deserialize)]
struct PlanStepDraft {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    depends_on: Vec<String>,
}

fn parse_plan_json(raw: &str) -> Option<Vec<PlanStep>> {
    serde_json::from_str::<Vec<PlanStep>>(raw).ok().or_else(|| {
        serde_json::from_str::<PlanEnvelope>(raw)
            .ok()
            .and_then(|envelope| {
                let drafts = if !envelope.steps.is_empty() {
                    envelope.steps
                } else {
                    envelope.plan
                };
                (!drafts.is_empty()).then(|| {
                    drafts
                        .into_iter()
                        .enumerate()
                        .map(|(index, draft)| PlanStep {
                            id: draft.id.unwrap_or_else(|| format!("step-{}", index + 1)),
                            tool_name: draft
                                .tool_name
                                .unwrap_or_else(|| "assistant.reply".to_string()),
                            description: draft
                                .description
                                .unwrap_or_else(|| format!("Execute step-{}", index + 1)),
                            depends_on: draft
                                .depends_on
                                .into_iter()
                                .filter(|dependency| !dependency.trim().is_empty())
                                .collect(),
                        })
                        .collect()
                })
            })
    })
}

fn unwrap_code_fence(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let mut lines = trimmed.lines();
    let first = lines.next().unwrap_or_default();
    if !first.starts_with("```") {
        return trimmed.to_string();
    }

    let body = lines
        .take_while(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");

    body.trim().to_string()
}

fn emit(event_tx: &UnboundedSender<ServerEvent>, event: ServerEvent) -> Result<(), AgentError> {
    event_tx.send(event).map_err(|_| AgentError::EventChannel)
}

fn relative_workspace_path(workspace_root: &Path, file_path: &Path) -> String {
    file_path
        .strip_prefix(workspace_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| file_path.display().to_string())
}

async fn collect_stream_text(
    mut stream: impl futures_util::Stream<Item = Result<String, evohime_model_gateway::providers::ProviderError>>
        + Unpin,
) -> Result<String, AgentError> {
    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        output.push_str(&chunk?);
    }
    Ok(output)
}

async fn collect_stream_text_with_timeout(
    stream: impl futures_util::Stream<Item = Result<String, evohime_model_gateway::providers::ProviderError>>
        + Unpin,
    timeout: Duration,
    phase: &'static str,
) -> Result<String, AgentError> {
    tokio::time::timeout(timeout, collect_stream_text(stream))
        .await
        .map_err(|_| AgentError::ModelTimeout {
            phase,
            timeout_seconds: timeout.as_secs(),
        })?
}

fn format_plan(plan: &[PlanStep]) -> String {
    plan.iter()
        .map(|step| {
            if step.depends_on.is_empty() {
                format!("{}: {} ({})", step.id, step.description, step.tool_name)
            } else {
                format!(
                    "{}: {} ({}) depends on {}",
                    step.id,
                    step.description,
                    step.tool_name,
                    step.depends_on.join(", ")
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn default_plan() -> Vec<PlanStep> {
    vec![
        PlanStep {
            id: "step-1".to_string(),
            tool_name: "filesystem.read".to_string(),
            description: "Read workspace context".to_string(),
            depends_on: Vec::new(),
        },
        PlanStep {
            id: "step-2".to_string(),
            tool_name: "assistant.reply".to_string(),
            description: "Write the response using the collected context".to_string(),
            depends_on: vec!["step-1".to_string()],
        },
    ]
}

fn normalize_plan(mut plan: Vec<PlanStep>) -> Vec<PlanStep> {
    if plan.is_empty() {
        return default_plan();
    }

    for (index, step) in plan.iter_mut().enumerate() {
        if step.id.trim().is_empty() {
            step.id = format!("step-{}", index + 1);
        }
        if step.tool_name.trim().is_empty() {
            step.tool_name = "assistant.reply".to_string();
        }
        if step.description.trim().is_empty() {
            step.description = format!("Execute {}", step.tool_name);
        }
        step.depends_on
            .retain(|dependency| !dependency.trim().is_empty());
    }

    plan
}

fn parse_plan_line(index: usize, line: &str) -> Option<PlanStep> {
    let mut text = line.trim().trim_matches('*').trim();
    if text.is_empty() {
        return None;
    }

    while let Some(stripped) = text.strip_prefix(|ch: char| ch == '-' || ch == '*' || ch == '•') {
        text = stripped.trim_start();
    }

    if let Some((numbered, rest)) = text.split_once('.') {
        if numbered.chars().all(|ch| ch.is_ascii_digit()) {
            text = rest.trim_start();
        }
    }

    if text.is_empty() {
        return None;
    }

    let (body, depends_on) = split_dependencies(text);
    let (tool_name, description) = extract_tool_and_description(body, index);

    // Plain prose from a model is not a plan step. The old fallback mapped
    // every unrecognised line to assistant.reply, producing dozens of fake
    // steps when the model ignored the JSON-only planning instruction.
    let supported = [
        "filesystem.read",
        "filesystem.list",
        "filesystem.search",
        "filesystem.write",
        "filesystem.patch",
        "shell.execute",
        "git.status",
        "git.diff",
        "git.commit",
        "git.pull",
        "git.push",
        "assistant.reply",
    ];
    if !supported.contains(&tool_name.as_str()) {
        return None;
    }
    if tool_name == "assistant.reply" && !body.trim_start().starts_with("assistant.reply") {
        return None;
    }

    Some(PlanStep {
        id: format!("step-{}", index + 1),
        tool_name,
        description,
        depends_on,
    })
}

fn split_dependencies(text: &str) -> (&str, Vec<String>) {
    let lowered = text.to_lowercase();
    for marker in ["depends on", "after"] {
        if let Some(position) = lowered.find(marker) {
            let head = text[..position].trim().trim_end_matches([':', '-', ' ']);
            let tail = text[position + marker.len()..].trim();
            let depends_on = tail
                .split([',', ';'])
                .map(|item| item.trim().trim_start_matches("step ").trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
            return (head, depends_on);
        }
    }
    (text, Vec::new())
}

fn extract_tool_and_description(text: &str, index: usize) -> (String, String) {
    for tool_name in [
        "filesystem.read",
        "filesystem.list",
        "filesystem.search",
        "filesystem.write",
        "filesystem.patch",
        "shell.execute",
        "git.status",
        "git.diff",
        "git.commit",
        "git.pull",
        "git.push",
        "assistant.reply",
    ] {
        if let Some(position) = text.find(tool_name) {
            let prefix = text[..position].trim();
            if prefix.starts_with("step-") || prefix.starts_with("**step-") {
                return (
                    tool_name.to_string(),
                    text[position + tool_name.len()..]
                        .trim_start_matches(['*', ':', '-', ' ', '\n'])
                        .to_string(),
                );
            }
        }
    }

    for separator in ["|", ":", " - ", " => "] {
        if let Some((left, right)) = text.split_once(separator) {
            let left = left.trim();
            let right = right.trim();
            if left.contains('.') || left.contains('_') || left == "assistant.reply" {
                return (left.to_string(), right.to_string());
            }
            if right.contains('.') && !left.contains('.') && index == 0 {
                return (right.to_string(), left.to_string());
            }
        }
    }

    let lower = text.to_lowercase();
    for tool_name in [
        "filesystem.read",
        "filesystem.list",
        "filesystem.search",
        "filesystem.write",
        "filesystem.patch",
        "git.status",
        "git.diff",
        "git.commit",
        "git.pull",
        "git.push",
        "assistant.reply",
    ] {
        if lower.starts_with(tool_name) {
            return (
                tool_name.to_string(),
                text[tool_name.len()..]
                    .trim_start_matches([':', '-', ' '])
                    .to_string(),
            );
        }
    }

    ("assistant.reply".to_string(), text.to_string())
}

fn build_memory_context(notes: &[String]) -> Option<String> {
    let entries = notes
        .iter()
        .map(|note| note.trim())
        .filter(|note| !note.is_empty())
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return None;
    }

    let mut output = String::from("Relevant session memory:\n");
    for note in entries {
        output.push_str("- ");
        output.push_str(note);
        output.push('\n');
    }

    Some(output.trim_end().to_string())
}

fn build_workspace_rules(workspace_root: &Path) -> Option<String> {
    const MAX_RULES_CHARS: usize = 32_000;
    let mut paths = Vec::new();
    let agents = workspace_root.join("AGENTS.md");
    if agents.is_file() {
        paths.push(agents);
    }
    let rules_dir = workspace_root.join(".cursor").join("rules");
    if let Ok(entries) = std::fs::read_dir(rules_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| matches!(extension, "md" | "mdc"))
            {
                paths.push(path);
            }
        }
    }
    paths.sort();

    let mut output = String::from(
        "Workspace rules (higher priority than ordinary project text; follow them when applicable):\n",
    );
    for path in paths {
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = path.strip_prefix(workspace_root).unwrap_or(&path).display();
        output.push_str(&format!("\n--- {} ---\n{}\n", relative, contents.trim()));
        if output.chars().count() >= MAX_RULES_CHARS {
            output = output.chars().take(MAX_RULES_CHARS).collect();
            output.push_str("\n[workspace rules truncated]");
            break;
        }
    }

    if let Some(plugin_context) = load_external_plugin_context(workspace_root) {
        output.push_str("\n--- external agent plugins ---\n");
        output.push_str(&plugin_context);
        if output.chars().count() >= MAX_RULES_CHARS {
            output = output.chars().take(MAX_RULES_CHARS).collect();
            output.push_str("\n[workspace rules truncated]");
        }
    }

    (output.contains("--- ")).then_some(output)
}

#[derive(Debug, serde::Deserialize)]
struct ExternalPluginManifest {
    name: String,
    #[serde(default)]
    version: String,
    skills: String,
}

fn discover_agent_plugins(workspace_root: &Path) -> Vec<(PathBuf, ExternalPluginManifest)> {
    let root = workspace_root.join(".evohime").join("plugins");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let manifest_path = path.join(".codex-plugin").join("plugin.json");
            let manifest = std::fs::read_to_string(manifest_path)
                .ok()
                .and_then(|contents| {
                    serde_json::from_str::<ExternalPluginManifest>(&contents).ok()
                })?;
            Some((path, manifest))
        })
        .collect()
}

fn load_external_plugin_context(workspace_root: &Path) -> Option<String> {
    let mut output = String::new();
    for (plugin_root, manifest) in discover_agent_plugins(workspace_root) {
        let skills_root = plugin_root.join(&manifest.skills);
        if !skills_root.starts_with(&plugin_root) || !skills_root.is_dir() {
            continue;
        }
        output.push_str(&format!(
            "Plugin `{}` v{} loaded from `{}`.\n",
            manifest.name,
            if manifest.version.is_empty() {
                "unknown"
            } else {
                &manifest.version
            },
            skills_root
                .strip_prefix(workspace_root)
                .unwrap_or(&skills_root)
                .display()
        ));

        let mut skills = std::fs::read_dir(&skills_root)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && path.join("SKILL.md").is_file())
            .collect::<Vec<_>>();
        skills.sort();
        for skill in skills {
            let skill_name = skill
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");
            output.push_str(&format!(
                "- skill `{skill_name}`: read `{}` when applicable.\n",
                skill
                    .join("SKILL.md")
                    .strip_prefix(workspace_root)
                    .unwrap_or(&skill)
                    .display()
            ));
        }

        for bootstrap in ["using-superpowers", "verification-before-completion"] {
            let path = skills_root.join(bootstrap).join("SKILL.md");
            if let Ok(contents) = std::fs::read_to_string(path) {
                output.push_str(&format!(
                    "\nBootstrap skill `{bootstrap}`:\n{}\n",
                    contents.trim()
                ));
            }
        }
    }
    (!output.is_empty()).then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_path_from_workspace() {
        let root = PathBuf::from("/workspace");
        let file = PathBuf::from("/workspace/docs/a.md");
        assert_eq!(relative_workspace_path(&root, &file), "docs/a.md");
    }

    #[test]
    fn builds_memory_context_block() {
        let context =
            build_memory_context(&[" first fact ".into(), "".into(), "second fact".into()])
                .expect("context");
        assert!(context.contains("Relevant session memory:"));
        assert!(context.contains("first fact"));
        assert!(context.contains("second fact"));
    }

    #[test]
    fn loads_agents_and_cursor_rules_for_model_context() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(temp.path().join("AGENTS.md"), "Follow Rust tests.").expect("agents");
        std::fs::create_dir_all(temp.path().join(".cursor/rules")).expect("rules dir");
        std::fs::write(
            temp.path().join(".cursor/rules/project.mdc"),
            "Keep frontend presentation-only.",
        )
        .expect("cursor rule");

        let context = build_workspace_rules(temp.path()).expect("rules context");
        assert!(context.contains("Follow Rust tests."));
        assert!(context.contains("Keep frontend presentation-only."));
    }

    #[test]
    fn loads_external_plugin_manifest_and_skill_index() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plugin = temp.path().join(".evohime/plugins/demo");
        std::fs::create_dir_all(plugin.join(".codex-plugin")).expect("plugin metadata");
        std::fs::create_dir_all(plugin.join("skills/bootstrap")).expect("skill dir");
        std::fs::write(
            plugin.join(".codex-plugin/plugin.json"),
            r#"{"name":"demo","version":"1.0.0","skills":"./skills/"}"#,
        )
        .expect("manifest");
        std::fs::write(
            plugin.join("skills/bootstrap/SKILL.md"),
            "Use the demo skill.",
        )
        .expect("skill");

        let context = build_workspace_rules(temp.path()).expect("plugin context");
        assert!(context.contains("Plugin `demo` v1.0.0 loaded"));
        assert!(context.contains("skill `bootstrap`"));
    }

    #[test]
    fn prose_is_not_parsed_as_fake_reply_steps() {
        let plan = parse_plan(
            "Я вижу, что нужно исследовать проект.\n\nДавайте начнём с анализа структуры.",
        );
        assert_eq!(plan, default_plan());
    }

    #[test]
    fn builds_filesystem_write_input_from_code_block() {
        let input = tool_input(
            "filesystem.write",
            "Update `docs/test.md` with:\n```markdown\nhello\n```",
            Path::new("C:/workspace"),
        )
        .expect("write input");
        assert_eq!(input["path"], "docs/test.md");
        assert_eq!(input["content"], "hello\n");
    }

    #[test]
    fn builds_filesystem_write_input_from_declared_path() {
        let input = tool_input(
            "filesystem.write",
            "filesystem.write\npath: workers/python/handler.py\n```python\nprint('ok')\n```",
            Path::new("C:/workspace"),
        )
        .expect("write input");
        assert_eq!(input["path"], "workers/python/handler.py");
        assert_eq!(input["content"], "print('ok')\n");
    }

    #[test]
    fn refuses_write_without_a_path() {
        assert!(tool_input(
            "filesystem.write",
            "filesystem.write\n```text\ncontent\n```",
            Path::new("C:/workspace"),
        )
        .is_none());
    }

    #[test]
    fn builds_shell_input_from_structured_description() {
        let input = tool_input(
            "shell.execute",
            r#"{"program":"python","args":["-c","print('ok')"],"cwd":"workers/python","timeout_ms":30000}"#,
            Path::new("C:/workspace"),
        )
        .expect("shell input");
        assert_eq!(input["program"], "python");
        assert_eq!(input["args"][0], "-c");
        assert_eq!(input["cwd"], "workers/python");
    }

    #[test]
    fn normalizes_shell_command_alias_to_program() {
        let input = tool_input(
            "shell.execute",
            r#"{"command":"python","args":["--version"]}"#,
            Path::new("C:/workspace"),
        )
        .expect("shell input");
        assert_eq!(input["program"], "python");
        assert_eq!(input["args"][0], "--version");
    }

    #[test]
    fn reads_quoted_declared_paths_without_guessing() {
        let input = tool_input(
            "filesystem.read",
            r#"path "crates/server/src/main.rs""#,
            Path::new("C:/workspace"),
        )
        .expect("read input");
        assert_eq!(input["path"], "crates/server/src/main.rs");

        let directory = tool_input(
            "filesystem.read",
            r#"path "workers/python" --recursive"#,
            Path::new("C:/workspace"),
        )
        .expect("directory input");
        assert_eq!(directory["path"], "workers/python");

        let fallback = tool_input(
            "filesystem.read",
            "Inspect crates/server/src/main.rs",
            Path::new("C:/workspace"),
        )
        .expect("read fallback");
        assert_eq!(fallback["path"], "docs/sample-context.md");
    }

    #[test]
    fn parses_filesystem_write_plan_line() {
        let plan = parse_plan("filesystem.write: Update `docs/test.md`:\n```markdown\nhello\n```");
        assert_eq!(plan[0].tool_name, "filesystem.write");
        assert_eq!(
            plan[0].description,
            "Update `docs/test.md`:\n```markdown\nhello\n```"
        );
    }

    #[test]
    fn parses_markdown_write_step_with_declared_path() {
        let plan = parse_plan(
            "**step-1: filesystem.write**\npath: workers/python/handler.py\n```python\nprint('ok')\n```",
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tool_name, "filesystem.write");
        assert!(plan[0]
            .description
            .contains("path: workers/python/handler.py"));
    }

    #[test]
    fn parses_markup_tool_calls_from_model_output() {
        let plan = parse_plan(
            r#"<function_calls><invoke name="filesystem.write"><parameter name="path">workers/python/handler.py</parameter><parameter name="content">print('ok')</parameter></invoke></function_calls>"#,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tool_name, "filesystem.write");
        assert!(plan[0].description.contains("workers/python/handler.py"));
        assert!(plan[0].description.contains("print('ok')"));
    }

    #[test]
    fn parses_json_tool_call_from_model_output() {
        let plan = parse_plan(
            r#"Выполняю:
```json
{
  "type": "tool.call",
  "tool": "filesystem.write",
  "input": {
    "path": "docs/agent-dogfood-check.md",
    "content": "agent write verified"
  }
}
```"#,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tool_name, "filesystem.write");
        assert!(plan[0]
            .description
            .contains("path: docs/agent-dogfood-check.md"));
        assert!(plan[0].description.contains("agent write verified"));
    }

    #[test]
    fn parses_direct_typed_json_tool_call() {
        let plan = parse_plan(
            r#"```json
{"type":"filesystem.write","path":"docs/direct.md","content":"direct"}
```"#,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tool_name, "filesystem.write");
        assert!(plan[0].description.contains("docs/direct.md"));
    }

    #[test]
    fn parses_tagged_tool_call_from_model_output() {
        let plan = parse_plan(
            r#"<tool_call>
{"type":"filesystem.write","path":"docs/tagged.md","content":"tagged"}
</tool_call>"#,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].tool_name, "filesystem.write");
        assert!(plan[0].description.contains("path: docs/tagged.md"));
        assert!(plan[0].description.contains("tagged"));
    }

    #[test]
    fn keeps_valid_tagged_calls_when_a_later_call_is_truncated() {
        let plan = parse_plan(
            r#"<tool_call>{"type":"filesystem.write","path":"docs/first.md","content":"ok"}</tool_call>
<tool_call>{"type":"filesystem.write","path":"docs/broken.md"</tool_call>
<tool_call>{"type":"filesystem.write","path":"docs/third.md","content":"ok"}</tool_call>"#,
        );
        assert_eq!(plan.len(), 2);
        assert!(plan[0].description.contains("docs/first.md"));
        assert!(plan[1].description.contains("docs/third.md"));
    }

    #[test]
    fn keeps_valid_markup_calls_before_a_truncated_invoke() {
        let plan = parse_plan(
            r#"<invoke name="filesystem.write"><parameter name="path">docs/first.md</parameter><parameter name="content">ok</parameter></invoke>
<invoke name="filesystem.write"><parameter name="path">docs/broken.md</parameter>"#,
        );
        assert_eq!(plan.len(), 1);
        assert!(plan[0].description.contains("docs/first.md"));
    }

    #[tokio::test]
    async fn adds_project_index_context_to_model_prompt() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("docs")).expect("create docs dir");
        std::fs::write(
            temp.path().join("docs/notes.md"),
            "project index context and agent runtime notes",
        )
        .expect("write");

        let provider = RecordingProvider::new(vec![
            vec![
                r#"[{"id":"step-1","tool_name":"assistant.reply","description":"Respond","depends_on":[] }]"#
                    .to_string(),
            ],
            vec!["Indexed answer".to_string()],
        ]);
        let gateway = evohime_model_gateway::ModelGateway::from_provider(std::sync::Arc::new(
            provider.clone(),
        ));
        let tools = evohime_tool_runtime::ToolRegistry::bootstrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        let result = run_agent_loop(
            AgentConfig {
                task_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                user_message: "project index".to_string(),
                created_at: chrono::Utc::now(),
                demo_file_path: temp.path().join("docs/notes.md"),
                workspace_root: temp.path().to_path_buf(),
                model_route: "default".to_string(),
                model: None,
                planning_model_route: "default".to_string(),
                planning_model: None,
            },
            &gateway,
            &tools,
            vec![ChatMessage {
                role: ChatRole::User,
                content: "previous".to_string(),
            }],
            vec!["memory fact".to_string()],
            tx,
        )
        .await
        .expect("agent completes");

        assert_eq!(result.final_message, "Indexed answer");

        let calls = provider.calls.lock().expect("calls");
        assert!(calls.iter().any(|messages| messages
            .iter()
            .any(|message| message.content.contains("Relevant project context"))));
        assert!(calls.iter().any(|messages| messages
            .iter()
            .any(|message| message.content.contains("docs/notes.md"))));
    }

    #[test]
    fn parses_model_plan_json_and_dependencies() {
        let plan = parse_plan(
            r#"[{"id":"read","tool_name":"filesystem.read","description":"read context","depends_on":[]}]"#,
        );
        assert_eq!(plan[0].tool_name, "filesystem.read");
    }

    #[test]
    fn parses_fenced_json_plan_blocks() {
        let plan = parse_plan(
            "```json\n[{\"id\":\"read\",\"tool_name\":\"filesystem.read\",\"description\":\"read context\",\"depends_on\":[]}]\n```",
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].id, "read");
        assert_eq!(plan[0].tool_name, "filesystem.read");
    }

    #[test]
    fn parses_wrapped_plan_objects() {
        let plan = parse_plan(
            r#"{"steps":[{"tool_name":"filesystem.read","description":"read context","depends_on":[]} ]}"#,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].id, "step-1");
        assert_eq!(plan[0].tool_name, "filesystem.read");
    }

    #[tokio::test]
    async fn resumed_runs_skip_workspace_read() {
        let temp = tempfile::tempdir().expect("tempdir");
        let demo_file = temp.path().join("context.md");
        std::fs::write(&demo_file, "# Demo\nHello from workspace.").expect("write");

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let gateway = evohime_model_gateway::ModelGateway::from_provider(std::sync::Arc::new(
            RecordingProvider::new(vec![
                vec![r#"[{"id":"step-1","tool_name":"assistant.reply","description":"Respond","depends_on":[]}]"#.into()],
                vec!["Recovered response".into()],
            ]),
        ));
        let tools = evohime_tool_runtime::ToolRegistry::bootstrap();

        let result = run_agent_loop_resumed(
            AgentConfig {
                task_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                user_message: "Explain the file".to_string(),
                created_at: chrono::Utc::now(),
                demo_file_path: demo_file.clone(),
                workspace_root: temp.path().to_path_buf(),
                model_route: "default".to_string(),
                model: None,
                planning_model_route: "default".to_string(),
                planning_model: None,
            },
            &gateway,
            &tools,
            vec![ChatMessage {
                role: ChatRole::User,
                content: "previous".to_string(),
            }],
            vec!["memory fact".to_string()],
            tx,
            AgentResumeContext {
                workspace_context: Some("Recovered workspace context".to_string()),
            },
        )
        .await
        .expect("agent completes");

        assert!(result.final_message.contains("Recovered response"));

        let mut saw_task_started = false;
        let mut saw_tool_started = false;
        let mut saw_tool_output = false;
        while let Some(event) = rx.recv().await {
            match event {
                ServerEvent::TaskStarted { .. } => saw_task_started = true,
                ServerEvent::ToolStarted { .. } => saw_tool_started = true,
                ServerEvent::ToolOutput { .. } => saw_tool_output = true,
                _ => {}
            }
        }

        assert!(!saw_task_started);
        assert!(!saw_tool_started);
        assert!(!saw_tool_output);
    }
}

#[cfg(test)]
#[derive(Clone)]
struct RecordingProvider {
    responses: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<Vec<String>>>>,
    calls: std::sync::Arc<std::sync::Mutex<Vec<Vec<ChatMessage>>>>,
}

#[cfg(test)]
impl RecordingProvider {
    fn new(responses: Vec<Vec<String>>) -> Self {
        Self {
            responses: std::sync::Arc::new(std::sync::Mutex::new(responses.into())),
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

#[cfg(test)]
impl evohime_model_gateway::providers::ModelProvider for RecordingProvider {
    fn kind(&self) -> evohime_model_gateway::providers::ProviderKind {
        evohime_model_gateway::providers::ProviderKind::Mock
    }

    fn model_name(&self) -> &str {
        "recording-model"
    }

    fn base_url(&self) -> &str {
        "mock://recording"
    }

    fn stream_chat(
        &self,
        messages: &[ChatMessage],
    ) -> evohime_model_gateway::providers::TokenStream {
        self.calls.lock().expect("calls").push(messages.to_vec());

        let response = self
            .responses
            .lock()
            .expect("responses")
            .pop_front()
            .unwrap_or_default();
        Box::pin(futures_util::stream::iter(
            response
                .into_iter()
                .map(Ok::<_, evohime_model_gateway::providers::ProviderError>),
        ))
    }
}
