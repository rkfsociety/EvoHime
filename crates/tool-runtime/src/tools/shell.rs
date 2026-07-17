use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::{process::Command, select, time::timeout};
use tokio_util::sync::CancellationToken;

pub const NAME: &str = "shell.execute";
pub const DESCRIPTION: &str = "Execute a program directly inside the workspace";
pub const PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
pub const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT: usize = 1024 * 1024;

#[derive(Deserialize)]
struct Input {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
}

pub async fn execute(
    ctx: &ToolContext,
    value: Value,
    cancellation: CancellationToken,
) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|e| ToolError::InvalidInput {
        tool: NAME.into(),
        message: e.to_string(),
    })?;
    if input.program.is_empty()
        || input.program.contains(['/', '\\'])
        || matches!(
            input.program.to_ascii_lowercase().as_str(),
            "cmd"
                | "cmd.exe"
                | "powershell"
                | "powershell.exe"
                | "pwsh"
                | "pwsh.exe"
                | "sh"
                | "bash"
        )
    {
        return Err(ToolError::InvalidInput {
            tool: NAME.into(),
            message: "program must be an executable name".into(),
        });
    }
    let sandbox = ctx.sandbox()?;
    let cwd = match input.cwd.as_deref() {
        Some(path) => sandbox.resolve_existing(path)?,
        None => sandbox.root().to_path_buf(),
    };
    let mut command = Command::new(&input.program);
    command
        .args(&input.args)
        .current_dir(cwd.clone())
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::shell_env::apply_scrubbed_env(&mut command);
    let child = command
        .spawn()
        .map_err(|e| ToolError::Execution(format!("spawn failed: {e}")))?;
    let duration = Duration::from_millis(
        input
            .timeout_ms
            .unwrap_or(TIMEOUT.as_millis() as u64)
            .min(TIMEOUT.as_millis() as u64),
    );
    let result = select! {
        _ = cancellation.cancelled() => {
            return Err(ToolError::Execution("tool cancelled".into()));
        }
        result = timeout(duration, child.wait_with_output()) => {
            result
                .map_err(|_| ToolError::TimedOut(duration))?
                .map_err(|e| ToolError::Execution(format!("process failed: {e}")))?
        }
    };
    let stdout = String::from_utf8_lossy(&result.stdout)
        .chars()
        .take(MAX_OUTPUT)
        .collect::<String>();
    let stderr = String::from_utf8_lossy(&result.stderr)
        .chars()
        .take(MAX_OUTPUT)
        .collect::<String>();
    let exit_code = result.status.code();
    let output = format!(
        "program: {}\ncwd: {}\nexit_code: {}\nstdout:\n{}\nstderr:\n{}",
        input.program,
        cwd.display(),
        exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "terminated".to_string()),
        if stdout.is_empty() {
            "<empty>"
        } else {
            &stdout
        },
        if stderr.is_empty() {
            "<empty>"
        } else {
            &stderr
        }
    );
    Ok(ToolResult {
        output,
        structured: json!({
            "program": input.program,
            "cwd": cwd.display().to_string(),
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code,
            "timed_out": false
        }),
    })
}
