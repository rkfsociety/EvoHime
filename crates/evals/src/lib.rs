//! Golden-task eval harness (Stage 7.101, wave 1).
//!
//! A golden task pins down one agent scenario: user message → deterministic
//! model script → real tools in a temp workspace → checkable outcome. The
//! real `run_agent_loop` and `ToolRegistry::bootstrap()` are exercised; only
//! the model is replaced by a scripted `MockProvider`, because LLM quality is
//! the one thing EvoHime does not control.

use evohime_agent_runtime::{run_agent_loop, AgentConfig};
use evohime_model_gateway::providers::mock::MockProvider;
use evohime_model_gateway::{ChatResult, ModelGateway, NativeToolCall};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct GoldenTask {
    pub name: String,
    pub user_message: String,
    #[serde(default)]
    pub workspace_files: BTreeMap<String, String>,
    pub script: Vec<ScriptStep>,
    pub expect: Expectations,
    /// Grading criteria for LLM-as-judge in live mode; ignored in mock mode.
    #[serde(default)]
    pub rubric: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ScriptStep {
    Tool {
        tool: String,
        input: serde_json::Value,
    },
    Reply {
        reply: String,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Expectations {
    #[serde(default)]
    pub final_message_contains: Vec<String>,
    #[serde(default)]
    pub files: Vec<FileExpectation>,
    /// Wave 3B: thinking quality expectations
    #[serde(default)]
    pub thinking_contains: Vec<String>,
    /// Minimum expected thinking tokens (for reasoning-heavy tasks)
    #[serde(default)]
    pub min_thinking_tokens: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileExpectation {
    pub path: String,
    #[serde(default)]
    pub contains: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EvalReport {
    pub name: String,
    pub failures: Vec<String>,
    pub judge: Option<JudgeVerdict>,
}

impl EvalReport {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JudgeVerdict {
    pub pass: bool,
    #[serde(default)]
    pub score: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
}

pub fn parse_golden_task(raw: &str) -> Result<GoldenTask, String> {
    let task: GoldenTask =
        serde_json::from_str(raw).map_err(|error| format!("golden task parse failed: {error}"))?;
    if task.script.is_empty() {
        return Err(format!("golden task `{}` has an empty script", task.name));
    }
    Ok(task)
}

/// Translate the script into the deterministic model response sequence.
pub fn script_to_results(script: &[ScriptStep]) -> Vec<ChatResult> {
    script
        .iter()
        .enumerate()
        .map(|(index, step)| match step {
            ScriptStep::Tool { tool, input } => ChatResult {
                content: String::new(),
                thinking: None,
                tool_calls: vec![NativeToolCall {
                    id: format!("golden-{index}"),
                    name: tool.clone(),
                    arguments: input.to_string(),
                }],
                usage: None,
            },
            ScriptStep::Reply { reply } => {
                let (content, thinking) = extract_thinking(reply);
                ChatResult {
                    content,
                    thinking,
                    tool_calls: vec![],
                    usage: None,
                }
            }
        })
        .collect()
}

/// Split a scripted reply's inline `*thinking: ...*` block (Wave 3B golden-task
/// DSL) out of the visible message, mimicking a provider that returns thinking
/// as a separate field rather than mixed into the message.
fn extract_thinking(reply: &str) -> (String, Option<String>) {
    const MARKER: &str = "*thinking:";
    let Some(start) = reply.find(MARKER) else {
        return (reply.to_string(), None);
    };
    let after_marker = start + MARKER.len();
    let Some(rel_end) = reply[after_marker..].rfind('*') else {
        return (reply.to_string(), None);
    };
    let end = after_marker + rel_end;
    let thinking = reply[after_marker..end].trim().to_string();
    let content = format!(
        "{}\n\n{}",
        reply[..start].trim_end(),
        reply[end + 1..].trim_start()
    )
    .trim()
    .to_string();
    (content, Some(thinking))
}

pub fn check_expectations(
    expect: &Expectations,
    final_message: &str,
    workspace: &Path,
    thinking: Option<&str>,
) -> Vec<String> {
    let mut failures = Vec::new();

    // Check final message content
    for needle in &expect.final_message_contains {
        if !final_message.contains(needle) {
            failures.push(format!(
                "final message does not contain `{needle}` (got: `{}`)",
                final_message.chars().take(200).collect::<String>()
            ));
        }
    }

    // Check thinking content (Wave 3B)
    if !expect.thinking_contains.is_empty() {
        match thinking {
            Some(content) => {
                for needle in &expect.thinking_contains {
                    // Normalize for robust matching (remove spaces, convert operators)
                    let normalized_needle = needle
                        .replace("×", "*")
                        .replace("·", "*")
                        .replace("÷", "/")
                        .replace("−", "-")
                        .replace(" ", "")
                        .to_lowercase();
                    let normalized_content = content
                        .replace("×", "*")
                        .replace("·", "*")
                        .replace("÷", "/")
                        .replace("−", "-")
                        .replace(" ", "")
                        .to_lowercase();

                    if !normalized_content.contains(&normalized_needle) {
                        failures.push(format!(
                            "thinking does not contain `{needle}` (got: `{}`)",
                            content.chars().take(200).collect::<String>()
                        ));
                    }
                }
            }
            None => {
                if !expect.thinking_contains.is_empty() {
                    failures.push("thinking content expected but not available".to_string());
                }
            }
        }
    }

    // Check minimum thinking tokens
    if let Some(min_tokens) = expect.min_thinking_tokens {
        let actual_tokens = thinking.map(|t| (t.len() / 4) as u32).unwrap_or(0);
        if actual_tokens < min_tokens {
            failures.push(format!(
                "thinking tokens: {actual_tokens} < {min_tokens} (expected)"
            ));
        }
    }

    // Check files
    for file in &expect.files {
        let path = workspace.join(&file.path);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if let Some(needle) = &file.contains {
                    if !content.contains(needle) {
                        failures.push(format!("file `{}` does not contain `{needle}`", file.path));
                    }
                }
            }
            Err(error) => failures.push(format!("file `{}` unreadable: {error}", file.path)),
        }
    }
    failures
}

/// Deterministic mock run: the model is scripted from `task.script`.
pub async fn run_golden_task(task: &GoldenTask) -> EvalReport {
    let provider =
        MockProvider::with_tool_call_sequence("golden-mock", script_to_results(&task.script));
    let gateway = ModelGateway::from_provider(std::sync::Arc::new(provider));
    run_golden_task_with_gateway(task, &gateway, false).await
}

fn failed_report(task: &GoldenTask, failure: String) -> EvalReport {
    EvalReport {
        name: task.name.clone(),
        failures: vec![failure],
        judge: None,
    }
}

/// Run a golden task against any gateway (scripted mock or a live provider).
/// With `judge` set, tasks that carry a `rubric` get an LLM verdict from the
/// same gateway; a failed or unparsable verdict fails the task.
pub async fn run_golden_task_with_gateway(
    task: &GoldenTask,
    gateway: &ModelGateway,
    judge: bool,
) -> EvalReport {
    let workspace = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => return failed_report(task, format!("temp workspace failed: {error}")),
    };
    for (relative, content) in &task.workspace_files {
        let path = workspace.path().join(relative);
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return failed_report(task, format!("seed dir `{relative}` failed: {error}"));
            }
        }
        if let Err(error) = std::fs::write(&path, content) {
            return failed_report(task, format!("seed file `{relative}` failed: {error}"));
        }
    }
    let demo_file = workspace.path().join(".evohime-eval-context.md");
    let _ = std::fs::write(&demo_file, "# Eval context");

    // Golden tasks run in a throwaway temp workspace, so protected tools are
    // allowed outright — there is no operator to answer an approval prompt.
    let permissions = evohime_permissions::PermissionEngine::new();
    for permission in [
        evohime_permissions::Permission::FilesystemWrite,
        evohime_permissions::Permission::ShellExecute,
    ] {
        permissions
            .set_mode(permission, evohime_permissions::PermissionMode::Allow)
            .await;
    }
    let tools = evohime_tool_runtime::ToolRegistry::bootstrap_with_permissions(permissions);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let result = run_agent_loop(
        AgentConfig {
            task_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            operator_id: Uuid::nil(),
            user_message: task.user_message.clone(),
            created_at: chrono::Utc::now(),
            demo_file_path: demo_file,
            workspace_root: workspace.path().to_path_buf(),
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
        gateway,
        &tools,
        Vec::new(),
        Vec::new(),
        tx,
    )
    .await;

    // Collect thinking from events (Wave 3B)
    let mut thinking = String::new();
    while let Ok(event) = rx.try_recv() {
        if let evohime_protocol::ServerEvent::AgentThinking {
            thinking: chunk, ..
        } = event
        {
            thinking.push_str(&chunk);
        }
    }

    let (mut failures, final_message) = match result {
        Ok(mut run) => {
            run.thinking = if thinking.is_empty() {
                None
            } else {
                Some(thinking)
            };
            (
                check_expectations(
                    &task.expect,
                    &run.final_message,
                    workspace.path(),
                    run.thinking.as_deref(),
                ),
                Some(run.final_message),
            )
        }
        Err(error) => (vec![format!("agent loop failed: {error}")], None),
    };

    let mut verdict = None;
    if judge {
        if let (Some(rubric), Some(final_message)) = (&task.rubric, &final_message) {
            match judge_final_answer(gateway, task, rubric, final_message).await {
                Ok(judged) => {
                    if !judged.pass {
                        failures.push(format!(
                            "judge verdict: fail ({})",
                            judged.reason.as_deref().unwrap_or("no reason given")
                        ));
                    }
                    verdict = Some(judged);
                }
                Err(error) => failures.push(format!("judge failed: {error}")),
            }
        }
    }

    EvalReport {
        name: task.name.clone(),
        failures,
        judge: verdict,
    }
}

pub fn judge_prompt(task: &GoldenTask, rubric: &str, final_answer: &str) -> String {
    format!(
        "You are a strict evaluator of an AI agent's answer.\n\
         Task given to the agent:\n{}\n\n\
         Agent's final answer:\n{}\n\n\
         Grading rubric:\n{}\n\n\
         Respond with ONLY a JSON object: \
         {{\"pass\": true|false, \"score\": 0-10, \"reason\": \"short explanation\"}}",
        task.user_message, final_answer, rubric
    )
}

/// Extract the first JSON object from judge output; refuses to guess —
/// an unparsable verdict is an error, never a silent pass.
pub fn parse_judge_verdict(raw: &str) -> Result<JudgeVerdict, String> {
    let start = raw
        .find('{')
        .ok_or("judge output contains no JSON object")?;
    let mut depth = 0usize;
    let mut end = None;
    for (offset, character) in raw[start..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + offset + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or("judge output has an unterminated JSON object")?;
    serde_json::from_str(&raw[start..end])
        .map_err(|error| format!("judge verdict parse failed: {error}"))
}

async fn judge_final_answer(
    gateway: &ModelGateway,
    task: &GoldenTask,
    rubric: &str,
    final_answer: &str,
) -> Result<JudgeVerdict, String> {
    use evohime_model_gateway::providers::{ChatMessage, ChatRole};
    use evohime_model_gateway::ChatStreamItem;
    use futures_util::StreamExt;

    let prompt = judge_prompt(task, rubric, final_answer);
    let mut stream = gateway.stream_chat(&[ChatMessage::text(ChatRole::User, prompt)]);
    let mut output = String::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(ChatStreamItem::Delta(delta)) => output.push_str(&delta),
            Ok(ChatStreamItem::Thinking(_thinking)) => {
                // Judge-verdict extraction — only the final text matters here,
                // so reasoning traces are intentionally discarded (extended
                // reasoning streaming to the UI is handled separately, see 7.111).
            }
            Ok(ChatStreamItem::Usage(_)) => {}
            Err(error) => return Err(format!("judge model call failed: {error}")),
        }
    }
    parse_judge_verdict(&output)
}

/// Load every golden task from a directory; parse errors are failures with
/// the offending file name, never silent skips.
pub fn load_golden_dir(dir: &Path) -> Result<Vec<GoldenTask>, String> {
    let mut tasks = Vec::new();
    let entries =
        std::fs::read_dir(dir).map_err(|error| format!("golden dir unreadable: {error}"))?;
    let mut paths: Vec<_> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    for path in paths {
        let raw = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let task =
            parse_golden_task(&raw).map_err(|error| format!("{}: {error}", path.display()))?;
        tasks.push(task);
    }
    if tasks.is_empty() {
        return Err(format!("no golden tasks found in {}", dir.display()));
    }
    Ok(tasks)
}

pub fn golden_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("golden")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_task_parses_tool_and_reply_steps() {
        let task = parse_golden_task(
            r#"{
                "name": "demo",
                "user_message": "do it",
                "workspace_files": { "a.txt": "seed" },
                "script": [
                    { "tool": "filesystem.read", "input": { "path": "a.txt" } },
                    { "reply": "done" }
                ],
                "expect": { "final_message_contains": ["done"] }
            }"#,
        )
        .expect("parse");
        assert_eq!(task.name, "demo");
        assert_eq!(task.script.len(), 2);
        assert!(matches!(task.script[0], ScriptStep::Tool { .. }));
        assert!(matches!(task.script[1], ScriptStep::Reply { .. }));
    }

    #[test]
    fn empty_script_is_rejected() {
        let error = parse_golden_task(
            r#"{ "name": "x", "user_message": "m", "script": [], "expect": {} }"#,
        )
        .expect_err("rejected");
        assert!(error.contains("empty script"));
    }

    #[test]
    fn script_converts_to_chat_results() {
        let results = script_to_results(&[
            ScriptStep::Tool {
                tool: "filesystem.read".into(),
                input: serde_json::json!({ "path": "a.txt" }),
            },
            ScriptStep::Reply {
                reply: "done".into(),
            },
        ]);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_calls[0].name, "filesystem.read");
        assert_eq!(results[0].tool_calls[0].id, "golden-0");
        assert!(results[1].tool_calls.is_empty());
        assert_eq!(results[1].content, "done");
    }

    #[test]
    fn judge_verdict_parses_plain_and_wrapped_json() {
        let verdict = parse_judge_verdict(r#"{"pass": true, "score": 9, "reason": "solid"}"#)
            .expect("plain json");
        assert!(verdict.pass);
        assert_eq!(verdict.score, Some(9.0));

        let wrapped = parse_judge_verdict(
            "Here is my verdict:\n```json\n{\"pass\": false, \"score\": 3, \"reason\": \"missed {braces}\"}\n``` done",
        )
        .expect("wrapped json");
        assert!(!wrapped.pass);
        assert_eq!(wrapped.reason.as_deref(), Some("missed {braces}"));

        assert!(parse_judge_verdict("no json here").is_err());
        assert!(parse_judge_verdict("{\"score\": 5}").is_err());
        assert!(parse_judge_verdict("{ unterminated").is_err());
    }

    #[test]
    fn judge_prompt_includes_task_answer_and_rubric() {
        let task = parse_golden_task(
            r#"{ "name": "j", "user_message": "solve it", "script": [{ "reply": "x" }], "expect": {} }"#,
        )
        .expect("task");
        let prompt = judge_prompt(&task, "must mention 42", "the answer is 42");
        assert!(prompt.contains("solve it"));
        assert!(prompt.contains("the answer is 42"));
        assert!(prompt.contains("must mention 42"));
        assert!(prompt.contains("\"pass\""));
    }

    #[tokio::test]
    async fn failing_judge_verdict_fails_the_task() {
        // MockProvider chunks feed both the agent's fallback reply and the
        // judge stream, so a single provider drives the whole path.
        let verdict_json = r#"{"pass": false, "score": 2, "reason": "too vague"}"#;
        let provider = MockProvider::new("judge-mock", vec![verdict_json.to_string()]);
        let gateway = ModelGateway::from_provider(std::sync::Arc::new(provider));
        let task = parse_golden_task(
            r#"{
                "name": "judged",
                "user_message": "answer well",
                "script": [{ "reply": "ignored in gateway mode" }],
                "expect": {},
                "rubric": "answer must be specific"
            }"#,
        )
        .expect("task");

        let report = run_golden_task_with_gateway(&task, &gateway, true).await;
        assert!(!report.passed());
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("too vague")));
        let verdict = report.judge.expect("verdict recorded");
        assert!(!verdict.pass);
        assert_eq!(verdict.score, Some(2.0));
    }

    #[tokio::test]
    async fn passing_judge_verdict_keeps_task_green() {
        let verdict_json = r#"{"pass": true, "score": 8, "reason": "good"}"#;
        let provider = MockProvider::new("judge-mock", vec![verdict_json.to_string()]);
        let gateway = ModelGateway::from_provider(std::sync::Arc::new(provider));
        let task = parse_golden_task(
            r#"{
                "name": "judged-pass",
                "user_message": "answer well",
                "script": [{ "reply": "ignored" }],
                "expect": {},
                "rubric": "anything"
            }"#,
        )
        .expect("task");

        let report = run_golden_task_with_gateway(&task, &gateway, true).await;
        assert!(report.passed(), "failures: {:?}", report.failures);
        assert!(report.judge.expect("verdict").pass);
    }

    #[test]
    fn expectations_report_precise_failures() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("ok.txt"), "hello world").expect("write");

        let expect = Expectations {
            final_message_contains: vec!["present".into(), "absent".into()],
            files: vec![
                FileExpectation {
                    path: "ok.txt".into(),
                    contains: Some("hello".into()),
                },
                FileExpectation {
                    path: "ok.txt".into(),
                    contains: Some("nope".into()),
                },
                FileExpectation {
                    path: "missing.txt".into(),
                    contains: None,
                },
            ],
            thinking_contains: vec![],
            min_thinking_tokens: None,
        };
        let failures = check_expectations(&expect, "present here", dir.path(), None);
        assert_eq!(failures.len(), 3);
        assert!(failures[0].contains("absent"));
        assert!(failures[1].contains("nope"));
        assert!(failures[2].contains("missing.txt"));
    }
}
