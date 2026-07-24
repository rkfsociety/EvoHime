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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ScriptStep {
    Tool { tool: String, input: serde_json::Value },
    Reply { reply: String },
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Expectations {
    #[serde(default)]
    pub final_message_contains: Vec<String>,
    #[serde(default)]
    pub files: Vec<FileExpectation>,
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
}

impl EvalReport {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
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
                tool_calls: vec![NativeToolCall {
                    id: format!("golden-{index}"),
                    name: tool.clone(),
                    arguments: input.to_string(),
                }],
                usage: None,
            },
            ScriptStep::Reply { reply } => ChatResult {
                content: reply.clone(),
                tool_calls: vec![],
                usage: None,
            },
        })
        .collect()
}

pub fn check_expectations(
    expect: &Expectations,
    final_message: &str,
    workspace: &Path,
) -> Vec<String> {
    let mut failures = Vec::new();
    for needle in &expect.final_message_contains {
        if !final_message.contains(needle) {
            failures.push(format!(
                "final message does not contain `{needle}` (got: `{}`)",
                final_message.chars().take(200).collect::<String>()
            ));
        }
    }
    for file in &expect.files {
        let path = workspace.join(&file.path);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if let Some(needle) = &file.contains {
                    if !content.contains(needle) {
                        failures.push(format!(
                            "file `{}` does not contain `{needle}`",
                            file.path
                        ));
                    }
                }
            }
            Err(error) => failures.push(format!("file `{}` unreadable: {error}", file.path)),
        }
    }
    failures
}

pub async fn run_golden_task(task: &GoldenTask) -> EvalReport {
    let workspace = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => {
            return EvalReport {
                name: task.name.clone(),
                failures: vec![format!("temp workspace failed: {error}")],
            }
        }
    };
    for (relative, content) in &task.workspace_files {
        let path = workspace.path().join(relative);
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return EvalReport {
                    name: task.name.clone(),
                    failures: vec![format!("seed dir `{relative}` failed: {error}")],
                };
            }
        }
        if let Err(error) = std::fs::write(&path, content) {
            return EvalReport {
                name: task.name.clone(),
                failures: vec![format!("seed file `{relative}` failed: {error}")],
            };
        }
    }
    let demo_file = workspace.path().join(".evohime-eval-context.md");
    let _ = std::fs::write(&demo_file, "# Eval context");

    let provider =
        MockProvider::with_tool_call_sequence("golden-mock", script_to_results(&task.script));
    let gateway = ModelGateway::from_provider(std::sync::Arc::new(provider));
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
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let result = run_agent_loop(
        AgentConfig {
            task_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
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
        &gateway,
        &tools,
        Vec::new(),
        Vec::new(),
        tx,
    )
    .await;

    let failures = match result {
        Ok(run) => check_expectations(&task.expect, &run.final_message, workspace.path()),
        Err(error) => vec![format!("agent loop failed: {error}")],
    };
    EvalReport { name: task.name.clone(), failures }
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
            ScriptStep::Reply { reply: "done".into() },
        ]);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_calls[0].name, "filesystem.read");
        assert_eq!(results[0].tool_calls[0].id, "golden-0");
        assert!(results[1].tool_calls.is_empty());
        assert_eq!(results[1].content, "done");
    }

    #[test]
    fn expectations_report_precise_failures() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("ok.txt"), "hello world").expect("write");

        let expect = Expectations {
            final_message_contains: vec!["present".into(), "absent".into()],
            files: vec![
                FileExpectation { path: "ok.txt".into(), contains: Some("hello".into()) },
                FileExpectation { path: "ok.txt".into(), contains: Some("nope".into()) },
                FileExpectation { path: "missing.txt".into(), contains: None },
            ],
        };
        let failures = check_expectations(&expect, "present here", dir.path());
        assert_eq!(failures.len(), 3);
        assert!(failures[0].contains("absent"));
        assert!(failures[1].contains("nope"));
        assert!(failures[2].contains("missing.txt"));
    }
}
