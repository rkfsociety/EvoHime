use chrono::{DateTime, Utc};
use evohime_model_gateway::{providers::ChatMessage, providers::ChatRole, ModelGateway};
use evohime_project_index::ProjectIndex;
use evohime_protocol::{PlanStep, ServerEvent};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::StreamExt;
use serde_json::json;
use std::path::{Path, PathBuf};
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

    let parsed: Vec<PlanStep> = normalized
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
    let project_context =
        ProjectIndex::new(config.workspace_root.clone()).build_context(&config.user_message, 5);
    let memory_context = build_memory_context(&memory_notes);

    let mut planning_messages = Vec::with_capacity(history.len() + 4);
    planning_messages.push(ChatMessage {
        role: ChatRole::System,
        content: PLANNING_PROMPT.to_string(),
    });
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

    let raw_plan = collect_stream_text(
        gateway.stream_chat_for_route_with_model(&config.planning_model_route, config.planning_model.as_deref(), &planning_messages)?,
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

    let mut messages = Vec::with_capacity(history.len() + 4);
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: SYSTEM_PROMPT.to_string(),
    });
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
    let mut stream = gateway.stream_chat_for_route_with_model(&config.model_route, config.model.as_deref(), &messages)?;

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
            .map(|envelope| {
                let drafts = if !envelope.steps.is_empty() {
                    envelope.steps
                } else {
                    envelope.plan
                };
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
