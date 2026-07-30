//! Git-worktree isolation for concurrently-running tasks (Stage 7.107).
//!
//! All functions here shell out to the `git` CLI via `tokio::process::Command`,
//! matching the pattern already used in `evohime_tool_runtime`'s git tools and
//! `crate::github_api`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug)]
pub(crate) enum WorktreeError {
    NotAGitRepo(String),
    Conflict(String),
    Io(String),
}

impl fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorktreeError::NotAGitRepo(message) => write!(f, "not a git repository: {message}"),
            WorktreeError::Conflict(message) => write!(f, "merge conflict: {message}"),
            WorktreeError::Io(message) => write!(f, "{message}"),
        }
    }
}

/// Quick, metadata-only git operations (rev-parse, worktree add/remove/prune).
/// Overridable via `EVOHIME_WORKTREE_OP_TIMEOUT_SECS` (default 30s). Same
/// mechanism as `merge_timeout()` below (env var + cached `OnceLock`) —
/// one consistent configuration approach for both timeout categories,
/// rather than a hardcoded constant for this one and an env-configurable
/// function for the other.
pub(crate) fn worktree_op_timeout() -> Duration {
    static VALUE: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| {
        std::env::var("EVOHIME_WORKTREE_OP_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|secs: &u64| *secs > 0)
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(30))
    })
}

async fn run_git(repo: &Path, args: &[&str], timeout: Duration) -> Result<String, WorktreeError> {
    let run = async {
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to run git: {error}")))
    };
    let output = tokio::time::timeout(timeout, run)
        .await
        .map_err(|_| {
            WorktreeError::Io(format!(
                "git -C {} {} timed out after {timeout:?}",
                repo.display(),
                args.join(" ")
            ))
        })??;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let message = if stderr.is_empty() { stdout } else { stderr };
        return Err(WorktreeError::Io(format!(
            "git -C {} {} failed: {message}",
            repo.display(),
            args.join(" ")
        )));
    }
    Ok(stdout)
}

pub(crate) async fn rev_parse_head(repo: &Path) -> Result<String, WorktreeError> {
    run_git(repo, &["rev-parse", "HEAD"], worktree_op_timeout())
        .await
        .map_err(|error| WorktreeError::NotAGitRepo(error.to_string()))
}

pub(crate) async fn add_worktree(
    repo: &Path,
    worktree_path: &Path,
    base_sha: &str,
) -> Result<(), WorktreeError> {
    // Defense in depth, two layers:
    // 1. A cheap lexical check on `worktree_path` itself — no I/O, works
    //    even though `worktree_path` doesn't exist yet (nothing to
    //    canonicalize). Catches the direct case: whatever constructed
    //    `worktree_path` handed this function a path that's textually
    //    inside `repo`.
    if repo == worktree_path || worktree_path.starts_with(repo) {
        return Err(WorktreeError::Io(format!(
            "refusing to create worktree {} nested inside primary root {}",
            worktree_path.display(),
            repo.display()
        )));
    }
    // 2. A canonical check on the OS temp *root* (`worktree_path` is always
    //    `temp_root/evohime-worktrees/<task_id>`, and temp_root always
    //    exists, unlike `worktree_path` itself) against `repo`, both
    //    canonicalized — catches a misconfigured TMPDIR/TEMP that's a
    //    symlink or Windows junction pointing inside `repo`, which check 1's
    //    lexical comparison alone could miss. `repo` is already canonical
    //    by the time it reaches here (it comes from `resolve_workspace_path`,
    //    which canonicalizes), but canonicalizing it again is idempotent.
    let temp_root = std::env::temp_dir();
    let canonical_temp_root = temp_root
        .canonicalize()
        .map_err(|error| WorktreeError::Io(format!("failed to canonicalize {}: {error}", temp_root.display())))?;
    let canonical_repo = repo
        .canonicalize()
        .map_err(|error| WorktreeError::Io(format!("failed to canonicalize {}: {error}", repo.display())))?;
    if canonical_temp_root == canonical_repo || canonical_temp_root.starts_with(&canonical_repo) {
        return Err(WorktreeError::Io(format!(
            "refusing to create worktree {} — the OS temp directory {} is nested inside primary root {}",
            worktree_path.display(),
            canonical_temp_root.display(),
            canonical_repo.display()
        )));
    }

    if worktree_path.exists() {
        // `git worktree add` refuses to target an existing directory. Since
        // `worktree_path` is derived from a fresh task_id, this only
        // happens if a prior attempt for this exact task_id partially
        // failed — e.g. `git worktree add` succeeded but the subsequent
        // `task_worktrees` insert failed *and* that failure's own rollback
        // (`remove_worktree`) also failed. Not reachable through this
        // design's normal call pattern today, but cheap to make
        // provisioning self-healing against it. Prefer `remove_worktree`
        // (git-aware — cleans up `.git/worktrees/<id>/` metadata registered
        // for it, avoiding a `fatal: ... is a missing but locked working
        // tree` / `prune` error on the *next* `git worktree add`) over a
        // raw directory delete. Only if that fails (e.g. this directory was
        // never actually registered as a worktree of `repo` at all) fall
        // back to a raw removal, and only after confirming it looks like a
        // worktree checkout (`.git` marker file present) rather than
        // blindly deleting an unrelated directory that happened to land at
        // this path.
        if let Err(remove_error) = remove_worktree(repo, worktree_path).await {
            // `remove_worktree` already ran `git worktree prune` internally
            // before returning this error (see its own implementation
            // above), so `.git/worktrees/<id>/` metadata for this path is
            // already cleared even though the directory removal itself
            // failed — the raw `remove_dir_all` below only has to deal with
            // the filesystem, not leftover git-internal bookkeeping.
            if worktree_path.exists() {
                if !worktree_path.join(".git").exists() {
                    return Err(WorktreeError::Io(format!(
                        "worktree path {} already exists, is not a recognized git worktree, and git worktree remove failed ({remove_error}) — refusing to delete it blindly",
                        worktree_path.display()
                    )));
                }
                tokio::fs::remove_dir_all(&worktree_path).await.map_err(|error| {
                    WorktreeError::Io(format!(
                        "worktree path {} already exists and could not be cleared: {error}",
                        worktree_path.display()
                    ))
                })?;
            }
        }
    }
    if let Some(parent) = worktree_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to create {}: {error}", parent.display())))?;
    }
    let worktree_path_str = worktree_path.to_string_lossy().into_owned();
    run_git(
        repo,
        &["worktree", "add", "--detach", &worktree_path_str, base_sha],
        worktree_op_timeout(),
    )
    .await?;
    Ok(())
}

pub(crate) async fn remove_worktree(repo: &Path, worktree_path: &Path) -> Result<(), WorktreeError> {
    // `worktree remove`'s own failure is captured, not propagated
    // immediately with `?` — `prune` below must always run regardless, or
    // `.git/worktrees/<id>/` metadata for this path is left registered in
    // `repo`. A subsequent `git worktree add` at the same path would then
    // fail with "missing but locked working tree" even after the directory
    // itself is gone (e.g. via `add_worktree`'s raw `remove_dir_all`
    // fallback below, which has no other way to clear that metadata).
    let remove_result = if worktree_path.exists() {
        let worktree_path_str = worktree_path.to_string_lossy().into_owned();
        run_git(
            repo,
            &["worktree", "remove", "--force", &worktree_path_str],
            worktree_op_timeout(),
        )
        .await
        .map(|_| ())
    } else {
        Ok(())
    };
    run_git(repo, &["worktree", "prune"], worktree_op_timeout()).await?;
    remove_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn run(dir: &Path, args: &[&str]) {
        let status = StdCommand::new(args[0])
            .args(&args[1..])
            .current_dir(dir)
            .status()
            .expect("run command");
        assert!(status.success(), "{:?} failed", args);
    }

    fn init_repo(dir: &Path) {
        run(dir, &["git", "init"]);
        run(dir, &["git", "config", "user.email", "test@example.com"]);
        run(dir, &["git", "config", "user.name", "Test"]);
        std::fs::write(dir.join("README.md"), "hello\n").expect("write");
        run(dir, &["git", "add", "."]);
        run(dir, &["git", "commit", "-m", "init"]);
    }

    #[tokio::test]
    async fn rev_parse_head_returns_a_sha() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());

        let sha = rev_parse_head(repo.path()).await.expect("rev-parse");
        assert_eq!(sha.len(), 40);
    }

    #[tokio::test]
    async fn rev_parse_head_fails_on_non_git_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = rev_parse_head(dir.path()).await.unwrap_err();
        assert!(matches!(error, WorktreeError::NotAGitRepo(_)));
    }

    #[tokio::test]
    async fn add_and_remove_worktree_round_trips() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");

        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");
        assert!(worktree_path.join("README.md").exists());

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
        assert!(!worktree_path.exists());
    }

    #[tokio::test]
    async fn remove_worktree_tolerates_an_already_missing_directory() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let missing = repo.path().join("never-existed");

        remove_worktree(repo.path(), &missing)
            .await
            .expect("remove of missing worktree must not error");
    }

    #[tokio::test]
    async fn add_worktree_refuses_a_path_nested_inside_the_repo() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");
        let nested = repo.path().join("nested-worktree");

        let error = add_worktree(repo.path(), &nested, &base_sha)
            .await
            .unwrap_err();
        assert!(matches!(error, WorktreeError::Io(_)));
        assert!(!nested.exists());
    }

    #[tokio::test]
    async fn add_worktree_self_heals_a_leftover_directory_from_a_prior_attempt() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");

        // First attempt succeeds normally.
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("first add worktree");

        // A second call for the exact same path — simulating provisioning
        // being retried after some earlier failure left this directory
        // behind — must not fail with "already exists".
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree must self-heal a pre-existing directory");
        assert!(worktree_path.join("README.md").exists());

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }
}
