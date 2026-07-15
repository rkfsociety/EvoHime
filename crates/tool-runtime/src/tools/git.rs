use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::process::Command;

pub const STATUS_NAME: &str = "git.status";
pub const STATUS_DESCRIPTION: &str = "Show repository status";
pub const STATUS_PERMISSIONS: &[Permission] = &[Permission::GitRead];
pub const STATUS_TIMEOUT: Duration = Duration::from_secs(10);

pub const DIFF_NAME: &str = "git.diff";
pub const DIFF_DESCRIPTION: &str = "Show repository diff";
pub const DIFF_PERMISSIONS: &[Permission] = &[Permission::GitRead];
pub const DIFF_TIMEOUT: Duration = Duration::from_secs(10);

pub const COMMIT_NAME: &str = "git.commit";
pub const COMMIT_DESCRIPTION: &str = "Create a git commit";
pub const COMMIT_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const COMMIT_TIMEOUT: Duration = Duration::from_secs(20);

pub const PULL_NAME: &str = "git.pull";
pub const PULL_DESCRIPTION: &str = "Pull updates from the configured remote";
pub const PULL_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const PULL_TIMEOUT: Duration = Duration::from_secs(30);

pub const PUSH_NAME: &str = "git.push";
pub const PUSH_DESCRIPTION: &str = "Push commits to the configured remote";
pub const PUSH_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const PUSH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize, Default)]
struct DiffInput {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommitInput {
    message: String,
}

#[derive(Debug, Deserialize, Default)]
struct RemoteInput {
    remote: Option<String>,
    branch: Option<String>,
}

pub async fn status(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    ensure_no_input(input)?;
    run_git(ctx, &["status", "--short", "--branch"]).await
}

pub async fn diff(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input = parse_optional_input::<DiffInput>(&input, DIFF_NAME)?;
    let mut args = vec!["diff", "--no-ext-diff", "--unified=3"];
    if let Some(path) = input.path.as_deref() {
        args.push("--");
        args.push(path);
    }
    run_git(ctx, &args).await
}

pub async fn commit(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input: CommitInput =
        serde_json::from_value(input).map_err(|error| ToolError::InvalidInput {
            tool: COMMIT_NAME.to_string(),
            message: error.to_string(),
        })?;

    run_git(ctx, &["add", "-A"]).await?;
    run_git(ctx, &["commit", "-m", &input.message]).await
}

pub async fn pull(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input = parse_optional_input::<RemoteInput>(&input, PULL_NAME)?;
    let mut args = vec!["pull", "--ff-only"];
    if let Some(remote) = input.remote.as_deref() {
        args.push(remote);
    }
    if let Some(branch) = input.branch.as_deref() {
        args.push(branch);
    }
    run_git(ctx, &args).await
}

pub async fn push(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input = parse_optional_input::<RemoteInput>(&input, PUSH_NAME)?;
    let mut args = vec!["push"];
    if let Some(remote) = input.remote.as_deref() {
        args.push(remote);
    }
    if let Some(branch) = input.branch.as_deref() {
        args.push(branch);
    }
    run_git(ctx, &args).await
}

async fn run_git(ctx: &ToolContext, args: &[&str]) -> Result<ToolResult, ToolError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(&ctx.workspace_root);
    for arg in args {
        command.arg(arg);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = command
        .output()
        .await
        .map_err(|error| ToolError::Execution(format!("failed to run git: {error}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let command_text = format!("git -C {} {}", ctx.workspace_root.display(), args.join(" "));

    if !output.status.success() {
        let message = if stderr.is_empty() {
            stdout.clone()
        } else {
            stderr.clone()
        };
        return Err(ToolError::Execution(format!(
            "{command_text} failed: {message}"
        )));
    }

    Ok(ToolResult {
        output: if stdout.is_empty() {
            format!("{command_text} completed successfully")
        } else {
            stdout.clone()
        },
        structured: json!({
            "command": command_text,
            "stdout": stdout,
            "stderr": stderr,
            "status_code": output.status.code(),
        }),
    })
}

fn parse_optional_input<T: for<'de> Deserialize<'de> + Default>(
    input: &Value,
    tool: &str,
) -> Result<T, ToolError> {
    if input.is_null()
        || input
            .as_object()
            .map(|object| object.is_empty())
            .unwrap_or(false)
    {
        Ok(T::default())
    } else {
        serde_json::from_value(input.clone()).map_err(|error| ToolError::InvalidInput {
            tool: tool.to_string(),
            message: error.to_string(),
        })
    }
}

fn ensure_no_input(input: Value) -> Result<(), ToolError> {
    if input.is_null()
        || input
            .as_object()
            .map(|object| object.is_empty())
            .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(ToolError::InvalidInput {
            tool: STATUS_NAME.to_string(),
            message: "this tool does not accept input".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs as std_fs, path::Path, process::Command as StdCommand};
    use tempfile::tempdir;
    use uuid::Uuid;

    fn run(workdir: &Path, args: &[&str]) {
        let status = StdCommand::new(args[0])
            .args(&args[1..])
            .current_dir(workdir)
            .status()
            .expect("git command");
        assert!(status.success(), "command failed: {args:?}");
    }

    fn init_repo() -> (tempfile::TempDir, ToolContext, String) {
        let dir = tempdir().expect("tempdir");
        run(dir.path(), &["git", "init"]);
        run(dir.path(), &["git", "branch", "-M", "main"]);
        run(
            dir.path(),
            &["git", "config", "user.email", "dev@example.com"],
        );
        run(dir.path(), &["git", "config", "user.name", "Dev User"]);
        let workspace_root = dir.path().to_path_buf();
        (
            dir,
            ToolContext {
                workspace_root,
                task_id: Uuid::nil(),
            },
            "main".to_string(),
        )
    }

    #[tokio::test]
    async fn status_reports_changes() {
        let (_dir, ctx, _) = init_repo();
        std_fs::write(ctx.workspace_root.join("notes.txt"), "hello\n").expect("write");

        let result = status(&ctx, Value::Null).await.expect("status succeeds");
        assert!(result.output.contains("notes.txt"));
        assert_eq!(
            result.structured["command"],
            Value::String(format!(
                "git -C {} status --short --branch",
                ctx.workspace_root.display()
            ))
        );
    }

    #[tokio::test]
    async fn diff_shows_patch_for_file() {
        let (_dir, ctx, _) = init_repo();
        std_fs::write(ctx.workspace_root.join("notes.txt"), "hello\n").expect("write");
        run(ctx.workspace_root.as_path(), &["git", "add", "."]);

        std_fs::write(ctx.workspace_root.join("notes.txt"), "hello\nworld\n").expect("write");

        let result = diff(
            &ctx,
            json!({
                "path": "notes.txt"
            }),
        )
        .await
        .expect("diff succeeds");

        assert!(result.output.contains("world"));
    }

    #[tokio::test]
    async fn commit_creates_commit() {
        let (_dir, ctx, _) = init_repo();
        std_fs::write(ctx.workspace_root.join("notes.txt"), "hello\n").expect("write");

        let result = commit(
            &ctx,
            json!({
                "message": "Add notes"
            }),
        )
        .await
        .expect("commit succeeds");

        assert!(
            result.output.contains("Add notes") || result.output.contains("completed successfully")
        );
    }

    #[tokio::test]
    async fn pull_and_push_support_local_remote() {
        let remote = tempdir().expect("remote");
        run(remote.path(), &["git", "init", "--bare"]);

        let (_dir, ctx, branch) = init_repo();
        run(
            ctx.workspace_root.as_path(),
            &[
                "git",
                "remote",
                "add",
                "origin",
                remote.path().to_str().expect("remote path"),
            ],
        );

        std_fs::write(ctx.workspace_root.join("notes.txt"), "hello\n").expect("write");
        run(ctx.workspace_root.as_path(), &["git", "add", "."]);
        run(
            ctx.workspace_root.as_path(),
            &["git", "commit", "-m", "seed"],
        );

        let push_result = push(
            &ctx,
            json!({
                "remote": "origin",
                "branch": branch
            }),
        )
        .await
        .expect("push succeeds");
        assert!(!push_result.output.is_empty());

        let pull_result = pull(
            &ctx,
            json!({
                "remote": "origin",
                "branch": branch
            }),
        )
        .await
        .expect("pull succeeds");
        assert!(!pull_result.output.is_empty());
    }
}
