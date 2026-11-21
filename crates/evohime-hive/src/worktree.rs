//! Automatic Git worktree isolation for Hive tasks.
//!
//! Ordinary chats and the Hive can target the same repository at the same time.
//! To prevent concurrent edits and Git index lock contention, each Hive task
//! runs in its own detached worktree under `.evohime/worktrees/` whenever the
//! target directory is a Git repository with an existing `HEAD` commit.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use evohime_hive_contract::{HiveJournalProvider, HiveWorktreeLeaseRequest};
use tokio::process::Command;

/// Active detached worktree allocated for a single Hive task.
#[derive(Debug, Clone)]
pub struct HiveWorktreeLease {
    /// Base Git repository root directory.
    pub base_root: PathBuf,
    /// Isolated detached worktree root directory.
    pub worktree_root: PathBuf,
    /// Stable identifier of the worktree lease.
    pub worktree_id: String,
}

fn sanitize_worktree_id(task_id: &str) -> String {
    let suffix: String = task_id
        .bytes()
        .filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        .take(48)
        .map(char::from)
        .collect();
    if suffix.is_empty() {
        format!("hive-{}", uuid::Uuid::new_v4())
    } else {
        format!("hive-{suffix}")
    }
}

async fn run_git(root: &Path, args: &[&str]) -> Option<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()
}

/// Ensures an isolated detached worktree exists for a Hive task when
/// `base_root` is a Git repository with a valid `HEAD`.
///
/// Returns `None` when `base_root` is not a Git repository (for example, in
/// plain directory tests) or when worktree creation fails, allowing the caller
/// to fall back cleanly without breaking non-Git workspaces.
pub async fn prepare_for_task(
    journal: Option<&dyn HiveJournalProvider>,
    task_id: &str,
    base_root: &Path,
) -> Option<HiveWorktreeLease> {
    let head_output = run_git(base_root, &["rev-parse", "--verify", "HEAD"]).await?;
    if !head_output.status.success() {
        return None;
    }
    let base_commit = String::from_utf8_lossy(&head_output.stdout)
        .trim()
        .to_owned();
    if base_commit.is_empty() {
        return None;
    }

    let worktree_id = sanitize_worktree_id(task_id);
    let worktree_root = base_root
        .join(".evohime")
        .join("worktrees")
        .join(&worktree_id);

    if !worktree_root.exists() {
        if let Some(parent) = worktree_root.parent() {
            if tokio::fs::create_dir_all(parent).await.is_err() {
                return None;
            }
        }
        let worktree_str = worktree_root.to_string_lossy().into_owned();
        let add_output = run_git(
            base_root,
            &["worktree", "add", "--detach", &worktree_str, &base_commit],
        )
        .await?;
        if !add_output.status.success() {
            tracing::warn!(
                task_id = %task_id,
                stderr = %String::from_utf8_lossy(&add_output.stderr).trim(),
                "failed to create isolated hive worktree; falling back to base workspace"
            );
            return None;
        }
    }

    if let Some(journal) = journal {
        journal
            .register_worktree_lease(HiveWorktreeLeaseRequest {
                worktree_id: worktree_id.clone(),
                task_id: task_id.to_owned(),
                base_root: base_root.to_string_lossy().into_owned(),
                branch: "detached".into(),
                root_ref: format!(".evohime/worktrees/{worktree_id}"),
                base_commit,
            })
            .await;
    }

    Some(HiveWorktreeLease {
        base_root: base_root.to_path_buf(),
        worktree_root,
        worktree_id,
    })
}

/// Removes a Hive task worktree after execution if and only if its working tree
/// is completely clean. Dirty worktrees are preserved so unintegrated work is
/// never lost.
///
/// A detached workflow started by this task may still run inside the worktree
/// after the agent has answered. Removing the directory then would break the
/// child agents' reads, so cleanup waits until every workflow run owned by the
/// task reaches a terminal state.
pub async fn cleanup_if_clean(
    journal: Option<&dyn HiveJournalProvider>,
    lease: &HiveWorktreeLease,
    task_id: &str,
) {
    if !lease.worktree_root.exists() {
        return;
    }
    if let Some(journal) = journal {
        if journal.has_active_workflow_run(task_id).await {
            return;
        }
    }
    let Some(status_output) = run_git(&lease.worktree_root, &["status", "--porcelain"]).await
    else {
        return;
    };
    if !status_output.status.success()
        || !String::from_utf8_lossy(&status_output.stdout)
            .trim()
            .is_empty()
    {
        return;
    }

    let worktree_str = lease.worktree_root.to_string_lossy().into_owned();
    let Some(remove_output) =
        run_git(&lease.base_root, &["worktree", "remove", &worktree_str]).await
    else {
        return;
    };
    if !remove_output.status.success() {
        return;
    }

    if let Some(journal) = journal {
        journal
            .mark_worktree_cleanup_pending(&lease.worktree_id)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_worktree_id_strips_unsafe_characters() {
        assert_eq!(
            sanitize_worktree_id("hive-123/abc:xyz"),
            "hive-hive-123abcxyz"
        );
        assert!(sanitize_worktree_id("///").starts_with("hive-"));
    }
}
