use crate::execution_policy_profiles::{
    apply_environment, reject_user_environment, validate_program_name, ExecutionPolicyProfile,
    ProcessGuard,
};
use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::io::AsyncRead;

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
    let resolved =
        ExecutionPolicyProfile::resolve(NAME).map_err(|error| ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: error.to_string(),
        })?;
    reject_user_environment(input.env.as_ref()).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;
    validate_program_name(&input.command).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;

    let cwd = if let Some(path) = input.cwd {
        ctx.sandbox()?.resolve_existing(&path)?
    } else {
        ctx.workspace_root.clone()
    };

    let mut cmd = tokio::process::Command::new(&input.command);
    cmd.current_dir(&cwd).args(&input.args);

    apply_environment(&mut cmd);

    let timeout_duration = if let Some(ms) = input.timeout_ms {
        resolved.timeout(Some(ms))
    } else {
        resolved.timeout(None)
    };
    cmd.kill_on_drop(true);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| ToolError::Execution(format!("process execution failed: {e}")))?;
    let _process_guard = ProcessGuard::attach(&child, &resolved)
        .map_err(|e| ToolError::Execution(format!("execution backend unavailable: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::Execution("missing stdout pipe".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ToolError::Execution("missing stderr pipe".into()))?;
    let stdout_task = tokio::spawn(read_limited(stdout, resolved.profile.max_output_bytes));
    let stderr_task = tokio::spawn(read_limited(stderr, resolved.profile.max_output_bytes));
    let result = tokio::time::timeout(timeout_duration, child.wait()).await;

    match result {
        Ok(Ok(status)) => {
            drop(_process_guard);
            let stdout = String::from_utf8_lossy(
                &stdout_task
                    .await
                    .map_err(|e| ToolError::Execution(format!("stdout join: {e}")))?
                    .map_err(ToolError::Execution)?,
            )
            .to_string();
            let stderr = String::from_utf8_lossy(
                &stderr_task
                    .await
                    .map_err(|e| ToolError::Execution(format!("stderr join: {e}")))?
                    .map_err(ToolError::Execution)?,
            )
            .to_string();

            let output_text = if !stdout.is_empty() {
                stdout.clone()
            } else {
                stderr.clone()
            };

            Ok(ToolResult {
                output: output_text,
                structured: json!({
                    "exit_code": status.code(),
                    "success": status.success(),
                    "stdout": stdout,
                    "stderr": stderr,
                    "resolved_profile": {
                        "profile_id": resolved.profile.profile_id,
                        "version": resolved.profile.version,
                        "hash": resolved.profile_hash,
                        "backend": resolved.backend
                    }
                }),
            })
        }
        Ok(Err(e)) => Err(ToolError::Execution(format!(
            "process execution failed: {}",
            e
        ))),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            Err(ToolError::TimedOut(timeout_duration))
        }
    }
}

async fn read_limited<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;
    let mut output = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn process_run_direct_executable_works() {
        let ctx = ToolContext {
            workspace_root: std::env::temp_dir(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let (command, args, expected_output) = if cfg!(windows) {
            // `cmd` is intentionally rejected by the execution profile. Use a
            // real executable so this test exercises direct process spawning.
            ("git", vec!["--version"], "git version")
        } else {
            ("echo", vec!["hello", "world"], "hello")
        };
        let result = execute(&ctx, json!({"command": command, "args": args}))
            .await
            .expect("run");

        assert!(result.output.contains(expected_output));
        assert_eq!(result.structured["resolved_profile"]["version"], 1);
        assert_eq!(
            result.structured["resolved_profile"]["backend"],
            if cfg!(windows) {
                "windows_job_object"
            } else {
                "portable"
            }
        );
    }

    #[tokio::test]
    async fn user_environment_is_rejected_before_spawn() {
        let ctx = ToolContext {
            workspace_root: std::env::temp_dir(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let result = execute(
            &ctx,
            json!({"command": "git", "env": {"EVOHIME_API_TOKEN": "secret"}}),
        )
        .await;
        assert!(matches!(result, Err(ToolError::InvalidInput { .. })));
    }
}
