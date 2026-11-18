//! Self-development tools of the Queen (`eva_workspace_*`).
//!
//! The Queen may study her own sources, patch them, run tests and commit the
//! result, but only inside an isolated checkout that the operator configured
//! through `HiveSettings::eva_workspace_path`. The running product binary is
//! never the target of these tools: the workspace is a separate clone, and
//! every mutation is gated behind a successful test run in the same session.
//!
//! The module intentionally keeps the security-critical decisions (path
//! resolution, command allow-list, test gate) free of any tool-registry
//! plumbing so that they can be unit-tested without an execution context.

use sha2::Digest;
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use crate::execution_policy_profiles::{
    validate_process_egress, ExecutionPolicyProfile, ProcessGuard,
};
use serde::Deserialize;
use serde_json::{json, Value};
use thiserror::Error;
use tokio::{io::AsyncReadExt, process::Command};

use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;

/// Capability that unlocks the self-development tool set.
pub const SELF_DEVELOPMENT_CAPABILITY: &str = "eva_self_development";

/// Hard ceiling for a single `eva_workspace_run_command` invocation.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Maximum number of bytes returned from a single file read.
pub const MAX_READ_BYTES: usize = 512 * 1024;

/// Maximum number of bytes accepted by a single file write.
pub const MAX_WRITE_BYTES: usize = 512 * 1024;

/// Prefix shared by every tool owned by this module.
pub const TOOL_PREFIX: &str = "eva_workspace_";

/// Canonical tool ids exposed by this module.
pub const TOOL_READ_FILE: &str = "eva_workspace_read_file";
/// Canonical id of the workspace write tool.
pub const TOOL_WRITE_FILE: &str = "eva_workspace_write_file";
/// Canonical id of the workspace command tool.
pub const TOOL_RUN_COMMAND: &str = "eva_workspace_run_command";
/// Canonical id of the workspace git tool.
pub const TOOL_GIT: &str = "eva_workspace_git";

/// Failures of the self-development loop.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SelfDevelopmentError {
    /// `HiveSettings::eva_workspace_path` is not configured.
    #[error("eva_workspace_path is not configured; self-development tools are unavailable")]
    WorkspaceNotConfigured,
    /// The configured path does not exist or is not a directory.
    #[error("eva_workspace_path is not an existing directory: {0}")]
    WorkspaceMissing(String),
    /// The configured directory is not a git checkout.
    #[error("eva_workspace_path is not a git repository: {0}")]
    WorkspaceNotGitRepository(String),
    /// The configured checkout contains the currently running product binary.
    #[error("eva_workspace_path contains the running EvoHime executable")]
    WorkspaceContainsRunningBinary,
    /// A requested path escapes the workspace root.
    #[error("path escapes the eva workspace: {0}")]
    PathEscapesWorkspace(String),
    /// The command is not on the allow-list.
    #[error("command is not permitted in the eva workspace: {0}")]
    CommandNotAllowed(String),
    /// `git push` was requested before a passing test run.
    #[error("git push requires a passing eva_workspace_run_command test run in this session")]
    PushRequiresPassingTests,
    /// `git push` was requested while the checkout is dirty.
    #[error("git push requires a clean workspace; uncommitted changes detected")]
    PushRequiresCleanWorkspace,
    /// The git sub-command is unknown.
    #[error("unsupported git action: {0}")]
    UnsupportedGitAction(String),
    /// The caller profile is not allowed to use self-development tools.
    #[error("profile {0} is not permitted to use eva_workspace_* tools")]
    ProfileNotPermitted(String),
    /// Payload limits exceeded.
    #[error("payload exceeds the {0} byte limit")]
    PayloadTooLarge(usize),
}

/// Where the Queen is allowed to work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaWorkspace {
    root: PathBuf,
}

impl EvaWorkspace {
    /// Validate an operator-supplied workspace path.
    ///
    /// The path must resolve to a repository root and must not contain the
    /// currently running executable.
    pub fn open(path: &str) -> Result<Self, SelfDevelopmentError> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(SelfDevelopmentError::WorkspaceNotConfigured);
        }

        let root = PathBuf::from(trimmed);
        if !root.is_dir() {
            return Err(SelfDevelopmentError::WorkspaceMissing(trimmed.to_string()));
        }

        // A worktree may keep `.git` as a file; verify Git itself rather than
        // trusting that name as a marker in an arbitrary directory.
        let git_check = std::process::Command::new("git")
            .args(["-C", trimmed, "rev-parse", "--show-toplevel"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        let Ok(git_output) = git_check else {
            return Err(SelfDevelopmentError::WorkspaceNotGitRepository(
                trimmed.to_string(),
            ));
        };
        if !git_output.status.success() {
            return Err(SelfDevelopmentError::WorkspaceNotGitRepository(
                trimmed.to_string(),
            ));
        }

        let root = root
            .canonicalize()
            .map_err(|_| SelfDevelopmentError::WorkspaceMissing(trimmed.to_string()))?;
        let git_root = PathBuf::from(String::from_utf8_lossy(&git_output.stdout).trim());
        let git_root = git_root
            .canonicalize()
            .map_err(|_| SelfDevelopmentError::WorkspaceNotGitRepository(trimmed.to_string()))?;
        if root != git_root {
            return Err(SelfDevelopmentError::WorkspaceNotGitRepository(
                trimmed.to_string(),
            ));
        }
        // Refuse the active repository even if it is itself a checkout. The
        // configured workspace must be an isolated clone/worktree.
        if std::env::current_dir()
            .ok()
            .and_then(|active| active.canonicalize().ok())
            .is_some_and(|active| active.starts_with(&root) || root.starts_with(&active))
        {
            return Err(SelfDevelopmentError::WorkspaceContainsRunningBinary);
        }
        if std::env::current_exe()
            .ok()
            .and_then(|executable| executable.canonicalize().ok())
            .is_some_and(|executable| executable.starts_with(&root))
        {
            return Err(SelfDevelopmentError::WorkspaceContainsRunningBinary);
        }
        Ok(Self { root })
    }

    /// Workspace root as an absolute, normalized path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a workspace-relative path, rejecting escapes and absolutes.
    pub fn resolve(&self, relative: &str) -> Result<PathBuf, SelfDevelopmentError> {
        let candidate = Path::new(relative);
        let has_windows_root = relative.starts_with("\\\\")
            || relative.starts_with("//")
            || relative
                .as_bytes()
                .get(1)
                .is_some_and(|separator| *separator == b':');
        if candidate.is_absolute() || has_windows_root || relative.trim().is_empty() {
            return Err(SelfDevelopmentError::PathEscapesWorkspace(
                relative.to_string(),
            ));
        }
        if relative.split(['/', '\\']).any(|part| part == "..") {
            return Err(SelfDevelopmentError::PathEscapesWorkspace(
                relative.to_string(),
            ));
        }

        let mut resolved = self.root.clone();
        for component in candidate.components() {
            match component {
                Component::Normal(segment) => resolved.push(segment),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(SelfDevelopmentError::PathEscapesWorkspace(
                        relative.to_string(),
                    ));
                }
            }
        }

        let mut existing = resolved.as_path();
        while !existing.exists() {
            existing = existing
                .parent()
                .ok_or_else(|| SelfDevelopmentError::PathEscapesWorkspace(relative.to_string()))?;
        }
        let canonical_existing = existing
            .canonicalize()
            .map_err(|_| SelfDevelopmentError::PathEscapesWorkspace(relative.to_string()))?;
        if !canonical_existing.starts_with(&self.root) {
            return Err(SelfDevelopmentError::PathEscapesWorkspace(
                relative.to_string(),
            ));
        }

        let canonical = if resolved.exists() {
            resolved
                .canonicalize()
                .map_err(|_| SelfDevelopmentError::PathEscapesWorkspace(relative.to_string()))?
        } else {
            let suffix = resolved
                .strip_prefix(existing)
                .map_err(|_| SelfDevelopmentError::PathEscapesWorkspace(relative.to_string()))?;
            canonical_existing.join(suffix)
        };
        if !canonical.starts_with(&self.root) {
            return Err(SelfDevelopmentError::PathEscapesWorkspace(
                relative.to_string(),
            ));
        }

        Ok(canonical)
    }
}

/// Decide whether a profile may use the self-development tool set.
///
/// Only the Queen herself and the top-level coordinator are permitted;
/// sub-agents (`depth >= 1`) never are.
#[must_use]
pub fn profile_allows_self_development(profile: &str, depth: u32) -> bool {
    depth == 0 && matches!(profile, "queen" | "coordinator")
}

/// Result of the last command executed inside the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestGate {
    last_command: Option<String>,
    last_exit_code: Option<i32>,
    modified_paths: Vec<String>,
    workspace_was_clean: Option<bool>,
    modified_hashes: BTreeMap<String, String>,
    baseline_head: Option<String>,
    committed_head: Option<String>,
    tested_head: Option<String>,
}

impl TestGate {
    /// Record the outcome of a workspace command.
    pub fn record(&mut self, command: &str, exit_code: i32) {
        if !self.workspace_was_clean() || self.baseline_head.is_none() {
            self.reset();
            return;
        }
        self.last_command = Some(command.to_string());
        self.last_exit_code = Some(exit_code);
        self.tested_head = self.baseline_head.clone();
    }

    /// Forget the recorded outcome, for example when the workspace switches.
    pub fn reset(&mut self) {
        self.last_command = None;
        self.last_exit_code = None;
        self.tested_head = None;
    }

    /// `true` when the most recent command was a test command that passed.
    #[must_use]
    pub fn has_passing_tests(&self) -> bool {
        matches!(self.last_exit_code, Some(0))
            && self.last_command.as_deref().is_some_and(is_test_command)
            && self.tested_head.is_some()
            && self.tested_head == self.baseline_head
    }

    /// Last command text, when present.
    #[must_use]
    pub fn last_command(&self) -> Option<&str> {
        self.last_command.as_deref()
    }

    /// Last exit code, when present.
    #[must_use]
    pub fn last_exit_code(&self) -> Option<i32> {
        self.last_exit_code
    }

    /// Tracks a path changed through this tool set so commits never stage other
    /// processes' unrelated working-tree changes.
    pub fn record_modified_path(&mut self, path: &str) {
        if !self.modified_paths.iter().any(|known| known == path) {
            self.modified_paths.push(path.to_owned());
        }
        self.reset_test_result();
    }

    fn record_modified_hash(&mut self, path: &str, content: &[u8]) {
        self.record_modified_path(path);
        self.modified_hashes
            .insert(path.to_owned(), hex::encode(sha2::Sha256::digest(content)));
    }

    fn set_workspace_clean(&mut self, clean: bool) {
        self.workspace_was_clean.get_or_insert(clean);
    }

    fn workspace_was_clean(&self) -> bool {
        self.workspace_was_clean == Some(true)
    }

    fn reset_test_result(&mut self) {
        self.reset();
    }

    fn modified_paths(&self) -> &[String] {
        &self.modified_paths
    }
}

/// Commands that count as a real test run for the push gate.
const TEST_SUBCOMMANDS: &[&str] = &["test", "nextest"];

/// Binaries the Queen may invoke inside her workspace.
const ALLOWED_BINARIES: &[&str] = &["cargo", "npm", "npx", "git"];

/// Shell metacharacters that would let a single tool call escape the allow-list.
const FORBIDDEN_FRAGMENTS: &[&str] = &["&&", "||", ";", "|", ">", "<", "`", "$(", "&"];

/// `true` when the command is one of the supported Cargo test invocations.
#[must_use]
pub fn is_test_command(command: &str) -> bool {
    let mut words = command.split_whitespace();
    let Some(binary) = words.next() else {
        return false;
    };
    let binary = binary.trim_end_matches(".exe");
    match binary {
        "cargo" => words
            .find(|word| !word.starts_with('-'))
            .is_some_and(|word| TEST_SUBCOMMANDS.contains(&word)),
        "npm" => {
            let words = words.collect::<Vec<_>>();
            matches!(
                words.as_slice(),
                ["test", ..] | ["run", "test", ..] | ["run", "test:unit", ..]
            )
        }
        _ => false,
    }
}

/// Validate a command against the workspace allow-list.
pub fn validate_command(command: &str) -> Result<(), SelfDevelopmentError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(SelfDevelopmentError::CommandNotAllowed(
            "empty command".to_string(),
        ));
    }

    for fragment in FORBIDDEN_FRAGMENTS {
        if trimmed.contains(fragment) {
            return Err(SelfDevelopmentError::CommandNotAllowed(format!(
                "shell composition is not permitted: {fragment}"
            )));
        }
    }

    let words = trimmed.split_whitespace().collect::<Vec<_>>();
    let binary = words
        .first()
        .copied()
        .unwrap_or(trimmed)
        .trim_end_matches(".exe");
    if !ALLOWED_BINARIES.contains(&binary) {
        return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string()));
    }

    let first_subcommand = words.get(1).copied();
    let allowed_subcommand = match binary {
        "cargo" => matches!(first_subcommand, Some("check" | "test" | "fmt" | "clippy")),
        "npm" => matches!(first_subcommand, Some("run" | "test")),
        "npx" => matches!(first_subcommand, Some("prettier")),
        "git" => matches!(first_subcommand, Some("log")),
        _ => false,
    };
    if !allowed_subcommand {
        return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string()));
    }

    if binary == "git" {
        return validate_git_log_command(&words, trimmed);
    }

    if binary == "npx" {
        return validate_npx_prettier_command(&words, trimmed);
    }

    if binary == "npm" {
        let script = if first_subcommand == Some("run") {
            words.get(2).copied()
        } else {
            first_subcommand
        };
        if !matches!(
            script,
            Some("typecheck" | "check:protocol" | "test" | "test:unit" | "lint")
        ) || words
            .iter()
            .skip(if first_subcommand == Some("run") {
                3
            } else {
                2
            })
            .any(|word| !word.starts_with('-') || !matches!(*word, "--run" | "--passWithNoTests"))
        {
            return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string()));
        }
    }

    if binary == "cargo" {
        let mut index = 2;
        while index < words.len() {
            match words[index] {
                "--workspace"
                | "--all"
                | "--all-targets"
                | "--all-features"
                | "--no-default-features"
                | "--locked"
                | "--frozen"
                | "--offline"
                | "--lib"
                | "--tests"
                | "--bins"
                | "--examples"
                | "--benches"
                | "--release"
                | "--quiet"
                | "-q"
                | "--no-run"
                | "--doc" => index += 1,
                "--check" if first_subcommand == Some("fmt") => index += 1,
                "-p" | "--package" => {
                    let package = words.get(index + 1).copied().unwrap_or_default();
                    if package.is_empty()
                        || package.starts_with('-')
                        || !package.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                        })
                    {
                        return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string()));
                    }
                    index += 2;
                }
                "-j" | "--jobs" => {
                    let jobs = words.get(index + 1).copied().unwrap_or_default();
                    if jobs
                        .parse::<u8>()
                        .ok()
                        .filter(|jobs| (1..=32).contains(jobs))
                        .is_none()
                    {
                        return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string()));
                    }
                    index += 2;
                }
                "--" if first_subcommand == Some("fmt")
                    && words.get(index + 1).copied() == Some("--check") =>
                {
                    index += 2;
                }
                "--" if first_subcommand == Some("clippy") => {
                    index += 1;
                    while index < words.len() {
                        let flag = words[index];
                        let lint = words.get(index + 1).copied().unwrap_or_default();
                        if !matches!(flag, "-D" | "-W" | "-A")
                            || !(lint == "warnings" || lint.starts_with("clippy::"))
                        {
                            return Err(SelfDevelopmentError::CommandNotAllowed(
                                trimmed.to_string(),
                            ));
                        }
                        index += 2;
                    }
                }
                value
                    if first_subcommand == Some("test")
                        && !value.starts_with('-')
                        && value.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric()
                                || matches!(byte, b'_' | b'-' | b':' | b'.')
                        }) =>
                {
                    index += 1;
                }
                _ if matches!(words[index], "--edition" | "--config" | "--style-edition")
                    && matches!(first_subcommand, Some("fmt" | "clippy")) =>
                {
                    let value = words.get(index + 1).copied().unwrap_or_default();
                    if value.is_empty() || value.starts_with('-') {
                        return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string()));
                    }
                    index += 2;
                }
                _ if matches!(words[index], "--tests" | "--benches" | "--examples")
                    && matches!(first_subcommand, Some("clippy" | "check")) =>
                {
                    index += 1;
                }
                _ if first_subcommand == Some("clippy")
                    && !words[index].starts_with('-')
                    && words[index].bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
                    }) =>
                {
                    index += 1;
                }
                _ => return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string())),
            }
        }
    }

    Ok(())
}

fn is_safe_relative_arg(arg: &str) -> bool {
    !arg.is_empty()
        && !arg.starts_with('-')
        && !arg.starts_with('/')
        && !arg.starts_with('\\')
        && !arg.contains("..")
        && !arg.contains(':')
        && arg.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'*')
        })
}

fn is_valid_log_count(raw: &str) -> bool {
    raw.parse::<u16>()
        .ok()
        .is_some_and(|count| (1..=100).contains(&count))
}

fn validate_git_log_command(words: &[&str], trimmed: &str) -> Result<(), SelfDevelopmentError> {
    let mut index = 2;
    while index < words.len() {
        let arg = words[index];
        match arg {
            "-n" => {
                let count = words.get(index + 1).copied().unwrap_or_default();
                if !is_valid_log_count(count) {
                    return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string()));
                }
                index += 2;
            }
            "--oneline" | "--stat" | "--name-only" | "--name-status" | "--graph" | "--decorate"
            | "--no-merges" | "--merges" | "--first-parent" | "--reverse" | "--abbrev-commit" => {
                index += 1;
            }
            "--" => {
                if words[index + 1..]
                    .iter()
                    .any(|path| !is_safe_relative_arg(path))
                {
                    return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string()));
                }
                break;
            }
            _ if arg
                .strip_prefix("--max-count=")
                .is_some_and(is_valid_log_count) =>
            {
                index += 1;
            }
            _ if arg.strip_prefix('-').is_some_and(is_valid_log_count) => {
                index += 1;
            }
            _ => return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string())),
        }
    }
    Ok(())
}

fn validate_npx_prettier_command(
    words: &[&str],
    trimmed: &str,
) -> Result<(), SelfDevelopmentError> {
    for arg in &words[2..] {
        match *arg {
            "--write" | "--check" | "--list-different" => {}
            _ if is_safe_relative_arg(arg) => {}
            _ => return Err(SelfDevelopmentError::CommandNotAllowed(trimmed.to_string())),
        }
    }
    Ok(())
}

/// Git actions the Queen may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitAction {
    /// Show the working tree status.
    Status,
    /// Show the current diff.
    Diff,
    /// Commit staged changes with a message.
    Commit,
    /// Push the current branch.
    Push,
}

impl GitAction {
    /// Parse the tool argument.
    pub fn parse(action: &str) -> Result<Self, SelfDevelopmentError> {
        match action.trim().to_ascii_lowercase().as_str() {
            "status" => Ok(Self::Status),
            "diff" => Ok(Self::Diff),
            "commit" => Ok(Self::Commit),
            "push" => Ok(Self::Push),
            other => Err(SelfDevelopmentError::UnsupportedGitAction(
                other.to_string(),
            )),
        }
    }

    /// `true` when the action mutates or publishes the checkout.
    #[must_use]
    pub fn is_publishing(self) -> bool {
        matches!(self, Self::Push)
    }
}

/// Guard evaluated before a `git push`.
pub fn ensure_push_allowed(
    gate: &TestGate,
    workspace_dirty: bool,
) -> Result<(), SelfDevelopmentError> {
    if workspace_dirty {
        return Err(SelfDevelopmentError::PushRequiresCleanWorkspace);
    }
    if !gate.has_passing_tests() {
        return Err(SelfDevelopmentError::PushRequiresPassingTests);
    }
    Ok(())
}

/// Tool id declarations used by the registry and manifest catalog.
pub const READ_FILE_NAME: &str = TOOL_READ_FILE;
/// Tool id for isolated source writes.
pub const WRITE_FILE_NAME: &str = TOOL_WRITE_FILE;
/// Tool id for running allow-listed verification commands.
pub const RUN_COMMAND_NAME: &str = TOOL_RUN_COMMAND;
/// Tool id for status, diff, commit and guarded push operations.
pub const GIT_NAME: &str = TOOL_GIT;
/// Capability label exposed by each Self-Tool manifest.
pub const CAPABILITY_CLASS: &str = SELF_DEVELOPMENT_CAPABILITY;
/// Permission for self-workspace reads.
pub const READ_PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
/// Permission for self-workspace writes.
pub const WRITE_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
/// Permission for bounded self-workspace commands.
pub const COMMAND_PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
/// Permission for self-workspace Git reads and mutations.
pub const GIT_PERMISSIONS: &[Permission] = &[Permission::GitWrite];
/// Read operation timeout.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Write operation timeout.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Git operation timeout.
pub const GIT_TIMEOUT: Duration = Duration::from_secs(90);
/// Permission used to fail closed when a caller role/depth is not authorized.
pub const DENIED_PERMISSION: Permission = Permission::GitWrite;

const MAX_COMMAND_OUTPUT_BYTES: usize = 512 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadFileInput {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteFileInput {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunCommandInput {
    command: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitInput {
    action: String,
    #[serde(default)]
    message: String,
}

/// Executes one of the four self-development tools in the configured checkout.
pub async fn execute(
    ctx: &ToolContext,
    name: &str,
    input: Value,
    gate: &mut TestGate,
) -> Result<ToolResult, ToolError> {
    if !profile_allows_self_development(
        ctx.self_development_profile.as_deref().unwrap_or_default(),
        ctx.agent_depth,
    ) {
        return Err(ToolError::PermissionDenied(DENIED_PERMISSION));
    }
    let workspace_path = ctx.eva_workspace_root.as_deref().ok_or_else(|| {
        ToolError::Execution(SelfDevelopmentError::WorkspaceNotConfigured.to_string())
    })?;
    let workspace = EvaWorkspace::open(&workspace_path.to_string_lossy())
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    if ctx
        .workspace_root
        .canonicalize()
        .ok()
        .is_some_and(|active| {
            active.starts_with(&workspace.root) || workspace.root.starts_with(&active)
        })
    {
        return Err(ToolError::Execution(
            "eva_workspace_path must be isolated from the active task workspace".into(),
        ));
    }

    if gate.workspace_was_clean.is_none() {
        let status = git_status(&workspace).await?;
        gate.set_workspace_clean(status.trim().is_empty());
        if gate.workspace_was_clean() {
            gate.baseline_head = Some(git_head(&workspace).await?);
        }
    }
    match name {
        TOOL_READ_FILE => read_file(&workspace, input).await,
        TOOL_WRITE_FILE => write_file(&workspace, input, gate).await,
        TOOL_RUN_COMMAND => run_command(&workspace, input, gate).await,
        TOOL_GIT => git(&workspace, input, gate).await,
        _ => Err(ToolError::UnknownTool(name.to_owned())),
    }
}

/// Profile/depth check exposed for trusted runtime integration tests.
#[must_use]
pub fn profile_permitted(profile: &str, depth: u32) -> bool {
    profile_allows_self_development(profile, depth)
}

async fn read_file(workspace: &EvaWorkspace, value: Value) -> Result<ToolResult, ToolError> {
    let input: ReadFileInput = parse_input(TOOL_READ_FILE, value)?;
    let path = workspace.resolve(&input.path).map_err(exec_error)?;
    let metadata = tokio::fs::metadata(&path).await.map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > MAX_READ_BYTES as u64 {
        return Err(ToolError::Execution(format!(
            "file must be regular and no larger than {MAX_READ_BYTES} bytes"
        )));
    }
    let content = tokio::fs::read_to_string(path).await.map_err(io_error)?;
    Ok(ToolResult {
        output: content.clone(),
        structured: json!({"path": input.path, "content": content}),
    })
}

async fn write_file(
    workspace: &EvaWorkspace,
    value: Value,
    gate: &mut TestGate,
) -> Result<ToolResult, ToolError> {
    let input: WriteFileInput = parse_input(TOOL_WRITE_FILE, value)?;
    if input.content.len() > MAX_WRITE_BYTES {
        return Err(exec_error(SelfDevelopmentError::PayloadTooLarge(
            MAX_WRITE_BYTES,
        )));
    }
    let path = workspace.resolve(&input.path).map_err(exec_error)?;
    let relative_path = path.strip_prefix(workspace.root()).map_err(io_error)?;
    if is_protected_product_path(relative_path)
        || path
            .strip_prefix(workspace.root())
            .ok()
            .and_then(|path| path.components().next())
            .is_some_and(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(".git")
            })
    {
        return Err(ToolError::Execution(
            "writes to product executables and build-output directories are forbidden".into(),
        ));
    }
    if gate.workspace_was_clean.is_none() {
        let status = git_status(workspace).await?;
        gate.set_workspace_clean(status.trim().is_empty());
        if gate.workspace_was_clean() {
            gate.baseline_head = Some(git_head(workspace).await?);
        }
    }
    if !gate.workspace_was_clean() {
        return Err(ToolError::Execution(
            "self-development requires a clean isolated checkout before editing".into(),
        ));
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(io_error)?;
    }
    tokio::fs::write(&path, input.content.as_bytes())
        .await
        .map_err(io_error)?;
    let relative_path = relative_path.to_string_lossy().replace('\\', "/");
    gate.record_modified_hash(&relative_path, input.content.as_bytes());
    Ok(ToolResult {
        output: format!("updated {}", input.path),
        structured: json!({"path": input.path, "bytes": input.content.len()}),
    })
}

async fn run_command(
    workspace: &EvaWorkspace,
    value: Value,
    gate: &mut TestGate,
) -> Result<ToolResult, ToolError> {
    let input: RunCommandInput = parse_input(TOOL_RUN_COMMAND, value)?;
    validate_command(&input.command).map_err(exec_error)?;
    if !gate.workspace_was_clean() {
        return Err(ToolError::Execution(
            "self-development commands require a clean isolated checkout at session start".into(),
        ));
    }
    gate.reset();
    let mut words = input.command.split_whitespace();
    let program = words.next().ok_or_else(|| {
        exec_error(SelfDevelopmentError::CommandNotAllowed(
            "empty command".into(),
        ))
    })?;
    let args = words.map(ToOwned::to_owned).collect::<Vec<_>>();
    let command_cwd = if program == "npm" {
        let electron = workspace.root().join("desktop/evohime-electron");
        if electron.join("package.json").is_file() {
            electron
        } else {
            workspace.root().to_path_buf()
        }
    } else {
        workspace.root().to_path_buf()
    };
    let output = run_command_process(program, &args, &command_cwd).await?;
    let code = output.status.code().unwrap_or(-1);
    if is_test_command(&input.command) && code == 0 && gate.workspace_was_clean() {
        let status = git_status(workspace).await?;
        if only_self_modified_paths(&status, gate.modified_paths()) {
            let current_head = git_head(workspace).await?;
            if gate.baseline_head.as_deref() == Some(current_head.as_str()) {
                gate.record(&input.command, code);
            } else {
                gate.reset();
            }
        }
    }
    let stdout = output.stdout;
    let stderr = output.stderr;
    Ok(ToolResult {
        output: format!("exit_code: {code}\nstdout:\n{stdout}\nstderr:\n{stderr}"),
        structured: json!({"command": input.command, "stdout": stdout, "stderr": stderr, "exit_code": code}),
    })
}

async fn run_command_process(
    program: &str,
    args: &[String],
    cwd: &Path,
) -> Result<ProcessOutput, ToolError> {
    if program != "npm" {
        validate_process_egress(program, args).map_err(|error| {
            ToolError::Execution(format!("process policy denied command: {error}"))
        })?;
    }
    let mut child = Command::new(program);
    crate::execution_policy_profiles::apply_environment(&mut child);
    child
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env("GIT_TERMINAL_PROMPT", "0");
    let mut child = child.spawn().map_err(io_error)?;
    let profile = ExecutionPolicyProfile::resolve("shell_execute")
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    let _guard = ProcessGuard::attach(&child, &profile)
        .map_err(|error| ToolError::Execution(format!("process guard unavailable: {error}")))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::Execution("stdout unavailable".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| ToolError::Execution("stderr unavailable".into()))?;
    let stdout_task = tokio::spawn(async move { read_bounded(&mut stdout).await });
    let stderr_task = tokio::spawn(async move { read_bounded(&mut stderr).await });
    let status = tokio::time::timeout(COMMAND_TIMEOUT, child.wait()).await;
    let status = match status {
        Ok(status) => status.map_err(io_error)?,
        Err(_) => {
            child.start_kill().map_err(io_error)?;
            let _ = child.wait().await;
            return Err(ToolError::TimedOut(COMMAND_TIMEOUT));
        }
    };
    let stdout = stdout_task.await.map_err(io_error)?.map_err(io_error)?;
    let stderr = stderr_task.await.map_err(io_error)?.map_err(io_error)?;
    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
    })
}

async fn git(
    workspace: &EvaWorkspace,
    value: Value,
    gate: &mut TestGate,
) -> Result<ToolResult, ToolError> {
    let input: GitInput = parse_input(TOOL_GIT, value)?;
    let action = GitAction::parse(&input.action).map_err(exec_error)?;
    if action == GitAction::Commit {
        if input.message.trim().is_empty() || input.message.len() > 256 {
            return Err(ToolError::InvalidInput {
                tool: TOOL_GIT.into(),
                message: "commit requires a message of 1..256 bytes".into(),
            });
        }
        let status = git_status(workspace).await?;
        if !gate.workspace_was_clean() || !only_self_modified_paths(&status, gate.modified_paths())
        {
            return Err(exec_error(SelfDevelopmentError::PushRequiresCleanWorkspace));
        }
        return git_commit(workspace, &input.message, gate).await;
    }
    if action == GitAction::Push {
        let status = git_status(workspace).await?;
        ensure_push_allowed(gate, !status.trim().is_empty()).map_err(exec_error)?;
        let current_head = git_head(workspace).await?;
        if gate.tested_head.as_deref() != Some(current_head.as_str())
            || gate.committed_head.as_deref() != Some(current_head.as_str())
        {
            return Err(ToolError::Execution(
                "workspace HEAD changed outside the self-development commit".into(),
            ));
        }
    }
    let args: Vec<&str> = match action {
        GitAction::Status => vec!["status", "--short", "--branch"],
        GitAction::Diff => vec!["diff", "--"],
        GitAction::Commit => Vec::new(),
        GitAction::Push => vec!["push"],
    };
    if action == GitAction::Push {
        let output = run_process(
            "git",
            &[
                "-c",
                "core.hooksPath=",
                "-c",
                "push.followTags=false",
                "push",
                "--porcelain",
                "origin",
                "HEAD",
            ],
            workspace.root(),
            Duration::from_secs(60),
        )
        .await?;
        if !output.status.success() {
            return process_error("git push", output);
        }
        gate.reset();
        return process_result("git push".into(), output);
    }
    let mut git_args = vec!["-c", "core.hooksPath="];
    git_args.extend(args);
    let output = run_process("git", &git_args, workspace.root(), Duration::from_secs(60)).await?;
    if !output.status.success() {
        return process_error("git operation", output);
    }
    process_result(format!("git {}", input.action), output)
}

async fn git_status(workspace: &EvaWorkspace) -> Result<String, ToolError> {
    let output = run_process(
        "git",
        &["-c", "core.hooksPath=", "status", "--porcelain"],
        workspace.root(),
        Duration::from_secs(20),
    )
    .await?;
    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "git status failed ({}): {}{}",
            output.status, output.stdout, output.stderr
        )));
    }
    Ok(output.stdout)
}

async fn git_commit(
    workspace: &EvaWorkspace,
    message: &str,
    gate: &mut TestGate,
) -> Result<ToolResult, ToolError> {
    if !gate.has_passing_tests() {
        return Err(exec_error(SelfDevelopmentError::PushRequiresPassingTests));
    }
    if !gate.workspace_was_clean() {
        return Err(ToolError::Execution(
            "self-development commit requires a clean workspace at session start".into(),
        ));
    }
    let status = git_status(workspace).await?;
    if !only_self_modified_paths(&status, gate.modified_paths()) {
        return Err(exec_error(SelfDevelopmentError::PushRequiresCleanWorkspace));
    }
    let current_head = git_head(workspace).await?;
    if gate.baseline_head.as_deref() != Some(current_head.as_str())
        || gate.tested_head.as_deref() != Some(current_head.as_str())
    {
        return Err(ToolError::Execution(
            "workspace HEAD changed outside the self-development session".into(),
        ));
    }
    for (relative, expected_hash) in &gate.modified_hashes {
        let path = workspace.resolve(relative).map_err(exec_error)?;
        let bytes = tokio::fs::read(path).await.map_err(io_error)?;
        let actual_hash = hex::encode(sha2::Sha256::digest(bytes));
        if &actual_hash != expected_hash {
            return Err(ToolError::Execution(format!(
                "workspace file changed outside eva_workspace_write_file: {relative}"
            )));
        }
    }
    if gate.modified_paths().is_empty() {
        return Err(ToolError::Execution(
            "no files changed through eva_workspace_write_file in this session".into(),
        ));
    }
    let mut add_args = vec!["add", "--"];
    add_args.extend(gate.modified_paths().iter().map(String::as_str));
    let staged = run_process("git", &add_args, workspace.root(), Duration::from_secs(30)).await?;
    if !staged.status.success() {
        return process_error("git add self-development paths", staged);
    }
    let index = run_process(
        "git",
        &["diff", "--cached", "--name-only", "--"],
        workspace.root(),
        Duration::from_secs(20),
    )
    .await?;
    if !index.status.success()
        || index
            .stdout
            .lines()
            .any(|path| !gate.modified_paths().iter().any(|known| known == path))
    {
        return Err(ToolError::Execution(
            "Git index contains paths outside this self-development session".into(),
        ));
    }
    let mut commit_args = vec!["commit", "--only", "-m", message, "--"];
    commit_args.extend(gate.modified_paths().iter().map(String::as_str));
    let output = run_process(
        "git",
        &commit_args,
        workspace.root(),
        Duration::from_secs(60),
    )
    .await?;
    let result = process_result("git commit".into(), output)?;
    if result.structured["exit_code"] != 0 {
        return Err(ToolError::Execution(format!(
            "git commit failed: {}",
            result.output
        )));
    }
    if result.structured["exit_code"] == 0 {
        let committed_head = git_head(workspace).await?;
        for (relative, expected_hash) in &gate.modified_hashes {
            let path = workspace.resolve(relative).map_err(exec_error)?;
            let git_path = path
                .strip_prefix(workspace.root())
                .map_err(io_error)?
                .to_string_lossy()
                .replace('\\', "/");
            let spec = format!("HEAD:{git_path}");
            let output = run_process(
                "git",
                &["show", spec.as_str()],
                workspace.root(),
                Duration::from_secs(20),
            )
            .await?;
            if !output.status.success()
                || hex::encode(sha2::Sha256::digest(output.stdout.as_bytes())) != *expected_hash
            {
                return Err(ToolError::Execution(format!(
                    "committed file does not match the verified workspace content: {relative}"
                )));
            }
        }
        gate.committed_head = Some(committed_head.clone());
        gate.baseline_head = Some(committed_head.clone());
        gate.tested_head = Some(committed_head);
        let status = git_status(workspace).await?;
        if !status.trim().is_empty() {
            return Err(exec_error(SelfDevelopmentError::PushRequiresCleanWorkspace));
        }
    }
    Ok(result)
}

async fn git_head(workspace: &EvaWorkspace) -> Result<String, ToolError> {
    let output = run_process(
        "git",
        &["rev-parse", "HEAD"],
        workspace.root(),
        Duration::from_secs(20),
    )
    .await?;
    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "git rev-parse failed: {}{}",
            output.stdout, output.stderr
        )));
    }
    Ok(output.stdout.trim().to_owned())
}

fn only_self_modified_paths(status: &str, modified_paths: &[String]) -> bool {
    status.lines().all(|line| {
        let Some(path) = line.get(3..) else {
            return line.is_empty();
        };
        modified_paths.iter().any(|modified| modified == path)
    })
}

fn is_protected_product_path(path: &Path) -> bool {
    const PRODUCT_BINARIES: &[&str] =
        &["EvoHime.exe", "evohime-core.exe", "evohime-supervisor.exe"];
    path.file_name()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|name| {
            PRODUCT_BINARIES
                .iter()
                .any(|binary| name.eq_ignore_ascii_case(binary))
        })
        || path.components().any(|component| {
            matches!(
                component
                    .as_os_str()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .as_str(),
                "target" | "bin" | "obj"
            )
        })
}

struct ProcessOutput {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

async fn run_process(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
) -> Result<ProcessOutput, ToolError> {
    let mut command = Command::new(program);
    crate::execution_policy_profiles::apply_environment(&mut command);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .env("GIT_TERMINAL_PROMPT", "0");
    let mut child = command.spawn().map_err(io_error)?;
    let profile = ExecutionPolicyProfile::resolve("shell_execute")
        .map_err(|error| ToolError::Execution(error.to_string()))?;
    let _guard = ProcessGuard::attach(&child, &profile)
        .map_err(|error| ToolError::Execution(format!("process guard unavailable: {error}")))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::Execution("stdout unavailable".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| ToolError::Execution("stderr unavailable".into()))?;
    let stdout_task = tokio::spawn(async move { read_bounded(&mut stdout).await });
    let stderr_task = tokio::spawn(async move { read_bounded(&mut stderr).await });
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => status.map_err(io_error)?,
        Err(_) => {
            child.start_kill().map_err(io_error)?;
            let _ = child.wait().await;
            return Err(ToolError::TimedOut(timeout));
        }
    };
    let stdout = stdout_task.await.map_err(io_error)?.map_err(io_error)?;
    let stderr = stderr_task.await.map_err(io_error)?.map_err(io_error)?;
    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
    })
}

async fn read_bounded(reader: &mut (impl tokio::io::AsyncRead + Unpin)) -> std::io::Result<String> {
    let mut collected = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = reader.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        let remaining = MAX_COMMAND_OUTPUT_BYTES.saturating_sub(collected.len());
        collected.extend_from_slice(&chunk[..count.min(remaining)]);
    }
    Ok(String::from_utf8_lossy(&collected).into_owned())
}

fn process_result(label: String, output: ProcessOutput) -> Result<ToolResult, ToolError> {
    let code = output.status.code().unwrap_or(-1);
    let structured = json!({"action": label, "stdout": output.stdout, "stderr": output.stderr, "exit_code": code});
    Ok(ToolResult {
        output: structured.to_string(),
        structured,
    })
}

fn process_error(label: &str, output: ProcessOutput) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution(format!(
        "{label} failed ({}): {}{}",
        output.status, output.stdout, output.stderr
    )))
}

fn parse_input<T: for<'de> Deserialize<'de>>(name: &str, value: Value) -> Result<T, ToolError> {
    serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
        tool: name.into(),
        message: error.to_string(),
    })
}

fn exec_error(error: SelfDevelopmentError) -> ToolError {
    ToolError::Execution(error.to_string())
}

fn io_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::Execution(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        let status = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(dir.path())
            .status()
            .expect("git init");
        assert!(status.success(), "git init should create a test repository");
        for (key, value) in [
            ("user.email", "eva@example.invalid"),
            ("user.name", "Queen Eva"),
        ] {
            let configured = std::process::Command::new("git")
                .args(["config", key, value])
                .current_dir(dir.path())
                .status()
                .expect("git config");
            assert!(configured.success());
        }
        std::fs::write(dir.path().join("baseline.txt"), "baseline\n").expect("baseline file");
        let added = std::process::Command::new("git")
            .args(["add", "baseline.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git add baseline");
        assert!(added.success());
        let committed = std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "test: baseline"])
            .current_dir(dir.path())
            .status()
            .expect("git baseline commit");
        assert!(committed.success());
        dir
    }

    #[test]
    fn eva_workspace_path_validation() {
        let dir = workspace_root();
        let workspace =
            EvaWorkspace::open(dir.path().to_str().expect("utf8")).expect("valid workspace");
        assert_eq!(
            workspace.root(),
            &dir.path().canonicalize().expect("canonical")
        );

        assert_eq!(
            EvaWorkspace::open(""),
            Err(SelfDevelopmentError::WorkspaceNotConfigured)
        );
        assert!(matches!(
            EvaWorkspace::open("definitely/not/here"),
            Err(SelfDevelopmentError::WorkspaceMissing(_))
        ));

        let plain = tempfile::tempdir().expect("temp dir");
        assert!(matches!(
            EvaWorkspace::open(plain.path().to_str().expect("utf8")),
            Err(SelfDevelopmentError::WorkspaceNotGitRepository(_))
        ));
    }

    #[test]
    fn workspace_rejects_escaping_paths() {
        let dir = workspace_root();
        let workspace =
            EvaWorkspace::open(dir.path().to_str().expect("utf8")).expect("valid workspace");

        assert!(workspace.resolve("crates/tool-runtime/src/lib.rs").is_ok());
        assert_eq!(
            workspace.resolve("../etc/passwd"),
            Err(SelfDevelopmentError::PathEscapesWorkspace(
                "../etc/passwd".to_string()
            ))
        );
        assert!(matches!(
            workspace.resolve("C:\\Windows\\system32"),
            Err(SelfDevelopmentError::PathEscapesWorkspace(_))
        ));
    }

    #[test]
    fn command_allow_list_rejects_shell_composition() {
        assert!(validate_command("cargo check -p evohime-core").is_ok());
        assert!(validate_command("cargo test --lib").is_ok());
        assert!(validate_command("npm run typecheck").is_ok());
        assert!(validate_command("cargo clippy --workspace --all-targets -- -D warnings").is_ok());
        assert!(validate_command("cargo fmt --all --check").is_ok());
        assert!(validate_command("cargo test -p evohime-tool-runtime self_development").is_ok());
        assert!(validate_command("npm run typecheck").is_ok());
        assert!(is_test_command("npm test"));
        assert!(!is_test_command("npm run typecheck"));
        assert!(matches!(
            validate_command("cargo check && rm -rf /"),
            Err(SelfDevelopmentError::CommandNotAllowed(_))
        ));
        assert!(matches!(
            validate_command("curl http://example.com"),
            Err(SelfDevelopmentError::CommandNotAllowed(_))
        ));
        assert!(matches!(
            validate_command("cargo test --manifest-path C:/outside/Cargo.toml"),
            Err(SelfDevelopmentError::CommandNotAllowed(_))
        ));
    }

    #[test]
    fn test_gate_detects_passing_test_run() {
        let mut gate = TestGate::default();
        assert!(!gate.has_passing_tests());

        gate.record("cargo check", 0);
        assert!(!gate.has_passing_tests(), "cargo check is not a test run");

        gate.record("cargo test --lib", 1);
        assert!(
            !gate.has_passing_tests(),
            "failing tests must not unlock push"
        );

        gate.record("cargo test --lib", 0);
        assert!(
            !gate.has_passing_tests(),
            "test runs without a captured clean checkout baseline cannot unlock push"
        );
        gate.workspace_was_clean = Some(true);
        gate.baseline_head = Some("baseline".into());
        gate.record("cargo test --lib", 0);
        assert!(gate.has_passing_tests());
    }

    #[test]
    fn self_development_tools_are_only_available_to_the_queen() {
        assert!(profile_permitted("queen", 0));
        assert!(profile_permitted("coordinator", 0));
        assert!(!profile_permitted("researcher", 0));
        assert!(!profile_permitted("coordinator", 1));
        assert!(!profile_permitted("implementer", 2));
    }

    #[test]
    fn eva_cannot_push_without_passing_tests() {
        let gate = TestGate::default();
        assert_eq!(
            ensure_push_allowed(&gate, false),
            Err(SelfDevelopmentError::PushRequiresPassingTests)
        );
        let mut passed = TestGate {
            workspace_was_clean: Some(true),
            baseline_head: Some("baseline".into()),
            ..TestGate::default()
        };
        passed.record("cargo test -p evohime-core", 0);
        assert_eq!(
            ensure_push_allowed(&passed, true),
            Err(SelfDevelopmentError::PushRequiresCleanWorkspace)
        );
        assert!(ensure_push_allowed(&passed, false).is_ok());
    }

    #[tokio::test]
    async fn mock_self_development_loop_reads_writes_runs_and_commits() {
        let dir = workspace_root();
        let workspace = EvaWorkspace::open(dir.path().to_str().expect("utf8")).expect("repo");
        std::fs::write(workspace.root().join("note.txt"), "before\n").expect("seed file");
        let added = std::process::Command::new("git")
            .args(["add", "note.txt"])
            .current_dir(workspace.root())
            .status()
            .expect("git add test fixture");
        assert!(added.success());
        let committed = std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "test: add loop fixture"])
            .current_dir(workspace.root())
            .status()
            .expect("git commit test fixture");
        assert!(committed.success());
        let active = tempfile::tempdir().expect("active workspace");
        let mut gate = TestGate::default();
        let mut context = ToolContext {
            workspace_root: active.path().to_path_buf(),
            task_id: uuid::Uuid::new_v4(),
            session_id: None,
            progress_tx: None,
            eva_workspace_root: Some(workspace.root().to_path_buf()),
            self_development_profile: Some("queen".into()),
            agent_depth: 0,
            self_development_test_gate: Default::default(),
        };
        let read = execute(
            &context,
            TOOL_READ_FILE,
            json!({"path":"note.txt"}),
            &mut gate,
        )
        .await
        .expect("read source");
        assert_eq!(read.structured["content"], "before\n");
        execute(
            &context,
            TOOL_WRITE_FILE,
            json!({"path":"note.txt","content":"after\n"}),
            &mut gate,
        )
        .await
        .expect("write change");
        // The fixture has no Cargo project. Record the verified test result
        // directly so this test covers the commit gate without compilation.
        gate.record("cargo test --lib", 0);
        assert!(gate.has_passing_tests());

        let committed = git(
            &workspace,
            json!({"action":"commit","message":"test: eva workspace loop"}),
            &mut gate,
        )
        .await;
        assert!(committed.is_ok());
        context.self_development_profile = Some("researcher".into());
        assert!(matches!(
            execute(
                &context,
                TOOL_READ_FILE,
                json!({"path":"note.txt"}),
                &mut gate
            )
            .await,
            Err(ToolError::PermissionDenied(Permission::GitWrite))
        ));
    }

    #[test]
    fn self_development_allows_git_log_and_fmt() {
        assert!(validate_command("git log -n 5").is_ok());
        assert!(validate_command("git log --oneline -n 10").is_ok());
        assert!(validate_command("git log --stat --max-count=5 -- Cargo.toml").is_ok());
        assert!(validate_command("cargo fmt").is_ok());
        assert!(validate_command("cargo fmt --all").is_ok());
        assert!(validate_command("npm run lint").is_ok());
        assert!(validate_command("npx prettier --check src/index.ts").is_ok());

        assert!(matches!(
            validate_command("git push"),
            Err(SelfDevelopmentError::CommandNotAllowed(_))
        ));
        assert!(matches!(
            validate_command("git -c core.pager=sh log"),
            Err(SelfDevelopmentError::CommandNotAllowed(_))
        ));
        assert!(matches!(
            validate_command("git reset --hard"),
            Err(SelfDevelopmentError::CommandNotAllowed(_))
        ));
        assert!(matches!(
            validate_command("npx arbitrary-pkg"),
            Err(SelfDevelopmentError::CommandNotAllowed(_))
        ));

        let gate = TestGate::default();
        assert_eq!(
            ensure_push_allowed(&gate, false),
            Err(SelfDevelopmentError::PushRequiresPassingTests)
        );
    }
}
