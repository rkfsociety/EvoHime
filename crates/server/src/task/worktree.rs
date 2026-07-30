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
use tracing::{info, warn};
use uuid::Uuid;

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

/// Diff/apply/commit can be slower than metadata-only git operations on a
/// large patch.
/// Overridable via `EVOHIME_WORKTREE_MERGE_TIMEOUT_SECS` (default 5
/// minutes) — a fixed constant would be too tight for diff/apply/commit
/// against a very large repository or patch.
pub(crate) fn merge_timeout() -> Duration {
    static VALUE: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();
    // Read (and validate) the env var once per process rather than on every
    // call — this is on the hot path of every merge-back, and the value
    // can't meaningfully change mid-process anyway (nothing in this design
    // re-reads the environment after startup).
    *VALUE.get_or_init(|| {
        std::env::var("EVOHIME_WORKTREE_MERGE_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|secs: &u64| *secs > 0)
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(5 * 60))
    })
}

struct ChangedPaths {
    /// Paths that existed at `base_sha` (modified, deleted, or either side
    /// of a rename) — safe to restore with `git checkout HEAD --`.
    existing: Vec<String>,
    /// Paths newly created by the worktree, absent at `base_sha` — `git
    /// checkout HEAD --` can't restore something that never existed there;
    /// these are unstaged and deleted directly on rollback instead.
    added: Vec<String>,
}

impl ChangedPaths {
    fn all(&self) -> Vec<&str> {
        self.existing
            .iter()
            .chain(self.added.iter())
            .map(String::as_str)
            .collect()
    }
}

async fn changed_paths(worktree_path: &Path, base_sha: &str) -> Result<ChangedPaths, WorktreeError> {
    // `-z`: NUL-delimited, *unquoted* paths. Without it, git's default
    // `core.quotePath=true` C-style-quotes any path containing a non-ASCII
    // byte — not a hypothetical for a project whose own docs and commit
    // history are full of Cyrillic — wrapping it in literal quote
    // characters that a naive tab/newline split would leave embedded in
    // the path string, breaking every later `checkout`/`reset`/`commit`
    // pathspec built from this list ("file not found"). `-z` sidesteps
    // quoting entirely and also removes any ambiguity from a filename that
    // happens to contain a literal newline.
    let run = async {
        Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args(["diff", "--cached", "--name-status", "-z", base_sha])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to run git diff --name-status: {error}")))
    };
    let output = tokio::time::timeout(merge_timeout(), run)
        .await
        .map_err(|_| WorktreeError::Io("git diff --cached --name-status -z timed out".to_string()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::Io(format!(
            "git diff --cached --name-status -z failed: {stderr}"
        )));
    }

    // Each record is NUL-separated: "STATUS\0PATH\0" for add/modify/delete,
    // or "R<score>\0OLD\0NEW\0" for a rename — never ambiguous, since a NUL
    // byte is exactly what `-z` guarantees never appears inside a field.
    let mut fields = output
        .stdout
        .split(|&byte| byte == 0)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .filter(|field| !field.is_empty())
        .collect::<std::collections::VecDeque<_>>();

    let mut existing = Vec::new();
    let mut added = Vec::new();
    while let Some(status) = fields.pop_front() {
        match status.chars().next() {
            Some('A') => added.extend(fields.pop_front()),
            // `R` (rename) and `C` (copy) both emit three NUL-separated
            // fields (`STATUS\0OLD\0NEW\0`) instead of two. This invocation
            // never passes `-C`/`--find-copies`, so `C` shouldn't appear from
            // *our own* command line — but `diff.renames` can be set to
            // `copies` in a user's or CI's global/repo git config, which
            // makes plain `git diff` emit `C` without any local flag asking
            // for it. Treating `C` as `_` (one field only) desyncs every
            // record after it: the untouched NEW_PATH field is then
            // misread as the next record's STATUS. Same handling as `R`.
            Some('R') | Some('C') => {
                // The old name existed at base_sha (restorable via `git
                // checkout HEAD --`); the new name did not (must be treated
                // like an added path — unstaged and deleted directly on
                // rollback). Lumping both into `existing`, as an earlier
                // version of this function did, breaks conflict recovery:
                // `git checkout HEAD -- <old> <new>` errors out entirely on
                // the unmatched new-name pathspec, aborting the restore for
                // *every* path in that one invocation, not just the rename.
                existing.extend(fields.pop_front());
                added.extend(fields.pop_front());
            }
            _ => existing.extend(fields.pop_front()),
        }
    }
    Ok(ChangedPaths { existing, added })
}

async fn diff_cached_patch(worktree_path: &Path, base_sha: &str) -> Result<Vec<u8>, WorktreeError> {
    // `--binary` is required or a changed binary file's actual bytes are
    // omitted from the diff and can never be reconstructed by `git apply` —
    // without it, a binary file change would silently vanish from the merge.
    let run = async {
        Command::new("git")
            .arg("-C")
            .arg(worktree_path)
            .args(["diff", "--cached", "--binary", base_sha])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to run git diff: {error}")))
    };
    let output = tokio::time::timeout(merge_timeout(), run)
        .await
        .map_err(|_| WorktreeError::Io("git diff --cached --binary timed out".to_string()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::Io(format!(
            "git diff --cached --binary failed: {stderr}"
        )));
    }
    Ok(output.stdout)
}

async fn apply_patch(primary_root: &Path, patch: &[u8]) -> Result<(), WorktreeError> {
    let spawn = async {
        Command::new("git")
            .arg("-C")
            .arg(primary_root)
            .args(["apply", "--3way", "--index"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| WorktreeError::Io(format!("failed to spawn git apply: {error}")))
    };
    let mut child = spawn.await?;
    // `stdin` must be taken out of `child` *before* `wait_with_output()` is
    // called (which takes ownership of `child`), and the write must run
    // concurrently with draining stdout/stderr below — a large patch that
    // produces enough stdout/stderr (e.g. many per-file conflict messages
    // during a large 3-way merge) to fill the OS pipe buffer before the
    // parent starts reading would otherwise deadlock: the child blocks
    // writing to its full stdout/stderr pipe while the parent is still
    // blocked writing the remaining stdin.
    let mut stdin = child.stdin.take().expect("stdin piped");

    let io = async {
        use tokio::io::AsyncWriteExt;
        let write_fut = async {
            let result = stdin.write_all(patch).await;
            // Close stdin as soon as the write completes (rather than
            // waiting for the whole `io` future to finish) so `git apply`
            // sees EOF and can proceed even while stdout/stderr are still
            // being drained below.
            drop(stdin);
            result
        };
        tokio::join!(write_fut, child.wait_with_output())
    };

    let (write_result, output_result) = tokio::time::timeout(merge_timeout(), io)
        .await
        .map_err(|_| WorktreeError::Io("git apply timed out".to_string()))?;

    write_result.map_err(|error| WorktreeError::Io(format!("failed to write patch to git apply: {error}")))?;
    let output = output_result
        .map_err(|error| WorktreeError::Io(format!("failed to wait on git apply: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(WorktreeError::Io(format!(
            "git apply --3way --index failed: {stderr}"
        )));
    }
    Ok(())
}

/// Restores exactly `changed`'s paths to their `HEAD` state — never a
/// blanket `git reset --hard`, which would also discard any unrelated
/// uncommitted work belonging to a concurrently-running unisolated task
/// elsewhere in `repo`. Best-effort: logs and continues on partial failure
/// rather than propagating, since this itself only runs while already
/// recovering from a failed apply/commit — the original error is what the
/// caller returns.
/// Returns `false` if any part of the restore itself failed — the caller
/// folds that into its own error message rather than leaving "rollback may
/// be incomplete" as something only visible in a log line elsewhere. A
/// partial-restore failure still returns the *original* conflict/commit
/// error to the task (this function's own failures are secondary), just
/// with that extra fact appended so it isn't silently invisible outside
/// the logs.
async fn restore_paths(repo: &Path, changed: &ChangedPaths) -> bool {
    let mut clean = true;
    if !changed.existing.is_empty() {
        let mut args: Vec<&str> = vec!["checkout", "HEAD", "--"];
        args.extend(changed.existing.iter().map(String::as_str));
        if let Err(error) = run_git(repo, &args, merge_timeout()).await {
            warn!(%error, "failed to restore existing paths to HEAD after a failed merge-back");
            clean = false;
        }
    }
    if !changed.added.is_empty() {
        let mut args: Vec<&str> = vec!["reset", "--"];
        args.extend(changed.added.iter().map(String::as_str));
        if let Err(error) = run_git(repo, &args, worktree_op_timeout()).await {
            warn!(%error, "failed to unstage added paths after a failed merge-back");
            clean = false;
        }
        for path in &changed.added {
            // Intentionally ignored: this path may never have actually been
            // created on primary_root at all (the failed apply could have
            // hit its conflict on an earlier file in a multi-file patch,
            // before ever touching this one) — "already absent" is the
            // expected common case here, not an error worth surfacing.
            let _ = tokio::fs::remove_file(repo.join(path)).await;
        }
    }
    clean
}

pub(crate) async fn merge_worktree_into_primary(
    worktree_path: &Path,
    primary_root: &Path,
    base_sha: &str,
    task_id: Uuid,
) -> Result<(), WorktreeError> {
    // Diagnostic only: `git apply --3way` already handles a moved primary
    // HEAD correctly, but logging the drift up front means a later conflict
    // can be told apart from "another task's merge landed in between" vs.
    // "same base, genuine textual collision" without reconstructing it after
    // the fact.
    if let Ok(current_head) = rev_parse_head(primary_root).await {
        info!(
            task_id = %task_id,
            base_commit_sha = base_sha,
            primary_head = %current_head,
            "starting worktree merge-back"
        );
    }

    // Stage everything first: `git diff` never shows untracked files
    // regardless of which commit it's compared against.
    run_git(worktree_path, &["add", "-A"], merge_timeout()).await?;

    let changed = changed_paths(worktree_path, base_sha).await?;
    let all_paths = changed.all();
    if all_paths.is_empty() {
        // Nothing changed relative to base — nothing to merge or commit.
        return Ok(());
    }

    let patch = diff_cached_patch(worktree_path, base_sha).await?;

    if let Err(apply_error) = apply_patch(primary_root, &patch).await {
        let restored_cleanly = restore_paths(primary_root, &changed).await;
        return Err(WorktreeError::Conflict(if restored_cleanly {
            apply_error.to_string()
        } else {
            format!("{apply_error} (rollback incomplete — see logs, primary_root may still be dirty)")
        }));
    }

    // Only commit if THIS merge's own paths actually have staged changes —
    // a 3-way merge can legitimately produce no diff if primary already
    // matched. Scoped to all_paths, not the whole index: a concurrently
    // running unisolated task could have unrelated staged changes too.
    let mut quiet_args: Vec<&str> = vec!["diff", "--cached", "--quiet", "--"];
    quiet_args.extend(all_paths.iter().copied());
    let run = async {
        Command::new("git")
            .arg("-C")
            .arg(primary_root)
            .args(&quiet_args)
            .status()
            .await
            .map_err(|error| WorktreeError::Io(format!("failed to run git diff --cached --quiet: {error}")))
    };
    let staged = tokio::time::timeout(merge_timeout(), run)
        .await
        .map_err(|_| WorktreeError::Io("git diff --cached --quiet timed out".to_string()))??;
    if staged.success() {
        // Exit code 0 means no staged differences in this merge's own paths.
        return Ok(());
    }

    let message = format!("agent: task {task_id} (worktree merge)");
    let mut commit_args: Vec<&str> = vec!["commit", "-m", message.as_str(), "--"];
    commit_args.extend(all_paths.iter().copied());
    // Per git's own semantics, a `git commit` given pathspecs commits only
    // changes to those paths regardless of what else is staged/dirty
    // elsewhere in the index — this is what keeps a concurrently-running
    // unisolated task's unrelated state out of this commit.
    if let Err(commit_error) = run_git(primary_root, &commit_args, merge_timeout()).await {
        let restored_cleanly = restore_paths(primary_root, &changed).await;
        return Err(WorktreeError::Io(format!(
            "commit failed after a clean apply: {commit_error}{}",
            if restored_cleanly { "" } else { " (rollback incomplete — see logs, primary_root may still be dirty)" }
        )));
    }
    Ok(())
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

    #[tokio::test]
    async fn merge_back_lands_a_commit_for_an_untracked_file() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");
        let head_before = base_sha.clone();

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Agent creates a new file but never commits inside the worktree.
        std::fs::write(worktree_path.join("new-file.txt"), "generated\n").expect("write");

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        let head_after = rev_parse_head(repo.path()).await.expect("rev-parse after");
        assert_ne!(head_before, head_after, "HEAD must advance after merge-back");
        assert!(repo.path().join("new-file.txt").exists());

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_reports_a_conflict_without_advancing_head() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Worktree edits a line...
        std::fs::write(worktree_path.join("README.md"), "hello\nfrom worktree\n").expect("write");

        // ...while the primary checkout independently edits and commits the same line.
        std::fs::write(repo.path().join("README.md"), "hello\nfrom primary\n").expect("write");
        run(repo.path(), &["git", "add", "."]);
        run(repo.path(), &["git", "commit", "-m", "primary edit"]);
        let head_before_merge = rev_parse_head(repo.path()).await.expect("rev-parse");

        let result =
            merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4()).await;
        assert!(matches!(result, Err(WorktreeError::Conflict(_))));

        let head_after_merge = rev_parse_head(repo.path()).await.expect("rev-parse after");
        assert_eq!(
            head_before_merge, head_after_merge,
            "HEAD must not advance when merge-back conflicts"
        );

        // A failed 3-way apply must not leave primary_root with unmerged
        // index entries or a dirty working tree for the next task to inherit.
        let ls_files_unmerged = StdCommand::new("git")
            .args(["ls-files", "-u"])
            .current_dir(repo.path())
            .output()
            .expect("git ls-files -u");
        assert!(
            ls_files_unmerged.stdout.is_empty(),
            "primary_root must have no unmerged entries after a failed merge-back"
        );
        let status_output = StdCommand::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo.path())
            .output()
            .expect("git status --porcelain");
        assert!(
            status_output.stdout.is_empty(),
            "primary_root must be clean after a failed merge-back"
        );

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_never_touches_unrelated_uncommitted_primary_changes() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Worktree adds a new, unrelated file.
        std::fs::write(worktree_path.join("from-worktree.txt"), "isolated\n").expect("write");

        // Simulates a live unisolated task with in-progress, unrelated,
        // uncommitted edits directly in the primary checkout — one staged
        // (file A) and one merely written but never staged (file B). Both
        // shapes of "unrelated dirty state" must survive untouched.
        std::fs::write(repo.path().join("live-task-staged.txt"), "wip staged\n").expect("write");
        run(repo.path(), &["git", "add", "live-task-staged.txt"]);
        std::fs::write(repo.path().join("live-task-unstaged.txt"), "wip unstaged\n").expect("write");

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        assert!(repo.path().join("from-worktree.txt").exists());
        // The concurrently "live" task's staged and unstaged files must both
        // survive exactly as they were — neither committed into this
        // merge's commit nor reset away.
        assert!(repo.path().join("live-task-staged.txt").exists());
        assert!(repo.path().join("live-task-unstaged.txt").exists());
        let status = StdCommand::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(repo.path())
            .output()
            .expect("git diff --cached --name-only");
        let staged = String::from_utf8_lossy(&status.stdout);
        assert!(
            staged.contains("live-task-staged.txt"),
            "unrelated staged file must remain staged, not swept into this merge's commit"
        );
        let status_porcelain = StdCommand::new("git")
            .args(["status", "--porcelain", "live-task-unstaged.txt"])
            .current_dir(repo.path())
            .output()
            .expect("git status --porcelain live-task-unstaged.txt");
        assert!(
            String::from_utf8_lossy(&status_porcelain.stdout).contains("??"),
            "unrelated unstaged file must remain untracked/unstaged, not swept into this merge's commit"
        );

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_lands_binary_file_changes() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        let binary_content: &[u8] = &[0u8, 159, 146, 150, 0, 1, 2, 255, 254, 253];
        std::fs::write(worktree_path.join("image.bin"), binary_content).expect("write binary");

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        let landed = std::fs::read(repo.path().join("image.bin")).expect("read merged binary");
        assert_eq!(landed, binary_content, "binary content must survive merge-back byte-for-byte");

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_restores_cleanly_when_a_conflict_coincides_with_a_rename() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        std::fs::write(repo.path().join("other.txt"), "base\n").expect("write");
        run(repo.path(), &["git", "add", "."]);
        run(repo.path(), &["git", "commit", "-m", "add other.txt"]);
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Worktree renames README.md *and* edits an unrelated file.
        std::fs::rename(
            worktree_path.join("README.md"),
            worktree_path.join("RENAMED.md"),
        )
        .expect("rename");
        std::fs::write(worktree_path.join("other.txt"), "from worktree\n").expect("write");

        // Primary independently edits the same unrelated file, forcing a conflict.
        std::fs::write(repo.path().join("other.txt"), "from primary\n").expect("write");
        run(repo.path(), &["git", "add", "."]);
        run(repo.path(), &["git", "commit", "-m", "primary edit"]);
        let head_before_merge = rev_parse_head(repo.path()).await.expect("rev-parse");

        let result =
            merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4()).await;
        assert!(matches!(result, Err(WorktreeError::Conflict(_))));

        let head_after_merge = rev_parse_head(repo.path()).await.expect("rev-parse after");
        assert_eq!(head_before_merge, head_after_merge, "HEAD must not advance");

        // Regression check: lumping a rename's old+new name into the same
        // `existing` bucket makes `git checkout HEAD -- <old> <new>` error
        // out on the unmatched new-name pathspec, aborting the restore for
        // every path in that invocation — not just the rename — and leaving
        // the repo dirty.
        let status_output = StdCommand::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo.path())
            .output()
            .expect("git status --porcelain");
        assert!(
            status_output.stdout.is_empty(),
            "primary_root must be fully clean after a conflict that coincides with a rename"
        );

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }

    #[tokio::test]
    async fn merge_back_lands_a_file_with_a_non_ascii_name() {
        let repo = tempfile::tempdir().expect("tempdir");
        init_repo(repo.path());
        let base_sha = rev_parse_head(repo.path()).await.expect("rev-parse");

        let worktree_root = tempfile::tempdir().expect("tempdir");
        let worktree_path = worktree_root.path().join("wt-1");
        add_worktree(repo.path(), &worktree_path, &base_sha)
            .await
            .expect("add worktree");

        // Without `-z`, git's default core.quotePath C-style-quotes this
        // filename, and a naive tab/newline split would leave the quote
        // characters embedded in the "path", breaking every later
        // checkout/reset/commit pathspec built from it.
        let filename = "отчёт.txt";
        std::fs::write(worktree_path.join(filename), "содержимое\n").expect("write");

        merge_worktree_into_primary(&worktree_path, repo.path(), &base_sha, Uuid::new_v4())
            .await
            .expect("merge back");

        assert!(
            repo.path().join(filename).exists(),
            "a non-ASCII filename must land at the exact same name on primary_root"
        );

        remove_worktree(repo.path(), &worktree_path)
            .await
            .expect("remove worktree");
    }
}
