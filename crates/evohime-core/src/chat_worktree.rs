//! Optional isolated Git worktree sandbox for parallel chat sessions.
//!
//! When a chat task requests `IsolationMode::IsolatedWorktree`, Core allocates
//! a detached `git worktree` in a service directory outside the source tree,
//! records it in `EventJournal` with `idempotency_key = "chat_session:<task_id>"`,
//! and provides summary/apply/discard + crash recovery without silently
//! falling back to `shared_workspace` or losing uncommitted edits.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

use crate::EventJournal;

const IDEMPOTENCY_PREFIX: &str = "chat_session:";

/// Active detached worktree allocated for a single chat task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatWorktreeLease {
    pub(crate) base_root: PathBuf,
    pub(crate) worktree_root: PathBuf,
    pub(crate) worktree_id: String,
    pub(crate) task_id: String,
    pub(crate) base_commit: String,
}

/// Summary of uncommitted changes observed inside a chat worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatWorktreeSummary {
    pub(crate) worktree_id: String,
    pub(crate) task_id: String,
    pub(crate) worktree_root: PathBuf,
    pub(crate) changed_files: Vec<String>,
    pub(crate) diff_stat: String,
}

fn sanitize_worktree_id(task_id: &str) -> String {
    let suffix: String = task_id
        .bytes()
        .filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        .take(48)
        .map(char::from)
        .collect();
    if suffix.is_empty() {
        format!("chat-{}", uuid::Uuid::new_v4())
    } else {
        format!("chat-{suffix}")
    }
}

fn default_chat_worktree_base_dir(base_root: &Path) -> PathBuf {
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("EvoHime"));
    data_dir
        .join("chat-worktrees")
        .join(crate::task_memory::workspace_scope_id(base_root))
}

async fn run_git(root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| error.to_string())
}

fn unavailable_error(reason: impl std::fmt::Display) -> String {
    format!("isolated_worktree_unavailable: {reason}; switch to shared_workspace")
}

/// Allocates a detached `git worktree` for a chat task outside `base_root`.
///
/// Fails with a typed `isolated_worktree_unavailable` error when `base_root` is
/// not a Git repository with a valid `HEAD` or when `git worktree add` fails.
pub(crate) async fn prepare_for_chat_task(
    journal: Option<&EventJournal>,
    task_id: &str,
    base_root: &Path,
    worktree_base_dir: Option<&Path>,
) -> Result<ChatWorktreeLease, String> {
    let head_output = run_git(base_root, &["rev-parse", "--verify", "HEAD"])
        .await
        .map_err(unavailable_error)?;
    if !head_output.status.success() {
        let stderr = String::from_utf8_lossy(&head_output.stderr);
        return Err(unavailable_error(stderr.trim()));
    }
    let base_commit = String::from_utf8_lossy(&head_output.stdout)
        .trim()
        .to_owned();
    if base_commit.is_empty() {
        return Err(unavailable_error("HEAD commit is empty"));
    }

    let worktree_id = sanitize_worktree_id(task_id);
    let service_parent = worktree_base_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_chat_worktree_base_dir(base_root));
    let worktree_root = service_parent.join(&worktree_id);

    if !worktree_root.exists() {
        if let Some(parent) = worktree_root.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(unavailable_error)?;
        }
        let worktree_str = worktree_root.to_string_lossy().into_owned();
        let add_output = run_git(
            base_root,
            &["worktree", "add", "--detach", &worktree_str, &base_commit],
        )
        .await
        .map_err(unavailable_error)?;
        if !add_output.status.success() {
            let stderr = String::from_utf8_lossy(&add_output.stderr);
            return Err(unavailable_error(stderr.trim()));
        }
    }

    if let Some(journal) = journal {
        let database = journal.database().lock().await;
        let record = evohime_local_storage::domains::workflow::TaskWorktreeRecord {
            worktree_id: worktree_id.clone(),
            task_id: task_id.to_owned(),
            repository_scope: crate::task_memory::workspace_scope_id(base_root),
            branch: "detached".into(),
            root_ref: worktree_root.to_string_lossy().into_owned(),
            base_commit: base_commit.clone(),
            state: "ready".into(),
            version: 1,
            idempotency_key: format!("{IDEMPOTENCY_PREFIX}{task_id}"),
            updated_at_ms: crate::task_memory::now_millis() as i64,
        };
        let _ = evohime_local_storage::domains::workflow::create(database.connection(), &record);
    }

    Ok(ChatWorktreeLease {
        base_root: base_root.to_path_buf(),
        worktree_root,
        worktree_id,
        task_id: task_id.to_owned(),
        base_commit,
    })
}

async fn collect_worktree_summary(
    worktree_root: &Path,
    worktree_id: &str,
    task_id: &str,
) -> Result<Option<ChatWorktreeSummary>, String> {
    if !worktree_root.exists() {
        return Ok(None);
    }
    let status_output = run_git(worktree_root, &["status", "--porcelain"]).await?;
    if !status_output.status.success() {
        return Err(String::from_utf8_lossy(&status_output.stderr)
            .trim()
            .to_owned());
    }
    let status_text = String::from_utf8_lossy(&status_output.stdout);
    let changed_files: Vec<String> = status_text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_end();
            if trimmed.len() <= 3 {
                None
            } else {
                Some(trimmed[3..].trim().to_owned())
            }
        })
        .collect();
    if changed_files.is_empty() {
        return Ok(None);
    }
    let diff_stat = match run_git(worktree_root, &["diff", "--stat", "HEAD"]).await {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        _ => String::new(),
    };
    Ok(Some(ChatWorktreeSummary {
        worktree_id: worktree_id.to_owned(),
        task_id: task_id.to_owned(),
        worktree_root: worktree_root.to_path_buf(),
        changed_files,
        diff_stat,
    }))
}

async fn remove_and_prune_worktree(
    base_root: &Path,
    worktree_root: &Path,
    force: bool,
) -> Result<(), String> {
    if worktree_root.exists() {
        let worktree_str = worktree_root.to_string_lossy().into_owned();
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&worktree_str);
        let remove_output = run_git(base_root, &args).await?;
        if !remove_output.status.success() {
            return Err(String::from_utf8_lossy(&remove_output.stderr)
                .trim()
                .to_owned());
        }
    }
    let _ = run_git(base_root, &["worktree", "prune"]).await;
    Ok(())
}

async fn transition_worktree_state(
    journal: Option<&EventJournal>,
    worktree_id: &str,
    next_state: &str,
) {
    let Some(journal) = journal else {
        return;
    };
    let database = journal.database().lock().await;
    if let Ok(Some(current)) =
        evohime_local_storage::domains::workflow::get(database.connection(), worktree_id)
    {
        let _ = evohime_local_storage::domains::workflow::transition(
            database.connection(),
            worktree_id,
            current.version,
            next_state,
            crate::task_memory::now_millis() as i64,
        );
    }
}

/// Finalizes a chat worktree after task completion: cleans up clean worktrees
/// immediately and preserves dirty worktrees for explicit user apply/discard.
pub(crate) async fn finalize_chat_worktree(
    journal: Option<&EventJournal>,
    lease: &ChatWorktreeLease,
) -> Result<Option<ChatWorktreeSummary>, String> {
    if let Some(summary) =
        collect_worktree_summary(&lease.worktree_root, &lease.worktree_id, &lease.task_id).await?
    {
        return Ok(Some(summary));
    }
    remove_and_prune_worktree(&lease.base_root, &lease.worktree_root, false).await?;
    transition_worktree_state(journal, &lease.worktree_id, "cleaned").await;
    Ok(None)
}

/// Applies tracked and untracked changes from the chat worktree into `base_root`,
/// then removes the worktree and marks its record as `applied`.
pub(crate) async fn apply_chat_worktree(
    journal: Option<&EventJournal>,
    lease: &ChatWorktreeLease,
) -> Result<(), String> {
    let diff_output = run_git(&lease.worktree_root, &["diff", "--binary", "HEAD"]).await?;
    if !diff_output.status.success() {
        return Err(String::from_utf8_lossy(&diff_output.stderr)
            .trim()
            .to_owned());
    }
    if !diff_output.stdout.is_empty() {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&lease.base_root)
            .args(["apply", "--binary", "--whitespace=nowarn", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(&diff_output.stdout)
                .await
                .map_err(|error| error.to_string())?;
        }
        let apply_output = child
            .wait_with_output()
            .await
            .map_err(|error| error.to_string())?;
        if !apply_output.status.success() {
            return Err(String::from_utf8_lossy(&apply_output.stderr)
                .trim()
                .to_owned());
        }
    }

    let untracked_output = run_git(
        &lease.worktree_root,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .await?;
    if untracked_output.status.success() {
        let text = String::from_utf8_lossy(&untracked_output.stdout);
        for rel in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let rel_path = Path::new(rel);
            if rel_path.is_absolute()
                || rel_path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                continue;
            }
            let src = lease.worktree_root.join(rel_path);
            let dst = lease.base_root.join(rel_path);
            if let Some(parent) = dst.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            tokio::fs::copy(&src, &dst)
                .await
                .map_err(|error| error.to_string())?;
        }
    }

    remove_and_prune_worktree(&lease.base_root, &lease.worktree_root, true).await?;
    transition_worktree_state(journal, &lease.worktree_id, "applied").await;
    Ok(())
}

/// Discards uncommitted changes in the chat worktree and removes it.
pub(crate) async fn discard_chat_worktree(
    journal: Option<&EventJournal>,
    lease: &ChatWorktreeLease,
) -> Result<(), String> {
    remove_and_prune_worktree(&lease.base_root, &lease.worktree_root, true).await?;
    transition_worktree_state(journal, &lease.worktree_id, "discarded").await;
    Ok(())
}

/// Inspects persisted `chat_session:` worktree records after restart/recovery:
/// cleans up missing or clean worktrees and marks dirty ones `recovery_pending`.
pub(crate) async fn recover_orphaned_chat_worktrees(
    journal: &EventJournal,
    active_task_ids: &HashSet<String>,
    base_root_hint: Option<&Path>,
) -> Vec<ChatWorktreeSummary> {
    let records = {
        let database = journal.database().lock().await;
        evohime_local_storage::domains::workflow::list_active_by_idempotency_prefix(
            database.connection(),
            IDEMPOTENCY_PREFIX,
        )
        .unwrap_or_default()
    };
    let mut pending = Vec::new();
    for record in records {
        if active_task_ids.contains(&record.task_id) {
            continue;
        }
        let worktree_root = PathBuf::from(&record.root_ref);
        if !worktree_root.exists() {
            transition_worktree_state(Some(journal), &record.worktree_id, "cleaned").await;
            continue;
        }
        match collect_worktree_summary(&worktree_root, &record.worktree_id, &record.task_id).await {
            Ok(Some(summary)) => {
                if record.state != "recovery_pending" {
                    transition_worktree_state(
                        Some(journal),
                        &record.worktree_id,
                        "recovery_pending",
                    )
                    .await;
                }
                pending.push(summary);
            }
            Ok(None) => {
                if let Some(base_root) = base_root_hint {
                    let _ = remove_and_prune_worktree(base_root, &worktree_root, false).await;
                }
                transition_worktree_state(Some(journal), &record.worktree_id, "cleaned").await;
            }
            Err(_) => {
                if record.state != "recovery_pending" {
                    transition_worktree_state(
                        Some(journal),
                        &record.worktree_id,
                        "recovery_pending",
                    )
                    .await;
                }
            }
        }
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn init_git_repo(dir: &Path) {
        std::fs::create_dir_all(dir).expect("create repo dir");
        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test User"],
            vec!["config", "core.autocrlf", "false"],
        ] {
            let out = run_git(dir, &args).await.expect("git init");
            assert!(out.status.success(), "{args:?}");
        }
        std::fs::write(dir.join("shared.txt"), "initial\n").expect("write initial");
        for args in [vec!["add", "shared.txt"], vec!["commit", "-m", "initial"]] {
            let out = run_git(dir, &args).await.expect("git commit");
            assert!(out.status.success(), "{args:?}");
        }
    }

    #[tokio::test]
    async fn parallel_chat_worktrees_isolate_edits_and_apply_cleanly() {
        let root = std::env::temp_dir().join(format!("evohime-chat-wt-{}", uuid::Uuid::new_v4()));
        let repo = root.join("repo");
        let service_dir = root.join("service-worktrees");
        let journal = EventJournal::open(root.join("journal.db")).expect("journal opens");
        init_git_repo(&repo).await;

        let lease_a = prepare_for_chat_task(Some(&journal), "task-a", &repo, Some(&service_dir))
            .await
            .expect("lease A");
        let lease_b = prepare_for_chat_task(Some(&journal), "task-b", &repo, Some(&service_dir))
            .await
            .expect("lease B");

        assert!(lease_a.worktree_root.starts_with(&service_dir));
        assert!(!lease_a.worktree_root.starts_with(&repo));
        assert_ne!(lease_a.worktree_root, lease_b.worktree_root);

        std::fs::write(lease_a.worktree_root.join("shared.txt"), "from chat A\n").expect("edit A");
        std::fs::write(lease_a.worktree_root.join("added_a.txt"), "new file A\n").expect("add A");
        std::fs::write(lease_b.worktree_root.join("shared.txt"), "from chat B\n").expect("edit B");

        // Base working tree stays untouched while both parallel worktrees have edits.
        assert_eq!(
            std::fs::read_to_string(repo.join("shared.txt")).expect("read base"),
            "initial\n"
        );

        let summary_a = finalize_chat_worktree(Some(&journal), &lease_a)
            .await
            .expect("finalize A")
            .expect("dirty summary A");
        assert!(summary_a.changed_files.iter().any(|p| p == "shared.txt"));
        assert!(summary_a.changed_files.iter().any(|p| p == "added_a.txt"));

        apply_chat_worktree(Some(&journal), &lease_a)
            .await
            .expect("apply A");
        assert_eq!(
            std::fs::read_to_string(repo.join("shared.txt")).expect("read applied"),
            "from chat A\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("added_a.txt")).expect("read added"),
            "new file A\n"
        );
        assert!(!lease_a.worktree_root.exists());

        discard_chat_worktree(Some(&journal), &lease_b)
            .await
            .expect("discard B");
        assert!(!lease_b.worktree_root.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn non_git_workspace_returns_isolated_worktree_unavailable() {
        let root = std::env::temp_dir().join(format!("evohime-nongit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create dir");
        let err = prepare_for_chat_task(None, "task-nongit", &root, None)
            .await
            .expect_err("non-git fails");
        assert!(err.contains("isolated_worktree_unavailable"), "{err}");
        assert!(err.contains("shared_workspace"), "{err}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn recover_orphaned_chat_worktrees_preserves_dirty_and_cleans_clean() {
        let root =
            std::env::temp_dir().join(format!("evohime-chat-wt-rec-{}", uuid::Uuid::new_v4()));
        let repo = root.join("repo");
        let service_dir = root.join("service-worktrees");
        let journal = EventJournal::open(root.join("journal.db")).expect("journal opens");
        init_git_repo(&repo).await;

        let clean_lease =
            prepare_for_chat_task(Some(&journal), "task-clean", &repo, Some(&service_dir))
                .await
                .expect("clean lease");
        let dirty_lease =
            prepare_for_chat_task(Some(&journal), "task-dirty", &repo, Some(&service_dir))
                .await
                .expect("dirty lease");
        std::fs::write(dirty_lease.worktree_root.join("shared.txt"), "unsaved\n")
            .expect("dirty edit");

        let pending = recover_orphaned_chat_worktrees(&journal, &HashSet::new(), Some(&repo)).await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].task_id, "task-dirty");
        assert!(!clean_lease.worktree_root.exists());
        assert!(dirty_lease.worktree_root.exists());

        discard_chat_worktree(Some(&journal), &dirty_lease)
            .await
            .expect("cleanup dirty");
        let _ = std::fs::remove_dir_all(root);
    }
}
