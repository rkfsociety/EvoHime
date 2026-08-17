use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;

// ============================================================================
// branch: Create, switch, or delete branches
// ============================================================================

pub const BRANCH_NAME: &str = "git.branch";
pub const BRANCH_DESCRIPTION: &str = "Create, switch, list, or delete git branches";
pub const BRANCH_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const BRANCH_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct BranchInput {
    action: String, // "create" | "switch" | "list" | "delete" | "rename"
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    from: Option<String>,
}

pub async fn branch(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: BranchInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: BRANCH_NAME.to_string(),
        message: e.to_string(),
    })?;

    match opts.action.as_str() {
        "list" => {
            let output = Command::new("git")
                .arg("branch")
                .arg("-a")
                .arg("--format=%(refname:short) %(objectname:short)")
                .current_dir(&ctx.workspace_root)
                .output()
                .await
                .map_err(|e| ToolError::Execution(format!("git branch list failed: {e}")))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(ToolResult {
                output: stdout.clone(),
                structured: json!({
                    "action": "list",
                    "branches": stdout.lines().collect::<Vec<_>>()
                }),
            })
        }
        "create" => {
            let name = opts.name.ok_or_else(|| ToolError::InvalidInput {
                tool: BRANCH_NAME.to_string(),
                message: "name is required for create action".to_string(),
            })?;

            let mut cmd = Command::new("git");
            cmd.arg("branch").arg(&name);

            if let Some(from) = opts.from {
                cmd.arg(&from);
            }

            let output = cmd
                .current_dir(&ctx.workspace_root)
                .output()
                .await
                .map_err(|e| ToolError::Execution(format!("git branch create failed: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ToolError::Execution(format!(
                    "git branch create failed: {}",
                    stderr
                )));
            }

            Ok(ToolResult {
                output: format!("Branch '{}' created", name),
                structured: json!({"action": "create", "branch": name}),
            })
        }
        "switch" => {
            let name = opts.name.ok_or_else(|| ToolError::InvalidInput {
                tool: BRANCH_NAME.to_string(),
                message: "name is required for switch action".to_string(),
            })?;

            let output = Command::new("git")
                .arg("switch")
                .arg(&name)
                .current_dir(&ctx.workspace_root)
                .output()
                .await
                .map_err(|e| ToolError::Execution(format!("git switch failed: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ToolError::Execution(format!(
                    "git switch failed: {}",
                    stderr
                )));
            }

            Ok(ToolResult {
                output: format!("Switched to branch '{}'", name),
                structured: json!({"action": "switch", "branch": name}),
            })
        }
        "delete" => {
            let name = opts.name.ok_or_else(|| ToolError::InvalidInput {
                tool: BRANCH_NAME.to_string(),
                message: "name is required for delete action".to_string(),
            })?;

            let output = Command::new("git")
                .arg("branch")
                .arg("-d")
                .arg(&name)
                .current_dir(&ctx.workspace_root)
                .output()
                .await
                .map_err(|e| ToolError::Execution(format!("git branch delete failed: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ToolError::Execution(format!(
                    "git branch delete failed: {}",
                    stderr
                )));
            }

            Ok(ToolResult {
                output: format!("Branch '{}' deleted", name),
                structured: json!({"action": "delete", "branch": name}),
            })
        }
        _ => Err(ToolError::InvalidInput {
            tool: BRANCH_NAME.to_string(),
            message: format!(
                "unknown action '{}', expected: create|switch|list|delete",
                opts.action
            ),
        }),
    }
}

// ============================================================================
// merge: Merge branches
// ============================================================================

pub const MERGE_NAME: &str = "git.merge";
pub const MERGE_DESCRIPTION: &str = "Merge a branch into the current branch";
pub const MERGE_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const MERGE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct MergeInput {
    branch: String,
    #[serde(default)]
    strategy: Option<String>, // "recursive" | "resolve" | "ours" | "theirs"
    #[serde(default)]
    no_ff: bool,
}

pub async fn merge(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: MergeInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: MERGE_NAME.to_string(),
        message: e.to_string(),
    })?;

    let mut cmd = Command::new("git");
    cmd.arg("merge");

    if opts.no_ff {
        cmd.arg("--no-ff");
    }

    if let Some(strategy) = &opts.strategy {
        cmd.arg("-s").arg(strategy);
    }

    cmd.arg(&opts.branch);

    let output = cmd
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("git merge failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::Execution(format!(
            "git merge failed: {}\n{}",
            stderr, stdout
        )));
    }

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "merge",
            "branch": opts.branch,
            "success": true
        }),
    })
}

// ============================================================================
// reset: Reset commits
// ============================================================================

pub const RESET_NAME: &str = "git.reset";
pub const RESET_DESCRIPTION: &str = "Reset HEAD to a previous commit (soft/mixed/hard)";
pub const RESET_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const RESET_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
struct ResetInput {
    commit: String,
    #[serde(default = "default_mode")]
    mode: String, // "soft" | "mixed" | "hard"
}

fn default_mode() -> String {
    "mixed".to_string()
}

pub async fn reset(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: ResetInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: RESET_NAME.to_string(),
        message: e.to_string(),
    })?;

    if !matches!(opts.mode.as_str(), "soft" | "mixed" | "hard") {
        return Err(ToolError::InvalidInput {
            tool: RESET_NAME.to_string(),
            message: format!("invalid mode '{}', expected: soft|mixed|hard", opts.mode),
        });
    }

    let output = Command::new("git")
        .arg("reset")
        .arg(format!("--{}", opts.mode))
        .arg(&opts.commit)
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("git reset failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::Execution(format!(
            "git reset failed: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "reset",
            "commit": opts.commit,
            "mode": opts.mode
        }),
    })
}

// ============================================================================
// revert: Create a new commit that undoes changes
// ============================================================================

pub const REVERT_NAME: &str = "git.revert";
pub const REVERT_DESCRIPTION: &str =
    "Create a new commit that undoes changes from a previous commit";
pub const REVERT_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const REVERT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
struct RevertInput {
    commit: String,
    #[serde(default)]
    message: Option<String>,
}

pub async fn revert(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: RevertInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: REVERT_NAME.to_string(),
        message: e.to_string(),
    })?;

    let mut cmd = Command::new("git");
    cmd.arg("revert").arg("--no-edit").arg(&opts.commit);

    if let Some(msg) = opts.message {
        cmd.arg("-m").arg(msg);
    }

    let output = cmd
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("git revert failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::Execution(format!(
            "git revert failed: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "revert",
            "commit": opts.commit
        }),
    })
}

// ============================================================================
// cherry-pick: Apply a commit from another branch
// ============================================================================

pub const CHERRY_PICK_NAME: &str = "git.cherry_pick";
pub const CHERRY_PICK_DESCRIPTION: &str = "Apply a commit from another branch";
pub const CHERRY_PICK_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const CHERRY_PICK_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
struct CherryPickInput {
    commit: String,
}

pub async fn cherry_pick(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: CherryPickInput =
        serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
            tool: CHERRY_PICK_NAME.to_string(),
            message: e.to_string(),
        })?;

    let output = Command::new("git")
        .arg("cherry-pick")
        .arg(&opts.commit)
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("git cherry-pick failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::Execution(format!(
            "git cherry-pick failed: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "cherry_pick",
            "commit": opts.commit
        }),
    })
}

// ============================================================================
// rebase: Rebase current branch onto another
// ============================================================================

pub const REBASE_NAME: &str = "git.rebase";
pub const REBASE_DESCRIPTION: &str = "Rebase current branch onto another branch";
pub const REBASE_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const REBASE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct RebaseInput {
    onto: String,
    #[serde(default)]
    interactive: bool,
}

pub async fn rebase(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: RebaseInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: REBASE_NAME.to_string(),
        message: e.to_string(),
    })?;

    let mut cmd = Command::new("git");
    cmd.arg("rebase");

    if opts.interactive {
        cmd.arg("-i");
    }

    cmd.arg(&opts.onto);

    let output = cmd
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("git rebase failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ToolError::Execution(format!(
            "git rebase failed: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "rebase",
            "onto": opts.onto,
            "interactive": opts.interactive
        }),
    })
}

// ============================================================================
// tag: Manage tags
// ============================================================================

pub const TAG_NAME: &str = "git.tag";
pub const TAG_DESCRIPTION: &str = "Create, list, or delete git tags";
pub const TAG_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const TAG_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
struct TagInput {
    action: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

pub async fn tag(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: TagInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: TAG_NAME.to_string(),
        message: e.to_string(),
    })?;
    let mut cmd = Command::new("git");
    cmd.arg("tag");
    match opts.action.as_str() {
        "list" => {
            cmd.arg("--list");
        }
        "create" => {
            let name = opts.name.ok_or_else(|| ToolError::InvalidInput {
                tool: TAG_NAME.to_string(),
                message: "name is required for create".to_string(),
            })?;
            if let Some(message) = opts.message {
                cmd.args(["-a", &name, "-m", &message]);
            } else {
                cmd.arg(&name);
            }
            if let Some(commit) = opts.commit {
                cmd.arg(commit);
            }
        }
        "delete" => {
            let name = opts.name.ok_or_else(|| ToolError::InvalidInput {
                tool: TAG_NAME.to_string(),
                message: "name is required for delete".to_string(),
            })?;
            cmd.args(["-d", &name]);
        }
        action => {
            return Err(ToolError::InvalidInput {
                tool: TAG_NAME.to_string(),
                message: format!("unknown action '{action}', expected list|create|delete"),
            })
        }
    }
    run_git_command(ctx, cmd, TAG_NAME, json!({"action": opts.action})).await
}

// ============================================================================
// stash: Manage the working tree stash
// ============================================================================

pub const STASH_NAME: &str = "git.stash";
pub const STASH_DESCRIPTION: &str = "List, save, apply, pop, or drop git stashes";
pub const STASH_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const STASH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
struct StashInput {
    action: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    index: Option<String>,
    #[serde(default)]
    include_untracked: bool,
}

pub async fn stash(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: StashInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: STASH_NAME.to_string(),
        message: e.to_string(),
    })?;
    let mut cmd = Command::new("git");
    cmd.arg("stash");
    match opts.action.as_str() {
        "list" => {
            cmd.arg("list");
        }
        "push" => {
            cmd.arg("push");
            if opts.include_untracked {
                cmd.arg("--include-untracked");
            }
            if let Some(message) = opts.message {
                cmd.args(["-m", &message]);
            }
        }
        "pop" | "apply" | "drop" => {
            cmd.arg(opts.action.as_str());
            if let Some(index) = opts.index {
                cmd.arg(index);
            }
        }
        action => {
            return Err(ToolError::InvalidInput {
                tool: STASH_NAME.to_string(),
                message: format!("unknown action '{action}', expected list|push|pop|apply|drop"),
            })
        }
    }
    run_git_command(ctx, cmd, STASH_NAME, json!({"action": opts.action})).await
}

// ============================================================================
// remote: Inspect or configure remotes
// ============================================================================

pub const REMOTE_NAME: &str = "git.remote";
pub const REMOTE_DESCRIPTION: &str = "List, add, or remove git remotes";
pub const REMOTE_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
pub const REMOTE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
struct RemoteInput {
    action: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

pub async fn remote(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: RemoteInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: REMOTE_NAME.to_string(),
        message: e.to_string(),
    })?;
    let mut cmd = Command::new("git");
    cmd.arg("remote");
    match opts.action.as_str() {
        "list" => {
            cmd.args(["-v"]);
        }
        "add" => {
            cmd.arg("add");
            cmd.arg(opts.name.ok_or_else(|| ToolError::InvalidInput {
                tool: REMOTE_NAME.to_string(),
                message: "name is required for add".to_string(),
            })?);
            cmd.arg(opts.url.ok_or_else(|| ToolError::InvalidInput {
                tool: REMOTE_NAME.to_string(),
                message: "url is required for add".to_string(),
            })?);
        }
        "remove" => {
            cmd.arg("remove");
            cmd.arg(opts.name.ok_or_else(|| ToolError::InvalidInput {
                tool: REMOTE_NAME.to_string(),
                message: "name is required for remove".to_string(),
            })?);
        }
        action => {
            return Err(ToolError::InvalidInput {
                tool: REMOTE_NAME.to_string(),
                message: format!("unknown action '{action}', expected list|add|remove"),
            })
        }
    }
    run_git_command(ctx, cmd, REMOTE_NAME, json!({"action": opts.action})).await
}

async fn run_git_command(
    ctx: &ToolContext,
    mut command: Command,
    tool: &str,
    structured: Value,
) -> Result<ToolResult, ToolError> {
    let output = command
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("{tool} failed: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "{tool} failed: {stderr}{stdout}"
        )));
    }
    Ok(ToolResult {
        output: stdout,
        structured,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[tokio::test]
    async fn branch_list_works() {
        let dir = tempdir().expect("tempdir");
        Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .await
            .expect("git init");
        std_fs::write(dir.path().join("seed.txt"), "seed").expect("seed");
        Command::new("git")
            .args(["add", "seed.txt"])
            .current_dir(&dir)
            .output()
            .await
            .expect("git add");
        Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "seed",
            ])
            .current_dir(&dir)
            .output()
            .await
            .expect("git commit");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = branch(&ctx, json!({"action": "list"}))
            .await
            .expect("branch list");

        assert!(result.output.contains("main") || result.output.contains("master"));
    }
}
