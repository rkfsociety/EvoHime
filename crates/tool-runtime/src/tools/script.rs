use crate::execution_policy_profiles::{validate_script_arguments, ExecutionPolicyProfile};
use crate::tools::shell::run_bounded_process;
use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Registry identifier for workspace script execution.
pub const NAME: &str = "script_execute";
/// Short user-facing summary shown in tool catalogs.
pub const DESCRIPTION: &str = "Run a workspace script with an allow-listed interpreter (PowerShell, Python or Node); the script file must already exist in the workspace";
/// Permission required before a script interpreter may be started.
pub const PERMISSIONS: &[Permission] = &[Permission::ScriptExecute];
/// Default execution deadline; request-specific values cannot exceed the
/// execution profile cap.
pub const TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum size of a script file this tool will execute.
pub const MAX_SCRIPT_BYTES: u64 = 1024 * 1024;

/// Allow-listed interpreter with its launcher flags and banned switches.
struct InterpreterSpec {
    id: &'static str,
    /// Executable candidates tried in PATH order (first match wins).
    candidates: &'static [&'static str],
    /// Safety flags inserted before the script path.
    launcher_flags: &'static [&'static str],
    /// Interpreter switches that would replace the script file with inline
    /// code or load code from an arbitrary location; always rejected. Stored
    /// without leading dashes.
    forbidden_flags: &'static [&'static str],
}

const INTERPRETERS: &[InterpreterSpec] = &[
    InterpreterSpec {
        id: "powershell",
        candidates: &["pwsh", "powershell"],
        launcher_flags: &["-NoProfile", "-NonInteractive"],
        forbidden_flags: &["command", "c", "encodedcommand", "enc", "e", "ec", "file"],
    },
    InterpreterSpec {
        id: "python",
        candidates: &["python", "python3", "py"],
        launcher_flags: &[],
        forbidden_flags: &["c", "m"],
    },
    InterpreterSpec {
        id: "node",
        candidates: &["node"],
        launcher_flags: &[],
        forbidden_flags: &[
            "e",
            "p",
            "eval",
            "print",
            "input-type",
            "i",
            "r",
            "require",
            "import",
            "loader",
            "experimental-loader",
        ],
    },
];

/// Interpreter identifiers accepted by [`NAME`], in declaration order.
pub(crate) fn interpreter_ids() -> Vec<&'static str> {
    INTERPRETERS.iter().map(|spec| spec.id).collect()
}

#[derive(Deserialize)]
struct Input {
    interpreter: String,
    script: String,
    #[serde(default)]
    args: Vec<String>,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
}

/// Runs a workspace script file with an allow-listed interpreter.
///
/// The script is passed to the interpreter as a file, never as inline code:
/// the launched command stays a direct program invocation (no shell), the
/// script content is an auditable workspace artifact, and the run inherits
/// the bounded execution profile (scrubbed environment, no shell fallback,
/// bounded output, timeout, cooperative cancellation).
///
/// # Errors
///
/// Returns [`ToolError`] for invalid input, an unknown interpreter, a script
/// outside the workspace, banned interpreter switches, denied egress,
/// cancellation, timeout, or process failure.
pub async fn execute(
    ctx: &ToolContext,
    value: Value,
    cancellation: CancellationToken,
) -> Result<ToolResult, ToolError> {
    let input: Input = serde_json::from_value(value).map_err(|e| ToolError::InvalidInput {
        tool: NAME.into(),
        message: e.to_string(),
    })?;
    let spec = interpreter_spec(&input.interpreter).ok_or_else(|| ToolError::InvalidInput {
        tool: NAME.into(),
        message: format!(
            "interpreter must be one of: {}",
            INTERPRETERS
                .iter()
                .map(|spec| spec.id)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })?;
    validate_script_args(spec, &input.args)?;
    let resolved =
        ExecutionPolicyProfile::resolve(NAME).map_err(|error| ToolError::InvalidInput {
            tool: NAME.into(),
            message: error.to_string(),
        })?;

    let script_path = resolve_workspace_script(ctx, &input.script)?;
    let metadata =
        tokio::fs::metadata(&script_path)
            .await
            .map_err(|error| ToolError::NotFound {
                tool: NAME.into(),
                path: input.script.clone(),
                hint: format!("script file is not readable: {error}"),
            })?;
    if !metadata.is_file() {
        return Err(ToolError::InvalidInput {
            tool: NAME.into(),
            message: format!("script path is not a file: {}", input.script),
        });
    }
    if metadata.len() > MAX_SCRIPT_BYTES {
        return Err(ToolError::InvalidInput {
            tool: NAME.into(),
            message: format!(
                "script exceeds the {MAX_SCRIPT_BYTES}-byte limit; split it into smaller files"
            ),
        });
    }

    let cwd = match input.cwd.as_deref() {
        Some(path) => resolve_workspace_script(ctx, path)?,
        None => ctx.sandbox()?.root().to_path_buf(),
    };

    let script_display = script_path.display().to_string();
    let mut invocation: Vec<String> = spec
        .launcher_flags
        .iter()
        .map(ToString::to_string)
        .collect();
    invocation.push(script_display.clone());
    invocation.extend(input.args.iter().cloned());

    let (program, run) = run_first_available(
        ctx,
        spec,
        &invocation,
        cwd,
        resolved.timeout(input.timeout_ms),
        &resolved,
        cancellation,
    )
    .await?;

    let output = format!(
        "interpreter: {}\nscript: {}\ncwd: {}\nexit_code: {}\nstdout:\n{}\nstderr:\n{}",
        spec.id,
        input.script,
        run.cwd.display(),
        run.exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "terminated".to_string()),
        if run.stdout.is_empty() {
            "<empty>"
        } else {
            &run.stdout
        },
        if run.stderr.is_empty() {
            "<empty>"
        } else {
            &run.stderr
        }
    );
    let ok = run.exit_code == Some(0);
    Ok(ToolResult {
        output,
        structured: json!({
            "interpreter": spec.id,
            "program": program,
            "script": input.script,
            "script_resolved_path": script_display,
            "cwd": run.cwd.display().to_string(),
            "stdout": run.stdout,
            "stderr": run.stderr,
            "exit_code": run.exit_code,
            "timed_out": false,
            "ok": ok,
            "resolved_profile": {
                "profile_id": resolved.profile.profile_id,
                "version": resolved.profile.version,
                "hash": resolved.profile_hash,
                "backend": resolved.backend
            }
        }),
    })
}

/// Resolves a script or working-directory path strictly inside the workspace.
///
/// Unlike the read-only filesystem tools this never recovers a path by a unique
/// suffix: starting an interpreter on a guessed file would be worse than a
/// clean failure. Namespace prefixes (`uploads/`, `outputs/`, `scratch/`) use
/// the same resolver as the revision-safe file tools.
fn resolve_workspace_script(ctx: &ToolContext, path: &str) -> Result<PathBuf, ToolError> {
    use crate::revision_safe_workspace_files::{resolve_logical, RevisionError};

    match resolve_logical(ctx, path, false) {
        Ok((_, _, resolved)) => Ok(resolved),
        Err(RevisionError::Escape) => Err(ToolError::WorkspaceBoundaryDenied(
            Permission::FilesystemRead,
        )),
        Err(RevisionError::InvalidPath) => Err(ToolError::InvalidInput {
            tool: NAME.into(),
            message: format!("script path must be workspace-relative: {path}"),
        }),
        Err(RevisionError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(ToolError::NotFound {
                tool: NAME.into(),
                path: path.to_owned(),
                hint: "script file does not exist in the workspace".into(),
            })
        }
        Err(error) => Err(ToolError::Execution(error.to_string())),
    }
}

/// Returns the interpreter spec for a case-insensitive identifier.
fn interpreter_spec(id: &str) -> Option<&'static InterpreterSpec> {
    INTERPRETERS
        .iter()
        .find(|spec| spec.id.eq_ignore_ascii_case(id.trim()))
}

/// Rejects interpreter switches that would replace the script file with
/// inline code or load code from an arbitrary location.
fn validate_script_args(spec: &InterpreterSpec, args: &[String]) -> Result<(), ToolError> {
    for arg in args {
        if is_forbidden_switch(spec, arg) {
            return Err(ToolError::InvalidInput {
                tool: NAME.into(),
                message: format!(
                    "argument {arg:?} is not allowed for interpreter {}: the script must come from a workspace file, not inline code",
                    spec.id
                ),
            });
        }
    }
    Ok(())
}

/// Returns whether `arg` is one of the interpreter's banned switches.
///
/// Windows interpreters accept `-Command:code` and `--eval=code` spellings as
/// well as the separated form, so the comparison is made on the switch name
/// with any `=`, `:` value separator and extra leading dashes removed.
fn is_forbidden_switch(spec: &InterpreterSpec, arg: &str) -> bool {
    let trimmed = arg.trim();
    let Some(rest) = trimmed
        .strip_prefix('-')
        .or_else(|| trimmed.strip_prefix('/'))
    else {
        return false;
    };
    let name = rest
        .split(['=', ':'])
        .next()
        .unwrap_or_default()
        .trim_start_matches('-')
        .to_ascii_lowercase();
    spec.forbidden_flags.contains(&name.as_str())
}

/// Validates one resolved interpreter invocation before any process starts.
///
/// The interpreter binary is resolved from the closed allow-list rather than
/// from call input; this pins that identity and applies the argument rules
/// (no inline code, no secret-exfiltration markers).
fn validate_invocation(
    spec: &InterpreterSpec,
    program: &str,
    args: &[String],
) -> Result<(), ToolError> {
    let stem = std::path::Path::new(program)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(program);
    if !spec
        .candidates
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(stem))
    {
        return Err(ToolError::InvalidInput {
            tool: NAME.into(),
            message: format!(
                "interpreter {} must be resolved from the allow-list",
                spec.id
            ),
        });
    }
    validate_script_args(spec, args)?;
    validate_script_arguments(args).map_err(|error| ToolError::InvalidInput {
        tool: NAME.into(),
        message: error.to_string(),
    })
}

/// Resolves an interpreter candidate against `PATH` and runs the script.
///
/// Candidates are tried in order; the first executable found on `PATH` wins.
/// The program name (not the resolved path) is what egress validation sees.
///
/// # Errors
///
/// Returns [`ToolError`] propagated from the bounded run.
async fn run_first_available(
    ctx: &ToolContext,
    spec: &InterpreterSpec,
    invocation: &[String],
    cwd: PathBuf,
    duration: Duration,
    resolved: &crate::execution_policy_profiles::ResolvedExecutionProfile,
    cancellation: CancellationToken,
) -> Result<(String, crate::tools::shell::BoundedRun), ToolError> {
    let mut last_error: Option<ToolError> = None;
    for candidate in spec.candidates {
        let program = resolve_on_path(candidate);
        validate_invocation(spec, &program, invocation)?;
        match run_bounded_process(
            ctx,
            &program,
            invocation,
            cwd.clone(),
            duration,
            resolved,
            cancellation.clone(),
        )
        .await
        {
            Ok(run) => return Ok((program, run)),
            Err(ToolError::Execution(message)) if message.starts_with("spawn failed") => {
                last_error = Some(ToolError::Execution(message));
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| ToolError::Execution("no interpreter candidate".into())))
}

/// Resolves an executable name against `PATH` (with the Windows `PATHEXT`
/// `.exe` suffix) and returns the found path or the bare name as fallback.
fn resolve_on_path(candidate: &str) -> String {
    let Some(path_var) = std::env::var_os("PATH") else {
        return candidate.to_owned();
    };
    for dir in std::env::split_paths(&path_var) {
        let file_name = if cfg!(windows) {
            format!("{candidate}.exe")
        } else {
            candidate.to_owned()
        };
        let full = dir.join(&file_name);
        if full.is_file() {
            return full.display().to_string();
        }
    }
    candidate.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreter_lookup_is_case_insensitive_and_closed() {
        assert!(interpreter_spec("PowerShell").is_some());
        assert!(interpreter_spec("python").is_some());
        assert!(interpreter_spec("node").is_some());
        assert!(interpreter_spec("cmd").is_none());
        assert!(interpreter_spec("bash").is_none());
        assert!(interpreter_spec("wscript").is_none());
        assert!(interpreter_spec("").is_none());
    }

    #[test]
    fn inline_code_switches_are_rejected() {
        let powershell = interpreter_spec("powershell").unwrap();
        for banned in [
            "-Command",
            "-command",
            "-EncodedCommand",
            "-c",
            "-Command:Get-Process",
            "-c:Get-Process",
        ] {
            let error = validate_script_args(powershell, &[banned.to_owned()])
                .expect_err("banned switch must be rejected");
            assert!(
                error.to_string().contains("not allowed"),
                "unexpected error: {error}"
            );
        }
        let python = interpreter_spec("python").unwrap();
        assert!(validate_script_args(python, &["-c".into()]).is_err());
        assert!(validate_script_args(python, &["--verbose".into()]).is_ok());
        let node = interpreter_spec("node").unwrap();
        assert!(validate_script_args(node, &["-e".into()]).is_err());
        assert!(validate_script_args(node, &["--eval".into()]).is_err());
        assert!(validate_script_args(node, &["--eval=1+1".into()]).is_err());
        assert!(validate_script_args(node, &["--require".into()]).is_err());
        assert!(validate_script_args(node, &["-r".into()]).is_err());
        assert!(validate_script_args(node, &["--max-old-space-size=64".into()]).is_ok());
    }

    #[test]
    fn resolved_interpreter_path_is_not_treated_as_a_shell_program() {
        // `shell_execute` bans interpreter names and absolute paths via
        // `validate_program_name`; a script run is allowed to use an absolute
        // interpreter path resolved from PATH.
        let powershell = interpreter_spec("powershell").unwrap();
        assert!(validate_invocation(
            powershell,
            r"C:\Program Files\PowerShell\7\pwsh.exe",
            &[
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "scripts/check.ps1".into(),
            ],
        )
        .is_ok());
        assert!(validate_invocation(powershell, "/usr/bin/pwsh", &[]).is_ok());
        assert!(validate_invocation(powershell, r"C:\Windows\System32\cmd.exe", &[]).is_err());
    }

    #[test]
    fn benign_script_args_are_accepted() {
        let powershell = interpreter_spec("powershell").unwrap();
        assert!(
            validate_script_args(powershell, &["-Verbose".into(), "-Path".into(), "x".into()])
                .is_ok()
        );
    }

    #[tokio::test]
    async fn unknown_interpreter_fails_with_closed_message() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
            eva_workspace_root: None,
            self_development_profile: None,
            agent_depth: 1,
            self_development_test_gate: std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::tools::self_development::TestGate::default(),
            )),
        };
        let error = execute(
            &ctx,
            serde_json::json!({"interpreter": "bash", "script": "run.sh"}),
            CancellationToken::new(),
        )
        .await
        .expect_err("unknown interpreter must fail");
        assert!(error.to_string().contains("interpreter must be one of"));
    }

    #[tokio::test]
    async fn script_outside_workspace_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
            eva_workspace_root: None,
            self_development_profile: None,
            agent_depth: 1,
            self_development_test_gate: std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::tools::self_development::TestGate::default(),
            )),
        };
        let error = execute(
            &ctx,
            serde_json::json!({"interpreter": "python", "script": "../outside.py"}),
            CancellationToken::new(),
        )
        .await
        .expect_err("escape attempt must fail");
        assert!(
            error.to_string().contains("boundary") || error.to_string().contains("not readable"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn script_resolution_never_guesses_a_path_by_suffix() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("deep")).unwrap();
        std::fs::write(dir.path().join("deep/run.py"), "print(1)").unwrap();
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
            eva_workspace_root: None,
            self_development_profile: None,
            agent_depth: 1,
            self_development_test_gate: std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::tools::self_development::TestGate::default(),
            )),
        };

        let resolved = resolve_workspace_script(&ctx, "deep/run.py")
            .expect("existing script resolves exactly");
        assert!(resolved.ends_with("deep/run.py") || resolved.ends_with("deep\\run.py"));

        // `find_unique_suffix` would have matched `deep/run.py` here; executing
        // a different file than the model named is never acceptable.
        let error =
            resolve_workspace_script(&ctx, "../run.py").expect_err("traversal must not be guessed");
        assert!(matches!(error, ToolError::WorkspaceBoundaryDenied(_)));
    }

    #[test]
    fn path_resolution_falls_back_to_bare_name() {
        let resolved = resolve_on_path("definitely-not-a-real-binary-evohime");
        assert_eq!(resolved, "definitely-not-a-real-binary-evohime");
    }
}
