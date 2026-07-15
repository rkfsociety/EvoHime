use chrono::{DateTime, Utc};
use evohime_model_gateway::{providers::ChatMessage, providers::ChatRole, ModelGateway};
use evohime_protocol::{PlanStep, ServerEvent};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::StreamExt;
use serde_json::json;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

pub fn parse_plan(raw: &str) -> Vec<PlanStep> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return default_plan();
    }

    if let Ok(plan) = serde_json::from_str::<Vec<PlanStep>>(trimmed) {
        return normalize_plan(plan);
    }

    let parsed: Vec<PlanStep> = trimmed
        .lines()
        .enumerate()
        .filter_map(|(index, line)| parse_plan_line(index, line))
        .collect();

    if parsed.is_empty() {
        default_plan()
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
}

pub async fn run_agent_loop(
    config: AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    history: Vec<ChatMessage>,
    event_tx: UnboundedSender<ServerEvent>,
) -> Result<AgentRunResult, AgentError> {
    run_agent_loop_inner(config, gateway, tools, history, event_tx, true, None).await
}

pub async fn run_agent_loop_resumed(
    config: AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    history: Vec<ChatMessage>,
    event_tx: UnboundedSender<ServerEvent>,
    resume: AgentResumeContext,
) -> Result<AgentRunResult, AgentError> {
    run_agent_loop_inner(
        config,
        gateway,
        tools,
        history,
        event_tx,
        false,
        resume.workspace_context,
    )
    .await
}

async fn run_agent_loop_inner(
    config: AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    history: Vec<ChatMessage>,
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
    };

    let mut planning_messages = Vec::with_capacity(history.len() + 3);
    planning_messages.push(ChatMessage {
        role: ChatRole::System,
        content: PLANNING_PROMPT.to_string(),
    });
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

    let raw_plan = collect_stream_text(gateway.stream_chat(&planning_messages)).await?;
    let plan = parse_plan(&raw_plan);

    emit(
        &event_tx,
        ServerEvent::AgentPlanUpdated {
            task_id: config.task_id,
            plan: plan.clone(),
        },
    )?;

    let mut messages = Vec::with_capacity(history.len() + 3);
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: SYSTEM_PROMPT.to_string(),
    });
    messages.extend(history);
    messages.push(ChatMessage {
        role: ChatRole::User,
        content: format!(
            "{}\n\nPlan:\n{}\n\nContext from `{}`:\n```\n{}\n```",
            config.user_message,
            format_plan(&plan),
            config.demo_file_path.display(),
            tool_output
        ),
    });

    let mut final_message = String::new();
    let mut stream = gateway.stream_chat(&messages);

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

const SYSTEM_PROMPT: &str = "You are EvoHime, a helpful AI coding assistant. Answer concisely using the provided workspace context when relevant.";
const PLANNING_PROMPT: &str = "You are EvoHime's task planner. Return only JSON: an array of objects with fields id, tool_name, description, and depends_on. Use stable step ids like step-1, step-2, and keep depends_on empty unless a step truly depends on another step. If no tool call is needed, use assistant.reply as the tool_name for the final response step.";

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
    let mut text = line.trim();
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
    if lower.starts_with("filesystem.read") {
        return (
            "filesystem.read".to_string(),
            text["filesystem.read".len()..]
                .trim_start_matches([':', '-', ' '])
                .to_string(),
        );
    }

    ("assistant.reply".to_string(), text.to_string())
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
    fn parses_model_plan_json_and_dependencies() {
        let plan = parse_plan(
            r#"[{"id":"read","tool_name":"filesystem.read","description":"read context","depends_on":[]}]"#,
        );
        assert_eq!(plan[0].tool_name, "filesystem.read");
    }

    #[tokio::test]
    async fn resumed_runs_skip_workspace_read() {
        let temp = tempfile::tempdir().expect("tempdir");
        let demo_file = temp.path().join("context.md");
        std::fs::write(&demo_file, "# Demo\nHello from workspace.").expect("write");

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let gateway = evohime_model_gateway::mock_gateway(vec![
            r#"[{"id":"step-1","tool_name":"assistant.reply","description":"Respond","depends_on":[] }]"#.into(),
            "Recovered response".into(),
        ]);
        let tools = evohime_tool_runtime::ToolRegistry::bootstrap();

        let result = run_agent_loop_resumed(
            AgentConfig {
                task_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                user_message: "Explain the file".to_string(),
                created_at: chrono::Utc::now(),
                demo_file_path: demo_file.clone(),
                workspace_root: temp.path().to_path_buf(),
            },
            &gateway,
            &tools,
            vec![ChatMessage {
                role: ChatRole::User,
                content: "previous".to_string(),
            }],
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
