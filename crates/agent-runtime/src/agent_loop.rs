use chrono::{DateTime, Utc};
use evohime_model_gateway::{providers::ChatMessage, providers::ChatRole, ModelGateway};
use evohime_protocol::ServerEvent;
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::StreamExt;
use serde_json::json;
use std::path::PathBuf;
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    pub depends_on: Vec<String>,
}

pub fn parse_plan(raw: &str) -> Vec<PlanStep> {
    if let Ok(plan) = serde_json::from_str::<Vec<PlanStep>>(raw) {
        return plan;
    }
    raw.lines().enumerate().filter_map(|(index, line)| {
        let description = line.trim().trim_start_matches(['-', '*', ' ']).to_string();
        if description.is_empty() { None } else { Some(PlanStep { id: format!("step-{}", index + 1), tool_name: "none".to_string(), description, depends_on: Vec::new() }) }
    }).collect()
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
    emit(
        &event_tx,
        ServerEvent::TaskStarted {
            task_id: config.task_id,
            session_id: config.session_id,
            user_message: config.user_message.clone(),
            created_at: config.created_at,
        },
    )?;

    emit(
        &event_tx,
        ServerEvent::AgentPlanUpdated {
            task_id: config.task_id,
            plan: vec![
                "read workspace context".to_string(),
                "stream response from model".to_string(),
            ],
        },
    )?;

    emit(
        &event_tx,
        ServerEvent::ToolStarted {
            task_id: config.task_id,
            tool_name: "filesystem.read".to_string(),
        },
    )?;

    let relative_path = relative_workspace_path(&config.workspace_root, &config.demo_file_path);
    let tool_ctx = ToolContext {
        workspace_root: config.workspace_root.clone(),
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

    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: SYSTEM_PROMPT.to_string(),
    });
    messages.extend(history);
    messages.push(ChatMessage {
        role: ChatRole::User,
        content: format!(
            "{}\n\nContext from `{}`:\n```\n{}\n```",
            config.user_message,
            config.demo_file_path.display(),
            tool_result.output
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

fn emit(event_tx: &UnboundedSender<ServerEvent>, event: ServerEvent) -> Result<(), AgentError> {
    event_tx.send(event).map_err(|_| AgentError::EventChannel)
}

fn relative_workspace_path(workspace_root: &PathBuf, file_path: &PathBuf) -> String {
    file_path
        .strip_prefix(workspace_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| file_path.display().to_string())
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
        let plan = parse_plan(r#"[{"id":"read","tool_name":"filesystem.read","description":"read context","depends_on":[]}]"#);
        assert_eq!(plan[0].tool_name, "filesystem.read");
    }
}
