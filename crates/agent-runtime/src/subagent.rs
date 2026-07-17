//! Budgeted subagent fan-out (`agent.run`, Stage 7.31).

use crate::agent_loop::{run_agent_loop_as_subagent, AgentConfig, AgentError, AgentRunResult};
use evohime_model_gateway::ModelGateway;
use evohime_protocol::ServerEvent;
use evohime_tool_runtime::agent::{format_result, parse_input, AgentRunInput};
use evohime_tool_runtime::{ToolError, ToolRegistry, ToolResult};
use serde_json::Value;
use std::env;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{mpsc::UnboundedSender, Semaphore};
use tracing::warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubagentBudget {
    pub max_concurrent: usize,
    pub max_depth: u32,
    pub max_steps: usize,
    pub timeout: Duration,
}

impl SubagentBudget {
    pub fn from_env() -> Self {
        Self {
            max_concurrent: env_usize("EVOHIME_SUBAGENT_MAX_CONCURRENT", 2).clamp(1, 16),
            max_depth: env_u32("EVOHIME_SUBAGENT_MAX_DEPTH", 1).clamp(1, 4),
            max_steps: env_usize("EVOHIME_SUBAGENT_MAX_STEPS", 6).clamp(1, 32),
            timeout: Duration::from_millis(
                env_u64("EVOHIME_SUBAGENT_TIMEOUT_MS", 120_000).clamp(1_000, 600_000),
            ),
        }
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn concurrency_limiter() -> Arc<Semaphore> {
    static LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMITER
        .get_or_init(|| {
            let n = SubagentBudget::from_env().max_concurrent.max(1);
            Arc::new(Semaphore::new(n))
        })
        .clone()
}

pub async fn execute_agent_run(
    parent: &AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    event_tx: &UnboundedSender<ServerEvent>,
    input: Value,
) -> Result<ToolResult, ToolError> {
    let parsed = parse_input(&input)?;
    let budget = SubagentBudget::from_env();
    run_budgeted_subagent(parent, gateway, tools, event_tx, parsed, budget)
        .await
        .map_err(|error| ToolError::Execution(error.to_string()))
}

async fn run_budgeted_subagent(
    parent: &AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    event_tx: &UnboundedSender<ServerEvent>,
    input: AgentRunInput,
    budget: SubagentBudget,
) -> Result<ToolResult, AgentError> {
    let child_depth = parent.subagent_depth.saturating_add(1);
    if child_depth > budget.max_depth {
        return Err(AgentError::PlanStepFailed {
            step_id: "agent.run".into(),
            tool_name: "agent.run".into(),
            message: format!(
                "subagent depth {child_depth} exceeds EVOHIME_SUBAGENT_MAX_DEPTH={}",
                budget.max_depth
            ),
        });
    }

    let permit = concurrency_limiter()
        .acquire_owned()
        .await
        .map_err(|error| AgentError::PlanStepFailed {
            step_id: "agent.run".into(),
            tool_name: "agent.run".into(),
            message: format!("subagent concurrency closed: {error}"),
        })?;

    let max_steps = input
        .max_steps
        .unwrap_or(budget.max_steps)
        .min(budget.max_steps);
    let timeout = input
        .timeout_ms
        .map(Duration::from_millis)
        .unwrap_or(budget.timeout)
        .min(budget.timeout);

    let model_route = input
        .model_route
        .unwrap_or_else(|| parent.model_route.clone());

    let child = AgentConfig {
        task_id: parent.task_id,
        session_id: parent.session_id,
        user_message: input.prompt,
        created_at: chrono::Utc::now(),
        demo_file_path: parent.demo_file_path.clone(),
        workspace_root: parent.workspace_root.clone(),
        model_route: model_route.clone(),
        model: parent.model.clone(),
        planning_model_route: model_route,
        planning_model: parent.planning_model.clone(),
        memory_pool: parent.memory_pool.clone(),
        workspace_key: parent.workspace_key.clone(),
        is_subagent: true,
        subagent_depth: child_depth,
        subagent_max_steps: Some(max_steps),
    };

    let run = tokio::time::timeout(
        timeout,
        run_agent_loop_as_subagent(child, gateway, tools, event_tx.clone()),
    )
    .await;

    drop(permit);

    match run {
        Ok(Ok(AgentRunResult {
            final_message,
            steps_run,
            truncated,
        })) => Ok(format_result(
            &final_message,
            steps_run,
            truncated,
            child_depth,
        )),
        Ok(Err(error)) => Err(error),
        Err(_) => {
            warn!(
                timeout_ms = timeout.as_millis() as u64,
                "subagent timed out"
            );
            Err(AgentError::ModelTimeout {
                phase: "subagent",
                timeout_seconds: timeout.as_secs().max(1),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_from_env_clamps_defaults() {
        let budget = SubagentBudget::from_env();
        assert!((1..=16).contains(&budget.max_concurrent));
        assert!((1..=4).contains(&budget.max_depth));
        assert!((1..=32).contains(&budget.max_steps));
    }
}
