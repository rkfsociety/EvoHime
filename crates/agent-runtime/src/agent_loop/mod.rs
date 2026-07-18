//! Agent orchestration: plan → execute → replan → respond.
//!
//! Split in Stage 7.29: [`parse`], [`plan`], [`execute`], [`context`], [`util`].

mod context;
mod execute;
mod parse;
mod plan;
mod tool_budget;
mod util;

pub use tool_budget::{budget_tool_result_list, budget_tool_results, ToolResultBudget};

use chrono::{DateTime, Utc};
use evohime_model_gateway::{providers::ChatMessage, providers::ChatRole, ModelGateway};
use evohime_project_index::ProjectIndex;
use evohime_protocol::{PlanStep, ServerEvent};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::StreamExt;
use serde_json::json;
use std::{collections::HashSet, path::PathBuf};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use context::{build_memory_context, build_workspace_rules, relative_workspace_path};
use execute::{execute_plan_steps, requires_mutation};
use parse::{format_plan, parse_model_tool_calls};
use plan::{
    collect_plan_steps, parse_replan_decision, plan_needs_replan_cycle, planning_prompt_for_tools,
    ReplanDecision, REPLAN_PROMPT,
};
use util::{
    collect_llm_stream_with_telemetry, emit, MAX_REPLAN_ROUNDS, MODEL_REQUEST_COOLDOWN,
    PLANNING_TIMEOUT, RESPONSE_TIMEOUT,
};

#[derive(Clone)]
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
    /// Optional, untrusted playbook hints for the planning phase.
    pub planning_memory_context: Option<String>,
    /// When set, enables on-demand `memory.search` against PostgreSQL.
    pub memory_pool: Option<sqlx::PgPool>,
    /// Workspace/project scope key for memory retrieval (same as prompt inject).
    pub workspace_key: String,
    /// True when this loop was spawned via `agent.run`.
    pub is_subagent: bool,
    /// Nesting depth (root = 0).
    pub subagent_depth: u32,
    /// Hard cap on plan steps for this loop (subagents).
    pub subagent_max_steps: Option<usize>,
    /// Optional LLM usage sink (server metrics / TokenJam counters).
    pub telemetry: Option<std::sync::Arc<dyn crate::llm_telemetry::LlmTelemetry>>,
}

#[derive(Debug, Clone)]
pub struct AgentRunResult {
    pub final_message: String,
    pub steps_run: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AgentResumeContext {
    pub workspace_context: Option<String>,
    pub plan: Option<Vec<PlanStep>>,
    pub completed_step_ids: Vec<String>,
    pub tool_results: Vec<String>,
    pub pause_reason: Option<String>,
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

pub async fn run_agent_loop_as_subagent(
    config: AgentConfig,
    gateway: &ModelGateway,
    tools: &ToolRegistry,
    event_tx: UnboundedSender<ServerEvent>,
) -> Result<AgentRunResult, AgentError> {
    run_agent_loop_inner(
        config,
        gateway,
        tools,
        Vec::new(),
        Vec::new(),
        event_tx,
        false,
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
        Some(resume),
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
    resume: Option<AgentResumeContext>,
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

    let resume = resume.unwrap_or_default();
    let tool_output = match resume.workspace_context.clone() {
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
                    session_id: Some(config.session_id),
                    progress_tx: None,
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
        content: if crate::native_tools::native_tool_calls_enabled() {
            crate::native_tools::native_planning_prompt()
        } else {
            planning_prompt_for_tools(tools)
        },
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
    if let Some(context) = &config.planning_memory_context {
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

    let (plan, mut plan_outputs, mut mutation_executed, mut satisfied_steps, truncated) =
        if let Some(existing_plan) = resume.plan.clone() {
            emit(
                &event_tx,
                ServerEvent::AgentPlanUpdated {
                    task_id: config.task_id,
                    plan: existing_plan.clone(),
                },
            )?;
            let satisfied: HashSet<String> = resume.completed_step_ids.iter().cloned().collect();
            let pending: Vec<PlanStep> = existing_plan
                .iter()
                .filter(|step| !satisfied.contains(&step.id))
                .cloned()
                .collect();
            let mut outputs = resume.tool_results.clone();
            let (new_outputs, mutation, newly_satisfied) = if pending.is_empty() {
                (Vec::new(), false, satisfied.clone())
            } else {
                execute_plan_steps(&pending, &satisfied, &config, gateway, tools, &event_tx).await?
            };
            outputs.extend(new_outputs);
            let outputs = budget_tool_result_list(&outputs, ToolResultBudget::from_env());
            (existing_plan, outputs, mutation, newly_satisfied, false)
        } else {
            let mut plan =
                match collect_plan_steps(gateway, &config, tools, &planning_messages).await {
                    Ok(plan) => plan,
                    Err(error) => return Err(error),
                };
            let mut truncated = false;
            if let Some(max_steps) = config.subagent_max_steps {
                if plan.len() > max_steps {
                    plan.truncate(max_steps);
                    truncated = true;
                }
            }

            emit(
                &event_tx,
                ServerEvent::AgentPlanUpdated {
                    task_id: config.task_id,
                    plan: plan.clone(),
                },
            )?;

            let (outputs, mutation, satisfied) =
                execute_plan_steps(&plan, &HashSet::new(), &config, gateway, tools, &event_tx)
                    .await?;
            let outputs = budget_tool_result_list(&outputs, ToolResultBudget::from_env());
            (plan, outputs, mutation, satisfied, truncated)
        };

    let mut accumulated_plan = plan.clone();
    let resumed_with_plan = resume.plan.is_some();
    let had_incomplete_steps = plan
        .iter()
        .any(|step| !resume.completed_step_ids.iter().any(|id| id == &step.id));
    if !config.is_subagent
        && plan_needs_replan_cycle(&plan)
        && (!resumed_with_plan || had_incomplete_steps)
    {
        for round in 0..MAX_REPLAN_ROUNDS {
            tokio::time::sleep(MODEL_REQUEST_COOLDOWN).await;
            let observe = {
                let budgeted = budget_tool_results(&plan_outputs, ToolResultBudget::from_env());
                if budgeted.is_empty() {
                    "(no tool results yet)".to_string()
                } else {
                    budgeted
                }
            };
            let mut replan_messages = Vec::with_capacity(history.len() + 5);
            replan_messages.push(ChatMessage {
                role: ChatRole::System,
                content: REPLAN_PROMPT.to_string(),
            });
            if let Some(context) = &rules_context {
                replan_messages.push(ChatMessage {
                    role: ChatRole::System,
                    content: context.clone(),
                });
            }
            if let Some(context) = &config.planning_memory_context {
                replan_messages.push(ChatMessage {
                    role: ChatRole::System,
                    content: context.clone(),
                });
            }
            replan_messages.extend(history.clone());
            replan_messages.push(ChatMessage {
                role: ChatRole::User,
                content: format!(
                    "User request:\n{}\n\nCurrent plan:\n{}\n\nTool results so far:\n{}\n\nRound {} of {}.",
                    config.user_message,
                    format_plan(&accumulated_plan),
                    observe,
                    round + 1,
                    MAX_REPLAN_ROUNDS
                ),
            });

            let raw_replan = collect_llm_stream_with_telemetry(
                &config,
                gateway,
                &config.planning_model_route,
                config.planning_model.as_deref(),
                "replan",
                gateway.stream_chat_for_route_with_model(
                    &config.planning_model_route,
                    config.planning_model.as_deref(),
                    &replan_messages,
                )?,
                PLANNING_TIMEOUT,
            )
            .await?;

            match parse_replan_decision(&raw_replan.text) {
                ReplanDecision::Done => break,
                ReplanDecision::Continue(steps) => {
                    let existing: HashSet<String> = accumulated_plan
                        .iter()
                        .map(|step| step.id.clone())
                        .collect();
                    let new_steps: Vec<PlanStep> = steps
                        .into_iter()
                        .filter(|step| !existing.contains(&step.id))
                        .collect();
                    if new_steps.is_empty() {
                        break;
                    }
                    accumulated_plan.extend(new_steps.iter().cloned());
                    emit(
                        &event_tx,
                        ServerEvent::AgentPlanUpdated {
                            task_id: config.task_id,
                            plan: accumulated_plan.clone(),
                        },
                    )?;
                    let (outputs, round_mutation, round_satisfied) = execute_plan_steps(
                        &new_steps,
                        &satisfied_steps,
                        &config,
                        gateway,
                        tools,
                        &event_tx,
                    )
                    .await?;
                    plan_outputs.extend(outputs);
                    plan_outputs =
                        budget_tool_result_list(&plan_outputs, ToolResultBudget::from_env());
                    mutation_executed |= round_mutation;
                    satisfied_steps.extend(round_satisfied);
                }
            }
        }
    }

    tokio::time::sleep(MODEL_REQUEST_COOLDOWN).await;

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
    let tool_results_for_prompt = budget_tool_results(&plan_outputs, ToolResultBudget::from_env());
    let context = format!(
        "{}\n\nPlan tool results:\n{}",
        tool_output, tool_results_for_prompt
    );
    messages.push(ChatMessage {
        role: ChatRole::User,
        content: format!(
            "{}\n\nPlan:\n{}\n\nContext from `{}`:\n```\n{}\n```\n\nFinal answer rules: all requested tools have already been executed. Reply only with ordinary human-readable text. Never return JSON, tool.call, XML, or any internal protocol.",
            config.user_message,
            format_plan(&accumulated_plan),
            config.demo_file_path.display(),
            context
        ),
    });

    let mut final_message = String::new();
    let mut response_usage = None;
    let provider = gateway
        .route_provider_kind(&config.model_route)
        .map(|kind| kind.as_str().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let model_name = gateway
        .resolve_model_name(&config.model_route, config.model.as_deref())
        .unwrap_or_else(|_| "unknown".into());
    let llm_meta = crate::llm_telemetry::LlmCallMeta {
        phase: "respond",
        provider,
        model: model_name,
        session_id: config.session_id,
        task_id: config.task_id,
    };
    let (llm_span, llm_started) = crate::llm_telemetry::start_llm_span(&llm_meta);
    let mut stream = gateway.stream_chat_for_route_with_model(
        &config.model_route,
        config.model.as_deref(),
        &messages,
    )?;

    let stream_result = {
        let _guard = llm_span.enter();
        tokio::time::timeout(RESPONSE_TIMEOUT, async {
            while let Some(chunk) = stream.next().await {
                match chunk? {
                    evohime_model_gateway::ChatStreamItem::Delta(delta) => {
                        final_message.push_str(&delta);
                        emit(
                            &event_tx,
                            ServerEvent::AgentMessageDelta {
                                task_id: config.task_id,
                                delta,
                            },
                        )?;
                    }
                    evohime_model_gateway::ChatStreamItem::Usage(usage) => {
                        response_usage = Some(usage);
                    }
                }
            }
            Ok::<(), AgentError>(())
        })
        .await
    };
    let ok = stream_result.is_ok()
        && stream_result
            .as_ref()
            .ok()
            .map(|r| r.is_ok())
            .unwrap_or(false);
    crate::llm_telemetry::finish_llm_span(
        &llm_span,
        llm_started,
        &llm_meta,
        response_usage,
        ok,
        config.telemetry.as_ref(),
    );
    stream_result.map_err(|_| AgentError::ModelTimeout {
        phase: "response",
        timeout_seconds: RESPONSE_TIMEOUT.as_secs(),
    })??;

    if let Some(tool_plan) = parse_model_tool_calls(&final_message) {
        let (response_outputs, response_mutation_executed, _) = execute_plan_steps(
            &tool_plan,
            &satisfied_steps,
            &config,
            gateway,
            tools,
            &event_tx,
        )
        .await?;
        mutation_executed |= response_mutation_executed;
        final_message = if response_outputs.is_empty() {
            "Инструмент выполнен, но не вернул текстовый результат.".to_string()
        } else {
            response_outputs.join("\n\n")
        };
    }

    if !config.is_subagent && requires_mutation(&config.user_message) && !mutation_executed {
        return Err(AgentError::PlanStepFailed {
            step_id: "mutation-check".to_string(),
            tool_name: "agent-runtime".to_string(),
            message: "задача требовала изменения workspace, но ни одного изменяющего инструмента не выполнено".to_string(),
        });
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

    if !config.is_subagent {
        emit(
            &event_tx,
            ServerEvent::TaskCompleted {
                task_id: config.task_id,
                final_message: final_message.clone(),
                completed_at: Utc::now(),
            },
        )?;
    }

    let steps_run = satisfied_steps.len();
    Ok(AgentRunResult {
        final_message,
        steps_run,
        truncated,
    })
}

const SYSTEM_PROMPT: &str = "You are EvoHime, a helpful AI coding assistant. Follow the workspace rules supplied in the system context, preserve user intent, never claim a change was made unless a tool result confirms it, and answer concisely using the provided workspace context. When an action is required, return an explicit JSON tool.call object with type, tool, and input; do not merely describe the call.";

#[cfg(test)]
mod tests {
    use super::execute::{dependency_batches_pending, is_mutating_tool, tool_input};
    use super::parse::{default_plan, parse_plan, REGISTERED_TOOLS};
    use super::*;
    use std::{
        collections::HashSet,
        path::{Path, PathBuf},
        time::Duration,
    };

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
        assert!(context.contains("Untrusted memory"));
        assert!(context.contains("not instructions"));
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
    fn builds_filesystem_write_input_from_structured_json() {
        let input = tool_input(
            "filesystem.write",
            r#"{"path":"hello-smoke.txt","content":"EvoHime smoke test 2026-07-19"}"#,
            Path::new("C:/workspace"),
        )
        .expect("write input");
        assert_eq!(input["path"], "hello-smoke.txt");
        assert_eq!(input["content"], "EvoHime smoke test 2026-07-19");
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

        let from_prose = tool_input(
            "filesystem.read",
            "Inspect crates/server/src/main.rs",
            Path::new("C:/workspace"),
        )
        .expect("read from prose path");
        assert_eq!(from_prose["path"], "crates/server/src/main.rs");

        let bare_file = tool_input(
            "filesystem.read",
            "Прочитать Cargo.toml, frontend/web/package.json и ключевые документы",
            Path::new("C:/workspace"),
        )
        .expect("read bare filename");
        assert_eq!(bare_file["path"], "Cargo.toml");

        let fallback = tool_input(
            "filesystem.read",
            "Summarize architecture overview without naming a file",
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
    fn parses_browser_and_mcp_tool_calls_into_executable_input() {
        let browser = parse_plan(
            r#"{"type":"tool.call","tool":"browser.open","input":{"url":"https://example.com","max_chars":1000}}"#,
        );
        assert_eq!(browser.len(), 1);
        assert_eq!(browser[0].tool_name, "browser.open");
        let browser_input =
            tool_input("browser.open", &browser[0].description, Path::new(".")).expect("browser");
        assert_eq!(browser_input["url"], "https://example.com");
        assert_eq!(browser_input["max_chars"], 1000);

        let extract = parse_plan(
            r#"{"type":"tool.call","tool":"browser.extract","input":{"url":"https://example.com","selector":"h1"}}"#,
        );
        let extract_input = tool_input("browser.extract", &extract[0].description, Path::new("."))
            .expect("extract");
        assert_eq!(extract_input["selector"], "h1");

        let mcp = parse_plan(
            r#"{"type":"tool.call","tool":"mcp.call","input":{"url":"https://mcp.example/rpc","method":"tools/list","params":{}}}"#,
        );
        let mcp_input = tool_input("mcp.call", &mcp[0].description, Path::new(".")).expect("mcp");
        assert_eq!(mcp_input["method"], "tools/list");

        let memory = parse_plan(
            r#"[{"id":"m1","tool_name":"memory.search","description":"{\"query\":\"worktrees\",\"limit\":5}","depends_on":[]}]"#,
        );
        let memory_input =
            tool_input("memory.search", &memory[0].description, Path::new(".")).expect("memory");
        assert_eq!(memory_input["query"], "worktrees");
        assert_eq!(memory_input["limit"], 5);
    }

    #[test]
    fn registered_tools_cover_bootstrap_registry() {
        let registry = ToolRegistry::bootstrap();
        let names = registry
            .list()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<std::collections::HashSet<_>>();
        for tool in REGISTERED_TOOLS {
            if *tool == "assistant.reply" {
                continue;
            }
            assert!(
                names.contains(tool),
                "REGISTERED_TOOLS entry `{tool}` missing from ToolRegistry::bootstrap()"
            );
        }
        assert_eq!(names.len(), REGISTERED_TOOLS.len() - 1);
    }

    #[test]
    fn keeps_model_requests_apart_for_literouter_rate_limit() {
        assert!(MODEL_REQUEST_COOLDOWN >= Duration::from_secs(5));
    }

    #[test]
    fn identifies_mutating_tools() {
        assert!(is_mutating_tool("filesystem.write"));
        assert!(is_mutating_tool("shell.execute"));
        assert!(!is_mutating_tool("filesystem.read"));
    }

    #[test]
    fn detects_when_user_requested_a_mutation() {
        assert!(requires_mutation("Реализуй следующий пункт"));
        assert!(requires_mutation("Implement the feature"));
        assert!(requires_mutation("Добавь логирование в server"));
        assert!(!requires_mutation("Прочитай файл и объясни его содержимое"));
        // Trace regression: advisory "добавить?" must not trip mutation-check.
        assert!(!requires_mutation(
            "изучи дорожную карту и код, что можно улучшить или добавить?"
        ));
        assert!(!requires_mutation("What can we improve or add next?"));
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
                planning_memory_context: None,
                memory_pool: None,
                workspace_key: String::new(),
                is_subagent: false,
                subagent_depth: 0,
                subagent_max_steps: None,
                telemetry: None,
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

    #[tokio::test]
    async fn does_not_expose_response_tool_call_as_final_message() {
        let temp = tempfile::tempdir().expect("tempdir");
        let demo_file = temp.path().join("context.md");
        std::fs::write(&demo_file, "# Demo").expect("write demo");
        std::fs::write(temp.path().join("answer.txt"), "answer from file").expect("write answer");

        let provider = RecordingProvider::new(vec![
            vec![
                r#"[{"id":"step-1","tool_name":"assistant.reply","description":"Respond","depends_on":[]}]"#
                    .to_string(),
            ],
            vec![
                r#"{"type":"tool.call","tool":"filesystem.read","input":{"path":"answer.txt"}}"#.to_string(),
            ],
        ]);
        let gateway =
            evohime_model_gateway::ModelGateway::from_provider(std::sync::Arc::new(provider));
        let tools = evohime_tool_runtime::ToolRegistry::bootstrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        let result = run_agent_loop(
            AgentConfig {
                task_id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                user_message: "read answer.txt".to_string(),
                created_at: chrono::Utc::now(),
                demo_file_path: demo_file,
                workspace_root: temp.path().to_path_buf(),
                model_route: "default".to_string(),
                model: None,
                planning_model_route: "default".to_string(),
                planning_model: None,
                planning_memory_context: None,
                memory_pool: None,
                workspace_key: String::new(),
                is_subagent: false,
                subagent_depth: 0,
                subagent_max_steps: None,
                telemetry: None,
            },
            &gateway,
            &tools,
            vec![],
            vec![],
            tx,
        )
        .await
        .expect("agent completes");

        assert!(!result.final_message.contains("tool.call"));
        assert!(result.final_message.contains("answer from file"));
    }

    #[test]
    fn parses_replan_done_and_continue() {
        assert_eq!(
            parse_replan_decision(r#"{"done":true}"#),
            ReplanDecision::Done
        );
        let decision = parse_replan_decision(
            r#"{"done":false,"steps":[{"id":"step-2","tool_name":"filesystem.read","description":"read `docs/a.md`","depends_on":["step-1"]}]}"#,
        );
        match decision {
            ReplanDecision::Continue(steps) => {
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].id, "step-2");
                assert_eq!(steps[0].depends_on, vec!["step-1".to_string()]);
            }
            ReplanDecision::Done => panic!("expected continue"),
        }
        assert_eq!(
            parse_replan_decision("not json at all"),
            ReplanDecision::Done
        );
    }

    #[test]
    fn dependency_batches_run_independent_steps_together() {
        let plan = vec![
            PlanStep {
                id: "a".into(),
                tool_name: "filesystem.read".into(),
                description: "read a".into(),
                depends_on: vec![],
            },
            PlanStep {
                id: "b".into(),
                tool_name: "filesystem.read".into(),
                description: "read b".into(),
                depends_on: vec![],
            },
            PlanStep {
                id: "c".into(),
                tool_name: "filesystem.write".into(),
                description: "write c".into(),
                depends_on: vec!["a".into(), "b".into()],
            },
        ];
        let batches = dependency_batches_pending(&plan, &HashSet::new()).expect("batches");
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[1][0].id, "c");
    }

    #[test]
    fn dependency_batches_honor_prior_satisfied_steps() {
        let pending = vec![PlanStep {
            id: "c".into(),
            tool_name: "filesystem.write".into(),
            description: "write c".into(),
            depends_on: vec!["a".into()],
        }];
        let mut satisfied = HashSet::new();
        satisfied.insert("a".into());
        let batches = dependency_batches_pending(&pending, &satisfied).expect("batches");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0][0].id, "c");
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
    async fn resume_with_saved_plan_skips_planning_call() {
        let temp = tempfile::tempdir().expect("tempdir");
        let demo_file = temp.path().join("context.md");
        std::fs::write(&demo_file, "# Demo\nHello from workspace.").expect("write");

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let provider = RecordingProvider::new(vec![vec!["Answer from checkpoint resume".into()]]);
        let gateway = evohime_model_gateway::ModelGateway::from_provider(std::sync::Arc::new(
            provider.clone(),
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
                planning_memory_context: None,
                memory_pool: None,
                workspace_key: String::new(),
                is_subagent: false,
                subagent_depth: 0,
                subagent_max_steps: None,
                telemetry: None,
            },
            &gateway,
            &tools,
            vec![],
            vec![],
            tx,
            AgentResumeContext {
                workspace_context: Some("cached context".into()),
                plan: Some(vec![PlanStep {
                    id: "step-1".into(),
                    tool_name: "assistant.reply".into(),
                    description: "Respond".into(),
                    depends_on: vec![],
                }]),
                completed_step_ids: vec!["step-1".into()],
                tool_results: vec![],
                pause_reason: Some("approval_required".into()),
            },
        )
        .await
        .expect("agent completes");

        assert_eq!(result.final_message, "Answer from checkpoint resume");
        let calls = provider.calls.lock().expect("calls");
        assert_eq!(
            calls.len(),
            1,
            "planning should be skipped on resume with plan"
        );
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
                planning_memory_context: None,
                memory_pool: None,
                workspace_key: String::new(),
                is_subagent: false,
                subagent_depth: 0,
                subagent_max_steps: None,
                telemetry: None,
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
                ..AgentResumeContext::default()
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
        Box::pin(futures_util::stream::iter(response.into_iter().map(
            |chunk| {
                Ok::<_, evohime_model_gateway::providers::ProviderError>(
                    evohime_model_gateway::ChatStreamItem::Delta(chunk),
                )
            },
        )))
    }
}
