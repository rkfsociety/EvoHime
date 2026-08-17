use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

pub const NAME: &str = "process.run";
pub const DESCRIPTION: &str =
    "Run a process with improved timeout and streaming (replaces shell.execute)";
pub const PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
pub const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
struct Input {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    env: Option<std::collections::HashMap<String, String>>,
}

pub async fn execute(ctx: &ToolContext, value: serde_json::Value) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|e| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: e.to_string(),
    })?;

    let cwd = if let Some(path) = input.cwd {
        ctx.sandbox()?.resolve_existing(&path)?
    } else {
        ctx.workspace_root.clone()
    };

    let mut cmd = tokio::process::Command::new(&input.command);
    cmd.current_dir(&cwd).args(&input.args);

    if let Some(env) = input.env {
        for (key, val) in env {
            cmd.env(&key, &val);
        }
    }

    let timeout_duration = if let Some(ms) = input.timeout_ms {
        Duration::from_millis(ms)
    } else {
        TIMEOUT
    };

    let result = tokio::time::timeout(timeout_duration, cmd.output()).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let output_text = if !stdout.is_empty() {
                stdout.clone()
            } else {
                stderr.clone()
            };

            Ok(ToolResult {
                output: output_text,
                structured: json!({
                    "exit_code": output.status.code(),
                    "success": output.status.success(),
                    "stdout": stdout,
                    "stderr": stderr
                }),
            })
        }
        Ok(Err(e)) => Err(ToolError::Execution(format!(
            "process execution failed: {}",
            e
        ))),
        Err(_) => Err(ToolError::TimedOut(timeout_duration)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn process_run_echo_works() {
        let ctx = ToolContext {
            workspace_root: std::env::temp_dir(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let (command, args) = if cfg!(windows) {
            ("cmd", vec!["/C", "echo", "hello", "world"])
        } else {
            ("echo", vec!["hello", "world"])
        };
        let result = execute(&ctx, json!({"command": command, "args": args}))
            .await
            .expect("run");

        assert!(result.output.contains("hello"));
    }
}
