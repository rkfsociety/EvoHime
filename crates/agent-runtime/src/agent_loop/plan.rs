//! Planning / replan prompts and native/text plan collection.
use super::parse::{normalize_plan, parse_plan, parse_plan_json, unwrap_code_fence, PlanStepDraft};
use super::util::{collect_llm_stream_with_telemetry, PLANNING_TIMEOUT};
use super::{AgentConfig, AgentError};
use evohime_model_gateway::{providers::ChatMessage, ModelGateway};
use evohime_protocol::PlanStep;
use evohime_tool_runtime::ToolRegistry;
use serde::Deserialize;

pub(crate) const PLANNING_PROMPT: &str = "You are EvoHime's task planner. Return only JSON: an array of objects with fields id, tool_name, description, and depends_on. Use only these tool names: filesystem.read, filesystem.list, filesystem.search, filesystem.write, filesystem.patch, shell.execute, git.status, git.diff, git.commit, git.pull, git.push, browser.open, browser.extract, mcp.call, memory.search, agent.run, worker.run, assistant.reply. Use stable step ids like step-1, step-2, and keep depends_on empty unless a step truly depends on another step. Put exact relative paths in backticks. For filesystem.write, include complete content in a fenced code block. For filesystem.patch, include complete patch text in a fenced code block. For shell.execute, include a JSON object with program, args, cwd, and timeout_ms in the description. For browser.open, put a JSON object with url (and optional max_chars) in the description. For browser.extract, put JSON with url, selector, and optional attribute/limit. For mcp.call, put JSON with url, method, and optional params. For memory.search, put JSON with query and optional limit. For agent.run, put JSON with prompt and optional max_steps/timeout_ms/model_route — use for parallel research/subtasks (empty depends_on to fan out). For worker.run, put JSON with task and payload (and optional timeout_ms) to run a Python worker job to completion. For git.commit, include the requested commit message in quotes. Use git.pull and git.push only when explicitly asked. If no tool call is needed, use assistant.reply.";

pub(crate) fn planning_prompt_for_tools(tools: &ToolRegistry) -> String {
    let mut names = tools
        .list()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    names.push("assistant.reply".to_string());
    names.sort();
    names.dedup();
    format!(
        "{PLANNING_PROMPT}\n\nRuntime-registered tools for this session: {}.",
        names.join(", ")
    )
}

pub(crate) async fn collect_plan_steps(
    gateway: &ModelGateway,
    config: &AgentConfig,
    tools: &ToolRegistry,
    planning_messages: &[ChatMessage],
) -> Result<Vec<PlanStep>, AgentError> {
    if crate::native_tools::native_tool_calls_enabled() {
        let openai_tools = crate::native_tools::openai_tools_for_registry(tools);
        match gateway
            .chat_with_tools_for_route(
                &config.planning_model_route,
                config.planning_model.as_deref(),
                planning_messages,
                &openai_tools,
            )
            .await
        {
            Ok(result) if result.has_tool_calls() => {
                tracing::info!(
                    tool_calls = result.tool_calls.len(),
                    "planning via native provider tool_calls"
                );
                record_chat_result_telemetry(gateway, config, "plan", &result);
                return Ok(crate::native_tools::plan_from_native_tool_calls(
                    &result.tool_calls,
                ));
            }
            Ok(result) if !result.content.trim().is_empty() => {
                tracing::info!(
                    "native tools returned content without tool_calls; parsing text plan"
                );
                record_chat_result_telemetry(gateway, config, "plan", &result);
                return Ok(parse_plan(&result.content));
            }
            Ok(_) => {
                tracing::warn!("native tools returned empty result; falling back to text planning");
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "native tool_calls planning failed; falling back to text planning"
                );
            }
        }
    }

    let raw_plan = collect_llm_stream_with_telemetry(
        config,
        gateway,
        &config.planning_model_route,
        config.planning_model.as_deref(),
        "plan",
        gateway.stream_chat_for_route_with_model(
            &config.planning_model_route,
            config.planning_model.as_deref(),
            planning_messages,
        )?,
        PLANNING_TIMEOUT,
    )
    .await?;
    Ok(parse_plan(&raw_plan.text))
}

fn record_chat_result_telemetry(
    gateway: &ModelGateway,
    config: &AgentConfig,
    phase: &'static str,
    result: &evohime_model_gateway::ChatResult,
) {
    let provider = gateway
        .route_provider_kind(&config.planning_model_route)
        .map(|kind| kind.as_str().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let model = gateway
        .resolve_model_name(
            &config.planning_model_route,
            config.planning_model.as_deref(),
        )
        .unwrap_or_else(|_| "unknown".into());
    let meta = crate::llm_telemetry::LlmCallMeta {
        phase,
        provider,
        model,
        session_id: config.session_id,
        task_id: config.task_id,
    };
    let (span, started) = crate::llm_telemetry::start_llm_span(&meta);
    let _guard = span.enter();
    crate::llm_telemetry::finish_llm_span(
        &span,
        started,
        &meta,
        result.usage,
        true,
        config.telemetry.as_ref(),
    );
}

pub(crate) const REPLAN_PROMPT: &str = "You are EvoHime's replanner. Return ONLY JSON. If enough tool results exist to answer the user, return {\"done\":true}. Otherwise return {\"done\":false,\"steps\":[...]} with ONLY new steps still needed (id, tool_name, description, depends_on). Use the same tool names as planning. Do not repeat completed steps. Prefer the fewest new steps.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReplanDecision {
    Done,
    Continue(Vec<PlanStep>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReplanResponse {
    done: bool,
    #[serde(default)]
    steps: Vec<PlanStepDraft>,
}

pub(crate) fn plan_needs_replan_cycle(plan: &[PlanStep]) -> bool {
    plan.iter()
        .any(|step| step.tool_name != "assistant.reply" && !step.tool_name.is_empty())
}

pub(crate) fn parse_replan_decision(raw: &str) -> ReplanDecision {
    let normalized = unwrap_code_fence(raw);
    if normalized.trim().is_empty() {
        return ReplanDecision::Done;
    }

    if let Ok(response) = serde_json::from_str::<ReplanResponse>(&normalized) {
        if response.done {
            return ReplanDecision::Done;
        }
        let steps = normalize_plan(
            response
                .steps
                .into_iter()
                .enumerate()
                .map(|(index, draft)| PlanStep {
                    id: draft
                        .id
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| format!("step-{}", index + 1)),
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
                .collect(),
        );
        if steps.is_empty() {
            return ReplanDecision::Done;
        }
        return ReplanDecision::Continue(steps);
    }

    if let Some(plan) = parse_plan_json(&normalized) {
        let plan = normalize_plan(plan);
        if plan.is_empty() {
            ReplanDecision::Done
        } else {
            ReplanDecision::Continue(plan)
        }
    } else {
        ReplanDecision::Done
    }
}
