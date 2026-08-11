use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    select,
    time::timeout,
};
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
            "cmd" | "cmd.exe" | "powershell" | "powershell.exe" | "sh" | "bash"
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
    let mut child = command
        .spawn()
        .map_err(|e| ToolError::Execution(format!("spawn failed: {e}")))?;
    let duration = Duration::from_millis(
        input
            .timeout_ms
            .unwrap_or(TIMEOUT.as_millis() as u64)
            .min(TIMEOUT.as_millis() as u64),
    );

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::Execution("missing stdout pipe".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ToolError::Execution("missing stderr pipe".into()))?;

    let progress = ctx.progress_tx.clone();
    let stdout_task = tokio::spawn(pump_stream(stdout, "stdout", progress.clone()));
    let stderr_task = tokio::spawn(pump_stream(stderr, "stderr", progress));

    let status = select! {
        _ = cancellation.cancelled() => {
            let _ = child.start_kill();
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(ToolError::Execution("tool cancelled".into()));
        }
        result = timeout(duration, child.wait()) => {
            result
                .map_err(|_| ToolError::TimedOut(duration))?
                .map_err(|e| ToolError::Execution(format!("process failed: {e}")))?
        }
    };

    let stdout_text = stdout_task
        .await
        .map_err(|e| ToolError::Execution(format!("stdout join: {e}")))?;
    let stderr_text = stderr_task
        .await
        .map_err(|e| ToolError::Execution(format!("stderr join: {e}")))?;

    let stdout = stdout_text.chars().take(MAX_OUTPUT).collect::<String>();
    let stderr = stderr_text.chars().take(MAX_OUTPUT).collect::<String>();
    let exit_code = status.code();
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

async fn pump_stream<R>(
    reader: R,
    stream: &'static str,
    progress: Option<tokio::sync::mpsc::UnboundedSender<crate::ToolProgress>>,
) -> String
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    let mut collected = String::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let chunk = format!("{line}\n");
        if let Some(tx) = &progress {
            let _ = tx.send(crate::ToolProgress {
                stream,
                delta: chunk.clone(),
            });
        }
        if collected.len() < MAX_OUTPUT {
            let remaining = MAX_OUTPUT.saturating_sub(collected.len());
            collected.push_str(&chunk.chars().take(remaining).collect::<String>());
        }
    }
    collected
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_permissions::{PermissionEngine, PermissionMode};
    use uuid::Uuid;

    #[tokio::test]
    async fn streams_stdout_progress_chunks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(Permission::ShellExecute, PermissionMode::Allow)
            .await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: Some(tx),
        };
        // Use a portable-ish program: on Windows shell interpreters are blocked; use git.
        // `git` is usually present in this repo's CI/dev machines.
        let result = execute(
            &ctx,
            json!({
                "program": "git",
                "args": ["--version"],
                "timeout_ms": 10_000u64
            }),
            CancellationToken::new(),
        )
        .await
        .expect("git --version");

        let mut deltas = Vec::new();
        while let Ok(progress) = rx.try_recv() {
            deltas.push(progress);
        }
        assert!(
            !deltas.is_empty(),
            "expected at least one stdout progress chunk"
        );
        assert!(deltas.iter().any(|p| p.stream == "stdout"));
        assert!(
            result.output.contains("git version") || result.structured["stdout"].as_str().is_some()
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn runs_pwsh_without_shell_wrapper() {
        let dir = tempfile::tempdir().expect("tempdir");
        let permissions = PermissionEngine::new();
        permissions
            .set_mode(Permission::ShellExecute, PermissionMode::Allow)
            .await;
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = execute(
            &ctx,
            json!({
                "program": "pwsh.exe",
                "args": ["-NoLogo", "-NoProfile", "-Command", "Write-Output evohime-pwsh-ok"]
            }),
            CancellationToken::new(),
        )
        .await
        .expect("pwsh should be allowed as a direct executable");

        assert_eq!(result.structured["exit_code"], 0);
        assert!(result.structured["stdout"]
            .as_str()
            .expect("pwsh stdout")
            .contains("evohime-pwsh-ok"));
    }
}
