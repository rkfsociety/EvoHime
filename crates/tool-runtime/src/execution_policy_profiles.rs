//! Core-resolved execution profiles for process based tools.
//!
//! The command text is never allowed to select the backend or widen the
//! environment.  `ToolRegistry` and the two process tools use this module as
//! their single resolver.  Durable profile catalogs belong to Core/storage;
//! this crate only owns the bounded runtime contract and ephemeral process
//! guard.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io, time::Duration};
use tokio::process::{Child, Command};

/// Version of the runtime execution-profile contract.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum profile identifier length in bytes.
pub const MAX_PROFILE_ID: usize = 64;
/// Hard maximum process timeout in milliseconds.
pub const MAX_TIMEOUT_MS: u64 = 60_000;
/// Hard maximum captured output size in bytes.
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// Platform process backend selected by an execution profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendRequirement {
    /// Use the portable child-process backend without Windows job semantics.
    Portable,
    /// Require Windows Job Object assignment for process-tree cleanup.
    WindowsJobObject,
}

/// Network access policy for the child process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// Prevent the child process from inheriting network access.
    Deny,
    /// Permit the child to inherit the host's network access.
    Inherit,
}

/// Environment inheritance policy for the child process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentPolicy {
    /// Pass only the runtime's scrubbed environment allowlist.
    ScrubbedAllowlist,
}

/// Bounded policy contract for a process-based tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPolicyProfile {
    /// Profile contract version; must equal [`CONTRACT_VERSION`].
    pub schema_version: u32,
    /// Stable identifier for the selected profile.
    pub profile_id: String,
    /// Version of this specific profile configuration.
    pub version: u64,
    /// Process backend required to enforce the profile.
    pub backend: BackendRequirement,
    /// Whether the process must run under the platform sandbox backend.
    pub sandbox_required: bool,
    /// Network access policy for the child process.
    pub network: NetworkPolicy,
    /// Environment inheritance policy for the child process.
    pub environment: EnvironmentPolicy,
    /// Maximum process runtime in milliseconds.
    pub timeout_ms: u64,
    /// Maximum combined bytes captured from process output.
    pub max_output_bytes: usize,
    /// Whether terminating the process must also terminate its descendants.
    pub kill_process_tree: bool,
}

/// Validated profile with its digest and selected backend label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedExecutionProfile {
    /// Validated execution constraints.
    pub profile: ExecutionPolicyProfile,
    /// Digest of the canonical profile representation.
    pub profile_hash: String,
    /// Stable name of the backend selected for execution.
    pub backend: String,
}

/// Invalid or unsupported process execution profile configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionPolicyError {
    /// A profile field violates its supported bounds or policy.
    InvalidProfile(&'static str),
    /// The profile contract version differs from [`CONTRACT_VERSION`].
    UnsupportedVersion(u32),
    /// The requested execution backend is unavailable on this platform.
    BackendUnavailable,
    /// The tool name is not an allowed process entrypoint.
    UnsupportedTool,
    /// The invocation has an unapproved direct egress or sensitive-data pattern.
    EgressDenied,
}

impl std::fmt::Display for ExecutionPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidProfile(reason) => write!(f, "invalid execution profile: {reason}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported execution profile version: {version}")
            }
            Self::BackendUnavailable => f.write_str("required execution backend unavailable"),
            Self::UnsupportedTool => f.write_str("tool is not a process entrypoint"),
            Self::EgressDenied => f.write_str("process invocation violates egress safety policy"),
        }
    }
}

impl std::error::Error for ExecutionPolicyError {}

impl ExecutionPolicyProfile {
    /// Builds the current restrictive default profile for a process tool.
    ///
    /// ```
    /// use evohime_tool_runtime::execution_policy_profiles::ExecutionPolicyProfile;
    /// let profile = ExecutionPolicyProfile::default_for("shell_execute").unwrap();
    /// assert!(profile.validate().is_ok());
    /// ```
    pub fn default_for(tool: &str) -> Result<Self, ExecutionPolicyError> {
        if !matches!(tool, "shell_execute" | "process_run" | "script_execute") {
            return Err(ExecutionPolicyError::UnsupportedTool);
        }
        Ok(Self {
            schema_version: CONTRACT_VERSION,
            profile_id: "restricted-process-v1".into(),
            version: 1,
            backend: if cfg!(windows) {
                BackendRequirement::WindowsJobObject
            } else {
                BackendRequirement::Portable
            },
            sandbox_required: cfg!(windows),
            network: NetworkPolicy::Deny,
            environment: EnvironmentPolicy::ScrubbedAllowlist,
            timeout_ms: MAX_TIMEOUT_MS,
            max_output_bytes: MAX_OUTPUT_BYTES,
            kill_process_tree: true,
        })
    }

    /// Checks contract version, bounds, and mandatory process-tree cleanup.
    pub fn validate(&self) -> Result<(), ExecutionPolicyError> {
        if self.schema_version != CONTRACT_VERSION {
            return Err(ExecutionPolicyError::UnsupportedVersion(
                self.schema_version,
            ));
        }
        if self.profile_id.is_empty() || self.profile_id.len() > MAX_PROFILE_ID {
            return Err(ExecutionPolicyError::InvalidProfile("profile_id bound"));
        }
        if self.version == 0 || self.timeout_ms == 0 || self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(ExecutionPolicyError::InvalidProfile(
                "timeout/version bound",
            ));
        }
        if self.max_output_bytes == 0 || self.max_output_bytes > MAX_OUTPUT_BYTES {
            return Err(ExecutionPolicyError::InvalidProfile("output bound"));
        }
        if !self.kill_process_tree {
            return Err(ExecutionPolicyError::InvalidProfile(
                "tree cleanup is required",
            ));
        }
        if self.sandbox_required
            && !cfg!(windows)
            && self.backend == BackendRequirement::WindowsJobObject
        {
            return Err(ExecutionPolicyError::BackendUnavailable);
        }
        Ok(())
    }

    /// Resolves and hashes the built-in profile for an approved process tool.
    pub fn resolve(tool: &str) -> Result<ResolvedExecutionProfile, ExecutionPolicyError> {
        let profile = Self::default_for(tool)?;
        profile.validate()?;
        let canonical = serde_json::to_vec(&profile)
            .map_err(|_| ExecutionPolicyError::InvalidProfile("canonical serialization"))?;
        let mut hasher = Sha256::new();
        hasher.update(b"evohime-execution-policy-profile-v1\0");
        hasher.update(canonical);
        Ok(ResolvedExecutionProfile {
            backend: match profile.backend {
                BackendRequirement::Portable => "portable".into(),
                BackendRequirement::WindowsJobObject => "windows_job_object".into(),
            },
            profile,
            profile_hash: hex::encode(hasher.finalize()),
        })
    }
}

/// Rejects shell interpreters and path-bearing names; callers must execute a direct program.
pub fn validate_program_name(program: &str) -> Result<(), ExecutionPolicyError> {
    if program.is_empty()
        || program.contains(['/', '\\'])
        || matches!(
            program.to_ascii_lowercase().as_str(),
            "cmd"
                | "cmd.exe"
                | "powershell"
                | "powershell.exe"
                | "pwsh"
                | "pwsh.exe"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "wsl"
                | "wsl.exe"
                | "python"
                | "python3"
                | "python.exe"
                | "py"
                | "node"
                | "node.exe"
                | "npm"
                | "npm.cmd"
                | "npx"
                | "npx.cmd"
                | "uv"
                | "uvx"
                | "perl"
                | "ruby"
                | "php"
                | "wscript"
                | "wscript.exe"
                | "cscript"
                | "cscript.exe"
                | "mshta"
                | "mshta.exe"
                | "rundll32"
                | "rundll32.exe"
        )
    {
        return Err(ExecutionPolicyError::InvalidProfile(
            "program must be a direct executable name (script interpreters like node/python are disallowed; use filesystem_patch or filesystem_write to edit files)",
        ));
    }
    Ok(())
}

/// Rejects direct network clients and process arguments that combine likely
/// secret sources with outbound-transfer indicators.
///
/// This is a bounded defense-in-depth check, not an operating-system network
/// sandbox: arbitrary permitted executables may implement their own network
/// protocols. External effects still require the Core-owned permission policy.
pub fn validate_process_egress(program: &str, args: &[String]) -> Result<(), ExecutionPolicyError> {
    validate_program_name(program)?;
    let program = program.to_ascii_lowercase();
    let program = program.strip_suffix(".exe").unwrap_or(&program);
    if matches!(
        program,
        "curl"
            | "wget"
            | "aria2c"
            | "ftp"
            | "sftp"
            | "scp"
            | "ssh"
            | "nc"
            | "ncat"
            | "netcat"
            | "socat"
            | "cloudflared"
            | "ngrok"
            | "frpc"
            | "chisel"
            | "rclone"
    ) {
        return Err(ExecutionPolicyError::EgressDenied);
    }

    let normalized = args
        .iter()
        .map(|arg| arg.to_ascii_lowercase().replace('\\', "/"))
        .collect::<Vec<_>>()
        .join(" ");
    let without_exe = args
        .iter()
        .map(|arg| {
            let lower = arg.to_ascii_lowercase().replace('\\', "/");
            lower
                .strip_suffix(".exe")
                .map(str::to_owned)
                .unwrap_or(lower)
        })
        .collect::<Vec<_>>()
        .join(" ");
    if egress_markers_present(&normalized, &without_exe) {
        return Err(ExecutionPolicyError::EgressDenied);
    }
    Ok(())
}

/// Screens script arguments with the same egress markers as a direct command.
///
/// [`validate_program_name`] deliberately bans shell interpreters for
/// `shell_execute`. `script_execute` instead picks its interpreter from a
/// closed allow-list, so the binary is not caller-selected; only the arguments
/// still need the bounded secret-exfiltration check.
pub fn validate_script_arguments(args: &[String]) -> Result<(), ExecutionPolicyError> {
    let normalized = args
        .iter()
        .map(|arg| arg.to_ascii_lowercase().replace('\\', "/"))
        .collect::<Vec<_>>()
        .join(" ");
    let without_exe = args
        .iter()
        .map(|arg| {
            let lower = arg.to_ascii_lowercase().replace('\\', "/");
            lower
                .strip_suffix(".exe")
                .map(str::to_owned)
                .unwrap_or(lower)
        })
        .collect::<Vec<_>>()
        .join(" ");
    if egress_markers_present(&normalized, &without_exe) {
        return Err(ExecutionPolicyError::EgressDenied);
    }
    Ok(())
}

/// Returns whether argument text combines likely secret sources with outbound
/// transfer indicators.
fn egress_markers_present(normalized: &str, without_exe: &str) -> bool {
    if without_exe.contains(".trycloudflare.com") {
        return true;
    }

    let sensitive_source = [
        ".env",
        ".ssh/",
        "id_rsa",
        "id_ed25519",
        ".aws/credentials",
        ".config/solana/",
        "keystore",
        "private key",
        "mnemonic",
        "seed phrase",
        "private|secret|mnemonic|seed|key|pass",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));
    let outbound_transfer = [
        "http://", "https://", "--data", "--upload", "upload", "-x post", "-x put", "\"post\"",
        "'post'", "--remote",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));
    sensitive_source && outbound_transfer
}

impl ResolvedExecutionProfile {
    /// Returns the requested timeout capped by the profile maximum.
    pub fn timeout(&self, requested_ms: Option<u64>) -> Duration {
        Duration::from_millis(
            requested_ms
                .unwrap_or(self.profile.timeout_ms)
                .min(self.profile.timeout_ms),
        )
    }
}

/// Applies the profile's deny-by-default environment.  Values are deliberately
/// not accepted from a tool input; this map is only the bounded inherited
/// environment selected by `shell_env`.
pub fn apply_environment(command: &mut Command) {
    crate::shell_env::apply_scrubbed_env(command);
}

/// Rejects caller-supplied environment variables outside the runtime allowlist.
pub fn reject_user_environment(
    env: Option<&HashMap<String, String>>,
) -> Result<(), ExecutionPolicyError> {
    if env.is_some_and(|values| !values.is_empty()) {
        return Err(ExecutionPolicyError::InvalidProfile(
            "user environment is not allowed",
        ));
    }
    Ok(())
}

/// Keeps platform process-tree cleanup resources alive for a child process.
#[derive(Debug)]
pub struct ProcessGuard {
    #[cfg(windows)]
    #[allow(dead_code)]
    job: Option<WindowsJobObject>,
}

impl ProcessGuard {
    /// Attaches the profile's process-tree guard to an already spawned child.
    pub fn attach(child: &Child, profile: &ResolvedExecutionProfile) -> io::Result<Self> {
        #[cfg(windows)]
        {
            if profile.profile.backend == BackendRequirement::WindowsJobObject {
                return Ok(Self {
                    job: Some(WindowsJobObject::create_and_assign(child)?),
                });
            }
        }
        #[cfg(not(windows))]
        let _ = (child, profile);
        Ok(Self {
            #[cfg(windows)]
            job: None,
        })
    }

    /// Attaches a platform process-tree guard to an already spawned child so
    /// dropping the future (for example on timeout or cancellation) terminates
    /// the child and its descendants on Windows.
    pub(crate) fn attach_process_tree(child: &mut Child) -> io::Result<Self> {
        #[cfg(windows)]
        {
            match WindowsJobObject::create_and_assign(child) {
                Ok(job) => Ok(Self { job: Some(job) }),
                Err(error) => {
                    if child.try_wait().ok().flatten().is_some() {
                        Ok(Self { job: None })
                    } else {
                        Err(error)
                    }
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Ok(Self {})
        }
    }
}

#[cfg(windows)]
#[derive(Debug)]
struct WindowsJobObject(windows_sys::Win32::Foundation::HANDLE);

// The handle is an owned kernel object; ownership moves with the guard and
// Drop closes it exactly once.
#[cfg(windows)]
unsafe impl Send for WindowsJobObject {}
#[cfg(windows)]
unsafe impl Sync for WindowsJobObject {}

#[cfg(windows)]
impl WindowsJobObject {
    fn create_and_assign(child: &Child) -> io::Result<Self> {
        use std::{mem::size_of, ptr::null_mut};
        use windows_sys::Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
        };
        let handle: HANDLE = unsafe { CreateJobObjectW(null_mut(), null_mut()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::last_os_error());
        }
        let process = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("child process has no handle"))?;
        if unsafe { AssignProcessToJobObject(handle, process) } == 0 {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for WindowsJobObject {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_bounded_and_hashed() {
        let resolved = ExecutionPolicyProfile::resolve("shell_execute").unwrap();
        assert_eq!(resolved.profile.schema_version, 1);
        assert_eq!(resolved.profile_hash.len(), 64);
        assert_eq!(resolved.profile.network, NetworkPolicy::Deny);
    }

    #[test]
    fn arbitrary_environment_is_rejected() {
        let mut env = HashMap::new();
        env.insert("EVOHIME_API_TOKEN".into(), "secret".into());
        assert!(reject_user_environment(Some(&env)).is_err());
    }

    #[test]
    fn rejects_direct_egress_clients_and_tunnel_domains() {
        assert!(validate_process_egress("curl.exe", &[]).is_err());
        assert!(
            validate_process_egress("git", &["https://example.trycloudflare.com/c/id".into()])
                .is_err()
        );
        assert!(validate_process_egress("git", &["status".into()]).is_ok());
        assert!(validate_process_egress("git", &["https://github.com/org/repo".into()]).is_ok());
    }

    #[test]
    fn rejects_sensitive_source_combined_with_upload_arguments() {
        let args = vec![
            "--data-binary".into(),
            "@-".into(),
            "~/.ssh/id_ed25519".into(),
            "https://example.invalid/collect".into(),
        ];
        assert!(validate_process_egress("git", &args).is_err());
        assert!(validate_process_egress("cargo", &["test".into()]).is_ok());
    }

    #[test]
    fn detects_common_secret_locations_for_egress_checks() {
        for path in [
            ".env.production",
            "~/.ssh/id_ed25519",
            "~/.aws/credentials",
            "~/.config/solana/id.json",
            "~/.ethereum/keystore/key.json",
        ] {
            let args = vec![path.into(), "https://collector.invalid/collect".into()];
            assert!(validate_process_egress("grep", &args).is_err(), "{path}");
        }
        let grep_pattern = vec![
            ".env*".into(),
            "PRIVATE|SECRET|MNEMONIC|SEED|KEY|PASS".into(),
            "https://collector.invalid/collect".into(),
        ];
        assert!(validate_process_egress("grep", &grep_pattern).is_err());
    }
}
