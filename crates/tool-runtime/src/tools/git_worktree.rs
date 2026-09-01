use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;

pub const CREATE_NAME: &str = "git.worktree.create";
pub const CREATE_DESCRIPTION: &str = "Create a Core-owned detached Git worktree for one task";
pub const CREATE_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const CREATE_TIMEOUT: Duration = Duration::from_secs(30);
pub const REMOVE_NAME: &str = "git.worktree.remove";
pub const REMOVE_DESCRIPTION: &str = "Remove a clean Core-owned detached task worktree";
pub const REMOVE_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const REMOVE_TIMEOUT: Duration = Duration::from_secs(30);
pub const PREFLIGHT_NAME: &str = "git.worktree.preflight";
pub const PREFLIGHT_DESCRIPTION: &str =
    "Inspect a task worktree for safe integration without applying changes";
pub const PREFLIGHT_PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct CreateInput {
    worktree_id: String,
    base_commit: String,
}

fn safe_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'/'))
        && !value.starts_with('-')
}

pub async fn create(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input: CreateInput =
        serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
            tool: CREATE_NAME.into(),
            message: e.to_string(),
        })?;
    if !safe_token(&input.worktree_id, 128) || !safe_token(&input.base_commit, 128) {
        return Err(ToolError::InvalidInput {
            tool: CREATE_NAME.into(),
            message: "worktree id and base commit must be bounded safe tokens".into(),
        });
    }
    let root = ctx
        .workspace_root
        .join(".evohime")
        .join("worktrees")
        .join(&input.worktree_id);
    if root.exists() {
        return Err(ToolError::Execution(
            "worktree destination already exists".into(),
        ));
    }
    tokio::fs::create_dir_all(root.parent().expect("worktree parent"))
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&ctx.workspace_root)
        .arg("worktree")
        .arg("add")
        .arg("--detach")
        .arg(&root)
        .arg(&input.base_commit)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("git worktree unavailable: {e}")))?;
    if !output.status.success() {
        return Err(ToolError::Execution(
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(2000)
                .collect(),
        ));
    }
    Ok(ToolResult {
        output: format!("created detached worktree {}", input.worktree_id),
        structured: json!({"worktree_id": input.worktree_id, "root_ref": format!(".evohime/worktrees/{}", input.worktree_id), "base_commit": input.base_commit, "detached": true, "status": "ready"}),
    })
}

#[derive(Debug, Deserialize)]
struct RemoveInput {
    worktree_id: String,
}

pub async fn remove(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input: RemoveInput =
        serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
            tool: REMOVE_NAME.into(),
            message: e.to_string(),
        })?;
    if !safe_token(&input.worktree_id, 128) {
        return Err(ToolError::InvalidInput {
            tool: REMOVE_NAME.into(),
            message: "invalid worktree id".into(),
        });
    }
    let root = ctx
        .workspace_root
        .join(".evohime")
        .join("worktrees")
        .join(&input.worktree_id);
    let status = Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("status")
        .arg("--porcelain")
        .output()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    if !status.status.success() {
        return Err(ToolError::Execution(
            "worktree status is unavailable; cleanup is blocked".into(),
        ));
    }
    if !status.stdout.is_empty() {
        return Err(ToolError::Execution(
            "unintegrated changes prevent cleanup".into(),
        ));
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&ctx.workspace_root)
        .arg("worktree")
        .arg("remove")
        .arg(&root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    if !output.status.success() {
        return Err(ToolError::Execution(
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(2000)
                .collect(),
        ));
    }
    Ok(ToolResult {
        output: format!("removed clean worktree {}", input.worktree_id),
        structured: json!({"worktree_id": input.worktree_id, "status": "removed", "unintegrated_changes": false}),
    })
}

#[derive(Debug, Deserialize)]
struct PreflightInput {
    worktree_id: String,
    base_commit: String,
}

pub async fn preflight(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let input: PreflightInput =
        serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
            tool: PREFLIGHT_NAME.into(),
            message: e.to_string(),
        })?;
    if !safe_token(&input.worktree_id, 128) || !safe_token(&input.base_commit, 128) {
        return Err(ToolError::InvalidInput {
            tool: PREFLIGHT_NAME.into(),
            message: "invalid worktree or base token".into(),
        });
    }
    let root = ctx
        .workspace_root
        .join(".evohime")
        .join("worktrees")
        .join(&input.worktree_id);
    let head = Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let base = Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("rev-parse")
        .arg(&input.base_commit)
        .output()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let status = Command::new("git")
        .arg("-C")
        .arg(&root)
        .arg("status")
        .arg("--porcelain")
        .output()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    if !head.status.success() || !base.status.success() || !status.status.success() {
        return Err(ToolError::Execution(
            "worktree preflight unavailable".into(),
        ));
    }
    let current = String::from_utf8_lossy(&head.stdout).trim().to_owned();
    let resolved_base = String::from_utf8_lossy(&base.stdout).trim().to_owned();
    let dirty = !status.stdout.is_empty();
    let base_match = current == resolved_base;
    Ok(ToolResult {
        output: format!(
            "worktree preflight: dirty={dirty}, base_match={}",
            base_match
        ),
        structured: json!({"worktree_id": input.worktree_id, "current_head": current, "base_commit": resolved_base, "base_match": base_match, "dirty": dirty, "integration": if dirty || !base_match { "conflict" } else { "ready" }}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use tempfile::tempdir;
    #[test]
    fn rejects_ref_injection() {
        assert!(!safe_token("--help", 128));
        assert!(!safe_token("a b", 128));
        assert!(safe_token("eva_task-1", 128));
    }

    #[tokio::test]
    async fn creates_real_detached_worktree_inside_workspace() {
        let dir = tempdir().unwrap();
        let run = |args: &[&str]| {
            StdCommand::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
        };
        assert!(run(&["init"]).status.success());
        assert!(run(&["config", "user.email", "test@example.invalid"])
            .status
            .success());
        assert!(run(&["config", "user.name", "Test"]).status.success());
        std::fs::write(dir.path().join("README.md"), "hello").unwrap();
        assert!(run(&["add", "README.md"]).status.success());
        assert!(run(&["commit", "-m", "seed"]).status.success());
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::new_v4(),
            session_id: None,
            progress_tx: None,
        };
        let result = create(&ctx, json!({"worktree_id":"task-1", "base_commit":"HEAD"}))
            .await
            .unwrap();
        assert_eq!(result.structured["detached"], true);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".evohime/worktrees/task-1/README.md"))
                .unwrap(),
            "hello"
        );
    }
}
